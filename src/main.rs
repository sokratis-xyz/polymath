
use std::fmt::format;

use actix_web::{web, App, HttpServer, Responder};
use rayon::string;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use futures::stream::{self, StreamExt};
use log::{debug, info, warn};

use futures::future::join_all;
use reqwest::Client;
use readability::extractor;
use tokio::task;

#[derive(Deserialize)]
struct SearchQuery {
    q: String
}

async fn fetch_and_extract(url: String) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    
    // Use tokio's spawn_blocking to run synchronous readability operation
    let extracted_content = task::spawn_blocking(move || {
        extractor::scrape(&url)
            .map(|product| product.content)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }).await??;

    Ok(extracted_content)
}

async fn process_search_results(search_results: Value) -> Vec<Result<String, Box<dyn std::error::Error + Send + Sync>>> {
    
    let futures: Vec<_> = search_results["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|result| result["url"].as_str().map(String::from))
        .map(|url| {
            async move {
                fetch_and_extract(url).await
            }
        })
        .collect();

    join_all(futures).await
}

async fn search_and_index(
    query: web::Query<SearchQuery>,
) -> impl Responder {

    // Getting the internet search results

    let searxng_url = "http://37.27.27.0/search";
    let client = reqwest::Client::new();

    let search_results: Value= client
        .get(searxng_url)
        .query(&[("q", &query.q), ("format", &"json".to_string())])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();


    let results_output = search_results["results"].as_array().unwrap();

    let results = process_search_results(search_results).await;

    for (index, result) in results.iter().enumerate() {
        match result {
            Ok(content) => debug!("Successfully fetched content from URL {}: {} characters", index, content.len()),
            Err(e) => debug!("Error fetching URL {}: {}", index, e),
        }
    }
    // Creating an index of the search results

    // Using the LLM to generate the answer
    
    web::Json({})

}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    HttpServer::new(move || {
        App::new()
            .route("/search", web::get().to(search_and_index))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}