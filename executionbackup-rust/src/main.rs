use actix_web::{get, web, App, HttpServer, Responder};
use json;
use reqwest::{
    header::{HeaderMap, HeaderName},
    Client,
};
use std::time::Instant;
use tracing::{debug, info, warn, Level};
use tracing_subscriber::fmt;
#[macro_use(c)]
extern crate cute;

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

    pub async fn set_alive(&mut self) {
        self.status = true;
        info!(url = %self.url, "Node is alive");
    }

    pub async fn set_offline(&mut self) {
        self.status = false;
        info!(url = %self.url, "Node is offline");
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
            self.set_alive().await;
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

        // send request and if successful return the response. Otherwise return an error and set status to false
        let response = self
            .client
            .post(&self.url)
            .headers(map)
            .body(data)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.text().await?)
        } else {
            self.set_offline().await;
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Request failed",
            )))
        }
    }
}

struct NodeRouter {
    pub urls: Vec[String],
    pub index: i64,
    pub alive: Vec[NodeInstance],
    pub dead: Vec[NodeInstance]
}

impl NodeRouter {
    async pub fn new(urls: Vec[String]) {

        

        NodeRouter {
            urls: urls,
            index: 0,
        }
    }
}

#[tokio::main]
async fn main() {
    let format = fmt::format().with_target(false);
    let subscriber = fmt()
        .event_format(format)
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting default subscriber failed. File an issue at https://github.com/TennisBowling/executionbackup/issues");

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
