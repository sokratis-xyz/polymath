use std::fmt::format;
use std::sync::Arc;

use actix_web::{web, App, HttpServer, Responder, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use futures::future::join_all;
use log::{debug, info, warn};

use reqwest::Client;
use readability::extractor;
use tokio::task;
use anyhow::{Result, Context};
use fastembed::{TextEmbedding, EmbeddingModel, InitOptions};

#[derive(Deserialize)]
struct SearchQuery {
    q: String
}

const CHUNK_SIZE: usize = 512;

#[derive(Clone, Serialize)]
struct ProcessedContent {
    url: String,
    chunks: Vec<String>,
    embeddings: Vec<Vec<f32>>,
    error: Option<String>,
}

struct AppState {
    model: Arc<TextEmbedding>,
}

async fn fetch_and_process(url: String, model: Arc<TextEmbedding>) -> Result<ProcessedContent> {
    let urlc = url.clone();

    let content = task::spawn_blocking(move || {
        extractor::scrape(&url)
            .map(|product| product.content)
            .unwrap_or_else(|_| String::from("Failed to scrape content"))
    }).await?;

    let chunks = chunk_content(&content);
    
    let chunksc = chunks.clone();

    let embeddings = task::spawn_blocking(move || {
        model.embed(chunks.clone(), None)
    }).await??;

    Ok(ProcessedContent { url: urlc, chunks: chunksc, embeddings: embeddings , error: None})
}

fn chunk_content(content: &str) -> Vec<String> {
    content.split_whitespace()
        .collect::<Vec<&str>>()
        .chunks(CHUNK_SIZE)
        .map(|chunk| chunk.join(" "))
        .collect()
}

// Update the process_search_results function to handle potential errors
async fn process_search_results(search_results: Value, model: Arc<TextEmbedding>) -> Result<Vec<ProcessedContent>> {
    let futures: Vec<_> = search_results["results"]
        .as_array()
        .context("Results is not an array")?
        .iter()
        .filter_map(|result| result["url"].as_str().map(String::from))
        .take(10)
        .map(|url| async {
            match fetch_and_process(url.clone(), Arc::clone(&model)).await {
                Ok(content) => content,
                Err(e) => ProcessedContent {
                    url,
                    chunks: vec![],
                    embeddings: vec![],
                    error: Some(format!("Error processing content: {}", e)),
                },
            }
        })
        .collect();

    Ok(join_all(futures).await)
}

async fn search_and_index(
    query: web::Query<SearchQuery>,
    data: web::Data<AppState>,
) -> actix_web::Result<HttpResponse> {
    let searxng_url = "http://37.27.27.0/search";
    let client = reqwest::Client::new();

    let search_results: Value = client
        .get(searxng_url)
        .query(&[("q", &query.q), ("format", &"json".to_string())])
        .send()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
        .json()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let results = process_search_results(search_results, Arc::clone(&data.model))
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    
    for result in &results {
        println!("URL: {}", result.url);
        println!("Number of chunks: {}", result.chunks.len());
        println!("Number of embeddings: {}", result.embeddings.len());
        //println!("Embedding dimension: {}", result.embeddings[0].len());
        println!("---");
    }
    
    Ok(HttpResponse::Ok().json({}))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let model = Arc::new(TextEmbedding::try_new(InitOptions {
        model_name: EmbeddingModel::AllMiniLML6V2,
        show_download_progress: true,
        ..Default::default()
    }).expect("Failed to create TextEmbedding model"));

    let app_state = web::Data::new(AppState { model });

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/search", web::get().to(search_and_index))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}