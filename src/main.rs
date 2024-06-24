use std::fmt::format;
use std::sync::Arc;
use std::sync::Mutex;

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

const CHUNK_SIZE: usize = 512;

#[derive(Clone)]
struct ProcessedContent {
    url: String,
    chunks: Vec<String>,
    embeddings: Vec<Vec<f32>>,
}

struct WorkerPool {
    model: SentenceEmbeddingsModel,
}

impl WorkerPool {
    fn new() -> Result<Self> {
        let model = SentenceEmbeddingsBuilder::remote(rust_bert::pipelines::sentence_embeddings::SentenceEmbeddingsModelType::AllMiniLmL12V2)
            .create_model()
            .context("Failed to create BERT model")?;

        Ok(WorkerPool { model })
    }

    fn process(&self, chunks: &[String]) -> Result<Vec<Vec<f32>>> {
        self.model.encode(chunks).context("Failed to generate embeddings")
    }
}

async fn fetch_and_process(url: String, worker_pool: web::Data<Arc<Mutex<WorkerPool>>>) -> Result<ProcessedContent> {
    let urlc = url.clone();

    let content = task::spawn_blocking(move || {
        extractor::scrape(&url)
            .map(|product| product.content)
            .context("Failed to scrape content")
    }).await??;

    let chunks = chunk_content(&content);
    let chunksc = chunks.clone();

    let embeddings = task::spawn_blocking(move || {
        worker_pool.lock().unwrap().process(&chunks)
    }).await??;

    Ok(ProcessedContent { url: urlc, chunks: chunksc, embeddings: embeddings })
}

fn chunk_content(content: &str) -> Vec<String> {
    content.split_whitespace()
        .collect::<Vec<&str>>()
        .chunks(CHUNK_SIZE)
        .map(|chunk| chunk.join(" "))
        .collect()
}

async fn process_search_results(search_results: Value, worker_pool: web::Data<Arc<Mutex<WorkerPool>>>) -> Result<Vec<ProcessedContent>> {
    let futures: Vec<_> = search_results["results"]
        .as_array()
        .context("Results is not an array")?
        .iter()
        .filter_map(|result| result["url"].as_str().map(String::from))
        .map(|url| fetch_and_process(url, worker_pool.clone()))
        .collect();

    join_all(futures)
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()
}

async fn search_and_index(
    query: web::Query<SearchQuery>,
    worker_pool: web::Data<Arc<Mutex<WorkerPool>>>,
) -> impl Responder {
    let searxng_url = "http://37.27.27.0/search";
    let client = reqwest::Client::new();

    let search_results: Value = client
        .get(searxng_url)
        .query(&[("q", &query.q), ("format", &"json".to_string())])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let results = process_search_results(search_results, worker_pool).await;
    
    for result in &results {
        println!("---");
    }
    
    web::Json({})
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let worker_pool = Arc::new(Mutex::new(WorkerPool::new().expect("Failed to create WorkerPool")));

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(Arc::clone(&worker_pool)))
            .route("/search", web::get().to(search_and_index))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}