#![allow(dead_code)]

use axum::{
    body::to_bytes,
    extract::Request,
    http::{header, StatusCode},
    Router,
};
use eventpro::{
    config::{build_database_url, resolve_public_dir, AppConfig},
    models::{Event, User},
    routes::create_router_with_store,
    state::AppState,
};
use rust_decimal::Decimal;
use sqlx::MySqlPool;
use std::path::PathBuf;
use std::time::Duration;
use tower::ServiceExt;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

/// Returns true if strict MySQL integration test mode is enabled.
pub fn is_strict_mode() -> bool {
    std::env::var("STRICT_DB_TESTS")
        .or_else(|_| std::env::var("STRICT_TEST"))
        .or_else(|_| std::env::var("STRICT_MYSQL"))
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Resolves the test database URL and connects to MySQL with strict-mode enforcement.
pub async fn get_test_pool() -> Option<MySqlPool> {
    let _ = dotenvy::dotenv();
    let _ = dotenvy::from_path("../.env");

    let strict = is_strict_mode();

    if let Ok(raw) = std::env::var("DATABASE_URL") {
        if raw.trim().is_empty() {
            if strict {
                panic!("STRICT MODE ERROR: DATABASE_URL is set but empty.");
            } else {
                println!(
                    "SKIPPED: Real MySQL integration tests skipped because DATABASE_URL is empty."
                );
                return None;
            }
        }
    }

    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => match (
            std::env::var("DB_USER"),
            std::env::var("DB_PASSWORD"),
            std::env::var("DB_HOST"),
            std::env::var("DB_PORT"),
            std::env::var("DB_NAME"),
        ) {
            (Ok(u), Ok(p), Ok(h), Ok(port), Ok(d)) => build_database_url(&u, &p, &h, &port, &d),
            _ => {
                if strict {
                    panic!(
                        "STRICT MODE ERROR: Required MySQL integration tests cannot execute: DATABASE_URL is missing, and fallback DB_* variables are not set."
                    );
                } else {
                    println!(
                        "SKIPPED: Real MySQL integration tests skipped because DATABASE_URL / DB_* environment variables are not set."
                    );
                    return None;
                }
            }
        },
    };

    if !db_url.starts_with("mysql://") {
        if strict {
            panic!("STRICT MODE ERROR: DATABASE_URL is invalid (must start with 'mysql://').");
        } else {
            println!(
                "SKIPPED: Real MySQL integration tests skipped because DATABASE_URL is invalid."
            );
            return None;
        }
    }

    match sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&db_url)
        .await
    {
        Ok(pool) => {
            println!("REAL MYSQL DETECTED: Connected to MySQL test database successfully.");
            Some(pool)
        }
        Err(err) => {
            if strict {
                panic!(
                    "STRICT MODE ERROR: Failed to connect to MySQL database: {}",
                    err
                );
            } else {
                println!(
                    "SKIPPED: Real MySQL integration tests skipped because database is unreachable: {}",
                    err
                );
                None
            }
        }
    }
}

/// Creates a standard in-memory test `AppState` with lazy dummy pool for unit/template tests.
pub fn create_test_state() -> AppState {
    let public_dir = resolve_public_dir().expect("public directory should resolve");
    let upload_dir = public_dir.join("uploads");
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://dummy_user:dummy_pass@127.0.0.1:9999/dummy_db".to_string(),
        session_secret: "test_secret_must_be_at_least_16_chars".to_string(),
        app_env: "test".to_string(),
        public_dir,
        upload_dir,
    };

    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .acquire_timeout(Duration::from_millis(100))
        .connect_lazy(&config.database_url)
        .expect("lazy pool creation failed");

    AppState::new(pool, config)
}

/// Creates an `AppState` with a provided real MySQL pool.
pub fn create_test_state_with_pool(pool: &MySqlPool) -> AppState {
    let public_dir = resolve_public_dir().expect("public directory should resolve");
    let upload_dir = public_dir.join("uploads");
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://ignored".to_string(),
        session_secret: "test_secret_must_be_at_least_16_chars".to_string(),
        app_env: "test".to_string(),
        public_dir,
        upload_dir,
    };
    AppState::new(pool.clone(), config)
}

