use dotenvy::dotenv;
use eventpro::{config::AppConfig, metrics, routes::create_router, state::AppState};
use sqlx::mysql::MySqlPoolOptions;
use std::net::SocketAddr;
use std::time::Duration;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::MySqlStore;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Attempt to load .env from current dir or parent dir (Express root)
    if dotenv().is_err() {
        let _ = dotenvy::from_path("../.env");
    }

    // Initialize structured tracing
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "eventpro=info,tower_http=info,sqlx=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("========================================");
    tracing::info!("Starting EventPro Rust Backend");
    tracing::info!("========================================");

    // Load and validate application configuration
    let config = AppConfig::from_env()?;
    tracing::info!("Environment      : {}", config.app_env);
    tracing::info!("Listening target : {}:{}", config.host, config.port);
    tracing::info!("Public assets dir: {:?}", config.public_dir);
    tracing::info!("Database         : [REDACTED credentials]");

    // Connect to MySQL with connection pooling
    let pool_max: u32 = std::env::var("DB_POOL_MAX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    metrics::set_configured_pool_max(pool_max);
    tracing::info!("Database pool max: {}", pool_max);

    let pool = MySqlPoolOptions::new()
        .max_connections(pool_max)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to connect to MySQL database");
            e
        })?;

    tracing::info!("MySQL connection pool established successfully");

    // Configure persistent server-side session store in MySQL
    let session_store = MySqlStore::new(pool.clone());
    session_store.migrate().await.map_err(|e| {
        tracing::error!(
            error = %e,
            "Fatal startup error: failed to initialize MySQL session store schema"
        );
        e
    })?;
    tracing::info!("Session store migration executed / verified");

    let session_layer = SessionManagerLayer::new(session_store)
        .with_name("eventpro_sid")
        .with_secure(config.is_production())
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

    // Initialize shared state and router
    let state = AppState::new(pool, config.clone());
    let router = create_router(state, Some(session_layer));

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Rust service listening at http://{}", addr);

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("EventPro service shut down gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Shutdown signal received (Ctrl+C), terminating...");
        },
        _ = terminate => {
            tracing::info!("Shutdown signal received (SIGTERM), terminating...");
        },
    }
}
