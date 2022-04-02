import executionbackup
from sanic import Sanic, response
from sanic.request import Request
from platform import python_version, system, release, machine
import argparse



parser = argparse.ArgumentParser()
parser.add_argument('--nodes', nargs='+', required=True, help='Nodes to load-balance across. Example: --nodes http://localhost:8545 http://localhost:8546 \nMust be at least one.')
parser.add_argument('--port', type=int, default=8000, help='Port to run the load-balancer on.')
args = parser.parse_args()


app = Sanic('router')
    
router = executionbackup.NodeRouter(args.nodes)


@app.before_server_start
async def before_start(app: Sanic, loop):
    await router.setup()
    app.add_task(router.repeat_check())

@app.before_server_stop
async def after_stop(app: Sanic, loop):
    await router.stop() # no more requests come
    
@app.route('/', methods=['POST'])
async def route(request: Request):

    if request.json['method'].startswith('engine_'):
        await router.do_request_all(request)
    else:
        await router.route(request)


@app.route('/executionbackup/version', methods=['GET'])
async def ver(request: Request):
    return response.text(f'executionbackup-{executionbackup.__version__}/{system() + release()}-{machine()}/python{python_version()}')

@app.route('/executionbackup/status', methods=['GET'])
async def status(request: Request):
    #await router.recheck()
    ok = 200 if router.alive_count > 0 else 503
    return response.json({'status': ok, 'alive': router.alive_count, 'dead': router.dead_count}, status=ok)

@router.listener('node_offline')
async def node_offline(url: str):
    print(f'Node {url} is offline')

@router.listener('all_nodes_offline')
async def all_nodes_offline():
    print('All nodes are offline!')

@router.listener('node_online')
async def node_online(url: str):
    print(f'Node {url} is online')

@router.listener('node_error')
async def node_error(url: str, error: str):
    print(f'Node {url} error: {error}')

@router.listener('node_router_online')
async def node_router_online():
    print('Node router online')

app.run('0.0.0.0', port=args.port, access_log=True)