/// Creates a test `Router` with an in-memory session store.
pub fn create_test_app(state: AppState) -> Router {
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_name("eventpro_sid")
        .with_secure(false)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

    create_router_with_store(state, Some(session_layer))
}

/// Creates a test `Router` connected to a real MySQL pool with an isolated temporary upload directory.
pub fn create_test_app_with_pool(pool: &MySqlPool) -> (Router, TestUploadDir) {
    let temp_upload = TestUploadDir::new();
    let public_dir = resolve_public_dir().expect("public directory should resolve");
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://ignored".to_string(),
        session_secret: "test_secret_must_be_at_least_16_chars".to_string(),
        app_env: "test".to_string(),
        public_dir,
        upload_dir: temp_upload.path().clone(),
    };
    let state = AppState::new(pool.clone(), config);
    (create_test_app(state), temp_upload)
}

/// Percent-encodes a string for URL form encoded bodies.
pub fn urlencode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// Extracts the `eventpro_sid` cookie from HTTP response headers.
pub fn extract_cookie(headers: &header::HeaderMap) -> Option<String> {
    headers.get_all(header::SET_COOKIE).iter().find_map(|v| {
        let s = v.to_str().ok()?;
        if s.starts_with("eventpro_sid=") {
            Some(s.split(';').next().unwrap().to_string())
        } else {
            None
        }
    })
}

/// Extracts a CSRF token value from an HTML input tag.
pub fn extract_csrf_token(html: &str) -> Option<String> {
    let marker = "name=\"csrf_token\" value=\"";
    let start = html.find(marker)? + marker.len();
    let end = html[start..].find('"')? + start;
    Some(html[start..end].to_string())
}

