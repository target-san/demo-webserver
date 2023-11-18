use std::ops::RangeInclusive;

use actix_web::{get, App, HttpServer, Responder};
use anyhow::{Result, ensure, Context};
use futures::stream::{FuturesUnordered, StreamExt};
use rand::{thread_rng, Rng};
use reqwest::Client;
use serde::Serialize;
use std::future::Future;

/// Destination host
const DEST_HOST: &str = "https://httpbin.org/post";
/// Number of queries to destination host per one run
const QUERIES_PER_RUN: usize = 30;
const QUERY_RANGE: RangeInclusive<u32> = 0..=10;

#[derive(Serialize)]
struct Query {
    value: u32,
}

#[actix_web::main]
async fn main() -> Result<()> {
    HttpServer::new(|| App::new().service(run))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
        .map_err(Into::into)
}
/// Serves only to fix async block return type
fn async_block_type<T>(fut: impl Future<Output = T>) -> impl Future<Output = T> { fut }

#[get("/run")]
async fn run() -> impl Responder {
    let client = Client::new();
    let mut rng = thread_rng();

    let req_futures: FuturesUnordered<_> = (0..QUERIES_PER_RUN).map(|_| {
        let Ok(query) = serde_json::to_string(&Query {
            value: rng.gen_range(QUERY_RANGE),
        })
        else {
            // We can safely expect this serialization to never fail
            unreachable!("Unexpected failure when converting query struct ot JSON string")
        };

        let a = async {
            let resp = client
                .post(DEST_HOST)
                .body(query)
                .send()
                .await
                .with_context(|| "Failure sending HTTP request")?;

            let status = resp.status();
            ensure!(status.is_success(), "Server responded with failure code {}", status);
            // We simply expect UTF-8 text
            resp.text().await.with_context(|| "Failure reading response body")
        };

        async_block_type::<Result<_>>(a)
    })
    .collect();

    let req_results: Vec<_> = req_futures.collect().await;

    format!("{req_results:?}")
}
