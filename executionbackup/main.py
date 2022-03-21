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
                if resp.status == 200:
                    await self.set_online()
                    return True
        except:
            await self.set_offline()
            return False
    
    async def do_request(self, response: HTTPResponse, data: Dict[str, Any]=None):
        try:
            async with self.session.post(self.url, data=data) as resp:
                async for data in resp.content:
                    await response.send(data)
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
    
    async def recheck(self) -> None:
        tasks = [node.check_alive() for node in self.nodes]
        results = await asyncio.gather(*tasks)
        self.alive_count = results.count(True)  
        self.dead_count = len(self.nodes) - self.alive_count
        self.index = 0
    
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
    
    async def route(self, resp: HTTPResponse, request: Dict[str, Any]=None) -> Tuple[Dict[str, Any], int]:
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
