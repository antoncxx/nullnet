use axum::http::{StatusCode, header};
use axum::response::Response;

pub(super) async fn config_handler() -> Response {
    match tokio::fs::read_to_string("./services/services.toml").await {
        Ok(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(axum::body::Body::from(content))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::empty())
            .unwrap(),
    }
}
