use axum::response::IntoResponse;

pub(super) async fn health() -> impl IntoResponse {
    axum::Json(serde_json::json!({ "status": "ok" }))
}
