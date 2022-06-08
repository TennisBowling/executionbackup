from __future__ import annotations # to typehint check_alive
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
        self.status: int = 0    # 0 = offline, 1 = online, 2 = online but not synced
        self.dispatch = logger.dispatch
        
    async def setup_session(self):
        if self.url.startswith('w'):
            self.session = await websockets.connect(self.url)
            self.is_ws = True
        else:
            self.session = aiohttp.ClientSession(json_serialize=dumps)
            self.is_ws = False

    async def set_online(self):
        if self.status == 1:
            return
        self.status = 1
        await self.dispatch('node_online', self.url)
    
    async def set_offline(self):
        if self.status == 0:
            return
        self.status = 0
        await self.dispatch('node_offline', self.url)

    async def check_alive(self) -> Tuple[NodeInstance, int, float]:    # return the a tuple of nodeinstance, status, and the response time
        start = 0.0
        end = 0.0       # because otherwise we can't read it in the except block
        try:
            start = monotonic()
            resp = await self.do_request(data='{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}', json={"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}, headers={'Content-type': 'application/json'})
            end = monotonic()
            
            if isinstance(loads(resp[0])['result'], dict):
                await self.set_online()
                return (self, 2, ((end - start) * 1000))

            else:
                await self.set_online()
                return (self, 1, ((end - start) * 1000))

        except Exception:
            await self.set_offline()
            return (self, 0, ((end - start) * 1000))
    
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
    def __init__(self, urls: List[str], fcU_invalid_threshold: int):
        if not urls:
            raise ValueError('No nodes provided')
        self.urls = urls
        self.dispatch = logger.dispatch
        self.listener = logger.listener
        self.index = 0
        self.fcU_invalid_threshold = fcU_invalid_threshold

    async def recheck(self):
        tasks = [node.check_alive() for node in self.nodes]
        results = await asyncio.gather(*tasks)
        self.alive = [node for node, status, time in sorted(results, key=lambda x: x[1]) if node.status == 1]

        self.dead = [node for node, time, status in results if node.status == 0]

        self.alive_but_syncing = [node for node, status, time in results if node.status == 2]
    
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

    async def send_to_alive_and_syncing(self, *, data: str, json: dict, headers: Dict[str, Any], except_node: NodeInstance) -> None:
        for node in self.alive:
            if node != except_node:
                asyncio.create_task(node.do_request(data=data, json=json, headers=headers))

        for node in self.alive_but_syncing:
            asyncio.create_task(node.do_request(data=data, json=json, headers=headers))
    
    async def get_execution_node(self) -> NodeInstance:
        # get the same node, if offline, add 1 to the index and try again
        if not self.alive:
            return OutOfAliveNodes()
        n = self.alive[self.index]
        self.index = (self.index + 1) % len(self.alive)
        return n

    # find the what response has is the most common
    # majority must be at least fcU_invalid_threshold
    async def fcU_majority(self, resps: List[str]) -> Union[str, None]:   # even though no benefit in making this async, we use it to return control to the event loop more (preventing blockingd)
        counts = {}
        for resp in resps:
            if resp in counts:
                counts[resp] += 1
            else:
                counts[resp] = 1
        max_count = max(counts.values())
        if max_count / len(resps) >= self.fcU_invalid_threshold:
            return max(counts, key=counts.get)
        else:
            return None


    # logic for forkchoiceUpdated
    # there are three possible statuses: VALID, INVALID, SYNCING (SYNCING is safe and will stall the CL)
    # ONLY return VALID when all nodes return VALID
    # if any node returns INVALID, return SYNCING
    #
    # to return SYNCING, the body of the response should be: '{"jsonrpc":"2.0","id":1,"result":{"payloadStatus":{"status":"SYNCING","latestValidHash":null,"validationError":null},"payloadId":null}}'

    # BUT if fcU_majority returns INVALID, return it
    # if there is a resonse of INVALID and no majority, return SYNCING
    async def fcU_logic(self, resps: Tuple[Tuple[str, int, dict]]) -> Tuple[str, int, dict]:    # same reason for async as fcU_majority
        maj = await self.fcU_majority([resp[0] for resp in resps])    
        if loads(maj)['result']['payloadStatus']['status'] == 'INVALID':    # here we see if most responses are INVALID, if so, return INVALID
            return (maj, 200, {'Content-Encoding': 'identity', 'Content-Type': 'application/json', 'Vary': 'Origin', 'Content-Length': len(maj)})
        
        
        for resp in resps:  # and here we see if just one response is INVALID, if so, return SYNCING (since it isn't a majority, we can't be sure to reject it)
            json = loads(resp[0])
            if json['result']['payloadStatus']['status'] == 'INVALID':
                asyncio.create_task(self.dispatch('fcU_non_majority_invalid', resp[0]))
                return ('{"jsonrpc":"2.0","id":1,"result":{"payloadStatus":{"status":"SYNCING","latestValidHash":null,"validationError":null},"payloadId":null}}', resp[1], resp[2])
            elif json['result']['payloadStatus']['status'] == 'SYNCING':
                return (resp[0], resp[1], resp[2])
        
        # if we get here, all responses are VALID
        # send to syncing nodes
        for node in self.alive_but_syncing:
            asyncio.create_task(node.do_request(data=resps[0][0], json=loads(resps[0][0]), headers=resps[0][2]))
        return (resps[0][0], resps[0][1], resps[0][2])


    # https://github.com/ethereum/execution-apis/blob/main/src/engine/specification.md#load-balancing-and-advanced-configurations=
    async def do_engine_route(self, req: Request, ws, ws_text: str) -> None:
        if ws:  # you NEED to have application/json as a header for this to work when contacting http endpoints
            json_ws_text = loads(ws_text)
            if json_ws_text['method'] == 'engine_getPayloadV1':
                n = await self.get_execution_node()
                r = await n.do_request(data=req.body, json=req.json, headers={'Content-type': 'application/json'})
                await ws.send(r[0])

            elif json_ws_text['method'] == 'engine_forkchoiceUpdatedV1':
                resps = []
                for n in self.nodes:
                    r = await n.do_request(data=req.body, json=req.json, headers={'Content-type': 'application/json'})
                    resps.append(r)
                r = await self.fcU_logic(resps)
                await ws.send(r[0])

            else:
                # wait for one node to respond but send the request to all
                n = await self.get_execution_node()
                r = await n.do_request(data=req.body, json=req.json, headers={'Content-type': 'application/json'})
                await ws.send(r[0])
                asyncio.create_task(self.send_to_alive_and_syncing(data=req.body, json=req.json, headers={'Content-type': 'application/json'}, except_node=n))

        else:
            if req.json['method'] == 'engine_getPayloadV1':
                n = await self.get_execution_node()
                r = await n.do_request(data=req.body, json=req.json, headers=req.headers)
                resp = await req.respond(status=r[1], headers=r[2])
                await resp.send(r[0], end_stream=True)
            
            elif req.json['method'] == 'engine_forkchoiceUpdatedV1':
                resps = await asyncio.gather(*[node.do_request(data=req, json=req.json, headers=req.headers) for node in self.alive])

                logic_res = await self.fcU_logic(resps)
                resp = await req.respond(status=logic_res[1], headers=logic_res[2])
                await resp.send(logic_res[0], end_stream=True)
            
            else:   
                # wait for just one node to respond but send it to all
                n = await self.get_execution_node()
                r = await n.do_request(data=req.body, json=req.json, headers=req.headers)
                resp = await req.respond(status=r[1], headers=r[2])
                await resp.send(r[0], end_stream=True)
                asyncio.create_task(self.send_to_alive_and_syncing(data=req.body, json=req.json, headers=req.headers, except_node=n))


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
