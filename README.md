# executionbackup-cpp
[![Total alerts](https://img.shields.io/lgtm/alerts/g/TennisBowling/executionbackup.svg?logo=lgtm&logoWidth=18)](https://lgtm.com/projects/g/TennisBowling/executionbackup/alerts/)

executionbackup is a multiplexer between consensus and execution nodes.

executionbackup lets one (and only one) consensus node to pilot multiple execution nodes.  

It has been tested and are working on Phase 0, as well as on merged testnets (kiln).

## Running

### Pre-Built Binaries
Pre-Built Binaries are provided under the github realease.

### Install Rust
First, install Rust using [rustup](https://rustup.rs/). The rustup installer provides an easy way to update the Rust compiler, and works on all platforms.
Rust is needed no matter your platform.

### Linux
First, install the requirements:

```bash
sudo apt install -y build-essential cmake
```

Then clone the repo and build:

```bash
git clone --recursive https://github.com/tennisbowling/executionbackup.git
cd executionbackup
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

## Dockerized Version
You can find a dockerized version for easy setup under `dockerized` folder