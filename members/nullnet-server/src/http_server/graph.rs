use super::AppState;
use crate::graphviz::render_graphviz;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::Response;

pub(super) async fn graph_handler(State(state): State<AppState>) -> Response {
    let services = state.services.read().await;
    let dot = render_graphviz(&services);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(axum::body::Body::from(dot))
        .unwrap()
}
