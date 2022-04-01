from secrets import choice
import aiohttp
from typing import *
import asyncio
from . import logger
from sanic.response import HTTPResponse
from ujson import dumps


class ServerOffline(Exception):
    pass

class NodeInstance:
    def __init__(self, url: str):
        self.url: str = url
        self.session = aiohttp.ClientSession(headers={'Content-type': 'application/json'}, json_serialize=dumps)
        self.status: bool = False
        self.dispatch = logger.dispatch
    
    async def set_online(self):
        if self.status:
            return
        self.status = True
        await self.dispatch('node_online', self.url)
    
    async def set_offline(self):
        if not self.status:
            return
        self.status = False
        await self.dispatch('node_offline', self.url)

    async def check_alive(self) -> bool:
        try:
            async with self.session.post(self.url, json={'jsonrpc': '2.0', 'method': 'eth_syncing', 'params': [], 'id': 1}) as resp:
                if (await resp.json())['result']:
                    await self.set_offline()
                    return False
                else:
                    await self.set_online()
                    return True

        except:
            await self.set_offline()
            return False
    
    async def do_request(self, response: HTTPResponse, data: Dict[str, Any]=None):
        try:
            async with self.session.post(self.url, data=data) as resp:
                await response.send(await resp.text())
        except (aiohttp.ServerTimeoutError, aiohttp.ServerConnectionError):
            await self.set_offline()
            return ServerOffline()
    
    async def stop(self):
        await self.session.close()

class OutOfAliveNodes:
    pass

class NodeRouter:
    def __init__(self, urls: List[str]):
        if not urls:
            raise ValueError('No nodes provided')
        self.urls = urls
        self.dispatch = logger.dispatch
        self.listener = logger.listener
    
    async def recheck(self) -> List[NodeInstance]: # returns a list of alive nodes
        tasks = [node.check_alive() for node in self.nodes]
        results = await asyncio.gather(*tasks)
        self.alive_count = results.count(True)  
        self.dead_count = len(self.nodes) - self.alive_count
        self.index = 0
        return [node for node in self.nodes if node.status]
    
    async def repeat_check(self) -> None:
        while True:
            await self.recheck()
            await asyncio.sleep(60)

    async def setup(self) -> None:
        self.nodes: List[NodeInstance] = [NodeInstance(url) for url in self.urls]
        await self.recheck()
        await self.dispatch('node_router_online')
    
    async def get_alive_node(self) -> Optional[NodeInstance]:
        if self.alive_count == 0:
            return None
        if self.index >= self.alive_count:
            self.index = 0
        node = self.nodes[self.index]
        self.index += 1
        return node
    
    async def do_request(self, resp: HTTPResponse, request: Dict[str, Any]=None) -> Union[None, ServerOffline, OutOfAliveNodes]:
        node = await self.get_alive_node()
        try:
            await node.do_request(resp, request)
        except ServerOffline:
            return ServerOffline()
        except AttributeError:
            return OutOfAliveNodes() # you're out of nodes
    

    # https://github.com/ethereum/execution-apis/blob/main/src/engine/specification.md#load-balancing-and-advanced-configurations=

    # CL will be the one contacting us, and we route it to the node
    # - Choosing a payload from the getPayload responses (just picking the first is the easiest solution, choosing the most profitable is ideal but much harder).
    # - Selecting a response from newPayload and forkchoiceUpdated and ensuring they don't conflict.
    # - Detecting poor service from the nodes and switching between them.

    # debated: Regaring picking responses for newPayload and forkchoiceUpdated, the CL probably wants to try and stick with the same one, for consistency. Then switch over when the primary one is determined to have poor quality of service.
    async def do_request_all(self, resp: HTTPResponse, request: Dict[str, Any]=None) -> None:
        if request['method'] == 'engine_getPayloadV1':    # right now we just get one payload but later we will pick the most profitable one
            await self.route(resp, request)
            return  # we don't need to do anything else


        # send the request to all nodes
        tasks = [node.do_request(resp, request) for node in (await self.recheck())]
        resps = await asyncio.gather(*tasks)

        # find the majority response
        majority_response = resps[0]
        majority_count = 1
        for resp in resps[1:]:
            if resp == majority_response:
                majority_count += 1
            else:
                majority_count -= 1
        if majority_count < 0:
            majority_count = 0
        majority_response = majority_response if majority_count > len(resps) / 2 else choice(resps)

        await resp.send(majority_response)
        return
        
    
    async def route(self, resp: HTTPResponse, request: Dict[str, Any]=None) -> None:
        data = await self.do_request(resp, request)

        if isinstance(data, OutOfAliveNodes):
            await resp.send(dumps({'error': 'no upstream nodes'}), end_stream=True)
            return

        while isinstance(data, ServerOffline):
            await self.recheck()
            data = await self.do_request(resp, request)
            if isinstance(data, OutOfAliveNodes):
                await resp.send(dumps({'error': 'no upstream nodes'}), end_stream=True)
                return
    
    async def stop(self) -> None:
        tasks = [node.stop() for node in self.nodes]
        await asyncio.gather(*tasks)
