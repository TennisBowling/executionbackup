# executionbackup-cpp
[![Total alerts](https://img.shields.io/lgtm/alerts/g/TennisBowling/executionbackup.svg?logo=lgtm&logoWidth=18)](https://lgtm.com/projects/g/TennisBowling/executionbackup/alerts/)
[![Build Status](https://app.travis-ci.com/TennisBowling/executionbackup.svg?branch=master)](https://app.travis-ci.com/TennisBowling/executionbackup)

executionbackup (EB) is a multiplexer between consensus and execution nodes.

executionbackup lets one (and only one) consensus node to pilot multiple execution nodes.  

It has been tested and are working on Phase 0, as well as on merged testnets (kiln).


## How it works
Execution nodes need to be piloted by a consensus node (CL).  
In order to achieve redundancy, the consensus node can be multiplexed with multiple execution nodes/engine (EE).  
Consensus node requests are "replicated" to all EEs.

However when a CL sends a `engine_forkchoiceUpdated` request (the request to control which fork the execution node will follow) to EB, EB will check for responses of multiple nodes.  

Situation: If you have three EEs, the CL sends out a `engine_forkchoiceUpdated` and the first node thinks the fork is correct, the second node thinks it's correct, but the third one thinks it's wrong, EB will make the CL optimisically import the block, which makes the CL not attest to a bad fork (which could get you slashed).


### Pros
- EB lets you add multiple nodes for redundancy, letting you have downtime for your EE nodes (for updates, etc).
- EB prevents your CL from following a supermajority fork (which could get you slashed), as long as you have at least two EE nodes (that have different implementations). It instead makes the CL optimisically import the block, which makes your validators not attest, getting the penalties for being offline rather than being slashed.
- EB lets you safely run the majority EE node (ahem geth) without the risk of following a supermajority fork.

### Cons
- EB introduces extra complexity, requiring you to have multiple EE nodes.


## Running

### Linux
First, install the requirements:

```bash
sudo apt install -y build-essential cmake
```

Then clone the repo and build:

```bash
git clone --recursive https://github.com/tennisbowling/executionbackup.git
cd executionbackup/executionbackup-cpp
make
```

Binaries will be in the `bin` folder

### Windows
Requirements:
- [Visual Studio (with the c++ compiler)](https://visualstudio.microsoft.com/downloads/)
- [Cmake](https://cmake.org/download/)


Clone:
```bash
git clone --recursive https://github.com/tennisbowling/executionbackup.git
cd executionbackup/executionbackup-cpp
```

Then build:
```bash
cmake -S . -DCMAKE_BUILD_TYPE=Release
cmake --build . --config Release
```

Binaries will be in the `bin` folder




## Support
Contact me on discord at TennisBowling#7174


## TODO
- Add real tests - tests right now are made by spinning an actual EE (multiple) and CL on kiln/whatever current testnet there is. While this is extremely effective - simulating actual network conditions, it is slow.
- connection speedups to the EE (I already know how I'm planning on doing this)