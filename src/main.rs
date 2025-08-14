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
            } else {
                return HttpResponse::BadRequest()
                    .json(serde_json::json!({ "error": "Malformed JSON in payload" }));
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

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test};

    #[actix_web::test]
    async fn test_health_check_endpoint() {
        let app =
            test::init_service(App::new().route("/health_check", web::get().to(health_check)))
                .await;

        let req = test::TestRequest::get().uri("/health_check").to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "OK");
    }

    #[actix_web::test]
    async fn test_messages_endpoint_happy_path() {
        let app = test::init_service(App::new().route("/messages", web::post().to(messages))).await;

        let test_message = r#"{"trackingNumber":"1234567890","status":"OK","carrier":"FedEx","expectedDeliveryDate":"2023-12-31"}"#;

        let req = test::TestRequest::post()
            .uri("/messages")
            .set_payload(test_message)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], true);
    }

    #[actix_web::test]
    async fn test_messages_endpoint_success_with_multiple_messages() {
        let app = test::init_service(App::new().route("/messages", web::post().to(messages))).await;

        let test_messages = r#"{"trackingNumber":"1234567890","status":"OK","carrier":"FedEx","expectedDeliveryDate":"2023-12-31"}
        {"trackingNumber":"0987654321","status":"In Transit","carrier":"UPS","expectedDeliveryDate":"2024-01-15"}
        {"trackingNumber":"1122334455","status":"Delivered","carrier":"USPS","expectedDeliveryDate":"2023-12-28"}"#;

        let req = test::TestRequest::post()
            .uri("/messages")
            .set_payload(test_messages)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], true);
    }

    #[actix_web::test]
    async fn test_messages_endpoint_with_badly_formatted_message() {
        let app = test::init_service(App::new().route("/messages", web::post().to(messages))).await;

        let test_message = r#"{"trackingNumbr":"1234567890""status":"OK","carrier":"FedEx","expectedDeliveryDate":"2023-12-31"#;

        let req = test::TestRequest::post()
            .uri("/messages")
            .set_payload(test_message)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().as_u16() == 400);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["error"], "Malformed JSON in payload");
    }
}
