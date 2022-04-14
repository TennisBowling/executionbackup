# executionbackup

executionbackup is a multiplexer between consensus and execution nodes.

executionbackup lets one (and only one) consensus node to pilot multiple execution nodes.  

It modes have been tested and are working on Phase 0, as well as on merged testnets (kiln).

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
And then point your CL to `http://address.of.lb:portyouchose # in the example, port 8000`


## Support
Contact me on discord at TennisBowling#7174
