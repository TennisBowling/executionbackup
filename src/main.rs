use axum::{
    self,
    extract::DefaultBodyLimit,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Router,
};
use ethereum_types::U256;
use futures::future::join_all;
use jsonwebtoken::{self, EncodingKey};
use reqwest::{self, header};
use serde_json::json;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::{sync::RwLock, time::Duration};
mod verify_hash;
use lazy_static::lazy_static;
use types::*;
use verify_hash::verify_payload_block_hash;

const VERSION: &str = "1.1.2";
const DEFAULT_ALGORITHM: jsonwebtoken::Algorithm = jsonwebtoken::Algorithm::HS256;

lazy_static! {
    static ref TIMEOUT: Duration = {
        let dur = Duration::from_millis(7500);
        dur
    };
}

fn make_jwt(jwt_key: &jsonwebtoken::EncodingKey) -> Result<String, jsonwebtoken::errors::Error> {
    let claim_inst = Claims {
        iat: chrono::Utc::now().timestamp(),
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(DEFAULT_ALGORITHM),
        &claim_inst,
        jwt_key,
    )
}

fn make_response(id: &u64, result: serde_json::Value) -> String {
    json!({"jsonrpc":"2.0","id":id,"result":result}).to_string()
}

fn make_error(id: &u64, error: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32700, "message": error}}).to_string()
}

fn parse_result(resp: &str) -> Result<serde_json::Value, ParseError> {
    let j = match serde_json::from_str::<serde_json::Value>(resp) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Error deserializing response: {}", e);
            return Err(ParseError::InvalidJson);
        }
    };

    match j.get("error") {
        Some(error) => {
            tracing::error!("Response has error field: {}", error);
            return Err(ParseError::ElError);
        }
        None => {}
    }

    let result = match j.get("result") {
        Some(result) => result,
        None => {
            tracing::error!("Response has no result field");
            return Err(ParseError::MethodNotFound);
        }
    };

    Ok(result.clone())
}

fn parse_fcu(result: serde_json::Value) -> Result<forkchoiceUpdatedResponse, ParseError> {
    let j = match serde_json::from_value::<forkchoiceUpdatedResponse>(result) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Error deserializing response: {}", e);
            return Err(ParseError::InvalidJson);
        }
    };

    Ok(j)
}

fn parse_getpayload(result: &serde_json::Value) -> Result<getPayloadV2Response, ParseError> {
    let j = match getPayloadV2Response::from_json(&result) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Error deserializing response: {}", e);
            return Err(ParseError::InvalidJson);
        }
    };

    Ok(j)
}

fn make_syncing_str(id: &u64, payload: &serde_json::Value, method: &EngineMethod) -> String {
    match method {
        EngineMethod::engine_newPayloadV1 => {
            tracing::debug!(
                "Verifying execution payload blockhash {}.",
                payload["blockHash"]
            );
            let execution_payload = match ExecutionPayload::from_json(&payload) {
                Ok(execution_payload) => execution_payload,
                Err(e) => {
                    tracing::error!("Error deserializing execution payload: {}", e);
                    return e.to_string();
                }
            };

            if let Err(e) = verify_payload_block_hash(&execution_payload) {
                tracing::error!("Error verifying execution payload blockhash: {}", e);
                return e.to_string();
            }

            tracing::debug!(
                "Execution payload blockhash {} verified. Returning SYNCING",
                payload["blockHash"]
            );
            return json!({"jsonrpc":"2.0","id":id,"result":{"payloadStatus":{"status":"SYNCING","latestValidHash":null,"validationError":null}},"payloadId":null}).to_string()
        },

        _ => {
            return json!({"jsonrpc":"2.0","id":id,"result":{"payloadStatus":{"status":"SYNCING","latestValidHash":null,"validationError":null}},"payloadId":null}).to_string()
        }
    };
}

#[derive(Clone)]

pub struct Node
// represents an EE
{
    client: reqwest::Client,
    url: String,
    status: Arc<RwLock<NodeHealth>>,
}

impl Node {
    pub fn new(url: String) -> Node {
        let client = reqwest::Client::new();
        Node {
            client,
            url,
            status: Arc::new(RwLock::new(NodeHealth {
                status: SyncingStatus::NodeNotInitialized,
                resp_time: 0,
            })),
        }
    }

    async fn set_synced(&self) {
        let status = self.status.read().await;
        if status.status != SyncingStatus::Synced {
            tracing::info!("Node {} is synced", self.url);
        }
        drop(status);
        let mut status = self.status.write().await;
        status.status = SyncingStatus::Synced;
    }

