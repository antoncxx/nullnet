use super::AppState;
use crate::services::input::{detect_port_conflicts, validate_stack_toml};
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// A stack name must be a single bare identifier — no path separators or dots —
/// so it maps to exactly `./services/<stack>.toml` with no traversal.
fn valid_stack_name(stack: &str) -> bool {
    !stack.is_empty() && !stack.contains(['/', '\\', '.'])
}

/// GET the raw TOML of a stack's service configuration.
pub(super) async fn config_handler(Path(stack): Path<String>) -> Response {
    if !valid_stack_name(&stack) {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(axum::body::Body::empty())
            .unwrap();
    }
    let path = format!("./services/{stack}.toml");
    match tokio::fs::read_to_string(&path).await {
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

#[derive(Serialize)]
struct SaveResult {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn rejected(status: StatusCode, error: impl Into<String>) -> Response {
    (
        status,
        axum::Json(SaveResult {
            ok: false,
            error: Some(error.into()),
        }),
    )
        .into_response()
}

/// POST a new raw TOML for a stack. The body is validated the same way the loader
/// validates on reload (syntax + semantic rules + cross-stack port conflicts). On
/// success the file is written and the existing `./services` watcher hot-reloads
/// it live; on failure nothing is written, so the last valid config keeps running
/// and the response carries the parse error for the UI's status indicator.
pub(super) async fn save_handler(
    Path(stack): Path<String>,
    State(state): State<AppState>,
    body: String,
) -> Response {
    if !valid_stack_name(&stack) {
        return rejected(StatusCode::BAD_REQUEST, "invalid stack name");
    }

    // 1. Syntax + semantic validation (mirrors the loader).
    let parsed = match validate_stack_toml(&body) {
        Ok(map) => map,
        Err(e) => return rejected(StatusCode::UNPROCESSABLE_ENTITY, e),
    };

    // 2. Cross-stack port conflicts: check against the live set with this stack
    //    swapped in, so an edit can't collide with a listen_port owned elsewhere.
    let mut candidate = state.services.read().await.clone();
    candidate.insert(stack.clone(), parsed);
    if let Some(c) = detect_port_conflicts(&candidate)
        .into_iter()
        .find(|c| c.stack_a == stack || c.stack_b == stack)
    {
        let (other_stack, other_service) = if c.stack_a == stack {
            (c.stack_b, c.service_b)
        } else {
            (c.stack_a, c.service_a)
        };
        return rejected(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "listen_port {} ({:?}) already used by service '{other_service}' in stack '{other_stack}'",
                c.listen_port, c.protocol
            ),
        );
    }

    // 3. Valid → persist. The services watcher picks up the write and applies it.
    let path = format!("./services/{stack}.toml");
    if tokio::fs::write(&path, body).await.is_err() {
        return rejected(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to write configuration file",
        );
    }
    axum::Json(SaveResult {
        ok: true,
        error: None,
    })
    .into_response()
}
