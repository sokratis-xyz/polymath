use std::sync::Arc;

use actix_web::{web, App, HttpServer, Responder, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use futures::future::join_all;
use log::{debug, info, warn};

use anyhow::{Result, Context};
use std::collections::HashMap;
use usearch::{Index, IndexOptions, MetricKind, ScalarKind, new_index};
use futures::lock::Mutex;

#[derive(Deserialize)]
struct SearchQuery {
    q: String
}

#[derive(Debug, Deserialize)]
struct ProcessedContent {
    url: String,
    chunks: HashMap<String, String>,
    embeddings: HashMap<String, Vec<f32>>,
}

async fn process_search_results(search_results: Value, index: &mut Index) -> Result<HashMap<u64, (String, String)>> {
    let client = reqwest::Client::new();
    let chunk_map: Arc<Mutex<HashMap<u64, (String, String)>>> = Arc::new(Mutex::new(HashMap::new()));
    let chunk_counter: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));

    let futures: Vec<_> = search_results["results"]
        .as_array()
        .context("Results is not an array")?
        .iter()
        .filter_map(|result| result["url"].as_str().map(String::from))
        .take(10)
        .map(|url| {
            let chunk_map = Arc::clone(&chunk_map);
            let chunk_counter = Arc::clone(&chunk_counter);
            let index = index;

            async move {
                let params = json!({
                    "config": {
                        "chunking_size": 100,
                        "chunking_type": "words",
                        "embedding_model": "AllMiniLML6V2"
                    },
                    "url": url
                });

                let response = client
                    .post("https://localhost:8081/v1/process")
                    .json(&params)
                    .send()
                    .await?;

                let processed_content: ProcessedContent = response.json().await?;

               // Add each chunk and its corresponding embedding to the index
            for (chunk_id, chunk_text) in &processed_content.chunks {
                let embedding = processed_content.embeddings.get(chunk_id).context("Embedding not found")?;
                let mut chunk_counter = chunk_counter.lock().await;
                let key: u64 = *chunk_counter;
                index.add(key, embedding).context("Failed to add to index")?;
                chunk_map.lock().await.insert(key, (processed_content.url.clone(), chunk_text.clone()));
                *chunk_counter += 1;
            }
                Ok::<(), anyhow::Error>(())
            }
        })
        .collect();

    join_all(futures).await;
    Ok(chunk_map.lock().await.clone())
}

async fn search_and_index(
    query: web::Query<SearchQuery>,
) -> actix_web::Result<HttpResponse> {
    let searxng_url = "http://37.27.27.0/search";
    let client = reqwest::Client::new();

    let options = IndexOptions {
        dimensions: 384, // Set the dimensions to match the embedding size
        metric: MetricKind::IP,
        quantization: ScalarKind::F32,
        connectivity: 0,
        expansion_add: 0,
        expansion_search: 0,
        multi: false,
    };
    
    let mut index: Index = new_index(&options).unwrap();

    let search_results: Value = client
        .get(searxng_url)
        .query(&[("q", &query.q), ("format", &"json".to_string())])
        .send()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
        .json()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let chunk_map = process_search_results(search_results, &mut index)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    
    Ok(HttpResponse::Ok().json(chunk_map))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(move || {
        App::new()
            .route("/search", web::get().to(search_and_index))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}