    async fn set_offline(&self) {
        let status = self.status.read().await;
        if status.status != SyncingStatus::Offline {
            tracing::warn!("Node {} is offline", self.url);
        }
        drop(status);
        let mut status = self.status.write().await;
        status.status = SyncingStatus::Offline;
    }

    async fn set_online_and_syncing(&self) {
        let status = self.status.read().await;
        if status.status != SyncingStatus::OnlineAndSyncing {
            tracing::info!("Node {} is online and syncing", self.url);
        }
        drop(status);
        let mut status = self.status.write().await;
        status.status = SyncingStatus::OnlineAndSyncing;
    }

    async fn check_status(
        &self,
        jwt_key: Arc<jsonwebtoken::EncodingKey>,
    ) -> Result<NodeHealth, reqwest::Error> {
        // we need to use jwt here since we're talking directly to the EE's auth port
        let token = make_jwt(&jwt_key).unwrap();
        let start = std::time::Instant::now();
        let resp = self
            .client
            .post(self.url.clone())
            .header("Authorization", format!("Bearer {}", token))
            .json(&json!({"jsonrpc": "2.0", "method": "eth_syncing", "params": [], "id": 1}))
            .timeout(*TIMEOUT)
            .send()
            .await;
        let resp: reqwest::Response = match resp {
            Ok(resp) => resp,
            Err(e) => {
                self.set_offline().await;
                tracing::error!("Error while checking status of node {}: {}", self.url, e);
                return Err(e);
            }
        };

        let resp_time = start.elapsed().as_millis();

        // deserialize the json response.
        // result = false means node is online and not syncing
        // result = an object means node is syncing
        let json_body: serde_json::Value = resp.json().await?;
        let result = &json_body["result"];

        if result.is_boolean() {
            if !result.as_bool().unwrap() {
                self.set_synced().await;
            } else {
                self.set_online_and_syncing().await;
            }
        } else {
            self.set_online_and_syncing().await;
        }

        // update the status
        let mut status = self.status.write().await;
        status.resp_time = resp_time;
        Ok(status.clone())
    }

    async fn do_request(
        &self,
        data: &RpcRequest,
        jwt_token: String,
    ) -> Result<(String, u16), reqwest::Error> {
        let resp = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Authorization", jwt_token)
            .body(data.as_bytes())
            .timeout(*TIMEOUT)
            .send()
            .await;

        let resp = match resp {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!("Error while sending request to node {}: {}", self.url, e);
                return Err(e);
            }
        };

        let status = resp.status().as_u16();
        let resp_body = resp.text().await?;
        Ok((resp_body, status))
    }

    async fn do_request_no_timeout(
        &self,
        data: &RpcRequest,
        jwt_token: String,
    ) -> Result<(String, u16), reqwest::Error> {
        let resp = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Authorization", jwt_token)
            .body(data.as_bytes())
            .send()
            .await;

        let resp = match resp {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!("Error while sending request to node {}: {}", self.url, e);
                return Err(e);
            }
        };

        let status = resp.status().as_u16();
        let resp_body = resp.text().await?;
        Ok((resp_body, status))
    }

    async fn do_request_no_timeout_str(
        &self,
        data: String,
        jwt_token: String,
    ) -> Result<(String, u16), reqwest::Error> {
        let resp = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Authorization", jwt_token)
            .body(data)
            .send()
            .await;

        let resp = match resp {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!("Error while sending request to node {}: {}", self.url, e);
                return Err(e);
            }
        };

        let status = resp.status().as_u16();
        let resp_body = resp.text().await?;
        Ok((resp_body, status))
    }
}

struct NodeRouter {
    nodes: Vec<Arc<Node>>,
    alive_nodes: Arc<RwLock<Vec<Arc<Node>>>>,
    dead_nodes: Arc<RwLock<Vec<Arc<Node>>>>,
    alive_but_syncing_nodes: Arc<RwLock<Vec<Arc<Node>>>>,

    // this node will be the selected primary node used to route all requests
    primary_node: Arc<RwLock<Arc<Node>>>,

    // jwt encoded key used to make tokens for the EE's auth port
    jwt_key: Arc<jsonwebtoken::EncodingKey>,

    // percentage of nodes that need to agree for it to be deemed a majority
    majority_percentage: f32, // 0.1..0.9

    // setting to set if node timings are displayed
    node_timings_enabled: bool,
}

impl NodeRouter {
    fn new(
        jwt_key: &jsonwebtoken::EncodingKey,
        majority_percentage: f32,
        nodes: Vec<Arc<Node>>,
        primary_node: Arc<Node>,
        node_timings_enabled: bool,
    ) -> Self {
        NodeRouter {
            nodes: nodes.clone(),
            alive_nodes: Arc::new(RwLock::new(Vec::new())),
            dead_nodes: Arc::new(RwLock::new(Vec::new())),
            alive_but_syncing_nodes: Arc::new(RwLock::new(Vec::new())),
            primary_node: Arc::new(RwLock::new(primary_node)),
            jwt_key: Arc::new(jwt_key.clone()),
            majority_percentage,
            node_timings_enabled,
        }
    }

