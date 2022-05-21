import aiohttp
from typing import *
import asyncio
from . import logger
from ujson import dumps, loads
from sanic.request import Request
from time import monotonic
import websockets

__all__ = ['ServerOffline', 'NodeInstance', 'OutOfAliveNodes', 'NodeRouter']


class ServerOffline(Exception):
    pass


class NodeInstance:
    def __init__(self, url: str):
        self.url: str = url
        self.status: bool = False
        self.dispatch = logger.dispatch
        
    async def setup_session(self):
        if self.url.startswith('w'):
            self.session = await websockets.connect(self.url)
            self.is_ws = True
        else:
            self.session = aiohttp.ClientSession(json_serialize=dumps)
            self.is_ws = False

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

    async def check_alive(self):    # return the a tuple of nodeinstance, status, and the response time
        #await self.set_online()    # TODO: find a way to get these nodes synced but not get requests from them
        #return (self, True, 1)
        start = 0.0
        end = 0.0       # because otherwise we can't read it in the except block
        try:
            start = monotonic()
            resp = await self.do_request(data='{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}', json={"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}, headers={'Content-type': 'application/json'})
            end = monotonic()
            if (loads(resp[0]))['result']: # result is false if node is synced
                await self.set_offline()
                return (self, False, ((end - start) * 1000))
            else:
                await self.set_online()
                return (self, True, ((end - start) * 1000))

        except Exception:
            await self.set_offline()
            return (self, False, ((end - start) * 1000))
    
    async def do_request(self, *, data: str, json: dict, headers: Dict[str, Any]=None) -> Union[Tuple[str, int, dict], ServerOffline]:
        try:
            if self.is_ws:
                await self.session.send(data)
                resp = await self.session.recv()
                return (resp, 200, {'Content-Encoding': 'identity', 'Content-Type': 'application/json', 'Vary': 'Origin', 'Content-Length': len(resp)}) # geth response headers include the date but most clients (probably) don't care
            else:
                if json['method'] == 'eth_subscribe':
                    return ('{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"The method eth_subscribe does not exist/is not available"}}', 200, '{"Content-Encoding": "identity", "Content-Type": "application/json", "Vary": "Origin", "Content-Length": 117}')
                async with self.session.post(self.url, data=data, headers=headers, timeout=3) as resp:
                    return (await resp.text(), resp.status, dict(resp.headers))
        except Exception:
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
        self.nodes: List[NodeInstance] = []
        for url in self.urls:
            inst = NodeInstance(url)
            await inst.setup_session()
            self.nodes.append(inst)
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
    async def do_engine_route(self, req: Request, ws, ws_text) -> None:
        if ws:  # you NEED to have application/json as a header for this to work when contacting http endpoints
            if loads(ws_text)['method'] == 'engine_getPayloadV1':
                n = await self.get_execution_node()
                r = await n.do_request(data=req.body, json=req.json, headers={'Content-type': 'application/json'})
                await ws.send(r[0])
            else:
                # wait for one node to respond but send the request to all
                n = await self.get_execution_node()
                r = await n.do_request(data=req.body, json=req.json, headers={'Content-type': 'application/json'})
                await ws.send(r[0])
                [asyncio.create_task(node.do_request(data=req, json=req.json, headers={'Content-type': 'application/json'})) for node in self.alive if node != n]

        else:
            if req.json['method'] == 'engine_getPayloadV1':
                n = await self.get_execution_node()
                r = await n.do_request(data=req.body, json=req.json, headers=req.headers)
                resp = await req.respond(status=r[1], headers=r[2])
                await resp.send(r[0], end_stream=True)
            elif req.json['method'] == 'engine_forkchoiceUpdatedV1':
                #resps = await asyncio.gather(*[node.do_request(data=req, json=req.json, headers=req.headers) for node in self.alive])
                
                #for resp in resps:
                #    if not loads(resp[0])['result']['payloadStatus']['status'] == 'VALID':
                #        # TODO: return SYNCING
                #        return

                n = await self.get_execution_node()
                r = await n.do_request(data=req.body, json=req.json, headers=req.headers)
                resp = await req.respond(status=r[1], headers=r[2])
                await resp.send(r[0], end_stream=True)
                [asyncio.create_task(node.do_request(data=req.body, json=req.json, headers=req.headers)) for node in self.alive if node != n]
                print(r[0]) # professional debugging
            else:   
                # wait for just one node to respond but send it to all
                n = await self.get_execution_node()
                r = await n.do_request(data=req.body, json=req.json, headers=req.headers)
                resp = await req.respond(status=r[1], headers=r[2])
                await resp.send(r[0], end_stream=True)
                [asyncio.create_task(node.do_request(data=req.body, json=req.json, headers=req.headers)) for node in self.alive if node != n]


    async def route(self, req: Request, ws, ws_text) -> None:
        # handle "normal" requests
        if ws:  # you NEED to have application/json as a header for this to work when contacting http endpoints
            n = await self.get_execution_node()
            r = await n.do_request(data=ws_text, json=req.json, headers={'Content-type': 'application/json'})
            await ws.send(r[0])
        else:
            n = await self.get_execution_node()
            r = await n.do_request(data=req.body, json=req.json, headers=req.headers)
            resp = await req.respond(status=r[1], headers=r[2])
            await resp.send(r[0], end_stream=True)
            

    async def stop(self) -> None:
        tasks = [node.stop() for node in self.nodes]
        await asyncio.gather(*tasks)
