#![allow(unused_imports)]
#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

use actix_web::web::Bytes;
use actix_web::{App, HttpResponse, HttpServer, Responder, get, web};
use futures::StreamExt;

async fn health_check() -> impl Responder {
    let status = serde_json::json!({ "status": "OK" });
    HttpResponse::Ok().json(status)
}

/*  message schema:
 {
  "trackingNumber": "1234567890",
  "status": "OK",
  "carrier": "FedEx",
  "expectedDeliveryDate": "2023-12-31"
}
*/

use actix_web::web::Json;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;

#[derive(Deserialize, Serialize)]
struct Message {
    #[serde(rename = "trackingNumber")]
    tracking_number: String,
    status: String,
    carrier: String,
    #[serde(rename = "expectedDeliveryDate")]
    expected_delivery_date: String,
}

// Disable clippy::future_not_send warning
// actix-web handlers are not Send because they run as a tokio single-threaded async runtime
#[allow(clippy::future_not_send)]
async fn messages(mut payload: web::Payload) -> impl Responder {
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("messages.txt")
    else {
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({ "error": "Failed to open file" }));
    };

    while let Some(chunk) = payload.next().await {
        let Ok(chunk) = chunk else { continue };

        let data = String::from_utf8_lossy(&chunk);
        for line in data.lines() {
            if let Ok(message) = serde_json::from_str::<Message>(line) {
                let formatted_message = format!(
                    "Tracking: {}, Status: {}, Carrier: {}, Expected Delivery: {}\n",
                    message.tracking_number,
                    message.status,
                    message.carrier,
                    message.expected_delivery_date
                );

                if file.write_all(formatted_message.as_bytes()).is_err() {
                    return HttpResponse::InternalServerError()
                        .json(serde_json::json!({ "error": "Failed to write to file" }));
                }
            }
        }
    }

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