    async fn make_node_syncing(&self, node: Arc<Node>) {
        let mut alive_nodes = self.alive_nodes.write().await;
        let index = alive_nodes.iter().position(|x| *x.url == node.url);
        let index = match index {
            Some(index) => index,
            None => {
                // node is not in alive_nodes, so it's in syncing or worse, stop here
                return;
            }
        };


        let mut alive_but_syncing_nodes = self.alive_but_syncing_nodes.write().await;
        alive_nodes.remove(index);
        alive_but_syncing_nodes.push(node);
    }

    async fn recheck(&self) {
        // check the status of all nodes
        // order nodes in alive_nodes vector by response time
        // dont clone nodes, just clone the Arcs

        let mut new_alive_nodes = Vec::<(u128, Arc<Node>)>::with_capacity(self.nodes.len()); // resp time, node
        let mut new_dead_nodes = Vec::<Arc<Node>>::with_capacity(self.nodes.len());
        let mut new_alive_but_syncing_nodes = Vec::<Arc<Node>>::with_capacity(self.nodes.len());

        let mut checks = Vec::new();

        for node in self.nodes.iter() {
            let node_clone = node.clone();
            let jwt_key_clone = self.jwt_key.clone();
            let check = async move {
                match node_clone.check_status(jwt_key_clone).await {
                    Ok(status) => (status, node_clone.clone()),
                    Err(_) => (
                        NodeHealth {
                            status: SyncingStatus::Offline,
                            resp_time: 0,
                        },
                        node_clone.clone(),
                    ),
                }
            };
            checks.push(check);
        }

        let results = join_all(checks).await;

        for (status, node) in results {
            if status.status == SyncingStatus::Synced {
                new_alive_nodes.push((status.resp_time, node.clone()));

                if self.node_timings_enabled {
                    tracing::info!("{}: {}ms", node.url, status.resp_time);
                }
            } else if status.status == SyncingStatus::OnlineAndSyncing {
                new_alive_but_syncing_nodes.push(node.clone());

                if self.node_timings_enabled {
                    tracing::info!("{}: {}ms", node.url, status.resp_time);
                }
            } else {
                new_dead_nodes.push(node.clone());
            }
        }

        // sort alive_nodes by response time
        new_alive_nodes.sort_by(|a, b| a.0.cmp(&b.0));

        // update primary node to be the first alive node
        let mut primary_node = self.primary_node.write().await;
        *primary_node = match new_alive_nodes.first() {
            Some(node) => node.1.clone(),
            None => {
                // if there are no alive nodes, then set the primary node to a syncing node
                match new_alive_but_syncing_nodes.first() {
                    Some(node) => node.clone(),
                    None => {
                        // if there are no syncing nodes, then set the primary node to a dead node
                        match new_dead_nodes.first() {
                            Some(node) => node.clone(),
                            None => {
                                // if there are no dead nodes, then set the primary node to the first node
                                self.nodes[0].clone()
                            }
                        }
                    }
                }
            }
        };
        drop(primary_node);

        // lock alive_nodes, dead_nodes, and alive_but_syncing_nodes
        let mut alive_nodes = self.alive_nodes.write().await;
        let mut dead_nodes = self.dead_nodes.write().await;
        let mut alive_but_syncing_nodes = self.alive_but_syncing_nodes.write().await;

        // clear vectors and for alive nodes put the Arc<Node> in the vector
        alive_nodes.clear();
        dead_nodes.clear();
        alive_but_syncing_nodes.clear();

        for (_, node) in new_alive_nodes.iter() {
            alive_nodes.push(node.clone());
        }

        for node in new_dead_nodes.iter() {
            dead_nodes.push(node.clone());
        }

        for node in new_alive_but_syncing_nodes.iter() {
            alive_but_syncing_nodes.push(node.clone());
        }
    }

