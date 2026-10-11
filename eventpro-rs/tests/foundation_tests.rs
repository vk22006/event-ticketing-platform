use axum::{
    body::to_bytes,
    http::{Request, StatusCode},
};
use eventpro::{
    config::{AppConfig, ConfigError},
    error::AppError,
    routes::create_router,
    state::AppState,
};
use serde_json::Value;
use std::path::PathBuf;
use tower::ServiceExt;

#[test]
fn test_config_validation_success() {
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://user:pass@localhost:3306/testdb".to_string(),
        session_secret: "super_secret_session_key_12345".to_string(),
        app_env: "development".to_string(),
        public_dir: PathBuf::from("../public"),
    };

    assert!(config.validate().is_ok());
    assert!(!config.is_production());
}

#[test]
fn test_config_validation_invalid_db_url() {
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "postgres://localhost/testdb".to_string(),
        session_secret: "super_secret_session_key_12345".to_string(),
        app_env: "development".to_string(),
        public_dir: PathBuf::from("../public"),
    };

    assert_eq!(
        config.validate().unwrap_err(),
        ConfigError::InvalidDatabaseUrl
    );
}

#[test]
fn test_config_validation_short_secret() {
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://localhost/testdb".to_string(),
        session_secret: "short".to_string(),
        app_env: "development".to_string(),
        public_dir: PathBuf::from("../public"),
    };

    assert_eq!(
        config.validate().unwrap_err(),
        ConfigError::SessionSecretTooShort
    );
}

#[test]
fn test_config_debug_redaction() {
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://root:secretpass@localhost:3306/eventpro".to_string(),
        session_secret: "confidential_session_key_12345678".to_string(),
        app_env: "production".to_string(),
        public_dir: PathBuf::from("../public"),
    };

    let debug_str = format!("{:?}", config);
    assert!(!debug_str.contains("secretpass"));
    assert!(!debug_str.contains("confidential_session_key"));
    assert!(debug_str.contains("[REDACTED]"));
    assert!(config.is_production());
}

#[test]
fn test_error_status_codes() {
    let err_not_found = AppError::NotFound("Event not found".into());
    assert_eq!(err_not_found.status_code(), StatusCode::NOT_FOUND);

    let err_forbidden = AppError::Forbidden("Access denied".into());
    assert_eq!(err_forbidden.status_code(), StatusCode::FORBIDDEN);

    let err_unauthorized = AppError::Unauthorized("Login required".into());
    assert_eq!(err_unauthorized.status_code(), StatusCode::UNAUTHORIZED);

    let err_bad_request = AppError::BadRequest("Invalid parameters".into());
    assert_eq!(err_bad_request.status_code(), StatusCode::BAD_REQUEST);

    let err_internal = AppError::Internal("Something failed".into());
    assert_eq!(
        err_internal.status_code(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn test_liveness_endpoint() {
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://root@localhost:3306/dummy".to_string(),
        session_secret: "test_secret_must_be_long_enough".to_string(),
        app_env: "test".to_string(),
        public_dir: PathBuf::from("../public"),
    };

    // Construct pool with dummy options without connecting for router test
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .connect_lazy(&config.database_url)
        .expect("lazy pool creation failed");

    let state = AppState::new(pool, config);
    let app = create_router(state, None);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "eventpro-rs");
    assert!(json["version"].is_string());
}

#[tokio::test]
async fn test_static_asset_serving() {
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://root@localhost:3306/dummy".to_string(),
        session_secret: "test_secret_must_be_long_enough".to_string(),
        app_env: "test".to_string(),
        public_dir: PathBuf::from("../public"),
    };

    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .connect_lazy(&config.database_url)
        .expect("lazy pool creation failed");

    let state = AppState::new(pool, config);
    let app = create_router(state, None);

    // Request CSS asset that exists in public/css/main.css
    let response = app
        .oneshot(
            Request::builder()
                .uri("/css/main.css")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
