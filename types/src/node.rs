use crate::*;
use jsonwebtoken;
use reqwest;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;


#[derive(Clone)]
pub struct Node
// represents an EE
{
    pub client: reqwest::Client,
    pub url: String,
    pub status: Arc<RwLock<NodeHealth>>,
    pub jwt_key: jsonwebtoken::EncodingKey,
    pub timeout: Duration,
}

impl Node {
    pub fn new(url: String, jwt_key: jsonwebtoken::EncodingKey, timeout: Duration) -> Node {
        let client = reqwest::Client::new();
        Node {
            client,
            url,
            status: Arc::new(RwLock::new(NodeHealth {
                status: SyncingStatus::NodeNotInitialized,
                resp_time: 0,
            })),
            jwt_key,
            timeout,
        }
    }

    pub async fn set_synced(&self) {
        let status = self.status.read().await;
        if status.status != SyncingStatus::Synced {
            tracing::info!("Node {} is synced with a {}ms timeout", self.url, self.timeout.as_millis());
            drop(status);
            let mut status = self.status.write().await;
            status.status = SyncingStatus::Synced;
        }
        // it's already set as synced
    }

    pub async fn set_offline(&self) {
        let status = self.status.read().await;
        if status.status != SyncingStatus::Offline {
            tracing::warn!("Node {} is offline", self.url);
            drop(status);
            let mut status = self.status.write().await;
            status.status = SyncingStatus::Offline;
        }
    }

    pub async fn set_online_and_syncing(&self) {
        let status = self.status.read().await;
        if status.status != SyncingStatus::OnlineAndSyncing {
            tracing::info!("Node {} is online and syncing with a {}ms timeout", self.url, self.timeout.as_millis());
            drop(status);
            let mut status = self.status.write().await;
            status.status = SyncingStatus::OnlineAndSyncing;
        }
    }

    pub async fn check_status(&self) -> Result<NodeHealth, reqwest::Error> {
        // we need to use jwt here since we're talking directly to the EE's auth port
        let token = make_jwt(&self.jwt_key).unwrap();
        let start = std::time::Instant::now();
        let resp = self
            .client
            .post(self.url.clone())
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&json!({"jsonrpc": "2.0", "method": "eth_syncing", "params": [], "id": 1}))
            .timeout(self.timeout)
            .send()
            .await;
        let resp_time = start.elapsed().as_micros();
        let resp: reqwest::Response = match resp {
            Ok(resp) => resp,
            Err(e) => {
                self.set_offline().await;
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
                // unwrap is safe due to check above
                self.set_synced().await;
            } else {
                self.set_online_and_syncing().await;
            }
        } else {
            // syncing nodes return a object reporting the sync status
            self.set_online_and_syncing().await;
        }

        // update the status
        let mut status = self.status.write().await;
        status.resp_time = resp_time;
        Ok(status.clone())
    }

    pub async fn do_request(
        &self,
        data: &RpcRequest,
        jwt_token: String,
    ) -> Result<String, reqwest::Error> {
        let resp = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Authorization", jwt_token)
            .body(data.as_bytes())
            .timeout(self.timeout)
            .send()
            .await;

        let resp = match resp {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!("Error while sending request to node {}: {}", self.url, e);
                return Err(e);
            }
        };

        let resp_body = resp.text().await?;
        Ok(resp_body)
    }

    pub async fn do_request_no_timeout(
        &self,
        data: &RpcRequest,
        jwt_token: String,
    ) -> Result<String, reqwest::Error> {
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

        let resp_body = resp.text().await?;
        Ok(resp_body)
    }

    pub async fn do_request_no_timeout_str(
        &self,
        data: String,
        jwt_token: String,
    ) -> Result<String, reqwest::Error> {
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

        let resp_body = resp.text().await?;
        Ok(resp_body)
    }
}