    // try and return the primary node asap
    // if the primary node is offline, then we'll get the next node in the vector, and set the primary node to that node (if its online)
    // basically, return the node closest to the start of the vector that is online, and set that as the primary node
    // if there are no online nodes, try to use a syncing node
    // if there are no syncing nodes, return None
    async fn get_execution_node(&self) -> Option<Arc<Node>> {
        let primary_node = self.primary_node.read().await;

        if primary_node.status.read().await.status == SyncingStatus::Synced {
            return Some(primary_node.clone());
        }

        let alive_nodes = self.alive_nodes.read().await;

        if alive_nodes.is_empty() {
            let alive_but_syncing_nodes = self.alive_but_syncing_nodes.read().await;
            if alive_but_syncing_nodes.is_empty() {
                // no synced or syncing nodes, so return None
                return None;
            } else {
                // no synced nodes, but there are syncing nodes, so return the first syncing node

                let node = alive_but_syncing_nodes[0].clone();
                let mut primary_node = self.primary_node.write().await;
                *primary_node = node.clone();
                return Some(node);
            }
        } else {
            // there are synced nodes, so return the synced node (making sure its not the already checked primary node)
            for node in alive_nodes.iter() {
                if node.url != primary_node.url {
                    let node = node.clone();
                    let mut primary_node = self.primary_node.write().await;
                    *primary_node = node.clone();
                    return Some(node);
                }
            }
            // no synced nodes that are not the primary node, so return a syncing node
            let alive_but_syncing_nodes = self.alive_but_syncing_nodes.read().await;
            if alive_but_syncing_nodes.is_empty() {
                // no synced or syncing nodes, so return None
                return None;
            } else {
                // no synced nodes, but there are syncing nodes, so return the first syncing node

                let node = alive_but_syncing_nodes[0].clone();
                let mut primary_node = self.primary_node.write().await;
                *primary_node = node.clone();
                return Some(node);
            }
        }
    }

    // gets the majority response from a vector of respon   ses
    // must have at least majority_percentage of the nodes agree
    // if there is no majority, then return None
    // if there is a draw, just return the first response
    // u64 on the response should be the "id" field from the any of the responses
    fn fcu_majority(&self, results: &Vec<PayloadStatusV1>) -> Option<PayloadStatusV1> {
        let total_responses = results.len();
        let majority_count = (total_responses as f32 * self.majority_percentage) as usize;

        // Create a hashmap to store response frequencies
        let mut response_counts: std::collections::HashMap<&PayloadStatusV1, usize> =
            std::collections::HashMap::new();

        for response in results.iter() {
            *response_counts.entry(response).or_insert(0) += 1;
        }

        // Find the response with the most occurrences
        let mut majority_response = None;
        let mut max_count = 0;

        for (response, &count) in response_counts.iter() {
            if count > max_count {
                majority_response = Some(response);
                max_count = count;
            }
        }

        // Check if the majority count is greater than or equal to the required count
        if max_count >= majority_count {
            return majority_response.cloned().cloned();
        } else {
            None
        }
    }

    async fn fcu_logic(
        &self,
        resps: &Vec<PayloadStatusV1>,
        req: &RpcRequest,
        jwt_token: String,
    ) -> Result<PayloadStatusV1, FcuLogicError> {
        if resps.is_empty() {
            // no responses, so return SYNCING
            tracing::error!("No responses, returning SYNCING.");
            return Err(FcuLogicError::NoResponses);
        }

        let majority = match self.fcu_majority(resps) {
            Some(majority) => majority,
            None => {
                // no majority, so return SYNCING
                tracing::error!("No majority, returning SYNCING.");
                return Err(FcuLogicError::NoMajority);
            }
        };

        match majority.status {
            PayloadStatusV1Status::Invalid | PayloadStatusV1Status::InvalidBlockHash => {
                // majority is INVALID, so return INVALID (to not go through the next parts of the algorithm)
                return Ok(majority); // return Ok since this is not an error
            }
            _ => {} // there still can be invalid in the responses
        }

        for resp in resps {
            // check if any of the responses are INVALID

            match resp.status {
                PayloadStatusV1Status::Invalid | PayloadStatusV1Status::InvalidBlockHash => {
                    // a response is INVALID. One node could be right, no risks, return syncing to stall CL
                    return Err(FcuLogicError::OneNodeIsInvalid);
                }
                _ => {}
            }
        }

        // send to the syncing nodes to help them catch up with tokio::spawn so we don't have to wait for them
        let syncing_nodes = self.alive_but_syncing_nodes.clone();
        let jwt_token_clone = jwt_token.to_string();
        let req_clone = req.clone();
        tokio::spawn(async move {
            let syncing_nodes = syncing_nodes.read().await;
            tracing::debug!("sending fcU to {} syncing nodes", syncing_nodes.len());
            for node in syncing_nodes.iter() {
                if let Err(e) = node
                    .do_request_no_timeout(&req_clone, jwt_token_clone.clone())
                    .await
                {
                    // a lot of these syncing nodes are slow so we dont add a timeout
                    tracing::error!("error sending fcU to syncing node: {}", e);
                }
            }
        });

        // majority is checked and either VALID or SYNCING
        Ok(majority)
    }

