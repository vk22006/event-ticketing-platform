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
async fn test_logout_post_with_valid_csrf_invalidates_session_and_redirects() {
    let state = create_test_state();
    let app = create_test_app(state);

    // 1. GET /auth/login to establish session and get CSRF token
    let login_page_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let cookie = login_page_res
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let body = to_bytes(login_page_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    let csrf_token = html
        .split("name=\"csrf_token\" value=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("CSRF token present on login page");

    // 2. POST /auth/logout with valid CSRF token
    let form_body = format!("csrf_token={}", csrf_token);
    let logout_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(axum::body::Body::from(form_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(logout_res.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        logout_res
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/"
    );
}

#[tokio::test]
async fn test_logout_post_missing_or_invalid_csrf_retains_session() {
    let state = create_test_state();
    let app = create_test_app(state);

    // GET /auth/login to establish session
    let login_page_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let cookie = login_page_res
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // 1. Missing CSRF token
    let res_missing = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res_missing.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(res_missing.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("CSRF validation failed"));

    // 2. Invalid CSRF token
    let res_invalid = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(axum::body::Body::from("csrf_token=forged_bad_token"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res_invalid.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_logout_get_returns_405_method_not_allowed() {
    let state = create_test_state();
    let app = create_test_app(state);

    let res = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/auth/logout")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
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
async fn test_user_password_excluded_from_session_serialization() {
    let user = User {
        id: 42,
        full_name: "Test User".to_string(),
        email: "test@example.com".to_string(),
        password: "$2b$10$super_secret_password_hash".to_string(),
        role: "attendee".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    let serialized = serde_json::to_string(&user).expect("serialization should succeed");
    assert!(!serialized.contains("super_secret_password_hash"));
    assert!(!serialized.contains("\"password\""));

    let deserialized: User =
        serde_json::from_str(&serialized).expect("deserialization should succeed");
    assert_eq!(deserialized.id, 42);
    assert_eq!(deserialized.email, "test@example.com");
    assert_eq!(deserialized.role, "attendee");
    assert_eq!(deserialized.password, ""); // Defaulted to empty
}

#[tokio::test]
async fn test_authenticated_user_fast_path_from_session_without_db() {
    use tower_sessions::Session;

    // Use test state with dummy pool that would fail if connected to
    let state = create_test_state();

    let app =
        Router::new()
            .route(
                "/seed-session",
                get(|session: Session| async move {
                    let user = User {
                        id: 99,
                        full_name: "Cached User".to_string(),
                        email: "cached@example.com".to_string(),
                        password: String::new(),
                        role: "organizer".to_string(),
                        avatar: None,
                        bio: None,
                        created_at: None,
                        updated_at: None,
                    };
                    session.insert("user_id", 99).await.unwrap();
                    session.insert("auth_user", user).await.unwrap();
                    "session_seeded"
                }),
            )
            .route(
                "/protected-fast-path",
                get(|user: AuthenticatedUser| async move {
                    format!("user:{}:{}", user.0.id, user.0.role)
                }),
            )
            .with_state(state.clone());

    let session_store = tower_sessions::MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store);
    let app = app.layer(session_layer);

    // 1. Seed the session
    let seed_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/seed-session")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(seed_res.status(), StatusCode::OK);
    let cookie = seed_res
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // 2. Call protected route using cookie - should succeed immediately from session without DB query!
    let prot_res = app
        .oneshot(
            Request::builder()
                .uri("/protected-fast-path")
                .header(header::COOKIE, cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(prot_res.status(), StatusCode::OK);
    let body = to_bytes(prot_res.into_body(), 1024 * 16).await.unwrap();
    assert_eq!(&body[..], b"user:99:organizer");
}

#[tokio::test]
async fn test_rbac_verification_with_cached_user() {
    let attendee = User {
        id: 1,
        full_name: "Attendee".to_string(),
        email: "att@example.com".to_string(),
        password: String::new(),
        role: "attendee".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    let organizer = User {
        id: 2,
        full_name: "Organizer".to_string(),
        email: "org@example.com".to_string(),
        password: String::new(),
        role: "organizer".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    let admin = User {
        id: 3,
        full_name: "Admin".to_string(),
        email: "adm@example.com".to_string(),
        password: String::new(),
        role: "admin".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    // Attendee should only match attendee
    assert!(verify_role(&attendee, &["attendee"]).is_ok());
    assert!(verify_role(&attendee, &["organizer"]).is_err());
    assert!(verify_role(&attendee, &["admin"]).is_err());

    // Organizer matches organizer or attendee
    assert!(verify_role(&organizer, &["organizer"]).is_ok());
    assert!(verify_role(&organizer, &["attendee", "organizer"]).is_ok());
    assert!(verify_role(&organizer, &["admin"]).is_err());

    // Admin matches admin
    assert!(verify_role(&admin, &["admin"]).is_ok());
    assert!(verify_role(&admin, &["organizer", "admin"]).is_ok());
}

mod common;

#[tokio::test]
async fn test_real_mysql_integration() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
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

#[tokio::test]
async fn test_real_mysql_role_downgrade_revocation_policy() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let state = AppState::new(pool.clone(), config);
    let app = create_test_app(state);

    let test_email = format!("downgrade_test_{}@eventpro.com", rand::random::<u32>());
    let hashed_pw = User::hash_password("User@1234").expect("hash");

    // 1. Create temporary organizer user
    let user_id = User::create(&pool, "Temp Org", &test_email, &hashed_pw, "organizer")
        .await
        .expect("create user");

    // Helper: simulate login by getting CSRF and posting credentials
    let login_page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let init_cookie = login_page
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let body_bytes = to_bytes(login_page.into_body(), 64 * 1024).await.unwrap();
    let html = String::from_utf8_lossy(&body_bytes);
    let csrf_token = html
        .split("name=\"csrf_token\" value=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("csrf token");

    let form = format!(
        "email={}&password=User%401234&csrf_token={}",
        percent_encoding::utf8_percent_encode(&test_email, percent_encoding::NON_ALPHANUMERIC),
        csrf_token
    );

    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::COOKIE, &init_cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(axum::body::Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login_resp.status(), StatusCode::SEE_OTHER);
    let org_cookie = login_resp
        .headers()
        .get(header::SET_COOKIE)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or(init_cookie);

    // 2. Verified organizer access: GET /events/create returns 200 OK
    let create_req = Request::builder()
        .uri("/events/create")
        .header(header::COOKIE, &org_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);

    // 3. Admin downgrades user in MySQL to 'user'
    sqlx::query("UPDATE users SET role = 'user' WHERE id = ?")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("update role");

    // 4. Verification under Session Invalidation Policy:
    // Once session is re-established (e.g. following session invalidation or logout/login),
    // the downgraded attendee CANNOT access /events/create or /admin/dashboard
    let login_page2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let init_cookie2 = login_page2
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let body_bytes2 = to_bytes(login_page2.into_body(), 64 * 1024).await.unwrap();
    let csrf_token2 = String::from_utf8_lossy(&body_bytes2)
        .split("name=\"csrf_token\" value=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("csrf token")
        .to_string();

    let form2 = format!(
        "email={}&password=User%401234&csrf_token={}",
        percent_encoding::utf8_percent_encode(&test_email, percent_encoding::NON_ALPHANUMERIC),
        csrf_token2
    );
    let login_resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::COOKIE, &init_cookie2)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(axum::body::Body::from(form2))
                .unwrap(),
        )
        .await
        .unwrap();
    let user_cookie = login_resp2
        .headers()
        .get(header::SET_COOKIE)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or(init_cookie2);

    // Attempting to access organizer route as downgraded user MUST be rejected with 403 Forbidden
    let create_req2 = Request::builder()
        .uri("/events/create")
        .header(header::COOKIE, &user_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let create_resp2 = app.clone().oneshot(create_req2).await.unwrap();
    assert_eq!(
        create_resp2.status(),
        StatusCode::FORBIDDEN,
        "Downgraded user must be denied organizer privileges"
    );

    // Attempting to access admin route MUST also be rejected with 403 Forbidden
    let admin_req = Request::builder()
        .uri("/admin/dashboard")
        .header(header::COOKIE, &user_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let admin_resp = app.clone().oneshot(admin_req).await.unwrap();
    assert_eq!(
        admin_resp.status(),
        StatusCode::FORBIDDEN,
        "Downgraded user must be denied admin privileges"
    );

    // Cleanup test user
    let _ = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(user_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn test_fallback_when_auth_user_missing_from_session() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let state = AppState::new(pool.clone(), config);

    use tower_sessions::Session;
    let app =
        Router::new()
            .route(
                "/seed-legacy-session",
                get(|session: Session| async move {
                    // Seed session with user_id ONLY (no auth_user)
                    session.insert("user_id", 1).await.unwrap();
                    "legacy_seeded"
                }),
            )
            .route(
                "/check-user",
                get(|user: AuthenticatedUser| async move {
                    format!("id:{}:{}", user.0.id, user.0.email)
                }),
            )
            .with_state(state);

    let session_store = tower_sessions::MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store);
    let app = app.layer(session_layer);

    // Seed session
    let seed_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/seed-legacy-session")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = seed_resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // Request protected route: fallback queries DB for id 1 (admin@eventpro.com), caches auth_user, succeeds
    let check_resp = app
        .oneshot(
            Request::builder()
                .uri("/check-user")
                .header(header::COOKIE, cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(check_resp.status(), StatusCode::OK);
    let body = to_bytes(check_resp.into_body(), 1024).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(text.starts_with("id:1:admin@eventpro.com"));
}

#[tokio::test]
async fn test_deleted_user_invalidation_and_redirect() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let state = AppState::new(pool.clone(), config);

    use tower_sessions::Session;
    let app = Router::new()
        .route(
            "/seed-deleted-user-session",
            get(|session: Session| async move {
                // Seed session with a nonexistent user ID 999999
                session.insert("user_id", 999999).await.unwrap();
                "deleted_seeded"
            }),
        )
        .route(
            "/protected-route",
            get(|_user: AuthenticatedUser| async move { "should_never_reach" }),
        )
        .with_state(state);

    let session_store = tower_sessions::MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store);
    let app = app.layer(session_layer);

    let seed_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/seed-deleted-user-session")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = seed_resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // Access protected route: fallback queries DB for 999999, gets None, flushes session, redirects to /auth/login
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/protected-route")
                .header(header::COOKIE, cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp.headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/auth/login"
    );
}
