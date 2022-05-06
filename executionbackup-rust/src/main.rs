use actix_web::{post, web, App, HttpRequest, HttpServer, Responder};
use futures::future::join_all;
use json;
use reqwest::{
    header::{HeaderMap, HeaderName},
    Client,
};
use std::time::Instant;
use tracing::{debug, info, warn, Level};
use tracing_subscriber::fmt;

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
    pub urls: Vec<String>,
    pub index: usize,
    pub nodes: Vec<NodeInstance>,
    pub alive: Vec<NodeInstance>,
    pub dead: Vec<NodeInstance>,
}

impl NodeRouter {
    pub fn new(urls: Vec<String>) -> NodeRouter {
        let mut alive = Vec::new();
        let mut dead = Vec::new();
        let mut nodes = Vec::new();

        for url in urls {
            nodes.push(NodeInstance::new(url));
        }

        NodeRouter {
            urls: urls,
            index: 0,
            nodes: nodes,
            alive: alive,
            dead: dead,
        }
    }

    pub async fn recheck(&mut self) {
        let futs = Vec::new();
        for (i, node) in self.nodes.iter().enumerate() {
            futs.push(async { (i, node.check_alive().await) })
        }

        let results = join_all(futs).await;
        results.sort_unstable_by(|(_, i)| i);
        self.alive.clear();
        self.dead.clear();

        for node in results {
            if node.status {
                self.alive.push(node.node);
            } else {
                self.dead.push(node.node);
            }
        }
    }

    pub async fn setup(&mut self) {
        self.recheck().await;
        info!("node router online");
    }

    pub async fn get_execution_node(&mut self) -> Result<NodeInstance, Box<dyn std::error::Error>> {
        if self.alive.is_empty() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No alive nodes",
            )));
        }

        let n = self.alive[self.index];
        self.index = (self.index + 1) % self.alive.len();
        Ok(n)
    }

    /*python implem:
    async def route(self, req: Request, ws, ws_text) -> None:
        n = await self.get_execution_node()
        r = await n.do_request(data=req.body, headers=req.headers)
        resp = await req.respond(status=r[1], headers=r[2])
        await resp.send(r[0], end_stream=True) */

    pub async fn route(&mut self, body: String, headers: Vec<(String, String)>) -> String {
        let n = self.get_execution_node().await.unwrap();
        n.do_request(body, headers).await.unwrap()
    }
}

#[post("/")]
async fn route(request: HttpRequest, router: web::Data<NodeRouter>) -> impl Responder {
    let body = request.body_bytes().await.unwrap();
    let body = std::str::from_utf8(&body).unwrap();
    let headers = request.headers().iter().collect::<Vec<_>>();

    router.route(body.to_string(), headers).await;
}

#[actix_web::main]
async fn main() {
    let format = fmt::format().with_target(false);
    let subscriber = fmt()
        .event_format(format)
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting default subscriber failed. File an issue at https://github.com/TennisBowling/executionbackup/issues");

    // make the http server
    let mut router = NodeRouter::new(vec!["http://192.168.86.37:8545".to_string()]);

    router.setup().await;

    // make actix web server
    HttpServer::new(move || App::new().data(&router).service(route))
        .bind("localhost:8080")
        .unwrap()
        .run()
        .await;
}
