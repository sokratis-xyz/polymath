
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
use rust_bert::pipelines::sentence_embeddings::{SentenceEmbeddingsModel, SentenceEmbeddingsBuilder};
use anyhow::{Result, Context};

#[derive(Deserialize)]
struct SearchQuery {
    q: String
}
const CHUNK_SIZE: usize = 512;  // Adjust this based on your needs

#[derive(Clone)]
struct ProcessedContent {
    url: String,
    chunks: Vec<String>,
    embeddings: Vec<Vec<f32>>,
}

async fn fetch_and_process(url: String, model: SentenceEmbeddingsModel) -> Result<ProcessedContent> {

    let url_clone = url.clone();

    // Fetch and extract content
    let content = task::spawn_blocking(move || {
        extractor::scrape(&url)
            .map(|product| product.content)
            .context("Failed to scrape content")
    }).await??;

    // Chunk the content
    let chunks = chunk_content(&content);
    
    let chunks_clone = chunks.clone();

    // Generate embeddings
    let embeddings = task::spawn_blocking(move || {
        model.encode(&chunks)
            .context("Failed to generate embeddings")
    }).await??;

    Ok(ProcessedContent { url: url_clone , chunks: chunks_clone , embeddings: embeddings })
}

fn chunk_content(content: &str) -> Vec<String> {
    content.split_whitespace()
        .collect::<Vec<&str>>()
        .chunks(CHUNK_SIZE)
        .map(|chunk| chunk.join(" "))
        .collect()
}

async fn process_search_results(search_results: Value) -> Result<Vec<ProcessedContent>> {
    // Initialize the BERT model (this is computationally expensive, so we do it once)
    let model = SentenceEmbeddingsBuilder::remote(rust_bert::pipelines::sentence_embeddings::SentenceEmbeddingsModelType::AllMiniLmL12V2)
        .create_model()
        .context("Failed to create BERT model")?;

    let futures: Vec<_> = search_results["results"]
        .as_array()
        .context("Results is not an array")?
        .iter()
        .filter_map(|result| result["url"].as_str().map(String::from))
        .map(|url| {
            let model = model.clone();
            async move {
                fetch_and_process(url, model).await
            }
        })
        .collect();

    join_all(futures)
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()
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
    
    for result in &results {
        println!("URL: {}", result.url);
        println!("Number of chunks: {}", result.chunks.len());
        println!("Number of embeddings: {}", result.embeddings.len());
        println!("Embedding dimension: {}", result.embeddings[0].len());
        println!("---");
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