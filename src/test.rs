use actix_web::{web, App, HttpServer, Responder};
use reqwest;
use serde::{Deserialize, Serialize};
use tantivy::{schema::*, Index, ReloadPolicy};
use futures::{stream, StreamExt};
use anyhow::Result;
use redis::AsyncCommands;
use readability::extractor;
use sha2::{Sha256, Digest};
use std::time::Duration;

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    k: usize,
}

#[derive(Serialize, Deserialize)]
struct SearchResult {
    url: String,
    title: String,
}

#[derive(Serialize)]
struct ContentPart {
    content: String,
    url: String,
}

async fn get_cached_or_fetch(client: &reqwest::Client, redis_client: &redis::Client, url: &str) -> Result<String> {
    let mut con = redis_client.get_async_connection().await?;
    let cache_key = format!("content:{}", url);

    if let Ok(cached_content) = con.get::<_, String>(&cache_key).await {
        return Ok(cached_content);
    }

    let content = client.get(url).send().await?.text().await?;
    let _: () = con.set_ex(&cache_key, &content, 3600).await?; // Cache for 1 hour

    Ok(content)
}

fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

async fn search_and_index(
    query: web::Query<SearchQuery>,
    redis_client: web::Data<redis::Client>,
) -> impl Responder {
    let searxng_url = "http://37.27.27.0/search";
    let client = reqwest::Client::new();
    let search_results: Vec<SearchResult> = client
        .get(searxng_url)
        .query(&[("q", &query.q), ("format", &"json".to_string())])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let contents = stream::iter(search_results.iter())
        .map(|result| {
            let client = client.clone();
            let redis_client = redis_client.clone();
            tokio::task::spawn_blocking(move || {
                let product = extractor::scrape(&result.url).unwrap();
                //let html = get_cached_or_fetch(&client, &redis_client, &result.url).await?;
                let clean_content = product.content;
                Ok::<_, anyhow::Error>((result.url.clone(), clean_content))
            }).await
        })
        .buffer_unordered(10)
        .collect::<Vec<Result<(String, String), _>>>()
        .await;

    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("content", TEXT | STORED);
    schema_builder.add_text_field("url", STORED);
    let schema = schema_builder.build();

    let index = Index::create_in_ram(schema.clone());
    let mut index_writer = index.writer(50_000_000).unwrap();

    for result in contents.into_iter().filter_map(Result::ok) {
        let (url, content) = result;
        let content_hash = hash_content(&content);
        
        let mut redis_con = redis_client.get_async_connection().await.unwrap();
        let cache_key = format!("chunks:{}", content_hash);
        
        let chunks: Vec<String> = if let Ok(cached_chunks) = redis_con.get::<_, String>(&cache_key).await {
            serde_json::from_str(&cached_chunks).unwrap()
        } else {
            let new_chunks: Vec<String> = content
                .split('.')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string())
                .collect();
            
            let _: () = redis_con.set_ex(&cache_key, serde_json::to_string(&new_chunks).unwrap(), 3600).await.unwrap();
            new_chunks
        };

        for chunk in chunks {
            let mut doc = Document::new();
            doc.add_text(schema.get_field("content").unwrap(), &chunk);
            doc.add_text(schema.get_field("url").unwrap(), &url);
            index_writer.add_document(doc).unwrap();
        }
    }
    index_writer.commit().unwrap();

    let reader = index.reader().unwrap();
    let searcher = reader.searcher();
    let query_parser = tantivy::query::QueryParser::for_index(&index, vec![schema.get_field("content").unwrap()]);
    let query = query_parser.parse_query(&query.q).unwrap();
    let top_docs = searcher.search(&query, &tantivy::collector::TopDocs::with_limit(query.k)).unwrap();

    let content_parts: Vec<ContentPart> = top_docs
        .iter()
        .map(|(_, doc_address)| {
            let retrieved_doc = searcher.doc(*doc_address).unwrap();
            ContentPart {
                content: retrieved_doc.get_first(schema.get_field("content").unwrap()).unwrap().as_text().unwrap().to_string(),
                url: retrieved_doc.get_first(schema.get_field("url").unwrap()).unwrap().as_text().unwrap().to_string(),
            }
        })
        .collect();

    web::Json(content_parts)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let redis_client = redis::Client::open("redis://127.0.0.1/").unwrap();
    
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(redis_client.clone()))
            .route("/search", web::get().to(search_and_index))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}