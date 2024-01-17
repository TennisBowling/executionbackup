use axum::{
    self,
    extract::DefaultBodyLimit,
    http::{HeaderMap, StatusCode, header},
    Extension, Router, response::IntoResponse,
    response::Response,
};
use ethereum_types::U256;
use futures::future::join_all;
use jsonwebtoken::{self, EncodingKey};
use reqwest::{self};
use serde_json::json;
use std::{sync::Arc, net::SocketAddr, any::type_name, collections::HashMap};
use tokio::{sync::RwLock, time::Duration};
mod verify_hash;
use lazy_static::lazy_static;
use types::*;
use verify_hash::verify_payload_block_hash;


const VERSION: &str = "1.1.3beta";
const DEFAULT_ALGORITHM: jsonwebtoken::Algorithm = jsonwebtoken::Algorithm::HS256;

lazy_static! {
    static ref TIMEOUT: Duration = {
        Duration::from_millis(7500)
    };

    static ref JWT_HEADER: jsonwebtoken::Header = {
        jsonwebtoken::Header::new(DEFAULT_ALGORITHM)
    };
}

fn make_jwt(jwt_key: &jsonwebtoken::EncodingKey) -> Result<String, jsonwebtoken::errors::Error> {
    let claim_inst = Claims {
        iat: chrono::Utc::now().timestamp(),
    };
    jsonwebtoken::encode(
        &JWT_HEADER,
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

    if let Some(error)= j.get("error") {
        tracing::error!("Response has error field: {}", error);
        return Err(ParseError::ElError);
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

fn make_syncing_str(id: &u64, payload: &serde_json::Value, method: &EngineMethod) -> String {
    match method {
        EngineMethod::engine_newPayloadV1 => {
            tracing::debug!(
                "Verifying execution payload blockhash {}.",
                payload["blockHash"]
            );
            let execution_payload = match serde_json::from_value::<ExecutionPayload>(payload.clone()) {
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
            json!({"jsonrpc":"2.0","id":id,"result":{"payloadStatus":{"status":"SYNCING","latestValidHash":null,"validationError":null}},"payloadId":null}).to_string()
        },

        _ => {
            json!({"jsonrpc":"2.0","id":id,"result":{"payloadStatus":{"status":"SYNCING","latestValidHash":null,"validationError":null}},"payloadId":null}).to_string()
        }
    }
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
            .header("Content-Type", "application/json")
            .json(&json!({"jsonrpc": "2.0", "method": "eth_syncing", "params": [], "id": 1}))
            .timeout(*TIMEOUT)
            .send()
            .await;
        let resp_time = start.elapsed().as_micros();
        let resp: reqwest::Response = match resp {
            Ok(resp) => resp,
            Err(e) => {
                self.set_offline().await;
                tracing::error!("Error while checking status of node {}: {}", self.url, e);
                return Err(e);
            }
        };
        

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
        node.set_online_and_syncing().await;
        alive_but_syncing_nodes.push(node);
    }

    // returns Vec<T> where it tries to deserialize for each resp to T
    async fn concurrent_requests<T>(&self, request: &RpcRequest, jwt_token: String) -> Vec<T>
        where T: serde::de::DeserializeOwned {
        let alive_nodes = self.alive_nodes.read().await;
        let mut futs = Vec::with_capacity(alive_nodes.len());

        for node in alive_nodes.iter() {
            futs.push(node.do_request(request, jwt_token.clone()));
        }

        let mut out = Vec::with_capacity(alive_nodes.capacity());
        for resp in join_all(futs).await {
            match resp {
                Ok(resp) => {   // response from node
                    let result = match parse_result(&resp.0) {
                        Ok(result) => result,
                        Err(e) => {
                            tracing::error!("Couldn't parse node result for {:?}: {:?}", request.method, e);
                            continue;
                        }
                    };

                    match serde_json::from_value::<T>(result) {
                        Ok(deserialized) => {
                            out.push(deserialized);
                        },
                        Err(e) => {
                            tracing::error!("Couldn't deserialize response {:?} from node to type {}: {}", request.method, type_name::<T>(), e);
                        }

                    }

                },
                Err(e) => {
                    tracing::error!("{:?} error: {}", request.method, e);
                }
            }
        }

        out

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
                    Err(e) => {
                        if e.is_decode() {
                            tracing::error!("Error while checking node {}: {}; Maybe jwt related?", node_clone.url, e);
                        }
                        else {
                            tracing::error!("Error while checking node {}: {}", node_clone.url, e);
                        }
                        
                        (
                            NodeHealth {
                                status: SyncingStatus::Offline,
                                resp_time: 0,
                            },
                            node_clone.clone(),
                        )
                    }
                    
                }
            };
            checks.push(check);
        }

        let results = join_all(checks).await;

        for (status, node) in results {
            if status.status == SyncingStatus::Synced {
                new_alive_nodes.push((status.resp_time, node.clone()));

                if self.node_timings_enabled {
                    tracing::info!("{}: {:.2}ms", node.url, (status.resp_time as f64 / 1000.0));  // resp_time is in micros
                }
            } else if status.status == SyncingStatus::OnlineAndSyncing {
                new_alive_but_syncing_nodes.push(node.clone());

                if self.node_timings_enabled {
                    tracing::info!("{}: {:.2}ms", node.url, (status.resp_time as f64 / 1000.0));
                }
            } else {
                new_dead_nodes.push(node.clone());
                if self.node_timings_enabled {
                    tracing::warn!("Dead node: {}", node.url);
                }
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
                None
            } else {
                // no synced nodes, but there are syncing nodes, so return the first syncing node

                let node = alive_but_syncing_nodes[0].clone();
                let mut primary_node = self.primary_node.write().await;
                *primary_node = node.clone();
                Some(node)
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
                None
            } else {
                // no synced nodes, but there are syncing nodes, so return the first syncing node

                let node = alive_but_syncing_nodes[0].clone();
                let mut primary_node = self.primary_node.write().await;
                *primary_node = node.clone();
                Some(node)
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
        let mut response_counts: HashMap<&PayloadStatusV1, usize> =
            HashMap::new();

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
            majority_response.cloned().cloned()
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
            tracing::debug!("sending fcU or newPayload to {} syncing nodes", syncing_nodes.len());

            let mut futs = Vec::with_capacity(syncing_nodes.len());
            for node in syncing_nodes.iter() {
                futs.push(node.do_request_no_timeout(&req_clone, jwt_token_clone.clone()));
            }

            join_all(futs).await;

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
            },   // getPayloadV1

            EngineMethod::engine_getPayloadV2 => {
                // getPayloadV2 has a different schema, where alongside the executionPayload it has a blockValue
                // so we should send this to all the nodes and then return the one with the highest blockValue
                let resps: Vec<getPayloadResponse> = self.concurrent_requests(request, jwt_token).await;
                let most_profitable = resps.iter().max_by(|resp_a, resp_b| resp_a.block_value().cmp(&resp_b.block_value()));

                if let Some(most_profitable_payload) = most_profitable {
                    tracing::info!("Blocks profitability: {:?}. Using payload with value of {}", resps.iter().map(|payload| payload.block_value()).collect::<Vec<U256>>(), most_profitable_payload.block_value());
                    return (make_response(&request.id, json!(most_profitable_payload)), 200);
                }
                
                // we have no payloads
                tracing::warn!("No blocks found in engine_getPayloadV2 responses");
                (make_error(&request.id, "No blocks found in engine_getPayloadV2 responses"), 200)
            },      // getPayloadV2

            EngineMethod::engine_newPayloadV1 | EngineMethod::engine_newPayloadV2 => {
                tracing::debug!("Sending newPayload to alive nodes");
                let resps: Vec<PayloadStatusV1> = self.concurrent_requests(request, jwt_token.clone()).await;

                let resp = match self.fcu_logic(&resps, request, jwt_token).await {
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

                // we have a majority
                (make_response(&request.id, json!(resp)), 200)
            },      // newPayloadV1, V2

            EngineMethod::engine_forkchoiceUpdatedV1 | EngineMethod::engine_forkchoiceUpdatedV2 => {
                tracing::debug!("Sending fcU to alive nodes");
                let resps: Vec<forkchoiceUpdatedResponse> = self.concurrent_requests(request, jwt_token.clone()).await;

                let mut resps_new = Vec::<PayloadStatusV1>::with_capacity(resps.len()); // faster to allocate in one go
                let mut payload_id: Option<String> = None;

                for resp in resps {
                    if let Some(inner_payload_id) = resp.payloadId {    // todo: make this look cleaner. 
                        payload_id = Some(inner_payload_id);                     // if payloadId is not null, then use that. all resps will have the same payloadId
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


                // we have a majority
                (make_response(&request.id,
                        json!(forkchoiceUpdatedResponse {
                            payloadStatus: resp,
                            payloadId: payload_id,
                        }),),
                    200,
                )
            },       // fcU V1, V2

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


                // spawn a new task to replicate requests
                let alive_nodes = self.alive_nodes.clone();
                let jwt_token = jwt_token.to_owned();
                let request_clone = request.clone();
                tokio::spawn(async move {
                    let alive_nodes = alive_nodes.read().await;
                    let mut futs = Vec::with_capacity(alive_nodes.len());

                    for node in alive_nodes.iter() {
                        if node.url != primary_node.url {
                            futs.push(node.do_request_no_timeout(&request_clone, jwt_token.clone()));
                        }
                    }

                    join_all(futs).await;

                });


                // return resp from primary node
                match resp {
                    Ok(resp) => (resp.0, resp.1),
                    Err(e) => {
                        tracing::warn!("Error from primary node: {}", e);
                        (make_error(&request.id, &e.to_string()), 200)
                    }
                }

            }   // all other engine requests
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
            Err(e) => (make_error(&1, &e.to_string()), 200),
        }
    }
}

// func to take body and headers from a request and return a string
async fn route_all(
    headers: HeaderMap,
    Extension(router): Extension<Arc<NodeRouter>>,
    body: String,
) -> impl IntoResponse {
    let j: serde_json::Value = match serde_json::from_str(&body) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Couldn't deserialize request. Error: {}. Body: {}", e, body);
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, "application/json")
                .body(make_error(&0, "Couldn't deserialize request body").to_string())
                .unwrap();
        }
    };

    let meth = match j["method"].as_str() {
        Some(meth) => meth,
        None => {
            tracing::error!("Request has no method field");
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, "application/json")
                .body(make_error(&0, "Request has no method field").to_string())
                .unwrap();
        }
    };

    tracing::debug!("Request received, method: {}", j["method"]);

    if meth.starts_with("engine_") {
        tracing::trace!("Routing {} to engine route", j["method"]);

        let request: RpcRequest = match serde_json::from_str(&body) {
            Ok(request) => request,
            Err(e) => {
                tracing::error!("Error deserializing {} request: {}", j["method"], e);
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(make_error(&0, "Error deserializing request").to_string())
                    .unwrap();
            }
        };

        let jwt_token = match headers.get("Authorization") {
            Some(jwt_token) => match jwt_token.to_str() {
                Ok(jwt_token) => jwt_token,
                Err(e) => {
                    tracing::error!("Error while converting jwt token to string: {}", e);
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(make_error(&0, "Error while converting jwt token to string").to_string())
                        .unwrap();
                }
            },
            None => {
                tracing::error!("Request has no Authorization header");
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(make_error(&0, "Request has no Authorization header").to_string())
                    .unwrap();
            }
        };

        let (resp, status) = router
            .do_engine_route(&request, jwt_token.to_string())
            .await;

        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(resp)
            .unwrap()
    }       // engine requests
    else {
        tracing::trace!("Routing to normal route");

        let jwt_token = headers.get("Authorization");
        if jwt_token.is_none() {
            let (resp, status) = router
                .do_route_normal(
                    body,
                    format!("Bearer {}", make_jwt(&router.jwt_key).unwrap()),   // supporting requests without jwt tokens to authrpc is used for OE.
                )                                                               // open an issue if you need this to be changed
                .await;

            return Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, "application/json")
                .body(resp)
                .unwrap();
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

        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(resp)
            .unwrap()
    }   // all other non-engine requests
}

