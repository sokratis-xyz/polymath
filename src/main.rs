use std::sync::Arc;

use actix_web::{web, App, HttpServer, Responder, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use futures::future::join_all;
use log::{debug, info, warn};

use anyhow::{Result, Context};
use std::collections::HashMap;
use usearch::{Index, IndexOptions, MetricKind, ScalarKind, new_index};
use std::sync::Mutex;

use schemars::JsonSchema;
use actix_web::middleware::Logger;

use apistos::{api_operation, ApiComponent};
use apistos::app::{BuildConfig, OpenApiWrapper};
use apistos::info::Info;
use apistos::server::Server;
use apistos::spec::Spec;
use apistos::web::{get, post, resource, scope};
use apistos::{RapidocConfig, RedocConfig, ScalarConfig, SwaggerUIConfig};

#[derive(Deserialize,JsonSchema, ApiComponent)]
struct SearchQuery {
    q: String
}

#[derive(Debug, Deserialize)]
struct ProcessedContent {
    url: String,
    chunks: HashMap<String, String>,
    embeddings: HashMap<String, Vec<f32>>,
}
async fn process_search_results(search_results: Value, index: Arc<Mutex<Index>>) -> Result<HashMap<u64, (String, String)>> {

    let chunk_map: Arc<Mutex<HashMap<u64, (String, String)>>> = Arc::new(Mutex::new(HashMap::new()));
    let chunk_counter: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));

    let futures: Vec<_> = search_results["results"]
        .as_array()
        .context("Results is not an array")?
        .iter()
        .filter_map(|result| result["url"].as_str().map(String::from))
        .take(10)
        .map(|url| {
            let client: reqwest::Client = reqwest::Client::new();

            let chunk_map = Arc::clone(&chunk_map);
            let chunk_counter = Arc::clone(&chunk_counter);
            let index = Arc::clone(&index);

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
                    let mut chunk_counter = chunk_counter.lock().unwrap();
                    let key: u64 = *chunk_counter;
                    index.lock().unwrap().add(key, embedding).context("Failed to add to index")?;
                    chunk_map.lock().unwrap().insert(key, (processed_content.url.clone(), chunk_text.clone()));
                    *chunk_counter += 1;
                }
                Ok::<(), anyhow::Error>(())
            }
        })
        .collect();

    join_all(futures).await;
    let chunk_map_clone = chunk_map.lock().unwrap().clone();
    Ok(chunk_map_clone)
}

#[api_operation(summary = "Process a query and return processed content")]
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
    
    let index: Arc<Mutex<Index>> = Arc::new(Mutex::new(new_index(&options).unwrap()));

    debug!("Search results is being called");

    let search_results: Value = client
        .get(searxng_url)
        .query(&[("q", &query.q), ("format", &"json".to_string())])
        .send()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
        .json()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    
    debug!("Search results are in");

    let chunk_map = process_search_results(search_results, Arc::clone(&index))
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    
    debug!("Processed search results");

    Ok(HttpResponse::Ok().json(chunk_map))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(move || {

        let spec = Spec {
            info: Info {
              title: "Polymath API".to_string(),
              description: Some(
                "This is the polymath API".to_string(),
              ),
              ..Default::default()
            },
            servers: vec![Server {
              url: "/".to_string(),
              ..Default::default()
            }],
            ..Default::default()
          };
        
      App::new()
          .document(spec)
          .wrap(Logger::default())
          .service(scope("/v1")
              .service(resource("/search").route(get().to(search_and_index)))
      )
      .build_with(
          "/openapi.json",
          BuildConfig::default()
            .with(RapidocConfig::new(&"/rapidoc"))
            .with(RedocConfig::new(&"/redoc"))
            .with(ScalarConfig::new(&"/scalar"))
            .with(SwaggerUIConfig::new(&"/swagger")),
        )

    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}