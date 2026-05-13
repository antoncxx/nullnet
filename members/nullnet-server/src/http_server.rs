use axum::Router;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::RustEmbed;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

const HTTP_PORT: u16 = 8080;

#[derive(RustEmbed)]
#[folder = "ui/dist"]
struct UiAssets;

pub async fn serve() {
    let app = Router::new()
        .route("/api/health", get(health))
        .fallback(get(static_handler));

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), HTTP_PORT);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> impl IntoResponse {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match UiAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let mut res = Response::new(axum::body::Body::from(content.data));
            res.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime.as_ref()).unwrap(),
            );
            res
        }
        None => {
            // SPA fallback: serve index.html for any unmatched route
            match UiAssets::get("index.html") {
                Some(content) => {
                    let mut res = Response::new(axum::body::Body::from(content.data));
                    res.headers_mut().insert(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("text/html"),
                    );
                    res
                }
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            }
        }
    }
}
