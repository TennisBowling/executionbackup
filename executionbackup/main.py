import aiohttp
from typing import *
import asyncio
from . import logger
from ujson import dumps, loads
from sanic.request import Request
from time import monotonic


class ServerOffline(Exception):
    pass

class NodeInstance:
    def __init__(self, url: str):
        self.url: str = url
        self.session = aiohttp.ClientSession(json_serialize=dumps)
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

    async def check_alive(self):
        await self.set_online()
        return (self, True, 1)
        try:
            start = monotonic()
            async with self.session.post(self.url, json={'jsonrpc': '2.0', 'method': 'eth_syncing', 'params': [], 'id': 1}, headers={'Content-type': 'application/json'}, timeout=10) as resp:
                end = monotonic()
                if (await resp.json())['result']:
                    await self.set_offline()
                    return (self, False, ((end - start) * 1000))
                else:
                    await self.set_online()
                    return (self, True, ((end - start) * 1000))

        except:
            await self.set_offline()
            return (self, False, ((end - start) * 1000))
    
    async def do_request(self, headers: Dict[str, Any], data: Dict[str, Any]=None) -> Union[Tuple[str, int, str], ServerOffline]:
        try:
            async with self.session.post(self.url, data=data, headers=headers, timeout=3) as resp:   # if we go higher than 3, we may be more counter-productive waiting for all the nodes to respond 
                return (await resp.text(), resp.status, dict(resp.headers))
        except (aiohttp.ServerTimeoutError, aiohttp.ServerConnectionError, aiohttp.ClientConnectionError, aiohttp.ClientOSError, aiohttp.ClientResponseError):
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
        self.index = 0

    async def recheck(self):
        tasks = [node.check_alive() for node in self.nodes]
        results = await asyncio.gather(*tasks)
        self.alive = [node for node, status, time in sorted(results, key=lambda x: x[1]) if node.status]

        self.dead = [node for node, time, status in results if not node.status]
    
    async def repeat_check(self) -> None:
        while True:
            await self.recheck()
            await asyncio.sleep(60)

    async def setup(self) -> None:
        self.nodes: List[NodeInstance] = [NodeInstance(url) for url in self.urls]
        await self.recheck()
        await self.dispatch('node_router_online')
    
    async def get_execution_node(self) -> NodeInstance:
        # get the same node, if offline, add 1 to the index and try again
        if not self.alive:
            return OutOfAliveNodes()
        n = self.alive[self.index]
        self.index = (self.index + 1) % len(self.alive)
        return n

    # https://github.com/ethereum/execution-apis/blob/main/src/engine/specification.md#load-balancing-and-advanced-configurations=

    # CL will be the one contacting us, and we route it to the node
    # - Choosing a payload from the getPayload responses (just picking the first is the easiest solution, choosing the most profitable is ideal but much harder).
    # - Selecting a response from newPayload and forkchoiceUpdated and ensuring they don't conflict.
    # - Detecting poor service from the nodes and switching between them.

    # debated: Regaring picking responses for newPayload and forkchoiceUpdated, the CL probably wants to try and stick with the same one, for consistency. Then switch over when the primary one is determined to have poor quality of service.
    async def do_engine_route(self, req: Request) -> None:
        if req.json['method'] == 'engine_getPayloadV1':
            n = await self.get_execution_node()
            r = await n.do_request(req.headers, req.body)
            resp = await req.respond(status=r[1], headers=r[2])
            await resp.send(r[0], end_stream=True)
        else:
            # wait for just one node to respond but send it to all
            n = await self.get_execution_node()
            r = await n.do_request(req.headers, req.body)
            resp = await req.respond(status=r[1], headers=r[2])
            await resp.send(r[0], end_stream=True)
            [asyncio.create_task(node.do_request(req.headers, req.body)) for node in self.nodes if node.status and node != n]
            return
    
    async def route(self, req: Request) -> None:
        # handle "normal" requests
        n = await self.get_execution_node()
        r = await n.do_request(req.headers, req.body)
        resp = await req.respond(status=r[1], headers=r[2])
        await resp.send(r[0], end_stream=True)
            

    async def stop(self) -> None:
        tasks = [node.stop() for node in self.nodes]
        await asyncio.gather(*tasks)
