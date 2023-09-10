use arcstr::ArcStr;
use axum::{
    self,
    extract::DefaultBodyLimit,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Router,
};
use ethereum_types::U256;
use futures::{self};
use jsonwebtoken::{self, EncodingKey};
use reqwest::{self, header};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use std::{mem, net::SocketAddr};
use tokio::{sync::RwLock, time::Duration};
mod verify_hash;
use types::ExecutionPayload;
use verify_hash::verify_payload_block_hash;

const VERSION: &str = "1.0.6";
const DEFAULT_ALGORITHM: jsonwebtoken::Algorithm = jsonwebtoken::Algorithm::HS256;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    /// issued-at claim. Represented as seconds passed since UNIX_EPOCH.
    iat: i64,
}

struct CheckAliveResult {
    status: u8, // 0 = offline, 1 = online, 2 = online but syncing
    resp_time: u128,
}

pub fn make_jwt(
    jwt_key: &jsonwebtoken::EncodingKey,
) -> Result<String, jsonwebtoken::errors::Error> {
    let claim_inst = Claims {
        iat: chrono::Utc::now().timestamp(),
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(DEFAULT_ALGORITHM),
        &claim_inst,
        jwt_key,
    )
}

fn make_syncing_str(id: &u64, payload: &serde_json::Value, method: &str) -> String {
    if method == "engine_newPayloadV1" {
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
        json!({"jsonrpc":"2.0","id":id,"result":{"payloadStatus":{"status":"SYNCING","latestValidHash":null,"validationError":null}},"payloadId":null}).to_string()
    } else {
        json!({"jsonrpc":"2.0","id":id,"result":{"payloadStatus":{"status":"SYNCING","latestValidHash":null,"validationError":null}},"payloadId":null}).to_string()
    }
}

#[derive(Clone)]

struct Node
// represents an EE
{
    client: reqwest::Client,
    url: String,
    status: u8,      // 0 = offline, 1 = online, 2 = online but syncing
    resp_time: u128, // response time in ms for the last check_status call
}

impl Node {
    fn new(url: String) -> Node {
        let client = reqwest::Client::new();
        Node {
            client,
            url,
            status: 4,
            resp_time: 0,
        }
    }

    #[inline(always)]
    fn set_online(&mut self) {
        if self.status == 1 {
            return;
        }
        self.status = 1;
        tracing::info!("Node {} is online", self.url);
    }

    #[inline(always)]
    fn set_offline(&mut self) {
        if self.status == 0 {
            return;
        }
        self.status = 0;
        tracing::info!("Node {} is offline", self.url);
    }

    #[inline(always)]
    fn set_syncing(&mut self) {
        if self.status == 2 {
            return;
        }
        self.status = 2;
        tracing::info!("Node {} is syncing", self.url);
    }

    async fn check_status(
        &mut self,
        jwt_key: &jsonwebtoken::EncodingKey,
    ) -> Result<CheckAliveResult, reqwest::Error> {
        // we need to use jwt here since we're talking directly to the EE's auth port
        let token = make_jwt(jwt_key).unwrap();
        let start = std::time::Instant::now();
        let resp = self
            .client
            .post(self.url.clone())
            .header("Authorization", format!("Bearer {}", token))
            .json(&json!({"jsonrpc": "2.0", "method": "eth_syncing", "params": [], "id": 1}))
            .send()
            .await;
        let resp: reqwest::Response = match resp {
            Ok(resp) => resp,
            Err(e) => {
                self.set_offline();
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
                self.set_online();
            } else {
                self.set_syncing();
            }
        } else {
            self.set_syncing();
        }

        self.resp_time = resp_time;

        Ok(CheckAliveResult {
            status: self.status,
            resp_time,
        })
    }

