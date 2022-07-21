from sanic import Sanic, response
import argparse
#import jwt         # we're simply going to test jwt ourselves and not on tests

parser = argparse.ArgumentParser()
parser.add_argument('--port', type=int, required=True)
parser.add_argument('--name', type=str, required=True)
parser.add_argument('--jwt_secret', type=str, required=True)
args = parser.parse_args()

jwt_secret = None
with open(args.jwt_secret) as f:
    jwt_secret = f.read()[2:]


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
        #if not request.headers.get('tester') == '1':
        #    # we only check jwt here since otherwise we assume the CL will just pass the jwt
        #    # strip bearer
        #    token = request.headers.get('Authorization').split(' ')[1]
        #    claims = jwt.decode(token, jwt_secret, algorithms='HS256', options={'require': ['iat']})
        #    print(f'claims: {claims}')
        return response.json({"jsonrpc": "2.0", "id": 1, "result": False})

    resp = await request.respond()
    await resp.send(app.ctx.expectedresponse, end_stream=True)


app.run(host='0.0.0.0', port=args.port, workers=1, debug=False, access_log=False)