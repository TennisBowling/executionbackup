# ExecutionBackup
A Ethereum execution layer multiplexer enabling execution node failover and multiplexing.

## Installing
EB has provided releases for Linux, Windows, and MacOS.
You can download the latest release [here](https://github.com/tennisbowling/executionbackup/releases/latest).

You can also build from source using the following commands:
```bash
git clone https://github.com/tennisbowling/executionbackup.git
cd executionbackup
make build
```
Replace `make build` with `cargo build --profile highperf --target-dir bin` for windows.  

You'll then find the `executionbackup` executable in the `bin/` directory.

## Basic Usage
```bash
executionbackup --nodes http://node1:port,http://node2:port --jwt-secret /path/to/jwt_secret
```

Then set your Consensus Layer client (prysm, lighthouse, nimbus, etc) to connect to EB instead of the EL.

## Configuration
You can also use different jwt secrets for each node:
```
executionbackup --nodes http://node1:port#jwt-secret=/path/to/jwt_secret,http://node2:port#jwt-secret=/path/to/jwt_secret2
```
`http://node1:port` and `http://node2.port` are the authrpc (generally port 8551) endpoints for the Execution Layer nodes.  

## Example
Example: `http://localhost:8551` to connect to a local EL node.


The EL must use the same jwt as ExecutionBackup:  
### Geth node:
```bash
geth --authrpc.jwtsecret /path/to/jwt_secret ...
```

### ExecutionBackup:
```bash
executionbackup --nodes http://localhost:8551 --jwt-secret /path/to/jwt_secret
```


### Per-node Settings:

You can also use per-node jwt secrets:
```
executionbackup --nodes http://localhost:8551#jwt-secret=/path/to/jwt_secret,https://127.0.0.1:8552/#jwt-secret=/path/to/jwt_secret2
```
---

You can also override the global `--timeout` and set per-node timeouts:
```
#timeout=timeout_in_ms
executionbackup --nodes http://localhost:8551#timeout=3000,https://127.0.0.1:8552/#jwt-secret=/path/to/jwt_secret2#timeout=5200
```
In this example, `localhost:8551` has a timeout of 3s, while `127.0.0.1:8552` has a timeout of 5.2s and a custom jwt secret path of `/path/to/jwt_secret2`

---

Additionally, a node can be marked as "not for use" in the primary forkchoice and newpayload responses  
by simply appending `#do-not-use` to the node url, similarly to per-node jwt secrets and timeouts.

---
### Consensus Layer:
Now set your Consensus Layer client to connect to ExecutionBackup:
```bash
lighthouse bn --execution-endpoint http://localhost:7000
```
Default port for EB is 7000, but you can change it with the `--port` flag.

  
Now you have a multiplexed Execution Layer that can failover to another node if one goes down, and protects you against possible EL errors!


# API
ExecutionBackup has a REST API that can be used to get information about the EL nodes and the current state of the multiplexer.

---

### GET /metrics

#### Description
Get the current state of the EL nodes and the multiplexer.

#  

#### Request
| Parameter | Description |
|-----------|-------------|
| None      |             |


#### Response
```rust
struct MetricsReport {
    response_times: HashMap<String, u128>,  // EL node -> response time in microseconds
    alive_nodes: Array<String>,             // EL nodes that are alive
    syncing_nodes: Array<String>,           // EL nodes that are syncing
    dead_nodes: Array<String>,              // EL nodes that are dead (not responding)
    primary_node: String,                   // EL node selected for additional non-engine requests
}
```
--- 

### GET /recheck

#### Description
Rechecks the EL nodes and updates the state of the multiplexer. Returns an almost identical response to /metrics, including the time it took to recheck the EL nodes and update the state.

#  

#### Request
| Parameter | Description |
|-----------|-------------|
| None      |             |

#### Response
```rust
struct RecheckMetricsReport {
    response_times: HashMap<String, u128>,  // EL node -> response time in microseconds
    alive_nodes: Array<String>,             // EL nodes that are alive
    syncing_nodes: Array<String>,           // EL nodes that are syncing
    dead_nodes: Array<String>,              // EL nodes that are dead (not responding)
    primary_node: String,                   // EL node selected for additional non-engine requests 
    recheck_time: u128,                     // Time in microseconds it took to recheck the EL nodes
}
```

#  

### POST /add_nodes

#### Description
Add EL nodes to the multiplexer.

---

#### Request JSON Body
```rust
struct NodeList {
    nodes: Array<String>,                // EL node URLs.
}
```
Supports specific jwt secrets by appending #jwt-secret=/path/to/jwt_secret to the URL.  
If no jwt secret is provided, the default jwt secret will be used.


#### Response
```rust
struct RecheckMetricsReport {
    response_times: HashMap<String, u128>,  // EL node -> response time in microseconds
    alive_nodes: Array<String>,             // EL nodes that are alive
    syncing_nodes: Array<String>,           // EL nodes that are syncing
    dead_nodes: Array<String>,              // EL nodes that are dead (not responding)
    primary_node: String,                   // EL node selected for additional non-engine requests 
    recheck_time: u128,                     // Time in microseconds it took to recheck the EL nodes
}
```

# How it works
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

* Results of SYNCING are checked to verify if payload.block_hash is equal to keccak256(rlp(block_header)) to not get inconsistent block hashes in a supermajority.  
* Rows 3, 5, 6 are determined by the fcu-invalid-threshold parameter that determins what percentage of EL's are needed to be considered a majority and be the result.  
  
[Here](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=17b1a6038975267f9b1f61529cc4ca4c) is a rust playground where you can test the multiplexer logic.

# Execution API backup Dockerized

You can run a docker-compose enabled version of the tool by making use of the dockerized folder

# Testing
```bash
cargo test --all
```

