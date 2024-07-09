# ExecutionBackup
An Ethereum execution layer multiplexer enabling execution node failover and multiplexing.

## Installing
ExecutionBackup (EB), has provided releases for Linux, Windows, and MacOS.
You can download the latest release [here](https://github.com/tennisbowling/executionbackup/releases/latest).

You can also build from source using the following commands:
```bash
git clone https://github.com/tennisbowling/executionbackup.git
cd executionbackup
make build
```
Replace `make build` with `cargo build --profile highperf --target-dir bin` for windows.  

You'll then find the `executionbackup` executable in the `bin/` directory.

### ExecutionBackup Dockerized

You can run a docker-compose enabled version of the tool by making use of the dockerized folder.

## Basic Usage
```bash
executionbackup --nodes http://node1:port,http://node2:port --jwt-secret /path/to/jwt_secret
```

Then set your Consensus Layer client (prysm, lighthouse, nimbus, etc) to connect to EB instead of the EL.

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
### Consensus Layer:
Now set your Consensus Layer client to connect to ExecutionBackup:
```bash
lighthouse bn --execution-endpoint http://localhost:7000
```
Default port for EB is 7000, but you can change it with the `--port` flag.
  
Now you have a multiplexed Execution Layer that can failover to another node if one goes down, and protects you against possible EL errors!


## Configuration
You can also use different jwt secrets for each node:
```
executionbackup --nodes http://node1:port#jwt-secret=/path/to/jwt_secret,http://node2:port#jwt-secret=/path/to/jwt_secret2
```
`http://node1:port` and `http://node2.port` are the authrpc (generally port 8551) endpoints for the Execution Layer nodes.  

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

## How it works
ExecutionBackup will recieve requests from the CL to evaluate if a block is valid. EB then sends that request to every Execution Layer client you connected it to, and recieves the "PayloadStatus". The possible options for the status are: 
- The block is valid,
- The block is invalid,
- The EL is still syncing and can't approve or deny the block.

EB then gets a list of responses from the ELs. It decides how many responses need to be the same to call it a majority by:  
- Getting the number of EL responses it got
- Multiplying it by the percentage set by `--fcu-majority` (default 0.6)
- Rounding it

If the number of responses that are the same (majority) is greater than or equal to the number defined above, we deem that a majority.  
  

Now, we must choose what to actually return to the CL:
If there are no responses from any EL or no majority, then we return SYNCING to the CL.  
If the majority is INVALID, we directly return that to the CL.  
If **ANY** EL response is INVALID (even if the majority is VALID), we return SYNCING to the CL.  
  
If we're here, the majority is either VALID or SYNCING, and there are no INVALIDS from any EL, so we return the majority to the CL.  
  
> Note: Returning SYNCING to the EL is a safe bet, as it puts the CL in optimistic mode, where it does not perform any validator duties, guaranteeing you cannot get slashed. However, you will get inactivity penalties (although they are drastically less than slashing).

You can try *all* this logic out by visiting the [Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=d251f388ec01380cd2bb64b00c1022b9).

  

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

## Testing
```bash
cargo test --all
```

