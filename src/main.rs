use std::{collections::HashMap, ops::RangeInclusive};

use actix_web::{get, App, HttpServer, Responder};
use anyhow::{ensure, Context, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use rand::{thread_rng, Rng};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::future::Future;

/// Destination host
const DEST_HOST: &str = "https://httpbin.org/post";
/// Number of queries to destination host per one run
const QUERIES_PER_RUN: usize = 30;
const QUERY_RANGE: RangeInclusive<u32> = 0..=10;

#[derive(Serialize, Deserialize)]
struct Query {
    value: u32,
}

#[derive(Deserialize)]
struct Reply {
    json: Query,
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
/// 
/// # Generics
/// * `T` - Type of future's output, used only to specify it explicitly
/// 
/// # Parameters
/// * `fut` - any future, whose type must be specified
/// 
/// # Return
/// `fut`, but with `Output` specified explicitly
fn async_block_type<T>(fut: impl Future<Output = T>) -> impl Future<Output = T> {
    fut
}

#[get("/run")]
async fn run() -> impl Responder {
    let client = Client::new();
    let mut rng = thread_rng();

    let req_futures: FuturesUnordered<_> = (0..QUERIES_PER_RUN)
        .map(|_| {
            let Ok(query) = serde_json::to_string(&Query {
                value: rng.gen_range(QUERY_RANGE),
            }) else {
                // We can safely expect this serialization to never fail
                // NB: `unreachable` states explicitly we didn't forget error here but don't expect it here
                // in any sane scenario
                unreachable!("Unexpected failure when converting query struct ot JSON string")
            };

            let a = async {
                // Fetch response from server
                let resp = client
                    .post(DEST_HOST)
                    .body(query)
                    .send()
                    .await
                    .with_context(|| "Failure sending HTTP request")?;
                // Handle response status
                let status = resp.status();
                ensure!(
                    status.is_success(),
                    "Server responded with failure code {}",
                    status
                );
                // Retrieve and parse response as JSON
                let bytes = resp
                    .bytes()
                    .await
                    .with_context(|| "Failure reading response body")?;
                
                let Reply {
                    json: Query { value },
                } = serde_json::from_slice(&bytes)
                    .with_context(|| "Failure parsing server response")?;

                Ok(value)
            };

            async_block_type::<Result<_>>(a)
        })
        .collect();

    let req_results: Vec<_> = req_futures.collect().await;

    let counts = req_results
        .into_iter()
        .filter_map(|item| {
            item.map_err(|err| {
                log::error!("Request to remote host failed: {err}");
            })
            .ok()
        })
        // There's space for optimizations here, like clever use of vector of pairs,
        // but making super-optimal code is out-of-scope
        .fold(HashMap::new(), |mut map, value| {
            *map.entry(value).or_insert(0usize) += 1;
            map
        });

    let mut dupes: Vec<_> = counts
        .into_iter()
        .filter_map(|(value, count)| (count > 1).then_some(value))
        .collect();

    dupes.sort();

    format!("{dupes:?}")
}