/// Performs a complete login flow over the Axum oneshot service, returning `(session_cookie, dashboard_csrf)`.
pub async fn http_login(app: &Router, email: &str, password: &str) -> (String, String) {
    let login_page_req = Request::builder()
        .uri("/auth/login")
        .body(axum::body::Body::empty())
        .unwrap();
    let login_page_resp = app.clone().oneshot(login_page_req).await.unwrap();
    let cookie_1 = extract_cookie(login_page_resp.headers())
        .expect("Must receive session cookie on login page");
    let body = to_bytes(login_page_resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let csrf_1 =
        extract_csrf_token(&String::from_utf8_lossy(&body)).expect("Must extract login CSRF token");

    let form_body = format!(
        "email={}&password={}&csrf_token={}",
        urlencode(email),
        urlencode(password),
        urlencode(&csrf_1)
    );
    let login_req = Request::builder()
        .method("POST")
        .uri("/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &cookie_1)
        .body(axum::body::Body::from(form_body))
        .unwrap();

    let login_resp = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(
        login_resp.status(),
        StatusCode::SEE_OTHER,
        "Login must redirect to /dashboard"
    );
    let session_cookie = extract_cookie(login_resp.headers()).unwrap_or(cookie_1);

    let dash_req = Request::builder()
        .uri("/dashboard")
        .header(header::COOKIE, &session_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let dash_resp = app.clone().oneshot(dash_req).await.unwrap();
    assert_eq!(dash_resp.status(), StatusCode::OK);
    let dash_body = to_bytes(dash_resp.into_body(), 128 * 1024).await.unwrap();
    let dash_csrf = extract_csrf_token(&String::from_utf8_lossy(&dash_body)).unwrap_or(csrf_1);

    (session_cookie, dash_csrf)
}

/// Dispatches an in-memory GET request to the test Router and returns `(StatusCode, HeaderMap, String body)`.
pub async fn get_response(
    app: &Router,
    uri: &str,
    cookie: Option<&str>,
) -> (StatusCode, header::HeaderMap, String) {
    let mut builder = Request::builder().uri(uri);
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    let req = builder.body(axum::body::Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = to_bytes(resp.into_body(), 256 * 1024).await.unwrap();
    (status, headers, String::from_utf8_lossy(&body).to_string())
}

/// Dispatches an in-memory form-encoded POST request to the test Router and returns `(StatusCode, HeaderMap, String body)`.
pub async fn post_form_response(
    app: &Router,
    uri: &str,
    cookie: Option<&str>,
    form_body: &str,
) -> (StatusCode, header::HeaderMap, String) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    let req = builder
        .body(axum::body::Body::from(form_body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = to_bytes(resp.into_body(), 256 * 1024).await.unwrap();
    (status, headers, String::from_utf8_lossy(&body).to_string())
}

/// Dispatches an in-memory multipart POST request to the test Router and returns `(StatusCode, HeaderMap, String body)`.
pub async fn post_multipart_response(
    app: &Router,
    uri: &str,
    cookie: Option<&str>,
    content_type: &str,
    payload: Vec<u8>,
) -> (StatusCode, header::HeaderMap, String) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, content_type);
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    let req = builder.body(axum::body::Body::from(payload)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = to_bytes(resp.into_body(), 256 * 1024).await.unwrap();
    (status, headers, String::from_utf8_lossy(&body).to_string())
}

/// Constructs a sample user model for testing.
pub fn sample_user(id: i32, role: &str) -> User {
    User {
        id,
        full_name: format!("User {}", id),
        email: format!("user{}@example.com", id),
        password: "hashed_password".to_string(),
        role: role.to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    }
}

/// Constructs a sample event model for testing.
pub fn sample_event(id: i32, status: &str, organizer_id: i32) -> Event {
    Event {
        id,
        organizer_id,
        category_id: Some(1),
        title: "Global Tech Summit 2025".to_string(),
        description: Some("Premier conference covering cloud, AI, and systems.".to_string()),
        venue: Some("Convention Center Hall A".to_string()),
        city: Some("Bengaluru".to_string()),
        event_date: chrono::NaiveDateTime::parse_from_str(
            "2025-11-15 09:30:00",
            "%Y-%m-%d %H:%M:%S",
        )
        .unwrap(),
        registration_deadline: Some(
            chrono::NaiveDateTime::parse_from_str("2025-11-10 23:59:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
        ),
        total_seats: 500,
        available_seats: 120,
        ticket_price: Decimal::new(149900, 2),
        banner_image: Some("/uploads/tech.jpg".to_string()),
        status: status.to_string(),
        created_at: None,
        updated_at: None,
        organizer_name: "TechCorp Global".to_string(),
        organizer_email: Some("events@techcorp.com".to_string()),
        category_name: Some("Technology".to_string()),
        category_icon: Some("💻".to_string()),
    }
}

/// Temporary upload directory guard for isolated test runs. Removes directory on drop.
pub struct TestUploadDir {
    path: PathBuf,
}

impl TestUploadDir {
    pub fn new() -> Self {
        let unique_id = format!(
            "eventpro_test_uploads_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        );
        let path = std::env::temp_dir().join(unique_id);
        std::fs::create_dir_all(&path).expect("failed to create temporary upload dir");
        Self { path }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Default for TestUploadDir {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestUploadDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// Helper to build standard HTTP multipart/form-data payloads in memory for testing.
pub struct MultipartBuilder {
    boundary: String,
    body: Vec<u8>,
}

impl MultipartBuilder {
    pub fn new() -> Self {
        let boundary = format!("----TestBoundary{}", rand::random::<u64>());
        Self {
            boundary,
            body: Vec::new(),
        }
    }

    pub fn text(mut self, name: &str, value: &str) -> Self {
        self.body
            .extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
        self.body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
        );
        self.body.extend_from_slice(value.as_bytes());
        self.body.extend_from_slice(b"\r\n");
        self
    }

    pub fn file(mut self, name: &str, filename: &str, mime: &str, data: &[u8]) -> Self {
        self.body
            .extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
        self.body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                name, filename
            )
            .as_bytes(),
        );
        self.body
            .extend_from_slice(format!("Content-Type: {}\r\n\r\n", mime).as_bytes());
        self.body.extend_from_slice(data);
        self.body.extend_from_slice(b"\r\n");
        self
    }

    pub fn build(mut self) -> (String, Vec<u8>) {
        self.body
            .extend_from_slice(format!("--{}--\r\n", self.boundary).as_bytes());
        let content_type = format!("multipart/form-data; boundary={}", self.boundary);
        (content_type, self.body)
    }
}

impl Default for MultipartBuilder {
    fn default() -> Self {
        Self::new()
    }
}
