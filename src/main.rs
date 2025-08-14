#![allow(unused_imports)]
#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

use actix_web::{App, HttpResponse, HttpServer, Responder, get, web};

async fn health_check() -> impl Responder {
    let status = serde_json::json!({ "status": "OK" });
    HttpResponse::Ok().json(status)
}

async fn messages() -> impl Responder {
    let status = serde_json::json!({ "success": true });
    HttpResponse::Ok().json(status)
}

#[get("/hello/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    format!("Hello {name}!")
}

#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/health_check", web::get().to(health_check))
            .route("/messages", web::post().to(messages))
            .service(greet)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