async fn metrics(Extension(router): Extension<Arc<NodeRouter>>) -> impl IntoResponse {
    // report back everything we can
    let syncing_nodes = router.alive_but_syncing_nodes.read().await;
    let alive_nodes = router.alive_nodes.read().await;
    let mut both = syncing_nodes.clone();
    both.append(&mut alive_nodes.clone());
    drop(alive_nodes);
    drop(syncing_nodes);

    
    let mut futs = Vec::new();
    both.iter().for_each(|node| futs.push(async move {(node.url.clone(), node.status.read().await.resp_time)}));

    
    let resp_times: HashMap<String, u128> = join_all(futs).await.into_iter().collect();
    let dead_nodes = router.dead_nodes.read().await;


    let metrics_report = MetricsReport{
        response_times: resp_times,
        alive_nodes: router.alive_nodes.read().await.iter().map(|node| node.url.clone()).collect(),
        syncing_nodes: router.alive_but_syncing_nodes.read().await.iter().map(|node| node.url.clone()).collect(),
        dead_nodes: dead_nodes.iter().map(|node| node.url.clone()).collect(),
        primary_node: router.primary_node.read().await.url.clone(),
    };


    let resp_body = match serde_json::to_string(&metrics_report) {
        Ok(resp_body) => resp_body,
        Err(e) => {
            tracing::error!("Unable to serialize metrics report: {}", e);
            r#"{"error":"Unable to serialize metrics report"}"#.to_string()
        }
    };

    Response::builder()
        .status(200)
        .header(header::CONTENT_TYPE, "application/json")
        .body(resp_body)
        .unwrap()

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
            if !(0.0..=1.0).contains(&fcu_majority) {
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
        .route("/metrics", axum::routing::get(metrics))
        .layer(Extension(router.clone()))
        .layer(DefaultBodyLimit::disable()); // no body limit since some requests can be quite large

    let addr = format!("{}:{}", listen_addr, port);
    let addr: SocketAddr = addr.parse().unwrap();
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("Unable to bind to {}: {}", addr, e);
            return;
        }
    };
    tracing::info!("Listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
