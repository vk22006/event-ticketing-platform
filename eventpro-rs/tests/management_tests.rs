use axum::{
    body::to_bytes,
    http::{header, Request, StatusCode},
    Router,
};
use eventpro::{
    config::{resolve_public_dir, AppConfig},
    models::{
        Booking, Category, CreateEventParams, Event, RecentBooking, UpdateEventParams, User,
        UserBooking,
    },
    routes::events::can_manage,
    state::AppState,
};

fn urlencode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}
use rust_decimal::Decimal;
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

fn sample_user(id: i32, role: &str) -> User {
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

fn sample_event(id: i32, status: &str, organizer_id: i32) -> Event {
    Event {
        id,
        organizer_id,
        category_id: Some(1),
        title: "Summit 2026".to_string(),
        description: Some("Tech conference".to_string()),
        venue: Some("Auditorium".to_string()),
        city: Some("Hyderabad".to_string()),
        event_date: chrono::NaiveDateTime::parse_from_str(
            "2026-11-15 09:30:00",
            "%Y-%m-%d %H:%M:%S",
        )
        .unwrap(),
        registration_deadline: Some(
            chrono::NaiveDateTime::parse_from_str("2026-11-10 23:59:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
        ),
        total_seats: 250,
        available_seats: 150,
        ticket_price: Decimal::new(49900, 2),
        banner_image: Some("/uploads/summit.jpg".to_string()),
        status: status.to_string(),
        created_at: None,
        updated_at: None,
        organizer_name: "Organizer One".to_string(),
        organizer_email: Some("org1@example.com".to_string()),
        category_name: Some("Technology".to_string()),
        category_icon: Some("💻".to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Authorization and Ownership Rule Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_ownership_and_admin_management_policy() {
    let organizer_a = sample_user(10, "organizer");
    let organizer_b = sample_user(20, "organizer");
    let admin = sample_user(1, "admin");
    let attendee = sample_user(30, "user");

    let event_a = sample_event(100, "published", organizer_a.id);

    // Rule 1: Organizer A owns event A -> permitted
    assert!(
        can_manage(&event_a, &organizer_a),
        "Owning organizer must be allowed to manage event"
    );

    // Rule 2: Organizer B does NOT own event A -> denied (cross-organizer protection)
    assert!(
        !can_manage(&event_a, &organizer_b),
        "Non-owning organizer must NOT be allowed to manage event A"
    );

    // Rule 3: Admin does not own event A, but admin policy grants platform-wide management -> permitted
    assert!(
        can_manage(&event_a, &admin),
        "Admin must be permitted to manage any organizer's event"
    );

    // Rule 4: Regular attendee cannot manage organizer A's event -> denied
    assert!(
        !can_manage(&event_a, &attendee),
        "Attendee must NOT be permitted to manage event"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Unauthenticated Access Protection Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_unauthenticated_dashboard_redirects_to_login() {
    let state = create_test_state();
    let app = create_test_app(state);

    let endpoints = vec![
        ("GET", "/dashboard"),
        ("GET", "/admin/dashboard"),
        ("GET", "/events/create"),
        ("POST", "/events/create"),
        ("GET", "/events/1/edit"),
        ("POST", "/events/1/edit"),
        ("POST", "/events/1/toggle-status"),
        ("POST", "/events/1/delete"),
    ];

    for (method, uri) in endpoints {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(axum::body::Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();

        // Must reject unauthenticated request and redirect to /auth/login
        assert_eq!(
            resp.status(),
            StatusCode::SEE_OTHER,
            "Endpoint {} {} must redirect unauthenticated requests",
            method,
            uri
        );
        let location = resp
            .headers()
            .get(header::LOCATION)
            .expect("Location header must exist")
            .to_str()
            .unwrap();
        assert_eq!(location, "/auth/login");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Template Rendering Tests for Dashboards and Management Forms
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_organizer_dashboard_home_template_rendering() {
    let state = create_test_state();
    let template = state
        .jinja
        .get_template("dashboard/home.html")
        .expect("dashboard/home.html must be registered");

    let organizer = sample_user(2, "organizer");
    let event = sample_event(101, "published", 2);

    let rendered = template
        .render(minijinja::context! {
            title => "Dashboard",
            user => organizer,
            events => vec![event],
            bookings => Vec::<UserBooking>::new(),
            csrf_token => "test_csrf_token_12345",
        })
        .expect("template rendering should succeed");

    assert!(rendered.contains("Dashboard"));
    assert!(rendered.contains("Welcome back, User 2"));
    assert!(rendered.contains("Summit 2026"));
    assert!(rendered.contains("/events/101/edit"));
    assert!(rendered.contains("/events/101/toggle-status"));
    assert!(rendered.contains("/events/101/delete"));
    assert!(rendered.contains("test_csrf_token_12345"));
}

#[tokio::test]
async fn test_attendee_dashboard_home_template_rendering() {
    let state = create_test_state();
    let template = state
        .jinja
        .get_template("dashboard/home.html")
        .expect("dashboard/home.html must be registered");

    let attendee = sample_user(3, "user");
    let booking = UserBooking {
        id: 50,
        user_id: 3,
        event_id: 101,
        seat_type_id: None,
        quantity: 2,
        total_amount: Decimal::new(99800, 2),
        status: "confirmed".to_string(),
        payment_method: Some("UPI".to_string()),
        booked_at: None,
        event_title: "Music Festival 2026".to_string(),
        event_date: chrono::NaiveDateTime::parse_from_str(
            "2026-12-01 18:00:00",
            "%Y-%m-%d %H:%M:%S",
        )
        .unwrap(),
        venue: Some("Palace Grounds".to_string()),
        city: Some("Bengaluru".to_string()),
        banner_image: None,
        seat_type: None,
    };

    let rendered = template
        .render(minijinja::context! {
            title => "Dashboard",
            user => attendee,
            events => Vec::<Event>::new(),
            bookings => vec![booking],
            csrf_token => "test_csrf_token_12345",
        })
        .expect("template rendering should succeed");

    assert!(rendered.contains("Welcome back, User 3"));
    assert!(rendered.contains("Music Festival 2026"));
    assert!(rendered.contains("confirmed"));
    assert!(rendered.contains("Browse Events"));
}

#[tokio::test]
async fn test_admin_dashboard_template_rendering() {
    let state = create_test_state();
    let template = state
        .jinja
        .get_template("dashboard/admin.html")
        .expect("dashboard/admin.html must be registered");

    let admin = sample_user(1, "admin");
    let recent = RecentBooking {
        id: 77,
        user_id: 5,
        event_id: 101,
        total_amount: Decimal::new(149900, 2),
        status: "confirmed".to_string(),
        booked_at: None,
        full_name: "Rahul Sharma".to_string(),
        email: "rahul@example.com".to_string(),
        event_title: "Tech Summit".to_string(),
    };

    let stats = minijinja::context! {
        total_users => 150,
        organizers => 12,
        regular_users => 135,
        total_events => 45,
        published_events => 38,
        total_bookings => 320,
        revenue => Decimal::new(45000000, 2),
    };

    let rendered = template
        .render(minijinja::context! {
            title => "Admin Dashboard",
            user => admin,
            stats => stats,
            recent_bookings => vec![recent],
        })
        .expect("admin dashboard template rendering should succeed");

    assert!(rendered.contains("Admin Dashboard 🛡️"));
    assert!(rendered.contains("150"));
    assert!(rendered.contains("Rahul Sharma"));
    assert!(rendered.contains("rahul@example.com"));
    assert!(rendered.contains("Tech Summit"));
}

#[tokio::test]
async fn test_create_and_edit_event_templates_render_csrf_and_fields() {
    let state = create_test_state();

    let create_tmpl = state
        .jinja
        .get_template("dashboard/create-event.html")
        .expect("create-event.html must exist");

    let organizer = sample_user(2, "organizer");
    let categories = vec![Category {
        id: 1,
        name: "Conferences".to_string(),
        icon: Some("🎤".to_string()),
    }];

    let create_html = create_tmpl
        .render(minijinja::context! {
            title => "Create Event",
            categories => categories,
            error => None::<String>,
            user => organizer,
            csrf_token => "csrf_secret_998877",
        })
        .expect("create-event rendering should succeed");

    assert!(create_html.contains("Create New Event"));
    assert!(create_html.contains("name=\"csrf_token\" value=\"csrf_secret_998877\""));
    assert!(create_html.contains("name=\"title\""));
    assert!(create_html.contains("name=\"event_date\""));
    assert!(create_html.contains("name=\"total_seats\""));
    assert!(create_html.contains("name=\"ticket_price\""));

    let edit_tmpl = state
        .jinja
        .get_template("dashboard/edit-event.html")
        .expect("edit-event.html must exist");

    let event = sample_event(42, "draft", 2);

    let edit_html = edit_tmpl
        .render(minijinja::context! {
            title => "Edit Event",
            event => event,
            categories => Vec::<Category>::new(),
            error => None::<String>,
            user => organizer,
            csrf_token => "csrf_secret_445566",
        })
        .expect("edit-event rendering should succeed");

    assert!(edit_html.contains("Edit Event"));
    assert!(edit_html.contains("action=\"/events/42/edit\""));
    assert!(edit_html.contains("name=\"csrf_token\" value=\"csrf_secret_445566\""));
    assert!(edit_html.contains("value=\"Summit 2026\""));
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Live MySQL End-to-End Management & Security Lifecycle
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_real_mysql_management_lifecycle_and_security() {
    let _ = dotenvy::dotenv();
    let _ = dotenvy::from_path("../.env");

    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => match (
            std::env::var("DB_USER"),
            std::env::var("DB_PASSWORD"),
            std::env::var("DB_HOST"),
            std::env::var("DB_PORT"),
            std::env::var("DB_NAME"),
        ) {
            (Ok(u), Ok(p), Ok(h), Ok(port), Ok(d)) => {
                eventpro::config::build_database_url(&u, &p, &h, &port, &d)
            }
            _ => {
                println!(
                    "SKIPPED: Real MySQL management tests skipped because DATABASE_URL / DB_* environment variables are not set."
                );
                return;
            }
        },
    };

    let pool = match sqlx::mysql::MySqlPoolOptions::new()
        .acquire_timeout(Duration::from_secs(2))
        .connect(&db_url)
        .await
    {
        Ok(p) => p,
        Err(err) => {
            println!(
                "SKIPPED: Real MySQL management tests skipped because database is unreachable: {}",
                err
            );
            return;
        }
    };

    println!("REAL MYSQL DETECTED: Running Phase 4 management integration tests against MySQL.");

    // 1. Verify Platform Aggregations (User & Booking)
    let user_counts = User::count_by_role(&pool)
        .await
        .expect("User::count_by_role failed");
    assert!(
        user_counts.contains_key("admin"),
        "Admin role count should exist"
    );
    assert!(
        user_counts.contains_key("organizer"),
        "Organizer role count should exist"
    );

    let total_bookings = Booking::count_total(&pool)
        .await
        .expect("Booking::count_total failed");
    assert!(total_bookings >= 0);

    let total_revenue = Booking::total_revenue(&pool)
        .await
        .expect("Booking::total_revenue failed");
    assert!(total_revenue >= Decimal::ZERO);

    let recent = Booking::recent_bookings(&pool, 5)
        .await
        .expect("Booking::recent_bookings failed");
    for b in &recent {
        assert!(!b.full_name.is_empty());
        assert!(!b.event_title.is_empty());
    }

    // 2. Fetch seed users: Admin (id 1) and Organizer 1 (id 2)
    let admin_user = User::find_by_email(&pool, "admin@eventpro.com")
        .await
        .expect("Query admin failed")
        .expect("Admin seed account must exist");
    assert_eq!(admin_user.role, "admin");

    let organizer_1 = User::find_by_email(&pool, "alice@events.com")
        .await
        .expect("Query organizer failed")
        .expect("Organizer seed account must exist");
    assert_eq!(organizer_1.role, "organizer");

    // Create an isolated secondary organizer account to verify cross-organizer isolation
    let hashed_pw = User::hash_password("Org2@SecurePassword123").unwrap();
    let org2_email = format!(
        "test_org2_{}@eventpro.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let organizer_2_id = User::create(&pool, "Organizer Two", &org2_email, &hashed_pw, "organizer")
        .await
        .expect("Failed to create Organizer 2");
    let organizer_2 = User::find_by_id(&pool, organizer_2_id)
        .await
        .expect("Find org 2 failed")
        .expect("Org 2 must exist");

    // 3. Create a test event owned by Organizer 1
    let event_date = chrono::NaiveDate::from_ymd_opt(2026, 12, 25)
        .unwrap()
        .and_hms_opt(18, 0, 0)
        .unwrap();
    let deadline = chrono::NaiveDate::from_ymd_opt(2026, 12, 20)
        .unwrap()
        .and_hms_opt(23, 59, 59)
        .unwrap();

    let create_params = CreateEventParams {
        category_id: Some(1),
        title: "Phase 4 Secure Integration Summit".to_string(),
        description: Some("Automated test event for Phase 4 verification".to_string()),
        venue: Some("Innovation Hub".to_string()),
        city: Some("Hyderabad".to_string()),
        event_date,
        registration_deadline: Some(deadline),
        total_seats: 120,
        ticket_price: Decimal::new(29900, 2),
        status: "draft".to_string(),
        banner_image: None,
    };

    let test_event_id = Event::create(&pool, organizer_1.id, &create_params)
        .await
        .expect("Event::create should succeed");
    assert!(test_event_id > 0);

    // Verify created event record in DB
    let created_event = Event::find_by_id(&pool, test_event_id)
        .await
        .expect("find_by_id failed")
        .expect("Created event must exist");
    assert_eq!(created_event.organizer_id, organizer_1.id);
    assert_eq!(created_event.title, "Phase 4 Secure Integration Summit");
    assert_eq!(created_event.available_seats, 120);
    assert_eq!(created_event.status, "draft");

    // 4. Verification of Cross-Organizer Authorization Rules
    // Organizer 1 (owner) can manage
    assert!(can_manage(&created_event, &organizer_1));

    // Organizer 2 (non-owner) CANNOT manage
    assert!(
        !can_manage(&created_event, &organizer_2),
        "Cross-organizer authorization MUST be rejected!"
    );

    // Admin CAN manage
    assert!(can_manage(&created_event, &admin_user));

    // 5. Test Event Status Toggle
    Event::update_status(&pool, test_event_id, "published")
        .await
        .expect("update_status failed");
    let toggled_event = Event::find_by_id(&pool, test_event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(toggled_event.status, "published");

    // 6. Test Event Update
    let update_params = UpdateEventParams {
        category_id: Some(1),
        title: "Updated Summit 2026".to_string(),
        description: Some("Updated description".to_string()),
        venue: Some("Auditorium B".to_string()),
        city: Some("Bengaluru".to_string()),
        event_date,
        registration_deadline: Some(deadline),
        total_seats: 150,
        ticket_price: Decimal::new(34900, 2),
        status: "published".to_string(),
        banner_image: None,
    };
    Event::update(&pool, test_event_id, &update_params)
        .await
        .expect("Event::update failed");

    let updated_event = Event::find_by_id(&pool, test_event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_event.title, "Updated Summit 2026");
    assert_eq!(updated_event.total_seats, 150);
    assert_eq!(updated_event.city.as_deref(), Some("Bengaluru"));

    // 7. Test Organizer Event Listing
    let org1_events = Event::get_by_organizer(&pool, organizer_1.id)
        .await
        .expect("get_by_organizer failed");
    assert!(org1_events.iter().any(|e| e.id == test_event_id));

    let org2_events = Event::get_by_organizer(&pool, organizer_2.id)
        .await
        .expect("get_by_organizer failed");
    assert!(!org2_events.iter().any(|e| e.id == test_event_id));

    // 8. Test Event Deletion and Cleanup
    Event::delete(&pool, test_event_id)
        .await
        .expect("Event::delete failed");

    let deleted_event = Event::find_by_id(&pool, test_event_id).await.unwrap();
    assert!(
        deleted_event.is_none(),
        "Deleted event must no longer exist"
    );

    // Cleanup temporary test organizer
    let _ = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(organizer_2_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn test_real_mysql_http_management_security() {
    let _ = dotenvy::dotenv();
    let _ = dotenvy::from_path("../.env");

    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => match (
            std::env::var("DB_USER"),
            std::env::var("DB_PASSWORD"),
            std::env::var("DB_HOST"),
            std::env::var("DB_PORT"),
            std::env::var("DB_NAME"),
        ) {
            (Ok(u), Ok(p), Ok(h), Ok(port), Ok(d)) => {
                eventpro::config::build_database_url(&u, &p, &h, &port, &d)
            }
            _ => {
                println!(
                    "SKIPPED: Real MySQL HTTP management security tests skipped because DATABASE_URL / DB_* environment variables are not set."
                );
                return;
            }
        },
    };

    let pool = match sqlx::mysql::MySqlPoolOptions::new()
        .acquire_timeout(Duration::from_secs(2))
        .connect(&db_url)
        .await
    {
        Ok(p) => p,
        Err(err) => {
            println!(
                "SKIPPED: Real MySQL HTTP management security tests skipped because database is unreachable: {}",
                err
            );
            return;
        }
    };

    println!("REAL MYSQL DETECTED: Running Phase 4 HTTP route authorization tests against MySQL.");

    let public_dir = resolve_public_dir().expect("public dir");
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: db_url.clone(),
        session_secret: "phase4_integration_session_secret_123456".to_string(),
        app_env: "test".to_string(),
        public_dir,
    };

    let app_state = AppState::new(pool.clone(), config);
    let app = create_test_app(app_state);

    fn extract_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
        headers.get_all(header::SET_COOKIE).iter().find_map(|v| {
            let s = v.to_str().ok()?;
            if s.starts_with("eventpro_sid=") {
                Some(s.split(';').next().unwrap().to_string())
            } else {
                None
            }
        })
    }

    fn extract_csrf_token(html: &str) -> Option<String> {
        let marker = "name=\"csrf_token\" value=\"";
        let start = html.find(marker)? + marker.len();
        let end = html[start..].find('"')? + start;
        Some(html[start..end].to_string())
    }

    async fn http_login(app: &Router, email: &str, password: &str) -> (String, String) {
        // Step 1: GET /auth/login to obtain session cookie & CSRF token
        let login_page_req = Request::builder()
            .uri("/auth/login")
            .body(axum::body::Body::empty())
            .unwrap();
        let login_page_resp = app.clone().oneshot(login_page_req).await.unwrap();
        let cookie_1 =
            extract_cookie(login_page_resp.headers()).expect("Must receive session cookie");
        let body = to_bytes(login_page_resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        let csrf_1 =
            extract_csrf_token(&String::from_utf8_lossy(&body)).expect("Must extract CSRF token");

        // Step 2: POST /auth/login with credentials and CSRF
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
            "Login should redirect to /dashboard"
        );
        let session_cookie = extract_cookie(login_resp.headers()).unwrap_or(cookie_1);

        // Fetch dashboard to get a fresh CSRF token for subsequent mutations
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

    // A. Attendee access check: Attendee attempting to access /events/create -> 403 Forbidden
    let (attendee_cookie, _) = http_login(&app, "charlie@mail.com", "User@1234").await;
    let create_attendee_req = Request::builder()
        .uri("/events/create")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let create_attendee_resp = app.clone().oneshot(create_attendee_req).await.unwrap();
    assert_eq!(
        create_attendee_resp.status(),
        StatusCode::FORBIDDEN,
        "Attendee must be forbidden from accessing /events/create"
    );

    // B. Organizer 1 access checks
    let (org1_cookie, org1_csrf) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // Organizer accessing /admin/dashboard -> 403 Forbidden
    let admin_dash_req = Request::builder()
        .uri("/admin/dashboard")
        .header(header::COOKIE, &org1_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let admin_dash_resp = app.clone().oneshot(admin_dash_req).await.unwrap();
    assert_eq!(
        admin_dash_resp.status(),
        StatusCode::FORBIDDEN,
        "Organizer must be forbidden from accessing /admin/dashboard"
    );

    // Organizer accessing /events/create -> 200 OK
    let create_page_req = Request::builder()
        .uri("/events/create")
        .header(header::COOKIE, &org1_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let create_page_resp = app.clone().oneshot(create_page_req).await.unwrap();
    assert_eq!(create_page_resp.status(), StatusCode::OK);

    // C. Validation tests on POST /events/create
    // 1. Missing CSRF token -> 403 Forbidden
    let no_csrf_form = "title=Test+Event&total_seats=100&event_date=2026-11-20T18:00";
    let no_csrf_req = Request::builder()
        .method("POST")
        .uri("/events/create")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &org1_cookie)
        .body(axum::body::Body::from(no_csrf_form))
        .unwrap();
    let no_csrf_resp = app.clone().oneshot(no_csrf_req).await.unwrap();
    assert_eq!(no_csrf_resp.status(), StatusCode::FORBIDDEN);

    // 2. Invalid category_id -> 400 Bad Request
    let bad_cat_form = format!(
        "title=Test+Event&total_seats=100&event_date=2026-11-20T18:00&category_id=99999&csrf_token={}",
        urlencode(&org1_csrf)
    );
    let bad_cat_req = Request::builder()
        .method("POST")
        .uri("/events/create")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &org1_cookie)
        .body(axum::body::Body::from(bad_cat_form))
        .unwrap();
    let bad_cat_resp = app.clone().oneshot(bad_cat_req).await.unwrap();
    assert_eq!(bad_cat_resp.status(), StatusCode::BAD_REQUEST);
    let bad_cat_body = to_bytes(bad_cat_resp.into_body(), 64 * 1024).await.unwrap();
    assert!(String::from_utf8_lossy(&bad_cat_body).contains("Selected category does not exist."));

    // 3. Registration deadline after event date -> 400 Bad Request
    let bad_date_form = format!(
        "title=Test+Event&total_seats=100&event_date=2026-11-20T18:00&registration_deadline=2026-11-25T18:00&csrf_token={}",
        urlencode(&org1_csrf)
    );
    let bad_date_req = Request::builder()
        .method("POST")
        .uri("/events/create")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &org1_cookie)
        .body(axum::body::Body::from(bad_date_form))
        .unwrap();
    let bad_date_resp = app.clone().oneshot(bad_date_req).await.unwrap();
    assert_eq!(bad_date_resp.status(), StatusCode::BAD_REQUEST);
    let bad_date_body = to_bytes(bad_date_resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&bad_date_body)
        .contains("Registration deadline cannot be after the event date."));

    // 4. Successful event creation with FORGED organizer_id in form
    // Attacker submits organizer_id=999 to try and attribute event to someone else
    let valid_form = format!(
        "title=Forged+ID+Test+Event&total_seats=75&ticket_price=199.00&event_date=2026-11-30T10:00&organizer_id=999&csrf_token={}",
        urlencode(&org1_csrf)
    );
    let valid_req = Request::builder()
        .method("POST")
        .uri("/events/create")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &org1_cookie)
        .body(axum::body::Body::from(valid_form))
        .unwrap();
    let valid_resp = app.clone().oneshot(valid_req).await.unwrap();
    assert_eq!(
        valid_resp.status(),
        StatusCode::SEE_OTHER,
        "Successful creation redirects to /dashboard"
    );

    // Verify in DB that organizer_id is authenticated user 2, NOT forged 999
    let created_event: Event = sqlx::query_as(
        "SELECT e.id, e.organizer_id, e.category_id, e.title, e.description, \
                e.venue, e.city, e.event_date, e.registration_deadline, \
                e.total_seats, e.available_seats, e.ticket_price, e.banner_image, \
                e.status, e.created_at, e.updated_at, \
                u.full_name AS organizer_name, u.email AS organizer_email, \
                c.name AS category_name, c.icon AS category_icon \
         FROM events e \
         JOIN users u ON e.organizer_id = u.id \
         LEFT JOIN categories c ON e.category_id = c.id \
         WHERE e.title = 'Forged ID Test Event' ORDER BY e.id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("Created event must exist in DB");

    let event_id = created_event.id;
    assert_eq!(
        created_event.organizer_id, 2,
        "Organizer ID must be authenticated session user ID 2, ignoring forged form field"
    );

    // D. Cross-Organizer HTTP Rejections
    // Create a temporary secondary organizer account and log in
    let hashed_pw2 = User::hash_password("SecOrg2@Password123").unwrap();
    let sec_org_email = format!(
        "sec_org_{}@eventpro.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let sec_org_id = User::create(
        &pool,
        "Secondary Organizer",
        &sec_org_email,
        &hashed_pw2,
        "organizer",
    )
    .await
    .expect("Create secondary organizer failed");

    let (org2_cookie, org2_csrf) = http_login(&app, &sec_org_email, "SecOrg2@Password123").await;

    // 1. Organizer 2 accessing GET /events/:id/edit for Organizer 1's event -> 403 Forbidden!
    let edit_forbidden_req = Request::builder()
        .uri(format!("/events/{}/edit", event_id))
        .header(header::COOKIE, &org2_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let edit_forbidden_resp = app.clone().oneshot(edit_forbidden_req).await.unwrap();
    assert_eq!(
        edit_forbidden_resp.status(),
        StatusCode::FORBIDDEN,
        "Organizer 2 must be forbidden from editing Organizer 1's event"
    );

    // 2. Organizer 2 submitting POST /events/:id/edit for Organizer 1's event -> 403 Forbidden!
    let update_forbidden_form = format!(
        "title=Hacked+Event&total_seats=100&event_date=2026-11-30T10:00&csrf_token={}",
        urlencode(&org2_csrf)
    );
    let update_forbidden_req = Request::builder()
        .method("POST")
        .uri(format!("/events/{}/edit", event_id))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &org2_cookie)
        .body(axum::body::Body::from(update_forbidden_form))
        .unwrap();
    let update_forbidden_resp = app.clone().oneshot(update_forbidden_req).await.unwrap();
    assert_eq!(
        update_forbidden_resp.status(),
        StatusCode::FORBIDDEN,
        "Organizer 2 must be forbidden from updating Organizer 1's event"
    );

    // 3. Organizer 2 submitting POST /events/:id/toggle-status -> 403 Forbidden!
    let toggle_forbidden_form = format!("csrf_token={}", urlencode(&org2_csrf));
    let toggle_forbidden_req = Request::builder()
        .method("POST")
        .uri(format!("/events/{}/toggle-status", event_id))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &org2_cookie)
        .body(axum::body::Body::from(toggle_forbidden_form))
        .unwrap();
    let toggle_forbidden_resp = app.clone().oneshot(toggle_forbidden_req).await.unwrap();
    assert_eq!(
        toggle_forbidden_resp.status(),
        StatusCode::FORBIDDEN,
        "Organizer 2 must be forbidden from toggling Organizer 1's event status"
    );

    // 4. Organizer 2 submitting POST /events/:id/delete -> 403 Forbidden!
    let delete_forbidden_form = format!("csrf_token={}", urlencode(&org2_csrf));
    let delete_forbidden_req = Request::builder()
        .method("POST")
        .uri(format!("/events/{}/delete", event_id))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &org2_cookie)
        .body(axum::body::Body::from(delete_forbidden_form))
        .unwrap();
    let delete_forbidden_resp = app.clone().oneshot(delete_forbidden_req).await.unwrap();
    assert_eq!(
        delete_forbidden_resp.status(),
        StatusCode::FORBIDDEN,
        "Organizer 2 must be forbidden from deleting Organizer 1's event"
    );

    // E. Admin Management Permissions: Admin CAN edit and toggle Organizer 1's event
    let (admin_cookie, admin_csrf) = http_login(&app, "admin@eventpro.com", "Admin@1234").await;

    // Admin accessing /admin/dashboard -> 200 OK
    let admin_dash_ok_req = Request::builder()
        .uri("/admin/dashboard")
        .header(header::COOKIE, &admin_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let admin_dash_ok_resp = app.clone().oneshot(admin_dash_ok_req).await.unwrap();
    assert_eq!(admin_dash_ok_resp.status(), StatusCode::OK);

    // Admin accessing GET /events/:id/edit for Organizer 1's event -> 200 OK
    let admin_edit_req = Request::builder()
        .uri(format!("/events/{}/edit", event_id))
        .header(header::COOKIE, &admin_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let admin_edit_resp = app.clone().oneshot(admin_edit_req).await.unwrap();
    assert_eq!(
        admin_edit_resp.status(),
        StatusCode::OK,
        "Admin must have access to edit any event"
    );

    // Admin toggling status via POST /events/:id/toggle-status -> 303 Redirect to /dashboard
    let admin_toggle_form = format!("csrf_token={}", urlencode(&admin_csrf));
    let admin_toggle_req = Request::builder()
        .method("POST")
        .uri(format!("/events/{}/toggle-status", event_id))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &admin_cookie)
        .body(axum::body::Body::from(admin_toggle_form))
        .unwrap();
    let admin_toggle_resp = app.clone().oneshot(admin_toggle_req).await.unwrap();
    assert_eq!(admin_toggle_resp.status(), StatusCode::SEE_OTHER);

    // Verify status was changed in DB to 'published'
    let toggled_db_event = Event::find_by_id(&pool, event_id).await.unwrap().unwrap();
    assert_eq!(toggled_db_event.status, "published");

    // F. Clean up: Organizer 1 deletes their test event
    let delete_form = format!("csrf_token={}", urlencode(&org1_csrf));
    let delete_req = Request::builder()
        .method("POST")
        .uri(format!("/events/{}/delete", event_id))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &org1_cookie)
        .body(axum::body::Body::from(delete_form))
        .unwrap();
    let delete_resp = app.clone().oneshot(delete_req).await.unwrap();
    assert_eq!(delete_resp.status(), StatusCode::SEE_OTHER);

    let deleted_check = Event::find_by_id(&pool, event_id).await.unwrap();
    assert!(deleted_check.is_none());

    // Clean up temporary user
    let _ = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(sec_org_id)
        .execute(&pool)
        .await;
}
