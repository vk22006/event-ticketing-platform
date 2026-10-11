pub mod health;

use crate::state::AppState;
use axum::{routing::get, Router};
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tower_sessions::SessionManagerLayer;
use tower_sessions_sqlx_store::MySqlStore;
use tracing::Level;

pub fn create_router(
    state: AppState,
    session_layer: Option<SessionManagerLayer<MySqlStore>>,
) -> Router {
    let public_dir = &state.config.public_dir;

    let mut router = Router::new()
        .route("/health", get(health::liveness))
        .route("/ready", get(health::readiness))
        .nest_service("/css", ServeDir::new(public_dir.join("css")))
        .nest_service("/js", ServeDir::new(public_dir.join("js")))
        .nest_service("/uploads", ServeDir::new(public_dir.join("uploads")))
        .with_state(state);

    if let Some(layer) = session_layer {
        router = router.layer(layer);
    }

    router.layer(
        TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
            .on_response(DefaultOnResponse::new().level(Level::INFO)),
    )
}
