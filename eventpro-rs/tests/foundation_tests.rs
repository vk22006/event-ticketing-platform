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
    let public_dir = eventpro::config::resolve_public_dir()
        .expect("resolve_public_dir should succeed in repository environment");

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://user:pass@localhost:3306/testdb".to_string(),
        session_secret: "super_secret_session_key_12345".to_string(),
        app_env: "development".to_string(),
        public_dir,
    };

    assert!(config.validate().is_ok());
    assert!(!config.is_production());
}

#[test]
fn test_config_validation_invalid_db_url() {
    let public_dir = eventpro::config::resolve_public_dir()
        .expect("resolve_public_dir should succeed in repository environment");

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "postgres://localhost/testdb".to_string(),
        session_secret: "super_secret_session_key_12345".to_string(),
        app_env: "development".to_string(),
        public_dir,
    };

    assert_eq!(
        config.validate().unwrap_err(),
        ConfigError::InvalidDatabaseUrl
    );
}

#[test]
fn test_config_validation_short_secret() {
    let public_dir = eventpro::config::resolve_public_dir()
        .expect("resolve_public_dir should succeed in repository environment");

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://localhost/testdb".to_string(),
        session_secret: "short".to_string(),
        app_env: "development".to_string(),
        public_dir,
    };

    assert_eq!(
        config.validate().unwrap_err(),
        ConfigError::SessionSecretTooShort
    );
}

#[test]
fn test_config_validation_invalid_public_dir() {
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://user:pass@localhost:3306/testdb".to_string(),
        session_secret: "super_secret_session_key_12345".to_string(),
        app_env: "development".to_string(),
        public_dir: PathBuf::from("nonexistent_directory_for_test_12345"),
    };

    match config.validate() {
        Err(ConfigError::InvalidPublicDir(_)) => (),
        other => panic!("Expected InvalidPublicDir, got {:?}", other),
    }
}

#[test]
fn test_config_debug_redaction() {
    let public_dir = eventpro::config::resolve_public_dir()
        .expect("resolve_public_dir should succeed in repository environment");

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://root:secretpass@localhost:3306/eventpro".to_string(),
        session_secret: "confidential_session_key_12345678".to_string(),
        app_env: "production".to_string(),
        public_dir,
    };

    let debug_str = format!("{:?}", config);
    assert!(!debug_str.contains("secretpass"));
    assert!(!debug_str.contains("confidential_session_key"));
    assert!(debug_str.contains("[REDACTED]"));
    assert!(config.is_production());
}

#[test]
fn test_database_url_encoding_special_characters() {
    use eventpro::config::build_database_url;

    // Test with URI-reserved characters in username and password:
    // '@', ':', '/', '#', '%'
    let user = "admin@org:ops";
    let password = "p@ss:w/o#r%d";
    let host = "127.0.0.1";
    let port = "3306";
    let db_name = "event_lifecycle";

    let url = build_database_url(user, password, host, port, db_name);

    assert_eq!(
        url,
        "mysql://admin%40org%3Aops:p%40ss%3Aw%2Fo%23r%25d@127.0.0.1:3306/event_lifecycle"
    );

    // Verify URI delimiter boundaries are intact
    assert!(url.starts_with("mysql://"));
    let authority = url
        .strip_prefix("mysql://")
        .unwrap()
        .split('@')
        .next()
        .unwrap();
    let parts: Vec<&str> = authority.split(':').collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], "admin%40org%3Aops");
    assert_eq!(parts[1], "p%40ss%3Aw%2Fo%23r%25d");
}

#[test]
fn test_database_url_empty_password() {
    use eventpro::config::build_database_url;

    let user = "root";
    let password = "";
    let host = "localhost";
    let port = "3306";
    let db_name = "event_lifecycle";

    let url = build_database_url(user, password, host, port, db_name);
    assert_eq!(url, "mysql://root@localhost:3306/event_lifecycle");
}

#[test]
fn test_public_dir_validation_and_resolution() {
    use eventpro::config::{resolve_public_dir, validate_public_dir};

    let resolved = resolve_public_dir()
        .expect("resolve_public_dir should resolve repository public directory");
    assert!(resolved.is_dir());
    assert!(validate_public_dir(&resolved));

    // A directory missing required subdirectories should fail validation
    let temp_dir = std::env::temp_dir();
    assert!(!validate_public_dir(&temp_dir));
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
    let public_dir = eventpro::config::resolve_public_dir()
        .expect("resolve_public_dir should succeed in repository environment");

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://root@localhost:3306/dummy".to_string(),
        session_secret: "test_secret_must_be_long_enough".to_string(),
        app_env: "test".to_string(),
        public_dir,
    };

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
async fn test_readiness_endpoint_failure_sanitization() {
    let public_dir = eventpro::config::resolve_public_dir()
        .expect("resolve_public_dir should succeed in repository environment");

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://dummy_user:dummy_pass@127.0.0.1:9999/dummy_db".to_string(),
        session_secret: "test_secret_must_be_long_enough".to_string(),
        app_env: "test".to_string(),
        public_dir,
    };

    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(500))
        .connect_lazy(&config.database_url)
        .expect("lazy pool creation failed");

    let state = AppState::new(pool, config);
    let app = create_router(state, None);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Must return 503 Service Unavailable when DB is disconnected
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "unhealthy");
    assert_eq!(json["database"], "disconnected");
    // Ensure raw error details / driver specifics are never leaked to clients
    assert!(json.get("error").is_none());
}

#[tokio::test]
async fn test_static_asset_serving() {
    let public_dir = eventpro::config::resolve_public_dir()
        .expect("resolve_public_dir should succeed in repository environment");

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://root@localhost:3306/dummy".to_string(),
        session_secret: "test_secret_must_be_long_enough".to_string(),
        app_env: "test".to_string(),
        public_dir,
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
