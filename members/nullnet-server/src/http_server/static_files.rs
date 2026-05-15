use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "ui/dist"]
struct UiAssets;

pub(super) async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
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
        None => match UiAssets::get("index.html") {
            Some(content) => {
                let mut res = Response::new(axum::body::Body::from(content.data));
                res.headers_mut()
                    .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
                res
            }
            None => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(axum::body::Body::empty())
                .unwrap(),
        },
    }
}
