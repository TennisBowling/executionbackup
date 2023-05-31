# executionbackup
A Ethereum 2.0 multiplexer enabling execution node failover and multiplexing

## Installing
EB has provided releases for Linux, Windows, and MacOS.
You can download the latest release [here](https://github.com/tennisbowlin/executionbackup/releases/latest).

You can also build from source using the following commands:
```bash
git clone https://github.com/tennisbowling/executionbackup.git
cd executionbackup
make build
```
And replacing `make build` with `cargo build --profile highperf --target-dir bin` for windows.

## Usage
```bash
executionbackup -n http://node1:port,http://node2:port -j /path/to/jwt_secret
```

## How it works
EB multiplexes multiple EL's together.
Truth Table for responses to CL when EL's are different:
| EL1     | EL2     | EL3     | RESULT  |
|---------|---------|---------|---------|
| VALID   | VALID   | VALID   | VALID   |
| INVALID | INVALID | INVALID | INVALID |
| VALID   | VALID   | SYNCING | VALID   |
| VALID   | VALID   | INVALID | SYNCING |
| INVALID | INVALID | VALID   | INVALID |
| INVALID | INVALID | SYNCING | INVALID |

* Results of SYNCING are checked to verify if payload.block_hash is equal to keccak256(rlp(block_header)) to not get inconsistent block hashes in a supermajority  
* Rows 3, 5, 6 are determined by the fcu-invalid-threshold parameter that determins what percentage of EL's are needed to be considered a majority and be the result

