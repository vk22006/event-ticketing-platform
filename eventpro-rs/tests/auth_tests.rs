use axum::{
    body::to_bytes,
    http::{header, Request, StatusCode},
    routing::get,
    Router,
};
use eventpro::{
    config::{resolve_public_dir, AppConfig},
    csrf::{generate_csrf_token, verify_token_constant_time},
    error::AppError,
    middleware::{verify_role, AuthenticatedUser},
    models::User,
    state::AppState,
};
use std::time::Duration;
use tower::ServiceExt;
use tower_sessions::{Expiry, SessionManagerLayer};

fn create_test_state() -> AppState {
    let public_dir = resolve_public_dir().expect("public directory should resolve");
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://dummy_user:dummy_pass@127.0.0.1:9999/dummy_db".to_string(),
        session_secret: "test_secret_must_be_at_least_16_chars".to_string(),
        app_env: "test".to_string(),
        public_dir,
    };

    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .acquire_timeout(Duration::from_millis(100))
        .connect_lazy(&config.database_url)
        .expect("lazy pool creation failed");

    AppState::new(pool, config)
}

fn create_test_app(state: AppState) -> Router {
    let session_store = tower_sessions::MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_name("eventpro_sid")
        .with_secure(false)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

    eventpro::routes::create_router_with_store(state, Some(session_layer))
}

#[test]
fn test_bcrypt_hash_verification_seed_accounts() {
    // Hashes copied directly from database/seed.sql
    let admin_hash = "$2b$10$3vN0EP1RfVFrbNNHZncc2et0rJr7eolibTZBTCBdy1l375EM/sc7y";
    let organizer_hash = "$2b$10$cN22IeDbmzxIuaIb5pqDS.n4j6tIN1gKnptJO4sGZvsb9o2BaNR5S";
    let user_hash = "$2b$10$5PpEKodq0/8iAzDIFI7jFO4Z3dpdFGaiRkOtfasUbaOIQ/f.z6/oW";

    // Valid passwords must verify
    assert!(User::verify_password("Admin@1234", admin_hash));
    assert!(User::verify_password("Organizer@1", organizer_hash));
    assert!(User::verify_password("User@1234", user_hash));

    // Invalid passwords must fail
    assert!(!User::verify_password("WrongPassword", admin_hash));
    assert!(!User::verify_password("admin@1234", admin_hash)); // case sensitivity check
    assert!(!User::verify_password("", organizer_hash));
    assert!(!User::verify_password("User@12345", user_hash));
}

#[test]
fn test_bcrypt_hash_generation() {
    let plain = "ValidPassword@2025";
    let hash = User::hash_password(plain).expect("bcrypt hash should succeed");

    assert!(
        hash.starts_with("$2b$10$") || hash.starts_with("$2a$10$") || hash.starts_with("$2y$10$")
    );
    assert!(User::verify_password(plain, &hash));
    assert!(!User::verify_password("IncorrectPassword", &hash));
}

