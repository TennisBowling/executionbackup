import executionbackup
from sanic import Sanic, response
from sanic.request import Request
from sanic.log import logger, Colors
from ujson import loads
from platform import python_version, system, release, machine
import argparse
import logging
from os import cpu_count
from psutil import Process


parser = argparse.ArgumentParser()
parser.add_argument('--nodes', nargs='+', required=True, help='Nodes to load-balance across. Example: --nodes http://localhost:8545 http://localhost:8546 \nMust be at least one.')
parser.add_argument('--port', type=int, default=8000, help='Port to run the load-balancer on.')
args = parser.parse_args()


app = Sanic('router')
logger.setLevel(logging.ERROR) # we don't want to see the sanic logs
    
router = executionbackup.NodeRouter(args.nodes)

class coloredFormatter(logging.Formatter):

    reset =  "\x1b[0m"

    FORMATS = {
        logging.DEBUG: '[%(asctime)s] %(levelname)s - %(message)s' + reset,
        logging.INFO: f'[%(asctime)s] {Colors.GREEN}%(levelname)s{reset} - %(message)s',
        logging.WARNING: f'[%(asctime)s] {Colors.YELLOW}%(levelname)s{reset} - %(message)s',
        logging.ERROR: f'[%(asctime)s] {Colors.RED}%(levelname)s{reset} - %(message)s',
        logging.CRITICAL: f'[%(asctime)s] {Colors.RED}%(levelname)s{reset} - %(message)s'
    }

    def format(self, record):
        log_fmt = self.FORMATS.get(record.levelno)
        formatter = logging.Formatter(log_fmt)
        return formatter.format(record)

logger = logging.getLogger('router')
logger.setLevel(logging.INFO)
ch = logging.StreamHandler()
ch.setLevel(logging.INFO)
ch.setFormatter(coloredFormatter())
logger.addHandler(ch)


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
        await router.do_engine_route(request, None, None)
    else:
        await router.route(request, None, None)

@app.websocket('/')
async def route_ws(request: Request, ws):
    while True:
        req = await ws.recv()
        if loads(req)['method'].startswith('engine_'):
            await router.do_engine_route(request, ws, req)
        else:
            await router.route(request, ws, req)

@app.route('/executionbackup/version', methods=['GET'])
async def ver(request: Request):
    return response.text(f'executionbackup-{executionbackup.__version__}/{system() + release()}-{machine()}/python{python_version()}')

@app.route('/executionbackup/status', methods=['GET'])
async def status(request: Request):
    #await router.recheck()
    ok = 200 if len(router.alive) > 0 else 503
    return response.json({'status': ok, 'alive': len(router.alive), 'dead': len(router.dead), 'RAM usage': (Process().memory_info()[0] / float(2 ** 20))}, status=ok)

@router.listener('node_online')
async def node_online(url: str):
    logger.info(f'Node {url} is online')

@router.listener('node_offline')
async def node_offline(url: str):
    logger.info(f'Node {url} is offline')

@router.listener('all_nodes_offline')
async def all_nodes_offline():
    logger.critical('All nodes are offline')

@router.listener('node_error')
async def node_error(url: str, error: str):
    logger.warning(f'Node {url} has error: {error}')

@router.listener('node_router_online')
async def node_router_online():
    logger.info('Node router is online')

app.run('0.0.0.0', port=args.port, access_log=False, debug=False, workers=cpu_count())
