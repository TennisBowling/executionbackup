# executionbackup

executionbackup is a load-balancing proxy to load balance requests to multiple execution nodes, as well as a multiplexer between consensus and execution nodes.

It can be used in two modes: internal-lb (multiplexer) and commercial-lb.

Internal-lb lets one (and only one) consensus node to pilot multiple execution nodes.  
Commercial-lb lets multiple commerical clients to connect to execution nodes and get requests from them. Commercial-lb will get a majority response from execution nodes and return it to the client, assuring that the client does not get different responses for the same requests. (similar to Alchemy's Supernode) Commercial-lb also provides statistics about client's requests which are sent to a postgres database.

## Running

First, clone the repository, and install dependencies:

```bash
git clone https://github.com/tennisbowling/executionbackup.git
cd executionbackup
python3 -m pip install -r requirements.txt
```

Then, run it:

Internal-lb:

```bash
python3 lb.py --port 8000 --nodes http://node1:8545 http://node2:8546 # etc
```
And then point your CL to http://address.of.lb:portyouchose # in the example, port 8000

Commercial: Contact me on discord, I am TennisBowling#7174
since it's more complicated to setup.

## TODO
- get the most profitable block (query geth `eth_balance` with blockid of `pending`?) to give to CL
- test it
