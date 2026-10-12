pub mod auth;
pub mod events;
pub mod health;

use crate::state::AppState;
use axum::{routing::get, Router};
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tower_sessions::{SessionManagerLayer, SessionStore};
use tower_sessions_sqlx_store::MySqlStore;
use tracing::Level;

pub fn create_router(
    state: AppState,
    session_layer: Option<SessionManagerLayer<MySqlStore>>,
) -> Router {
    create_router_with_store(state, session_layer)
}

pub fn create_router_with_store<Store>(
    state: AppState,
    session_layer: Option<SessionManagerLayer<Store>>,
) -> Router
where
    Store: SessionStore + Clone + Send + Sync + 'static,
{
    let public_dir = &state.config.public_dir;

    let auth_routes = Router::new()
        .route("/login", get(auth::show_login).post(auth::login))
        .route("/register", get(auth::show_register).post(auth::register))
        .route("/logout", get(auth::logout));

    let mut router = Router::new()
        .route("/", get(events::root_redirect))
        .route("/events", get(events::list_events))
        .route("/events/:id", get(events::show_event))
        .route("/health", get(health::liveness))
        .route("/ready", get(health::readiness))
        .nest("/auth", auth_routes)
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
