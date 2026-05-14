use super::AppState;
use crate::graphviz::render_graph_json;
use axum::extract::State;
use axum::response::IntoResponse;

pub(super) async fn graph_handler(State(state): State<AppState>) -> impl IntoResponse {
    let services = state.services.read().await;
    axum::Json(render_graph_json(&services))
}
