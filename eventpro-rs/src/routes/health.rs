use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

pub async fn liveness() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "service": "eventpro-rs",
            "version": env!("CARGO_PKG_VERSION")
        })),
    )
}

pub async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({
                "status": "ready",
                "database": "connected"
            })),
        ),
        Err(err) => {
            tracing::error!(error = %err, "Readiness probe database check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "status": "unhealthy",
                    "database": "disconnected",
                    "error": err.to_string()
                })),
            )
        }
    }
}