    #[inline(always)]
    async fn do_request(
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
            .timeout(Duration::from_millis(2500))
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

    #[inline(always)]
    async fn do_request_no_timeout(
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
    nodes: Arc<RwLock<Arc<Vec<Node>>>>,
    alive_nodes: Arc<RwLock<Arc<Vec<Node>>>>,
    dead_nodes: Arc<RwLock<Arc<Vec<Node>>>>,
    alive_but_syncing_nodes: Arc<RwLock<Arc<Vec<Node>>>>,

    // this node will be the selected primary node used to route all requests
    primary_node: Arc<RwLock<Option<Arc<Node>>>>,

    // jwt encoded key used to make tokens for the EE's auth port
    jwt_key: Arc<jsonwebtoken::EncodingKey>,

    // percentage of nodes that need to agree for it to be deemed a majority
    majority_percentage: f32, // 0.1..0.9
}

impl NodeRouter {
    fn new(
        jwt_key: &jsonwebtoken::EncodingKey,
        majority_percentage: f32,
        nodes: Vec<Node>,
    ) -> Self {
        NodeRouter {
            nodes: Arc::new(RwLock::new(Arc::new(nodes))),
            alive_nodes: Arc::new(RwLock::new(Arc::new(Vec::new()))),
            dead_nodes: Arc::new(RwLock::new(Arc::new(Vec::new()))),
            alive_but_syncing_nodes: Arc::new(RwLock::new(Arc::new(Vec::new()))),
            primary_node: Arc::new(RwLock::new(None)),
            jwt_key: Arc::new(jwt_key.clone()),
            majority_percentage,
        }
    }
    async fn recheck(&self) {
        let mut nodes = self.nodes.write().await.as_ref().clone();

        // call tokio::spawn on each node to check its status
        let mut results = Vec::new();
        for node in nodes.iter_mut() {
            let fut = node.check_status(&self.jwt_key);
            results.push(fut);
        }

        // wait for all futures to complete
        let results = futures::future::join_all(results).await;

        // now we need to update the alive_nodes, dead_nodes, and alive_but_syncing_nodes vectors
        // get a read lock on those vectors
        // and then get drop read locks, get write locks, and update the vectors
        let mut alive_nodes = self.alive_nodes.read().await.as_ref().clone();
        let mut dead_nodes = self.alive_nodes.read().await.as_ref().clone();
        let mut alive_but_syncing_nodes = self.alive_nodes.read().await.as_ref().clone();

        // clear the vectors
        alive_nodes.clear();
        dead_nodes.clear();
        alive_but_syncing_nodes.clear();

        let mut alive_node_results = Vec::new();

        // put the nodes from the results into the correct vectors
        for (i, result) in results.iter().enumerate() {
            match result {
                Ok(result) => {
                    if result.status == 0 {
                        dead_nodes.push(nodes[i].clone());
                    } else if result.status == 1 {
                        alive_node_results.push((result, i));
                    } else if result.status == 2 {
                        alive_but_syncing_nodes.push(nodes[i].clone());
                    }
                }
                Err(_) => {
                    dead_nodes.push(nodes[i].clone());
                }
            }
        }

        // sort the alive nodes by response time (lowest to highest)
        alive_node_results.sort_by(|a, b| a.0.resp_time.cmp(&b.0.resp_time));

        // put the alive nodes into the alive_nodes vector
        for (_result, i) in alive_node_results {
            alive_nodes.push(nodes[i].clone());
        }

        if alive_nodes.is_empty() {
            if !alive_but_syncing_nodes.is_empty() {
                // if there are no alive nodes, but there are alive_but_syncing_nodes, then we can use one of those
                // as the primary node
                let primary_node = alive_but_syncing_nodes[0].clone();
                *self.primary_node.write().await = Some(Arc::new(primary_node));
                tracing::warn!("No alive nodes, using a syncing node as primary node");
            } else {
                // if there are no alive nodes and no alive_but_syncing_nodes, then we can't use any nodes
                // so we set the primary node to None
                *self.primary_node.write().await = None;
                tracing::error!("No nodes are alive or syncing!");
            }
        } else {
            // if there are alive nodes, then we can use one of those as the primary node
            let primary_node = alive_nodes[0].clone();
            *self.primary_node.write().await = Some(Arc::new(primary_node));
        }

        tracing::debug!(
            "Alive nodes: {}, Dead nodes: {}, Syncing nodes: {}",
            alive_nodes.len(),
            dead_nodes.len(),
            alive_but_syncing_nodes.len()
        );

        // update the vectors
        *self.alive_nodes.write().await = Arc::new(alive_nodes);
        *self.dead_nodes.write().await = Arc::new(dead_nodes);
        *self.alive_but_syncing_nodes.write().await = Arc::new(alive_but_syncing_nodes);
    }

    // always get the same node from the alive_nodes vector
    // if the primary node is offline, then we'll get the next node in the vector, and set the primary node to that node
    // if no alive nodes, call recheck and try again
    async fn get_execution_node(&self) -> Option<Arc<Node>> {
        let primary_node = self.primary_node.read().await;
        let alive_nodes = self.alive_nodes.read().await;

        if alive_nodes.len() == 0 {
            // no alive nodes, try to recheck
            tracing::info!("no alive nodes, rechecking");
            drop(primary_node);
            drop(alive_nodes);
            self.recheck().await;
            return None;
        }

        if primary_node.is_none() {
            // no primary node, set it to the first node in the alive_nodes vector
            drop(primary_node);
            let mut primary_node = self.primary_node.write().await;
            *primary_node = Some(Arc::new(alive_nodes[0].clone()));
            drop(primary_node);
        }

        let primary_node = self.primary_node.read().await;
        if primary_node.as_ref().unwrap().status == 0 {
            // primary node is offline, set primary node to the next node in the alive_nodes vector
            let primary_node_url = primary_node.as_ref().unwrap().url.clone();
            let primary_node_index = match alive_nodes
                .iter()
                .position(|x| x.url == primary_node_url) {
                    Some(index) => index,
                    None => {
                        tracing::error!("primary node not found in alive nodes vector");
                        return None;
                    }
                };

            drop(primary_node);
            let mut primary_node = self.primary_node.write().await;

            // if there are no more nodes in the alive_nodes vector, set primary node to a node in the syncing nodes vector
            if alive_nodes.is_empty() {
                let alive_but_syncing_nodes = self.alive_but_syncing_nodes.read().await;
                if alive_but_syncing_nodes.is_empty() {
                    // no nodes available
                    *primary_node = None;
                    return None;
                } else {
                    *primary_node = Some(Arc::new(alive_but_syncing_nodes[0].clone()));
                    return Some(primary_node.as_ref().unwrap().clone());
                }
            }

            if primary_node_index == alive_nodes.len() - 1 {
                // primary node is the last node in the alive_nodes vector, set it to the first node in the vector
                *primary_node = Some(Arc::new(alive_nodes[0].clone()));
            } else {
                // primary node is not the last node in the alive_nodes vector, set it to the next node in the vector
                *primary_node = Some(Arc::new(alive_nodes[primary_node_index + 1].clone()));
            }
            drop(primary_node);
        }

        let primary_node = self.primary_node.read().await; // we must lock here since we might've dropped it above
        Some(primary_node.as_ref().unwrap().clone())
    }

    // gets the majority response from a vector of responses
    // must have at least majority_percentage of the nodes agree
    // if there is no majority, then return None
    // if there is a draw, just return the first response
    // u64 on the response should be the "id" field from the any of the responses
    fn fcu_majority(&self, results: &Vec<&str>) -> Option<String> {
        let total_responses = results.len();
        let majority_count = (total_responses as f32 * self.majority_percentage) as usize;

        // Create a hashmap to store response frequencies
        let mut response_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

        for &response in results.iter() {
            *response_counts.entry(response).or_insert(0) += 1;
        }

        // Find the response with the most occurrences
        let mut majority_response = None;
        let mut max_count = 0;

        for (response, &count) in response_counts.iter() {
            if count > max_count {
                majority_response = Some(*response);
                max_count = count;
            }
        }

        // Check if the majority count is greater than or equal to the required count
        if max_count >= majority_count {
            majority_response.map(|response| response.to_string())
        } else {
            None
        }
    }

    async fn fcu_logic(&self, resps: &Vec<&str>, req: String, jwt_token: String, id: &u64) -> String {
        if resps.is_empty() {
            // no responses, so return SYNCING
            tracing::error!("No responses, returning SYNCING.");
            let req = serde_json::from_str::<serde_json::Value>(&req).unwrap();
            return make_syncing_str(id, &req["params"][0], &req["method"].as_str().unwrap());   // safe to call unwrap here as we've already checked the method
        }

        let majority = match self.fcu_majority(resps) {
            Some(majority) => majority,
            None => {
                // no majority, so return SYNCING
                tracing::error!("No majority, returning SYNCING.");
                let req = serde_json::from_str::<serde_json::Value>(&req).unwrap();
                return make_syncing_str(id, &req["params"][0], &req["method"].as_str().unwrap());
            }
        };

        let majorityjson: serde_json::Value = match serde_json::from_str(&majority) {
            Ok(majorityjson) => majorityjson,
            Err(e) => {
                // majority is not valid json, so return SYNCING and inform the user
                tracing::error!(
                    "Majority is not valid json, returning SYNCING. Error: {}",
                    e
                );
                let req = serde_json::from_str::<serde_json::Value>(&req).unwrap();
                return make_syncing_str(id, &req["params"][0], &req["method"].to_string());
            }
        };


        if majorityjson["result"]["payloadStatus"]["status"] == "INVALID" {
            // majority is INVALID, so return INVALID (to not go through the next parts of the algorithm)
            return majority;
        }

        for resp in resps {
            if resp.is_empty() {
                continue;
            }
            let respjson: serde_json::Value = match serde_json::from_str(resp) {
                Ok(respjson) => respjson,
                Err(_) => {
                    // resp is not valid json, so ignore
                    continue;
                }
            };

            if respjson["result"]["payloadStatus"]["status"] == "INVALID" {
                // at least one node is INVALID, so return SYNCING
                tracing::warn!("At least one node is INVALID, returning SYNCING.");
                let req = serde_json::from_str::<serde_json::Value>(&req).unwrap();
                return make_syncing_str(id, &req["params"][0], &req["method"].to_string());
            }
        }

        // if we get here, all responses are VALID or SYNCING, so return the majority
        // send to the syncing nodes to help them catch up with tokio::spawn
        let syncing_nodes = self.alive_but_syncing_nodes.clone();
        let req_clone = req.to_string();
        let jwt_token_clone = jwt_token.to_string();
        tokio::spawn(async move {
            let syncing_nodes = syncing_nodes.read().await;
            tracing::debug!("sending fcU to {} syncing nodes", syncing_nodes.len());
            for node in syncing_nodes.iter() {
                if let Err(e) = node.do_request_no_timeout(req_clone.clone(), jwt_token_clone.clone()).await {
                    // a lot of these syncing nodes are slow so we dont add a timeout
                    tracing::error!("error sending fcU to syncing node: {}", e);
                }
            }
        });

        majority
    }

    async fn do_engine_route(
        &self,
        data: String,
        j: &serde_json::Value,
        jwt_token: String,
    ) -> (String, u16) {
        if j["method"] == "engine_getPayloadV1"
        // getPayloadV1 is for getting a block to be proposed, so no use in getting from multiple nodes
        {
            let node = match self.get_execution_node().await {
                None => {
                    return (String::from("No nodes available"), 500);
                }
                Some(node) => node,
            };

            let resp = node.do_request_no_timeout(data, jwt_token).await;
            tracing::debug!("engine_getPayloadV1 sent to node: {}", node.url);
            match resp {
                Ok(resp) => (resp.0, resp.1),
                Err(e) => {
                    tracing::warn!("engine_getPayloadV1 error: {}", e);
                    (e.to_string(), 500)
                }
            }
        } else if j["method"] == "engine_getPayloadV2" {
            // getPayloadV2 has a different schema, where alongside the executionPayload it has a blockValue
            // so we should send this to all the nodes and then return the one with the highest blockValue
            let mut resps: Vec<ArcStr> = Vec::new();
            let alive_nodes = self.alive_nodes.read().await;
            for node in alive_nodes.iter() {
                let resp = node.do_request_no_timeout(data.clone(), jwt_token.clone()).await;
                match resp {
                    Ok(resp) => {
                        resps.push(resp.0.into());
                    }
                    Err(e) => {
                        tracing::error!("engine_getPayloadV2 error: {}", e);
                    }
                }
            }
            mem::drop(alive_nodes);
            let mut blocks: HashMap<U256, ArcStr> = HashMap::new();

            for resp in resps {
                let j = serde_json::from_str::<serde_json::Value>(&resp);

                let j = match j {
                    Ok(j) => j,
                    Err(e) => {
                        tracing::error!("unable to deserialize engine_getPayloadV2 response: {}", e);
                        continue;
                    }
                };

                let block_value = &j["result"]["blockValue"].as_str();
                let block_value = match block_value {
                    Some(block_value) => block_value,
                    None => {
                        // try and extract the error field
                        let error = &j["error"]["message"].as_str();
                        match error {
                            Some(error) => {
                                tracing::error!("engine_getPayloadV2 response has error field: {}", error);
                                continue;
                            }
                            None => {
                                tracing::error!("engine_getPayloadV2 response has no blockValue or error field");
                                continue;
                            }
                        }
                    }
                };


                let block_value = U256::from_str_radix(block_value, 16)
                        .unwrap();
                blocks.insert(block_value, resp);
            }

            match blocks.iter().max_by_key(|(k, _v)| *k) {
                Some((k, v)) => {
                    tracing::info!("engine_getPayloadV2: highest blockValue is {}", k);
                    return (v.to_string(), 200);
                }
                None => {
                    tracing::error!("No blocks found in engine_getPayloadV2 responses");
                    return (String::from("No blocks found"), 500);
                }
            }



        } else if j["method"] == "engine_forkchoiceUpdatedV1"
            || j["method"] == "engine_newPayloadV1"
            || j["method"] == "engine_forkchoiceUpdatedV2"
            || j["method"] == "engine_newPayloadV2"
        {
            tracing::debug!("Sending {} to alive nodes", j["method"]);
            let mut resps: Vec<String> = Vec::new();
            let alive_nodes = self.alive_nodes.read().await;
            for node in alive_nodes.iter() {
                let resp = node.do_request(data.clone(), jwt_token.clone()).await;
                match resp {
                    Ok(resp) => {
                        resps.push(resp.0);
                    }
                    Err(e) => {
                        tracing::error!("{} error: {}", j["method"], e);
                    }
                }
            }
            mem::drop(alive_nodes);
            let id = match j["id"].as_u64() {
                Some(id) => id,
                None => {
                    tracing::warn!("{} request has no id field", j["method"]);
                    return (String::from("No id field"), 500);
                }
            };

            let mut resps_new = Vec::<&str>::with_capacity(resps.len()); // faster to allocate in one go
            for item in &resps {
                resps_new.push(item);
            }

            let resp = self.fcu_logic(&resps_new, data, jwt_token, &id).await;
            (resp, 200)
        } else {
            // wait for primary node's response, but also send to all other nodes
            let primary_node = match self.get_execution_node().await {
                Some(primary_node) => primary_node,
                None => {
                    tracing::warn!("No primary node available");
                    return (String::from("No nodes available"), 500);
                }
            };

            let resp = primary_node
                .do_request_no_timeout(data.clone(), jwt_token.clone())
                .await;
            tracing::debug!("Sent to primary node: {}", primary_node.url);

            let alive_nodes = self.alive_nodes.clone();
            let data = data.to_owned();
            let jwt_token = jwt_token.to_owned();
            tokio::spawn(async move {
                let alive_nodes = alive_nodes.read().await;
                for node in alive_nodes.iter() {
                    if node.url != primary_node.url {
                        match node.do_request_no_timeout(data.clone(), jwt_token.clone()).await {
                            Ok(_) => {}
                            Err(e) => {
                                tracing::error!("error sending fcU to syncing node: {}", e);
                            }
                        };
                    }
                }
            });
            match resp {
                Ok(resp) => (resp.0, resp.1),
                Err(e) => {
                    tracing::warn!("Error from primary node: {}", e);
                    (e.to_string(), 500)
                }
            }
        }
    }

    async fn do_route_normal(&self, data: String, jwt_token: String) -> (String, u16) {
        // simply send request to primary node
        let primary_node = match self.get_execution_node().await {
            Some(primary_node) => primary_node,
            None => {
                tracing::warn!("No primary node available for normal request");
                return (String::from("No nodes available"), 500);
            }
        };

        let resp = primary_node.do_request_no_timeout(data, jwt_token).await;
        match resp {
            Ok(resp) => (resp.0, resp.1),
            Err(e) => (e.to_string(), 500),
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
            tracing::error!("Couldn't deserizlie request. Error: {}. Body: {}", e, body);
            return (
                StatusCode::from_u16(400).unwrap(),
                [(header::CONTENT_TYPE, "application/json")],
                "Couldn't deserialize request body".to_string(),
            );
        }
    };


    tracing::debug!("Request received, method: {}", j["method"]);
    let meth = match j["method"].as_str() {
        Some(meth) => meth,
        None => {
            tracing::error!("Request has no method field");
            return (
                StatusCode::from_u16(400).unwrap(),
                [(header::CONTENT_TYPE, "application/json")],
                "Request has no method field".to_string(),
            );
        }
    };

    if meth.starts_with("engine_") {
        tracing::trace!("Routing to engine route");
        let jwt_token = match headers.get("Authorization") {
            Some(jwt_token) => {
                match jwt_token.to_str() {
                    Ok(jwt_token) => jwt_token,
                    Err(e) => {
                        tracing::error!("Error while converting jwt token to string: {}", e);
                        return (
                            StatusCode::from_u16(400).unwrap(),
                            [(header::CONTENT_TYPE, "application/json")],
                            "Error while converting jwt token to string".to_string(),
                        );
                    }
                }
            },
            None => {
                tracing::error!("Request has no Authorization header");
                return (
                    StatusCode::from_u16(400).unwrap(),
                    [(header::CONTENT_TYPE, "application/json")],
                    "Request has no Authorization header".to_string(),
                );
            }
        };


        let (resp, status) = router.do_engine_route(body, &j, jwt_token.to_string()).await;

        return (
            StatusCode::from_u16(status).unwrap(),
            [(header::CONTENT_TYPE, "application/json")],
            resp.to_string(),
        );
    } else {
        tracing::trace!("Routing to normal route");

        let jwt_token = headers.get("Authorization");
        if jwt_token.is_none() {
            let (resp, status) = router
                .do_route_normal(body, format!("Bearer {}", make_jwt(&router.jwt_key).unwrap()))
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
                headers.get("Authorization").unwrap().to_str().unwrap().to_string(),
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
        .get_matches();

    let port = matches.value_of("port").unwrap();
    let nodes = matches.value_of("nodes").unwrap();
    let jwt_secret = matches.value_of("jwt-secret").unwrap();
    let fcu_invalid_threshold = matches.value_of("fcu-invalid-threshold").unwrap();
    let listen_addr = matches.value_of("listen-addr").unwrap();
    let log_level = matches.value_of("log-level").unwrap();

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

    tracing::info!("fcu invalid threshold set to: {}", fcu_invalid_threshold);
    let fcu_invalid_threshold = fcu_invalid_threshold
        .parse::<f32>()
        .expect("Invalid fcu threshold");

    let nodes = nodes.split(',').collect::<Vec<&str>>();
    let mut nodesinstances: Vec<Node> = Vec::new();
    for node in nodes {
        let node = Node::new(node.to_string());
        nodesinstances.push(node);
    }

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
        fcu_invalid_threshold,
        nodesinstances,
    ));

    // setup backround task to check if nodes are alive
    let router_clone = router.clone();
    tracing::debug!("Starting background recheck task");
    tokio::spawn(async move {
        loop {
            router_clone.recheck().await;
            tokio::time::sleep(Duration::from_secs(30)).await;
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