    async fn do_engine_route(&self, request: &RpcRequest, jwt_token: String) -> (String, u16) {
        match request.method {
            // getPayloadV1 is for getting a block to be proposed, so no use in getting from multiple nodes
            EngineMethod::engine_getPayloadV1 => {
                let node = match self.get_execution_node().await {
                    None => {
                        return (make_error(&request.id, "No nodes available"), 500);
                    }
                    Some(node) => node,
                };

                let resp = node.do_request_no_timeout(request, jwt_token).await; // no timeout since the CL will just time us out themselves
                tracing::debug!("engine_getPayloadV1 sent to node: {}", node.url);
                match resp {
                    Ok(resp) => (resp.0, resp.1),
                    Err(e) => {
                        tracing::warn!("engine_getPayloadV1 error: {}", e);

                        if e.is_connect() || e.is_timeout() || e.is_request() {
                            // if the error is a connection error, then we should set the node to syncing
                            self.make_node_syncing(node.clone()).await;
                        }

                        (make_error(&request.id, &e.to_string()), 200)
                    }
                }
            }
            EngineMethod::engine_getPayloadV2 => {
                // getPayloadV2 has a different schema, where alongside the executionPayload it has a blockValue
                // so we should send this to all the nodes and then return the one with the highest blockValue
                let mut resps: Vec<getPayloadV2Response> = Vec::new();
                let alive_nodes = self.alive_nodes.read().await;
                for node in alive_nodes.iter() {
                    let resp = node
                        .do_request(&request, jwt_token.clone())
                        .await;
                    match resp {
                        Ok(resp) => {

                            match parse_result(&resp.0) {
                                Ok(generic_value) => {

                                    match parse_getpayload(&generic_value) {
                                        Ok(fcu_res) => {
                                            resps.push(fcu_res);
                                        },
                                        Err(e) => {
                                            tracing::error!(
                                                "Error parsing response for {:?}: {:?}",
                                                request.method,
                                                e
                                            );
                                            continue;
                                        },
                                    }

                                },

                                Err(e) => {
                                    tracing::error!(
                                        "Error parsing response for {:?}: {:?}",
                                        request.method,
                                        e
                                    );
                                    continue;
                                },
                            }

                        },
                        Err(e) => {
                            tracing::error!("{:?} error: {}", request.method, e);
                            if e.is_connect() || e.is_timeout() || e.is_request() {
                                // if the error is a connection error, then we should set the node to syncing
                                self.make_node_syncing(node.clone()).await;
                            }
                        }
                    }
                }
                
                drop(alive_nodes);
                let most_profitable = resps.iter().max_by(|resp_a, resp_b| resp_a.blockValue.cmp(&resp_b.blockValue));

                if let Some(most_profitable_payload) = most_profitable {
                    tracing::info!("Blocks profitability: {:?}. Using payload with value of {}", resps.iter().map(|payload| payload.blockValue).collect::<Vec<U256>>(), most_profitable_payload.blockValue);
                    return (make_response(&request.id, json!(most_profitable_payload)), 200);
                }
                
                // we have no payloads
                tracing::warn!("No blocks found in engine_getPayloadV2 responses");
                (make_error(&request.id, "No blocks found in engine_getPayloadV2 responses"), 200)
            },

            EngineMethod::engine_newPayloadV1 | EngineMethod::engine_newPayloadV2 => {
                tracing::debug!("Sending newPayload to alive nodes");
                let mut resps: Vec<String> = Vec::new();
                let alive_nodes = self.alive_nodes.read().await;
                for node in alive_nodes.iter() {
                    let resp = node.do_request(request, jwt_token.clone()).await;
                    match resp {
                        Ok(resp) => {
                            resps.push(resp.0);
                        }
                        Err(e) => {
                            tracing::error!("{:?} error: {}", request.method, e);
                            if e.is_connect() || e.is_timeout() || e.is_request() {
                                // if the error is a connection error, then we should set the node to syncing
                                self.make_node_syncing(node.clone()).await;
                            }
                        }
                    }
                }
                drop(alive_nodes);

                let mut resps_new = Vec::<PayloadStatusV1>::with_capacity(resps.len()); // faster to allocate in one go
                for item in &resps {
                    let result = parse_result(item);
                    match result {
                        Err(e) => {
                            tracing::error!(
                                "Error parsing response for {:?}: {:?}",
                                request.method,
                                e
                            );
                            continue;
                        }
                        Ok(result) => {
                            let res: PayloadStatusV1 = match serde_json::from_value(result) {
                                // we need to get first item in the result array
                                Err(e) => {
                                    tracing::error!(
                                        "Error deserializing response for {:?}: {:?}",
                                        request.method,
                                        e
                                    );
                                    continue;
                                }
                                Ok(result) => result,
                            };
                            resps_new.push(res);
                        }
                    };
                }

                let resp = match self.fcu_logic(&resps_new, request, jwt_token).await {
                    Ok(resp) => resp,
                    Err(e) => match e {
                        FcuLogicError::NoResponses => {
                            tracing::error!(
                                "No responses for {:?}, returning SYNCING",
                                request.method
                            );
                            return (
                                make_syncing_str(&request.id, &request.params[0], &request.method),
                                200,
                            );
                        }
                        FcuLogicError::NoMajority => {
                            tracing::error!(
                                "No majority for {:?}, returning SYNCING",
                                request.method
                            );
                            return (
                                make_syncing_str(&request.id, &request.params[0], &request.method),
                                200,
                            );
                        }
                        FcuLogicError::OneNodeIsInvalid => {
                            tracing::error!(
                                "One node is invalid for {:?}, returning SYNCING",
                                request.method
                            );
                            return (
                                make_syncing_str(&request.id, &request.params[0], &request.method),
                                200,
                            );
                        }
                    },
                };

                (make_response(&request.id, json!(resp)), 200)
            }

            EngineMethod::engine_forkchoiceUpdatedV1 | EngineMethod::engine_forkchoiceUpdatedV2 => {
                tracing::debug!("Sending fcU to alive nodes");
                let mut resps: Vec<forkchoiceUpdatedResponse> = Vec::new();
                let alive_nodes = self.alive_nodes.read().await;
                for node in alive_nodes.iter() {
                    let resp = node.do_request(request, jwt_token.clone()).await;
                    match resp {
                        Ok(resp) => {

                            match parse_result(&resp.0) {
                                Ok(generic_value) => {

                                    match parse_fcu(generic_value) {
                                        Ok(fcu_res) => {
                                            resps.push(fcu_res);
                                        },
                                        Err(e) => {
                                            tracing::error!(
                                                "Error parsing response for {:?}: {:?}",
                                                request.method,
                                                e
                                            );
                                            continue;
                                        },
                                    }

                                },
                                Err(e) => {
                                    tracing::error!(
                                        "Error parsing response for {:?}: {:?}",
                                        request.method,
                                        e
                                    );
                                    continue;
                                },
                            }

                        }
                        Err(e) => {
                            tracing::error!("{:?} error: {}", request.method, e);
                            if e.is_connect() || e.is_timeout() || e.is_request() {
                                // if the error is a connection error, then we should set the node to syncing
                                self.make_node_syncing(node.clone()).await;
                            }
                        }
                    }
                }
                drop(alive_nodes);

                let mut resps_new = Vec::<PayloadStatusV1>::with_capacity(resps.len()); // faster to allocate in one go
                let mut payload_id: Option<String> = None;

                for resp in resps {
                    if let Some(inner_payload_id) = resp.payloadId {    // todo: make this look cleaner. 
                        payload_id = Some(inner_payload_id);                     // if payloadId is not null, then use that
                    };
                    resps_new.push(resp.payloadStatus);
                }
                
                let resp = match self.fcu_logic(&resps_new, request, jwt_token).await {
                    Ok(resp) => resp,
                    Err(e) => match e {
                        FcuLogicError::NoResponses => {
                            tracing::error!(
                                "No responses for {:?}, returning SYNCING",
                                request.method
                            );
                            return (
                                make_syncing_str(&request.id, &request.params[0], &request.method),
                                200,
                            );
                        }
                        FcuLogicError::NoMajority => {
                            tracing::error!(
                                "No majority for {:?}, returning SYNCING",
                                request.method
                            );
                            return (
                                make_syncing_str(&request.id, &request.params[0], &request.method),
                                200,
                            );
                        }
                        FcuLogicError::OneNodeIsInvalid => {
                            tracing::error!(
                                "One node is invalid for {:?}, returning SYNCING",
                                request.method
                            );
                            return (
                                make_syncing_str(&request.id, &request.params[0], &request.method),
                                200,
                            );
                        }
                    },
                };


                (
                    make_response(
                        &request.id,
                        json!(forkchoiceUpdatedResponse {
                            payloadStatus: resp,
                            payloadId: payload_id,
                        }),
                    ),
                    200,
                )
            }

            _ => {
                // wait for primary node's response, but also send to all other nodes
                let primary_node = match self.get_execution_node().await {
                    Some(primary_node) => primary_node,
                    None => {
                        tracing::warn!("No primary node available");
                        return (make_error(&request.id, "No nodes available"), 500);
                    }
                };

                let resp = primary_node
                    .do_request_no_timeout(request, jwt_token.clone())
                    .await;
                tracing::debug!("Sent to primary node: {}", primary_node.url);

                let alive_nodes = self.alive_nodes.clone();
                let jwt_token = jwt_token.to_owned();
                let request_clone = request.clone();
                tokio::spawn(async move {
                    let alive_nodes = alive_nodes.read().await;
                    for node in alive_nodes.iter() {
                        if node.url != primary_node.url {
                            match node
                                .do_request_no_timeout(&request_clone, jwt_token.clone())
                                .await
                            {
                                Ok(_) => {}
                                Err(e) => {
                                    tracing::error!("error replicating generic engine request to syncing nodes: {}", e);
                                }
                            };
                        }
                    }
                });
                match resp {
                    Ok(resp) => (resp.0, resp.1),
                    Err(e) => {
                        tracing::warn!("Error from primary node: {}", e);
                        return (make_error(&request.id, &e.to_string()), 200);
                    }
                }
            }
        }
    }

