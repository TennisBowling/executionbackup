from sanic import Sanic, response
import argparse

parser = argparse.ArgumentParser()
parser.add_argument('--port', type=int, required=True)
parser.add_argument('--name', type=str, required=True)
args = parser.parse_args()

app = Sanic(args.name)

app.ctx.expectedresponse = None

@app.route('/mock/set_response', methods=['POST'])
async def set_response(request):
    """Set the response for the mock EE."""
    app.ctx.expectedresponse = request.body.decode()
    return response.json({"status": "ok"})

@app.route('/', methods=['POST'])
async def get_response(request):
    """Get the response for the mock EE."""
    #if request.headers.get('Content-Type') != 'application/json':
    #    return response.json({"status": "error", "error": "Content-Type must be application/json"})
    
    if request.json['method'] == 'eth_syncing':
        return response.json({"jsonrpc": "2.0", "id": 1, "result": False})

    resp = await request.respond()
    await resp.send(app.ctx.expectedresponse, end_stream=True)


app.run(host='0.0.0.0', port=args.port, workers=1, debug=False, access_log=False)