import asyncio
import ujson
import executionbackup
from sanic import Sanic, response
from sanic.request import Request
from platform import python_version, system, release, machine
import asyncpg
from typing import Dict


app = Sanic('router')

Account = executionbackup.Account
    
router = executionbackup.NodeRouter(['http://127.0.0.1:8000'])
accounts: Dict[str, Account] = {}

# make db table: ("key" UNIQUE TEXT, "callAmount" BIGINT, "callJson" TEXT)

async def setAccounts():
    async with router.db.acquire() as con:
        async with con.transaction():
            async for record in con.cursor("""SELECT * FROM accounts;"""):
                accounts[record['key']] = Account(record['key'], record['callAmount'], ujson.loads(record['callJson']))   # TODO: make the calldict in the database and set it here

async def doDump():
    for k, v in accounts.items():
        await router.db.execute("""INSERT INTO accounts ($1, $2) ON CONFLICT (accounts.key) DO UPDATE "callAmount" = $2;""", v.key, v.calls, ujson.dumps(v.callDict))   # TODO: also add calldict here

async def dumpIntoDb():
    await asyncio.sleep(900) # since it's called at the start of the execution there are still no calls
    while True:
        await doDump()
        asyncio.sleep(900) # 15m

@app.before_server_start
async def before_start(app: Sanic, loop):
    await router.setup()
    app.add_task(router.repeat_check())
    router.db = await asyncpg.create_pool('postgresql://tennisbowling:hehe@127.0.0.1/executionbackup')
    await setAccounts()
    app.add_task(dumpIntoDb())

@app.before_server_stop
async def after_stop(app: Sanic, loop):
    await router.stop() # no more requests come
    for k, v in accounts.items():
        await router.db.execute("""INSERT INTO accounts VALUES ($1, $2, $3) ON CONFLICT (accounts.key) DO UPDATE SET "callAmount" = $2 AND "callsJson" = $3;""", v.key, v.calls, v.callDict)   # TODO: also add calldict here
    await router.db.close()
    
@app.route('/<path:path>', methods=['POST'], stream=True)
async def route(request: Request, path: str):
    auth = (request.raw_url.decode()).strip('/')
    
    accnt = accounts.get(auth)
    if not accnt:
        return response.json({'error': 'api key not authorized'}, status=503)

    response = await request.respond() # TODO: get geth response headers and put them here
    await router.route(response, request.body)
    call = (request.body.decode())['method']
    accnt[call] += 1

@app.route('/executionbackup/version', methods=['GET'])
async def ver(request: Request):
    return response.text(f'executionbackup-{executionbackup.__version__}/{system() + release()}-{machine()}/python{python_version()}')

@app.route('/executionbackup/status', methods=['GET'])
async def status(request: Request):
    #await router.recheck()
    ok = 200 if router.alive_count > 0 else 503
    return response.json({'status': ok, 'alive': router.alive_count, 'dead': router.dead_count, 'clients': len(accounts)}, status=ok)

@app.route('/executionbackup/addkey', methods=['POST'])
async def addkey(request: Request):
    key = request.json['key']

    if accounts.get(key):
        return response.json({'success': False, 'message': 'key already exsts'})        # TODO: add error codes
    await router.db.execute("""INSERT INTO accounts VALUES ($1, $2, $3)""", key, 0, '{}')
    accounts[key] = Account(key)
    return response.json({'success': True})

@app.route('/executionbackup/removekey', methods=['POST'])
async def removekey(request: Request):
    key = request.json['key']

    if not accounts.get(key):
        return response.json({'success': False, 'message': 'key does not exist'})
    await router.db.execute("""DELETE FROM accounts WHERE "key" = $1;""", key)
    del accounts[key]
    return response.json({'success': True})

@app.route('/executionbackup/cachedstats', methods=['GET'])
async def cachedstats(request: Request):
    key = request.json['key']

    if not accounts.get(key):
        return response.json({'success': False})
    
    return response.json({'success': True, 'stats': str(accounts[key].callDict)})

@router.listener('node_offline')
async def node_offline(url: str):
    print(f'Node {url} is offline')

@router.listener('all_nodes_offline')
async def all_nodes_offline():
    print('All nodes are offline!')

@router.listener('node_online')
async def node_online(url: str):
    print(f'Node {url} is online')

@router.listener('node_router_online')
async def node_router_online():
    print('Node router online')

app.run('127.0.0.1', port=8001, access_log=True)