    async fn do_route_normal(&self, request: String, jwt_token: String) -> (String, u16) {
        // simply send request to primary node
        let primary_node = match self.get_execution_node().await {
            Some(primary_node) => primary_node,
            None => {
                tracing::warn!("No primary node available for normal request");
                let id = match serde_json::from_str::<RpcRequest>(&request) {
                    Ok(request) => request.id,
                    Err(e) => {
                        tracing::error!("Error deserializing request: {}", e);
                        return (make_error(&0, &e.to_string()), 200);
                    }
                };
                return (make_error(&id, "No nodes available"), 500);
            }
        };

        let resp = primary_node
            .do_request_no_timeout_str(request, jwt_token)
            .await;
        match resp {
            Ok(resp) => (resp.0, resp.1),
            Err(e) => (make_error(&0, &e.to_string()), 200),
        }
    }
}

// func to take body and headers from a request and return a string
async fn route_all(
    body: String,
    headers: HeaderMap,
    Extension(router): Extension<Arc<NodeRouter>>,
) -> impl IntoResponse {
    let j: serde_json::Value = match serde_json::from_str(&body) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Couldn't deserialize request. Error: {}. Body: {}", e, body);
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "application/json")],
                make_error(&0, "Couldn't deserialize request body").to_string(),
            );
        }
    };

    let meth = match j["method"].as_str() {
        Some(meth) => meth,
        None => {
            tracing::error!("Request has no method field");
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "application/json")],
                make_error(&0, "Request has no method field").to_string(),
            );
        }
    };

    tracing::debug!("Request received, method: {}", j["method"]);

    if meth.starts_with("engine_") {
        tracing::trace!("Routing {} to engine route", j["method"]);

        let request: RpcRequest = match serde_json::from_str(&body) {
            Ok(request) => request,
            Err(e) => {
                tracing::error!("Error deserializing {} request: {}", j["method"], e);
                return (
                    StatusCode::BAD_REQUEST,
                    [(header::CONTENT_TYPE, "application/json")],
                    make_error(&0, "Error deserializing request").to_string(),
                );
            }
        };

        let jwt_token = match headers.get("Authorization") {
            Some(jwt_token) => match jwt_token.to_str() {
                Ok(jwt_token) => jwt_token,
                Err(e) => {
                    tracing::error!("Error while converting jwt token to string: {}", e);
                    return (
                        StatusCode::BAD_REQUEST,
                        [(header::CONTENT_TYPE, "application/json")],
                        make_error(&request.id, "Error while converting jwt token to string")
                            .to_string(),
                    );
                }
            },
            None => {
                tracing::error!("Request has no Authorization header");
                return (
                    StatusCode::BAD_REQUEST,
                    [(header::CONTENT_TYPE, "application/json")],
                    make_error(&request.id, "Request has no Authorization header").to_string(),
                );
            }
        };

        let (resp, status) = router
            .do_engine_route(&request, jwt_token.to_string())
            .await;

        return (
            StatusCode::from_u16(status).unwrap(),
            [(header::CONTENT_TYPE, "application/json")],
            resp,
        );
    } else {
        tracing::trace!("Routing to normal route");

        let jwt_token = headers.get("Authorization");
        if jwt_token.is_none() {
            let (resp, status) = router
                .do_route_normal(
                    body,
                    format!("Bearer {}", make_jwt(&router.jwt_key).unwrap()),
                )
                .await;

            return (
                StatusCode::from_u16(status).unwrap(),
                [(header::CONTENT_TYPE, "application/json")],
                resp.to_string(),
            );
        }

        let (resp, status) = router
            .do_route_normal(
                body,
                headers
                    .get("Authorization")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
            )
            .await;

        (
            StatusCode::from_u16(status).unwrap(),
            [(header::CONTENT_TYPE, "application/json")],
            resp.to_string(),
        )
    }
}

