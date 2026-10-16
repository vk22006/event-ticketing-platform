mod common;

use axum::http::StatusCode;
use common::{create_test_state, get_response};
use eventpro::{
    config::{resolve_public_dir, validate_public_dir, AppConfig, ConfigError},
    error::AppError,
    routes::create_router,
};
use serde_json::Value;
use std::path::PathBuf;

fn valid_config() -> AppConfig {
    let public_dir = resolve_public_dir().expect("resolve_public_dir should succeed");
    AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://user:pass@localhost:3306/testdb".to_string(),
        session_secret: "super_secret_session_key_12345".to_string(),
        app_env: "development".to_string(),
        upload_dir: public_dir.join("uploads"),
        public_dir,
    }
}

#[test]
fn test_config_validation_success() {
    let config = valid_config();
    assert!(config.validate().is_ok());
    assert!(!config.is_production());
}

#[test]
fn test_config_validation_invalid_db_url() {
    let mut config = valid_config();
    config.database_url = "postgres://localhost/testdb".to_string();
    assert_eq!(
        config.validate().unwrap_err(),
        ConfigError::InvalidDatabaseUrl
    );
}

#[test]
fn test_config_validation_short_secret() {
    let mut config = valid_config();
    config.session_secret = "short".to_string();
    assert_eq!(
        config.validate().unwrap_err(),
        ConfigError::SessionSecretTooShort
    );
}

#[test]
fn test_config_validation_invalid_public_dir() {
    let mut config = valid_config();
    config.public_dir = PathBuf::from("nonexistent_directory_for_test_12345");
    match config.validate() {
        Err(ConfigError::InvalidPublicDir(_)) => (),
        other => panic!("Expected InvalidPublicDir, got {:?}", other),
    }
}

#[test]
fn test_config_debug_redaction() {
    let mut config = valid_config();
    config.database_url = "mysql://root:secretpass@localhost:3306/eventpro".to_string();
    config.session_secret = "confidential_session_key_12345678".to_string();
    config.app_env = "production".to_string();

    let debug_str = format!("{:?}", config);
    assert!(!debug_str.contains("secretpass"));
    assert!(!debug_str.contains("confidential_session_key"));
    assert!(debug_str.contains("[REDACTED]"));
    assert!(config.is_production());
}

#[test]
fn test_database_url_encoding_special_characters() {
    use eventpro::config::build_database_url;

    let url = build_database_url(
        "admin@org:ops",
        "p@ss:w/o#r%d",
        "127.0.0.1",
        "3306",
        "event_lifecycle",
    );
    assert_eq!(
        url,
        "mysql://admin%40org%3Aops:p%40ss%3Aw%2Fo%23r%25d@127.0.0.1:3306/event_lifecycle"
    );
    assert!(url.starts_with("mysql://"));
}

#[test]
fn test_database_url_empty_password() {
    use eventpro::config::build_database_url;
    let url = build_database_url("root", "", "localhost", "3306", "event_lifecycle");
    assert_eq!(url, "mysql://root@localhost:3306/event_lifecycle");
}

#[test]
fn test_public_dir_validation_and_resolution() {
    let resolved = resolve_public_dir().expect("resolve_public_dir should succeed");
    assert!(resolved.is_dir());
    assert!(validate_public_dir(&resolved));
    assert!(!validate_public_dir(&std::env::temp_dir()));
}

#[test]
fn test_error_status_codes() {
    assert_eq!(
        AppError::NotFound("n".into()).status_code(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        AppError::Forbidden("f".into()).status_code(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        AppError::Unauthorized("u".into()).status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        AppError::BadRequest("b".into()).status_code(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        AppError::Internal("i".into()).status_code(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn test_liveness_endpoint() {
    let app = create_router(create_test_state(), None);
    let (status, _, body) = get_response(&app, "/health", None).await;
    assert_eq!(status, StatusCode::OK);

    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "eventpro-rs");
    assert!(json["version"].is_string());
}

#[tokio::test]
async fn test_readiness_endpoint_failure_sanitization() {
    let app = create_router(create_test_state(), None);
    let (status, _, body) = get_response(&app, "/ready", None).await;

    // Must return 503 Service Unavailable when DB is disconnected
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["status"], "unhealthy");
    assert_eq!(json["database"], "disconnected");
    // Ensure raw error details / driver specifics are never leaked to clients
    assert!(json.get("error").is_none());
}

#[tokio::test]
async fn test_static_asset_serving() {
    let app = create_router(create_test_state(), None);
    let (status, _, _) = get_response(&app, "/css/main.css", None).await;
    assert_eq!(status, StatusCode::OK);
}
