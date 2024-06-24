
use std::fmt::format;

use actix_web::{web, App, HttpServer, Responder};
use rayon::string;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use futures::stream::{self, StreamExt};
use log::{debug, info, warn};

use futures::future::join_all;
use reqwest::Client;

#[derive(Deserialize)]
struct SearchQuery {
    q: String
}

#[derive(Debug, Serialize, Deserialize)]
struct SNSearchResult {
    query: String,
    number_of_results: i32,
    results: Vec<SNResult>,
    answers: Vec<String>,
    corrections: Vec<String>,
    infoboxes: Vec<SNInfobox>,
    suggestions: Vec<String>,
    unresponsive_engines: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SNResult {
    url: String,
    title: String,
    content: String,
    published_date: Option<String>,
    thumbnail: Option<String>,
    engine: String,
    parsed_url: Vec<String>,
    template: String,
    engines: Vec<String>,
    positions: Vec<i32>,
    score: f64,
    category: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SNInfobox {
    infobox: String,
    id: String,
    content: String,
    img_src: Option<String>,
    urls: Vec<SNUrl>,
    engine: String,
    engines: Vec<String>,
    attributes: Vec<SNAttribute>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SNUrl {
    title: String,
    url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SNAttribute {
    label: String,
    value: String,
    entity: String,
}


async fn fetch_url(client: &Client, url: &str) -> Result<String, reqwest::Error> {
    let response = client.get(url).send().await?;
    response.text().await
}

async fn process_search_results(search_results: Value) -> Vec<Result<String, reqwest::Error>> {
    let client = Client::new();
    
    let futures: Vec<_> = search_results["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|result| result["url"].as_str())
        .map(|url| fetch_url(&client, url))
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