#[tokio::main]
async fn main() {
    let matches = clap::App::new("executionbackup")
        .version(VERSION)
        .author("TennisBowling <tennisbowling@tennisbowling.com>")
        .setting(clap::AppSettings::ColoredHelp)
        .about("A Ethereum 2.0 multiplexer enabling execution node failover post-merge")
        .long_version(&*format!(
            "executionbackup version {} by TennisBowling <tennisbowling@tennisbowling.com>",
            VERSION
        ))
        .arg(
            clap::Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .help("Port to listen on")
                .takes_value(true)
                .default_value("7000"),
        )
        .arg(
            clap::Arg::with_name("nodes")
                .short("n")
                .long("nodes")
                .value_name("NODES")
                .help("Comma-separated list of nodes to use")
                .takes_value(true)
                .required(true),
        )
        .arg(
            clap::Arg::with_name("jwt-secret")
                .short("j")
                .long("jwt-secret")
                .value_name("JWT")
                .help("Path to JWT secret file")
                .takes_value(true)
                .required(true),
        )
        .arg(
            clap::Arg::with_name("fcu-majority")
                .short("fcu")
                .long("fcu-majority")
                .value_name("FCU")
                .help("Threshold % (written like 0.1 for 10%) to call responses a majority from forkchoiceUpdated")
                .takes_value(true)
                .default_value("0.6"),
        )
        .arg(
            clap::Arg::with_name("listen-addr")
                .short("addr")
                .long("listen-addr")
                .value_name("LISTEN")
                .help("Address to listen on")
                .takes_value(true)
                .default_value("0.0.0.0"),
        )
        .arg(
            clap::Arg::with_name("log-level")
                .short("l")
                .long("log-level")
                .value_name("LOG")
                .help("Log level")
                .takes_value(true)
                .default_value("info"),
        )
        .arg(
            clap::Arg::with_name("node-timings")
            .long("node-timings")
            .help("Show node ping times")
        )
        .get_matches();

    let port = matches.value_of("port").unwrap();
    let nodes = matches.value_of("nodes").unwrap();
    let jwt_secret = matches.value_of("jwt-secret").unwrap();
    let fcu_majority = matches.value_of("fcu-majority").unwrap();
    let listen_addr = matches.value_of("listen-addr").unwrap();
    let log_level = matches.value_of("log-level").unwrap();
    let node_timings_enabled = matches.is_present("node-timings");

    let log_level = match log_level {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };
    // set log level with tracing subscriber
    let subscriber = tracing_subscriber::fmt().with_max_level(log_level).finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting executionbackup version {VERSION}");

    tracing::info!("fcu invalid threshold set to: {}", fcu_majority);
    let fcu_majority = fcu_majority.parse::<f32>();
    let fcu_majority = match fcu_majority {
        Ok(fcu_majority) => {
            if fcu_majority < 0.0 || fcu_majority > 1.0 {
                tracing::error!("fcu majority must be between 0.0 and 1.0");
                return;
            }
            fcu_majority
        }
        Err(e) => {
            tracing::error!("Error parsing fcu majority: {}", e);
            return;
        }
    };

    let nodes = nodes.split(',').collect::<Vec<&str>>();
    let mut nodesinstances: Vec<Arc<Node>> = Vec::new();
    for node in nodes.clone() {
        let node = Arc::new(Node::new(node.to_string()));
        nodesinstances.push(node);
    }
    let primary_node = nodesinstances[0].clone();

    let jwt_secret = std::fs::read_to_string(jwt_secret).expect("Unable to read JWT secret file");
    let jwt_secret = jwt_secret.trim().to_string();

    // check if jwt_secret starts with "0x" and remove it if it does
    let jwt_secret = jwt_secret
        .strip_prefix("0x")
        .unwrap_or(&jwt_secret)
        .to_string();
    let jwt_secret =
        &EncodingKey::from_secret(&hex::decode(jwt_secret).expect("Could not decode jwt secret"));

    let router = Arc::new(NodeRouter::new(
        jwt_secret,
        fcu_majority,
        nodesinstances,
        primary_node,
        node_timings_enabled,
    ));

    // setup backround task to check if nodes are alive
    let router_clone = router.clone();
    tracing::debug!("Starting background recheck task");
    tokio::spawn(async move {
        loop {
            router_clone.recheck().await;
            tokio::time::sleep(Duration::from_secs(15)).await;
        }
    });

    // setup axum server
    let app = Router::new()
        .route("/", axum::routing::post(route_all))
        .layer(Extension(router.clone()))
        .layer(DefaultBodyLimit::disable()); // no body limit since some requests can be quite large

    let addr = format!("{}:{}", listen_addr, port);
    let addr: SocketAddr = addr.parse().unwrap();
    tracing::info!("Listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
