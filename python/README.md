# executionbackup
[![Total alerts](https://img.shields.io/lgtm/alerts/g/TennisBowling/executionbackup.svg?logo=lgtm&logoWidth=18)](https://lgtm.com/projects/g/TennisBowling/executionbackup/alerts/)
[![Language grade: Python](https://img.shields.io/lgtm/grade/python/g/TennisBowling/executionbackup.svg?logo=lgtm&logoWidth=18)](https://lgtm.com/projects/g/TennisBowling/executionbackup/context:python)

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
python3 lb.py --port 8000 --nodes http://node1:8545 ws://node2:8546 # etc
```
And then point your CL to `http://address.of.lb:portyouchose # in the example, port 8000` (or `ws://address.of.lb:portyouchose`). Websocket requests can be sent to the lb with http nodes.

## Docker

Build the docker container:

```
docker build . --tag=executionbackup:latest
```

Then, run it:
```
docker run -p 8000:8000 executionbackup:latest --port 8000 --nodes http://node1:8545 ws://node2:8546
```

## Support
Contact me on discord at TennisBowling#7174


## TODO
- Add real tests - tests right now are made by spinning an actual EE (multiple) and CL on kiln/whatever current testnet there is. While this is extremely effective - simulating actual network conditions, it is slow.
- connection speedups to the EE (I already know how I'm planning on doing this)