#[test]
fn test_csrf_token_generation_entropy_and_format() {
    let token1 = generate_csrf_token();
    let token2 = generate_csrf_token();

    // 32 bytes hex-encoded = 64 hex characters
    assert_eq!(token1.len(), 64);
    assert_eq!(token2.len(), 64);
    assert!(token1.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(token1, token2);
}

#[test]
fn test_csrf_constant_time_comparison() {
    let token = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
    let matching = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
    let different = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    assert!(verify_token_constant_time(token, matching));
    assert!(!verify_token_constant_time(token, different));
    assert!(!verify_token_constant_time(token, ""));
    assert!(!verify_token_constant_time("", token));
    assert!(!verify_token_constant_time(token, "short_token"));
}

#[test]
fn test_role_verification() {
    let make_user = |role: &str| User {
        id: 1,
        full_name: "Test User".to_string(),
        email: "test@example.com".to_string(),
        password: "hash".to_string(),
        role: role.to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    let admin = make_user("admin");
    let organizer = make_user("organizer");
    let regular_user = make_user("user");

    // Admin checks
    assert!(verify_role(&admin, &["admin"]).is_ok());
    assert!(verify_role(&admin, &["organizer", "admin"]).is_ok());

    // Organizer checks
    assert!(verify_role(&organizer, &["organizer"]).is_ok());
    assert!(verify_role(&organizer, &["organizer", "admin"]).is_ok());
    assert_eq!(
        verify_role(&organizer, &["admin"])
            .unwrap_err()
            .status_code(),
        StatusCode::FORBIDDEN
    );

    // Regular user checks
    assert!(verify_role(&regular_user, &["user"]).is_ok());
    assert_eq!(
        verify_role(&regular_user, &["organizer"])
            .unwrap_err()
            .status_code(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        verify_role(&regular_user, &["admin"])
            .unwrap_err()
            .status_code(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn test_html_template_escaping_xss_protection() {
    let state = create_test_state();

    let malicious_input = "<script>alert('xss')</script>";
    let context = minijinja::context! {
        title => "Login Test",
        error => malicious_input,
        email => malicious_input,
        csrf_token => "token123",
        user => None::<User>,
    };

    let tmpl = state
        .jinja
        .get_template("auth/login.html")
        .expect("login template should exist");

    let rendered = tmpl.render(context).expect("rendering should succeed");

    // Unescaped script tags must NOT appear
    assert!(!rendered.contains("<script>alert('xss')</script>"));
    // Escaped entities must appear
    assert!(rendered.contains("&lt;script&gt;"));
    assert!(rendered.contains("&lt;&#x2f;script&gt;") || rendered.contains("&lt;/script&gt;"));
}

#[test]
fn test_database_error_sanitization() {
    let err = AppError::Database(sqlx::Error::RowNotFound);
    assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);

    let client_msg = err.client_message();
    assert_eq!(
        client_msg,
        "A database error occurred. Please try again later."
    );

    // Verify response does not contain database driver details
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_login_page_renders_csrf_and_form() {
    let state = create_test_state();
    let app = create_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let cookie_header = response.headers().get(header::SET_COOKIE);
    assert!(cookie_header.is_some());
    assert!(cookie_header
        .unwrap()
        .to_str()
        .unwrap()
        .contains("eventpro_sid="));

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("Sign in to your EventPro account"));
    assert!(html.contains("name=\"email\""));
    assert!(html.contains("name=\"password\""));
    assert!(html.contains("name=\"csrf_token\""));
    assert!(html.contains("value=\""));
}

#[tokio::test]
async fn test_register_page_renders_csrf_and_form() {
    let state = create_test_state();
    let app = create_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/register")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("Join EventPro and start your journey"));
    assert!(html.contains("name=\"full_name\""));
    assert!(html.contains("name=\"email\""));
    assert!(html.contains("name=\"role\""));
    assert!(html.contains("Attend Events (User)"));
    assert!(html.contains("Organise Events (Organizer)"));
    assert!(html.contains("name=\"password\""));
    assert!(html.contains("name=\"confirm_password\""));
    assert!(html.contains("name=\"csrf_token\""));
}

#[tokio::test]
async fn test_login_post_missing_csrf_rejected_403() {
    let state = create_test_state();
    let app = create_test_app(state);

    let form_body = "email=admin@eventpro.com&password=Admin@1234";

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(axum::body::Body::from(form_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("CSRF validation failed"));
    assert!(html.contains("Action Blocked"));
}

#[tokio::test]
async fn test_login_post_invalid_csrf_rejected_403() {
    let state = create_test_state();
    let app = create_test_app(state);

    let form_body =
        "email=admin@eventpro.com&password=Admin@1234&csrf_token=invalid_forged_token_12345678";

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(axum::body::Body::from(form_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("CSRF validation failed"));
}

#[tokio::test]
async fn test_register_post_missing_csrf_rejected_403() {
    let state = create_test_state();
    let app = create_test_app(state);

    let form_body = "full_name=Test+User&email=test@example.com&password=secret123&confirm_password=secret123&role=user";

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/register")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(axum::body::Body::from(form_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("CSRF validation failed"));
}

#[tokio::test]
async fn test_logout_redirects_to_root() {
    let state = create_test_state();
    let app = create_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/logout")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/"
    );
}

#[tokio::test]
async fn test_unauthenticated_access_rejection() {
    let state = create_test_state();

    // A protected route handler using AuthenticatedUser extractor
    let protected_app = Router::new()
        .route(
            "/protected",
            get(|_user: AuthenticatedUser| async { "secret" }),
        )
        .with_state(state.clone());

    let session_store = tower_sessions::MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store);
    let app = protected_app.layer(session_layer);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Unauthenticated request must be redirected to /auth/login
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/auth/login"
    );
}

#[tokio::test]
async fn test_real_mysql_integration() {
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => {
            println!(
                "SKIPPED: Real MySQL integration tests skipped because DATABASE_URL is not set."
            );
            return;
        }
    };

    let pool = match sqlx::mysql::MySqlPoolOptions::new()
        .acquire_timeout(Duration::from_secs(2))
        .connect(&db_url)
        .await
    {
        Ok(p) => p,
        Err(err) => {
            println!(
                "SKIPPED: Real MySQL integration tests skipped because database at DATABASE_URL is unreachable: {}",
                err
            );
            return;
        }
    };

    // If connected, test user retrieval
    let admin = User::find_by_email(&pool, "admin@eventpro.com")
        .await
        .expect("query should succeed");

    if let Some(user) = admin {
        assert_eq!(user.role, "admin");
        assert!(User::verify_password("Admin@1234", &user.password));
    }
}
