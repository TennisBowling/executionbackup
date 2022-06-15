# executionbackup-cpp
[![Total alerts](https://img.shields.io/lgtm/alerts/g/TennisBowling/executionbackup.svg?logo=lgtm&logoWidth=18)](https://lgtm.com/projects/g/TennisBowling/executionbackup/alerts/)

executionbackup is a multiplexer between consensus and execution nodes.

executionbackup lets one (and only one) consensus node to pilot multiple execution nodes.  

It has been tested and are working on Phase 0, as well as on merged testnets (kiln).

## Running

### Linux
First, install the requirements:

```bash
sudo apt install -y build-essential cmake libboost-dev libboost-program-options-dev
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
cmake --config Release --build .
```

Binaries will be in the `bin` folder




## Support
Contact me on discord at TennisBowling#7174


## TODO
- Add real tests - tests right now are made by spinning an actual EE (multiple) and CL on kiln/whatever current testnet there is. While this is extremely effective - simulating actual network conditions, it is slow.
- connection speedups to the EE (I already know how I'm planning on doing this)