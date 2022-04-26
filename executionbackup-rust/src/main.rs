use actix_web::{get, web, App, HttpServer, Responder};
use json;
use reqwest::{
    header::{HeaderMap, HeaderName},
    Client,
};
use std::time::Instant;

struct NodeInstance {
    pub url: String,
    pub status: bool,
    pub client: Client,
}

impl NodeInstance {
    pub fn new(url: String) -> NodeInstance {
        NodeInstance {
            url: url,
            status: false,
            client: Client::new(),
        }
    }

    pub async fn check_alive(&mut self) -> Result<(bool, u128), Box<dyn std::error::Error>> {
        let start = Instant::now();
        let r = json::parse(
            &self
                .do_request(
                    r#"{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}"#.to_string(),
                    vec![("Content-Type".to_string(), "application/json".to_string())],
                )
                .await?,
        )?;
        let end = start.elapsed();

        if r["result"] == false {
            self.status = true;
        }

        Ok((self.status, end.as_millis()))
    }

    pub async fn do_request(
        &mut self,
        data: String,
        headers: Vec<(String, String)>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        //convert headers to a map
        let mut map = HeaderMap::new();
        for (key, value) in headers {
            map.insert(
                HeaderName::from_bytes(key.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }

        let r = &self
            .client
            .post(&self.url)
            .body(data)
            .headers(map)
            .send()
            .await?
            .text()
            .await?;

        Ok(r.to_string())
    }
}

#[tokio::main]
async fn main() {
    let mut node: NodeInstance = NodeInstance::new(String::from("http://192.168.86.83:8545"));
    let res = node.check_alive().await;
    match res {
        Ok((status, time)) => {
            println!("{}", status);
            println!("{}", time);
        }
        Err(e) => println!("{}", e),
    }
}
