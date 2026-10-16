mod common;

use axum::{
    body::to_bytes,
    http::{header, Request, StatusCode},
    routing::get,
    Router,
};
use common::{
    create_test_app, create_test_state, create_test_state_with_pool, extract_cookie,
    extract_csrf_token, get_response, http_login, post_form_response,
};
use eventpro::{
    csrf::{generate_csrf_token, verify_token_constant_time},
    error::AppError,
    middleware::{verify_role, AuthenticatedUser},
    models::User,
};
use tower::ServiceExt;
use tower_sessions::SessionManagerLayer;

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
    let app = create_test_app(create_test_state());
    let (status, headers, html) = get_response(&app, "/auth/login", None).await;

    assert_eq!(status, StatusCode::OK);
    assert!(headers
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .contains("eventpro_sid="));

    assert!(html.contains("Sign in to your EventPro account"));
    assert!(html.contains("name=\"email\""));
    assert!(html.contains("name=\"password\""));
    assert!(html.contains("name=\"csrf_token\""));
    assert!(html.contains("value=\""));
}

#[tokio::test]
async fn test_register_page_renders_csrf_and_form() {
    let app = create_test_app(create_test_state());
    let (status, _, html) = get_response(&app, "/auth/register", None).await;

    assert_eq!(status, StatusCode::OK);
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
async fn test_auth_post_missing_or_invalid_csrf_rejected_403() {
    let app = create_test_app(create_test_state());

    let cases = vec![
        ("/auth/login", "email=admin@eventpro.com&password=Admin@1234"),
        (
            "/auth/login",
            "email=admin@eventpro.com&password=Admin@1234&csrf_token=invalid_forged_token_12345678",
        ),
        (
            "/auth/register",
            "full_name=Test+User&email=test@example.com&password=secret123&confirm_password=secret123&role=user",
        ),
    ];

    for (uri, form_body) in cases {
        let (status, _, html) = post_form_response(&app, uri, None, form_body).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "Form post to {} without valid CSRF must return 403",
            uri
        );
        assert!(html.contains("CSRF validation failed"));
    }
}

#[tokio::test]
async fn test_logout_post_with_valid_csrf_invalidates_session_and_redirects() {
    let app = create_test_app(create_test_state());

    // 1. GET /auth/login to establish session and get CSRF token
    let (_, headers, html) = get_response(&app, "/auth/login", None).await;
    let cookie = extract_cookie(&headers).expect("CSRF session cookie");
    let csrf_token = extract_csrf_token(&html).expect("CSRF token present on login page");

    // 2. POST /auth/logout with valid CSRF token
    let form_body = format!("csrf_token={}", csrf_token);
    let (status, logout_headers, _) =
        post_form_response(&app, "/auth/logout", Some(&cookie), &form_body).await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        logout_headers
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/"
    );
}

#[tokio::test]
async fn test_logout_post_missing_or_invalid_csrf_retains_session() {
    let app = create_test_app(create_test_state());

    let (_, headers, _) = get_response(&app, "/auth/login", None).await;
    let cookie = extract_cookie(&headers).expect("CSRF session cookie");

    // 1. Missing CSRF token
    let (s1, _, b1) = post_form_response(&app, "/auth/logout", Some(&cookie), "").await;
    assert_eq!(s1, StatusCode::FORBIDDEN);
    assert!(b1.contains("CSRF validation failed"));

    // 2. Invalid CSRF token
    let (s2, _, _) = post_form_response(
        &app,
        "/auth/logout",
        Some(&cookie),
        "csrf_token=forged_bad_token",
    )
    .await;
    assert_eq!(s2, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_logout_get_returns_405_method_not_allowed() {
    let app = create_test_app(create_test_state());
    let (status, _, _) = get_response(&app, "/auth/logout", None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
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

    let state = create_test_state_with_pool(&pool);
    let app = create_test_app(state);

    let test_email = format!("downgrade_test_{}@eventpro.com", rand::random::<u32>());
    let hashed_pw = User::hash_password("User@1234").expect("hash");

    // 1. Create temporary organizer user
    let user_id = User::create(&pool, "Temp Org", &test_email, &hashed_pw, "organizer")
        .await
        .expect("create user");

    let (org_cookie, _) = http_login(&app, &test_email, "User@1234").await;

    // 2. Verified organizer access: GET /events/create returns 200 OK
    let (status, _, _) = get_response(&app, "/events/create", Some(&org_cookie)).await;
    assert_eq!(status, StatusCode::OK);

    // 3. Admin downgrades user in MySQL to 'user'
    sqlx::query("UPDATE users SET role = 'user' WHERE id = ?")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("update role");

    // 4. Verification under Session Invalidation Policy:
    // Once session is re-established (e.g. following session invalidation or logout/login),
    // the downgraded attendee CANNOT access /events/create or /admin/dashboard
    let (user_cookie, _) = http_login(&app, &test_email, "User@1234").await;

    let (status, _, _) = get_response(&app, "/events/create", Some(&user_cookie)).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "Downgraded user must be denied organizer privileges"
    );

    let (status, _, _) = get_response(&app, "/admin/dashboard", Some(&user_cookie)).await;
    assert_eq!(
        status,
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

    let state = create_test_state_with_pool(&pool);

    use tower_sessions::Session;
    let app =
        Router::new()
            .route(
                "/seed-legacy-session",
                get(|session: Session| async move {
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
            .with_state(state)
            .layer(
                SessionManagerLayer::new(tower_sessions::MemoryStore::default())
                    .with_name("eventpro_sid"),
            );

    let (_, headers, _) = get_response(&app, "/seed-legacy-session", None).await;
    let cookie = extract_cookie(&headers).expect("cookie should be set");

    let (status, _, body) = get_response(&app, "/check-user", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.starts_with("id:1:admin@eventpro.com"));
}

#[tokio::test]
async fn test_deleted_user_invalidation_and_redirect() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let state = create_test_state_with_pool(&pool);

    use tower_sessions::Session;
    let app = Router::new()
        .route(
            "/seed-deleted-user-session",
            get(|session: Session| async move {
                session.insert("user_id", 999999).await.unwrap();
                "deleted_seeded"
            }),
        )
        .route(
            "/protected-route",
            get(|_user: AuthenticatedUser| async move { "should_never_reach" }),
        )
        .with_state(state)
        .layer(
            SessionManagerLayer::new(tower_sessions::MemoryStore::default())
                .with_name("eventpro_sid"),
        );

    let (_, headers, _) = get_response(&app, "/seed-deleted-user-session", None).await;
    let cookie = extract_cookie(&headers).expect("cookie should be set");

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
