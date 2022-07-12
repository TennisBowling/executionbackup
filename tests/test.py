"""A python program to test executionbackup scenarios."""
import time
import requests
from sanic import Sanic, response
import ujson
import asyncio
import subprocess

# we start two mock EE's (which will just be sanic servers)
# We need to fork off running the executionbackup (EB) binary that connect to the mock EEs
# then we send requests to EB, and check if they match the expected response

class MockEE:
    """A mock EE server."""
    def __init__(self, port, name):
        self.port = port
        self.app = Sanic(name)
    
    # listen on /mock/set_response
    # the test will send a POST request to this endpoint to set the expected response that we should get from EB
    async def set_response(self, request):
        """Set the response for the mock EE."""
        self.response = request.body
        return response.json({"status": "ok"})
    
    # listen on /
    # EB will send a POST request to this and we should return the expected response (self.response)
    async def get_response(self, request):
        """Get the response for the mock EE."""
        if request.headers.get('Content-Type') != 'application/json':
            return response.json({"status": "error", "error": "Content-Type must be application/json"})
        
        if request.json['method'] == 'eth_syncing':
            return response.json({"jsonrpc": "2.0", "id": 1, "result": False})

        return response.json(self.response)
    
    async def run(self):
        """Run the mock EE."""
        self.app.add_route('POST', '/mock/set_response', self.set_response)
        self.app.add_route('POST', '/', self.get_response)
        self.server = await self.app.create_server(host='0.0.0.0', port=self.port)

def send_to_EEs(data):
    """Send data to the mock EEs."""
    requests.post('http://localhost:8001/mock/set_response', data=data)
    requests.post('http://localhost:8002/mock/set_response', data=data)
    requests.post('http://localhost:8003/mock/set_response', data=data)

def check_syncing():
    # no need to send expected response since mock ee are never syncing

    # send a request to EB and check if we get the expected response
    response = requests.post('http://localhost:8545/', data='{"id":1,"jsonrpc":"2.0","method":"eth_syncing","params":null}', headers={'Content-Type': 'application/json', 'tester': '1'})
    assert response.json() == {"jsonrpc": "2.0", "id": 1, "result": False}, 'Expected response did not match actual response for eth_syncing'

def check_fcu_valid():
    expected = {
        'result': {
            'payloadStatus': {
                'status': 'VALID'
            }
        }
    }
    send_to_EEs(ujson.dumps(expected))

    response = requests.post('http://localhost:8545/', data=ujson.dumps({'method': 'engine_forkchoiceUpdatedV1'}), headers={'Content-Type': 'application/json', 'tester': '1'})
    
    print(f'Expected response: {expected}, got {response.json()}')
    
    assert response.json() == expected, 'Expected response did not match actual response for engine_forkchoiceUpdatedV1 (expected VALID)'

def check_fcu_invalid():
    expected = {
        'result': {
            'payloadStatus': {
                'status': 'INVALID'
            }
        }
    }
    send_to_EEs(ujson.dumps(expected))

    response = requests.post('http://localhost:8545/', data=ujson.dumps({'method': 'engine_forkchoiceUpdatedV1'}), headers={'Content-Type': 'application/json', 'tester': '1'})
    
    print(f'Expected response: {expected}, got {response.json()}')

    assert response.json() == expected, 'Expected response did not match actual response for engine_forkchoiceUpdatedV1 (expected INVALID)'

def check_fcu_syncing():
    # this is a special test because we are going to expect 2/3 of EEs to return VALID
    # and the last one to return INVALID
    # and we expect SYNCING from EB

    requests.post('http://localhost:8001/mock/set_response', data=ujson.dumps({'result': {'payloadStatus': {'status': 'VALID'}}}))
    requests.post('http://localhost:8002/mock/set_response', data=ujson.dumps({'result': {'payloadStatus': {'status': 'VALID'}}}))
    requests.post('http://localhost:8003/mock/set_response', data=ujson.dumps({'result': {'payloadStatus': {'status': 'INVALID'}}}))

    response = requests.post('http://localhost:8545/', data=ujson.dumps({'method': 'engine_forkchoiceUpdatedV1'}), headers={'Content-Type': 'application/json', 'tester': '1'})

    expected = {'result': {'payloadStatus': {'status': 'SYNCING'}}}

    print(f"Expected status: {expected['result']['payloadStatus']['status']}, got {response.json()['result']['payloadStatus']['status']}")

    # we just check the status since the expected variable here is a stripped down version of the response
    assert response.json()['result']['payloadStatus']['status'] == expected['result']['payloadStatus']['status'], 'Expected response did not match actual response for engine_forkchoiceUpdatedV1 (expected SYNCING)'

async def main():

    # start three mock EE's
    subprocess.Popen(['python', 'tests/mockee.py', '--port', '8001', '--name', 'mockEE1', '--jwt_secret', 'jwt.txt'])
    subprocess.Popen(['python', 'tests/mockee.py', '--port', '8002', '--name', 'mockEE2', '--jwt_secret', 'jwt.txt'])
    subprocess.Popen(['python', 'tests/mockee.py', '--port', '8003', '--name', 'mockEE3', '--jwt_secret', 'jwt.txt'])
    time.sleep(5)
    
    # start the EB binary
    # we need to fork off the binary and run it
    subprocess.Popen(['build/executionbackup', '--nodes', 'http://localhost:8001,http://localhost:8002,http://localhost:8003', '--port', '8545', '--jwt-secret', 'jwt.txt'])

    time.sleep(2)
    print('starting tests')
    # call the check functions
    try:
        check_syncing()
        print('check_syncing passed')
        check_fcu_valid()
        print('check_fcu with valid response passed')
        check_fcu_invalid()
        print('check_fcu with invalid response passed')
        check_fcu_syncing()
        print('check_fcu with syncing response passed')
    except AssertionError as e:
        print(e)
        exit(1)

asyncio.run(main())