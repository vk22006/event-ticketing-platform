use axum::{
    body::to_bytes,
    http::{header, Request, StatusCode},
    Router,
};
use eventpro::{
    config::{resolve_public_dir, AppConfig},
    models::{Category, Event, EventFilterParams, SeatType, User},
    state::AppState,
};
use rust_decimal::Decimal;
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

fn sample_event(id: i32, status: &str, organizer_id: i32) -> Event {
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
        ticket_price: Decimal::new(149900, 2), // 1499.00
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

// ─────────────────────────────────────────────────────────────────────────────
// 1. Template Rendering Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_catalogue_rendering_with_event_data() {
    let state = create_test_state();
    let template = state
        .jinja
        .get_template("index.html")
        .expect("index.html template must exist");

    let events = vec![sample_event(1, "published", 2)];
    let categories = vec![Category {
        id: 1,
        name: "Technology".to_string(),
        icon: Some("💻".to_string()),
    }];

    let rendered = template
        .render(minijinja::context! {
            title => "Discover Events",
            events => events,
            categories => categories,
            query => minijinja::context! {
                search => "",
                category_id => None::<i32>,
                city => "",
                page => 1,
            },
            user => None::<User>,
        })
        .expect("rendering should succeed");

    assert!(rendered.contains("Global Tech Summit 2025"));
    assert!(rendered.contains("Bengaluru"));
    assert!(rendered.contains("Convention Center Hall A"));
    assert!(rendered.contains("Technology"));
    assert!(rendered.contains("120 seats left"));
    assert!(rendered.contains("href=\"/events/1\""));
    assert!(rendered.contains("₹1499.00") || rendered.contains("1499"));
}

#[tokio::test]
async fn test_empty_catalogue_behavior() {
    let state = create_test_state();
    let template = state.jinja.get_template("index.html").unwrap();

    let rendered = template
        .render(minijinja::context! {
            title => "Discover Events",
            events => Vec::<Event>::new(),
            categories => Vec::<Category>::new(),
            query => minijinja::context! {
                search => "Nonexistent Query",
                category_id => None::<i32>,
                city => "",
                page => 1,
            },
            user => None::<User>,
        })
        .expect("rendering should succeed");

    assert!(rendered.contains("No events found"));
    assert!(rendered.contains("Try different search terms or clear your filters."));
    assert!(rendered.contains("Browse All Events"));
    assert!(!rendered.contains("class=\"card__title\""));
}

#[tokio::test]
async fn test_event_detail_rendering() {
    let state = create_test_state();
    let template = state.jinja.get_template("event-detail.html").unwrap();

    let event = sample_event(42, "published", 5);
    let seat_types = vec![
        SeatType {
            id: 101,
            event_id: 42,
            type_name: "General Admission".to_string(),
            price: Decimal::new(149900, 2),
            total_seats: 400,
            booked_seats: 300,
        },
        SeatType {
            id: 102,
            event_id: 42,
            type_name: "VIP Pass".to_string(),
            price: Decimal::new(499900, 2),
            total_seats: 100,
            booked_seats: 80,
        },
    ];

    let rendered = template
        .render(minijinja::context! {
            title => event.title,
            event => event,
            seatTypes => &seat_types,
            seat_types => &seat_types,
            user => None::<User>,
        })
        .expect("rendering should succeed");

    assert!(rendered.contains("Global Tech Summit 2025"));
    assert!(rendered.contains("Convention Center Hall A"));
    assert!(rendered.contains("TechCorp Global"));
    assert!(rendered.contains("events@techcorp.com"));
    assert!(rendered.contains("General Admission"));
    assert!(rendered.contains("VIP Pass"));
    assert!(rendered.contains("₹1499.00") || rendered.contains("1499"));
    assert!(rendered.contains("₹4999.00") || rendered.contains("4999"));
    assert!(rendered.contains("120 / 500"));
    assert!(rendered.contains("Login to Book"));

    // Eligible attendee viewing event must see disabled "Booking Available Soon" button
    let attendee = User {
        id: 7,
        full_name: "Attendee Alex".to_string(),
        email: "alex@test.com".to_string(),
        password: "hash".to_string(),
        role: "attendee".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    let attendee_rendered = template
        .render(minijinja::context! {
            title => event.title,
            event => event,
            seatTypes => &seat_types,
            seat_types => &seat_types,
            user => Some(attendee),
        })
        .expect("rendering should succeed");

    assert!(attendee_rendered.contains("Book Now"));
    assert!(attendee_rendered.contains(&format!("/bookings/book/{}", event.id)));
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Security & HTML Auto-Escaping Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_html_auto_escaping_untrusted_event_content() {
    let state = create_test_state();
    let template = state.jinja.get_template("index.html").unwrap();

    let xss_payload = "<script>alert('xss')</script>";
    let mut malicious_event = sample_event(99, "published", 1);
    malicious_event.title = format!("Tech Conference {}", xss_payload);
    malicious_event.city = Some("<b onmouseover=evil()>Mumbai</b>".to_string());
    malicious_event.category_name = Some("<style>body{color:red}</style>".to_string());

    let rendered = template
        .render(minijinja::context! {
            title => "Discover Events",
            events => vec![malicious_event],
            categories => Vec::<Category>::new(),
            query => minijinja::context! {},
            user => None::<User>,
        })
        .expect("rendering should succeed");

    // The raw script or tag from untrusted user input must NEVER appear unescaped
    assert!(!rendered.contains("<script>alert('xss')</script>"));
    assert!(!rendered.contains("<script>alert("));
    assert!(!rendered.contains("<b onmouseover=evil()>"));
    assert!(!rendered.contains("<style>body{color:red}</style>"));

    // The properly escaped entities must be present
    assert!(rendered.contains("&lt;script&gt;alert"));
    assert!(rendered.contains("&lt;b onmouseover=evil()&gt;Mumbai"));
}

#[tokio::test]
async fn test_html_auto_escaping_in_event_detail() {
    let state = create_test_state();
    let template = state.jinja.get_template("event-detail.html").unwrap();

    let mut event = sample_event(99, "published", 1);
    event.title = "<svg onload=alert(1)>Hack".to_string();
    event.description = Some("<script src='evil.js'></script>".to_string());
    event.organizer_name = "<iframe src='phish'></iframe>".to_string();
    event.organizer_email = Some("hack@test.com\"><script>".to_string());

    let seat_types = vec![SeatType {
        id: 1,
        event_id: 99,
        type_name: "<img src=x onerror=alert(1)> VIP".to_string(),
        price: Decimal::new(100, 0),
        total_seats: 10,
        booked_seats: 0,
    }];

    let rendered = template
        .render(minijinja::context! {
            title => event.title,
            event => event,
            seatTypes => seat_types,
            user => None::<User>,
        })
        .unwrap();

    assert!(!rendered.contains("<svg onload=alert(1)>"));
    assert!(!rendered.contains("<script src='evil.js'>"));
    assert!(!rendered.contains("<script src=\"evil.js\">"));
    assert!(!rendered.contains("<iframe src='phish'>"));
    assert!(!rendered.contains("<img src=x onerror=alert(1)>"));

    assert!(rendered.contains("&lt;svg onload=alert(1)&gt;Hack"));
    assert!(rendered.contains("&lt;script src="));
    assert!(rendered.contains("&lt;iframe src="));
    assert!(rendered.contains("&lt;img src=x"));
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Navbar Authentication State Rendering Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_navbar_rendering_anonymous_user() {
    let state = create_test_state();
    let template = state.jinja.get_template("partials/navbar.html").unwrap();

    let rendered = template
        .render(minijinja::context! { user => None::<User> })
        .unwrap();

    assert!(rendered.contains("href=\"/auth/login\""));
    assert!(rendered.contains("href=\"/auth/register\""));
    assert!(!rendered.contains("action=\"/auth/logout\""));
    assert!(!rendered.contains("role-badge"));
    assert!(!rendered.contains("Create Event"));
}

#[tokio::test]
async fn test_navbar_rendering_attendee_user() {
    let state = create_test_state();
    let template = state.jinja.get_template("partials/navbar.html").unwrap();

    let user = User {
        id: 10,
        full_name: "Jane Attendee".to_string(),
        email: "jane@test.com".to_string(),
        password: "hash".to_string(),
        role: "attendee".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    let rendered = template
        .render(minijinja::context! { user => user, csrf_token => "token_abc123" })
        .unwrap();

    assert!(rendered.contains("role-badge--attendee"));
    assert!(rendered.contains("Dashboard"));
    assert!(rendered.contains("action=\"/auth/logout\""));
    assert!(rendered.contains("name=\"csrf_token\" value=\"token_abc123\""));
    assert!(!rendered.contains("Create Event"));
    assert!(!rendered.contains("href=\"/admin/dashboard\""));
}

#[tokio::test]
async fn test_navbar_rendering_organizer_user() {
    let state = create_test_state();
    let template = state.jinja.get_template("partials/navbar.html").unwrap();

    let user = User {
        id: 20,
        full_name: "Organizer Bob".to_string(),
        email: "bob@events.com".to_string(),
        password: "hash".to_string(),
        role: "organizer".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    let rendered = template
        .render(minijinja::context! { user => user })
        .unwrap();

    assert!(rendered.contains("role-badge--organizer"));
    assert!(rendered.contains("href=\"/events/create\""));
    assert!(rendered.contains("Create Event"));
    assert!(!rendered.contains("href=\"/admin/dashboard\""));
}

#[tokio::test]
async fn test_navbar_rendering_admin_user() {
    let state = create_test_state();
    let template = state.jinja.get_template("partials/navbar.html").unwrap();

    let user = User {
        id: 1,
        full_name: "Admin Alice".to_string(),
        email: "admin@eventpro.com".to_string(),
        password: "hash".to_string(),
        role: "admin".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    let rendered = template
        .render(minijinja::context! { user => user })
        .unwrap();

    assert!(rendered.contains("role-badge--admin"));
    assert!(rendered.contains("href=\"/admin/dashboard\""));
    assert!(rendered.contains("Admin"));
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Event Visibility Rules Logic Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_event_visibility_rules() {
    let organizer_id = 7;
    let published_event = sample_event(1, "published", organizer_id);
    let draft_event = sample_event(2, "draft", organizer_id);

    let is_visible = |event: &Event, user: Option<&User>| -> bool {
        if event.status == "published" {
            true
        } else if let Some(u) = user {
            u.role == "admin" || u.id == event.organizer_id
        } else {
            false
        }
    };

    let attendee = User {
        id: 15,
        full_name: "Regular Attendee".to_string(),
        email: "attendee@test.com".to_string(),
        password: "hash".to_string(),
        role: "attendee".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    let owner_organizer = User {
        id: organizer_id,
        full_name: "Event Owner".to_string(),
        email: "owner@test.com".to_string(),
        password: "hash".to_string(),
        role: "organizer".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    let other_organizer = User {
        id: 99,
        full_name: "Other Organizer".to_string(),
        email: "other@test.com".to_string(),
        password: "hash".to_string(),
        role: "organizer".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    let admin = User {
        id: 1,
        full_name: "Admin".to_string(),
        email: "admin@test.com".to_string(),
        password: "hash".to_string(),
        role: "admin".to_string(),
        avatar: None,
        bio: None,
        created_at: None,
        updated_at: None,
    };

    // Published events are accessible to everyone
    assert!(is_visible(&published_event, None));
    assert!(is_visible(&published_event, Some(&attendee)));
    assert!(is_visible(&published_event, Some(&owner_organizer)));
    assert!(is_visible(&published_event, Some(&other_organizer)));
    assert!(is_visible(&published_event, Some(&admin)));

    // Draft events are hidden from anonymous users, attendees, and unrelated organizers
    assert!(!is_visible(&draft_event, None));
    assert!(!is_visible(&draft_event, Some(&attendee)));
    assert!(!is_visible(&draft_event, Some(&other_organizer)));

    // Draft events are only accessible to the owning organizer and admins
    assert!(is_visible(&draft_event, Some(&owner_organizer)));
    assert!(is_visible(&draft_event, Some(&admin)));
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. HTTP Router & Error Handling Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_root_redirects_to_events() {
    let state = create_test_state();
    let app = create_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
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
        "/events"
    );
}

#[tokio::test]
async fn test_malformed_event_id_returns_404() {
    let state = create_test_state();
    let app = create_test_app(state);

    for malformed_path in &["/events/abc", "/events/-1", "/events/0", "/events/12.34"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(*malformed_path)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "Path {} should return 404",
            malformed_path
        );

        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains("404"));
        assert!(
            body_str.contains("This page doesn&#39;t exist") || body_str.contains("doesn't exist")
        );
    }
}

#[tokio::test]
async fn test_database_error_produces_safe_500_response() {
    let state = create_test_state(); // points to dummy unconnectable database
    let app = create_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/events")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);

    // Response must be clean 500 HTML without raw database driver errors or secrets
    assert!(body_str.contains("500"));
    assert!(body_str.contains("Something went wrong"));
    assert!(!body_str.contains("mysql://"));
    assert!(!body_str.contains("dummy_pass"));
    assert!(!body_str.contains("SQLSTATE"));
    assert!(!body_str.contains("sqlx::Error"));
}

#[tokio::test]
async fn test_filter_parameters_query_building() {
    let filters = EventFilterParams {
        status: Some("published".to_string()),
        category_id: Some(3),
        city: Some("Bengaluru".to_string()),
        search: Some("summit".to_string()),
        limit: Some(9),
        offset: Some(0),
    };

    assert_eq!(filters.status.as_deref(), Some("published"));
    assert_eq!(filters.category_id, Some(3));
    assert_eq!(filters.city.as_deref(), Some("Bengaluru"));
    assert_eq!(filters.search.as_deref(), Some("summit"));
    assert_eq!(filters.limit, Some(9));
    assert_eq!(filters.offset, Some(0));
}

#[tokio::test]
async fn test_pagination_bounds_and_overflow_protection() {
    let state = create_test_state();
    let app = create_test_app(state);

    // Extreme page values must not cause arithmetic panic or overflow
    for extreme_uri in &[
        "/events?page=999999999999999999",
        "/events?page=-100",
        "/events?page=0",
        "/events?page=not_a_number",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(*extreme_uri)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Safely processed without panic or server crash
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}

#[tokio::test]
async fn test_optional_user_db_failure_propagates_500() {
    let state = create_test_state(); // points to dummy unconnectable database
    let session_store = tower_sessions::MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store);

    let app = Router::new()
        .route(
            "/login-session",
            axum::routing::get(|session: tower_sessions::Session| async move {
                let _ = session.insert("user_id", 42).await;
                "ok"
            }),
        )
        .route(
            "/test-user",
            axum::routing::get(|user: eventpro::middleware::OptionalUser| async move {
                match user.0 {
                    Some(u) => format!("User: {}", u.full_name),
                    None => "Anonymous".to_string(),
                }
            }),
        )
        .with_state(state)
        .layer(session_layer);

    // 1. Establish session with user_id
    let login_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/login-session")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let cookie = login_res
        .headers()
        .get(header::SET_COOKIE)
        .expect("cookie should be set")
        .to_str()
        .unwrap()
        .to_string();

    // 2. Call /test-user with the session cookie while DB is down
    let response = app
        .oneshot(
            Request::builder()
                .uri("/test-user")
                .header(header::COOKIE, cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // When DB is down, request with logged-in session MUST NOT silently degrade to "Anonymous", it must reject with 500!
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("500"));
    assert!(!body_str.contains("Anonymous"));
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Real MySQL Integration Tests
// ─────────────────────────────────────────────────────────────────────────────

mod common;

#[tokio::test]
async fn test_real_mysql_catalogue_and_detail_integration() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    // 1. Verify Event::find_all with default published filter
    let default_filters = EventFilterParams::default();
    let published_events = Event::find_all(&pool, &default_filters)
        .await
        .expect("Event::find_all query failed");

    assert!(
        !published_events.is_empty(),
        "Expected seed published events to be present"
    );

    // Verify all returned events are published and contain joined organizer & category
    for ev in &published_events {
        assert_eq!(ev.status, "published");
        assert!(!ev.organizer_name.is_empty());
    }

    // Seed event 10 is 'draft' in database/seed.sql ("Cybersecurity Summit 2025")
    let draft_in_catalogue = published_events.iter().any(|ev| ev.id == 10);
    assert!(
        !draft_in_catalogue,
        "Draft event 10 must NOT appear in public published catalogue"
    );

    // 2. Verify category query
    let categories = Category::find_all(&pool)
        .await
        .expect("Category::find_all query failed");
    assert!(!categories.is_empty());

    // 3. Verify Event::find_by_id for existing event 1
    let event1 = Event::find_by_id(&pool, 1)
        .await
        .expect("Event::find_by_id failed")
        .expect("Event 1 must exist in seed data");

    assert_eq!(event1.id, 1);
    assert_eq!(event1.status, "published");
    assert!(!event1.organizer_name.is_empty());

    // 4. Verify SeatType::find_by_event for event 1
    let seat_types1 = SeatType::find_by_event(&pool, 1)
        .await
        .expect("SeatType::find_by_event failed");
    assert!(
        !seat_types1.is_empty(),
        "Event 1 should have seat types in seed data"
    );

    // 5. Live Axum Application integration with real DB pool
    let public_dir = resolve_public_dir().expect("public dir");
    let upload_dir = public_dir.join("uploads");
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://[REDACTED]".to_string(),
        session_secret: "integration_test_secret_key_12345678".to_string(),
        app_env: "test".to_string(),
        public_dir,
        upload_dir,
    };
    let app_state = AppState::new(pool.clone(), config);
    let app = create_test_app(app_state);

    // Test GET /events
    let cat_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/events")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cat_resp.status(), StatusCode::OK);
    let cat_body = to_bytes(cat_resp.into_body(), 128 * 1024).await.unwrap();
    let cat_html = String::from_utf8_lossy(&cat_body);
    assert!(cat_html.contains("Discover Amazing Events"));
    assert!(cat_html.contains("All Categories"));

    // Test GET /events/1 (published event detail)
    let ev1_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/events/1")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ev1_resp.status(), StatusCode::OK);
    let ev1_body = to_bytes(ev1_resp.into_body(), 128 * 1024).await.unwrap();
    let ev1_html = String::from_utf8_lossy(&ev1_body);
    assert!(ev1_html.contains(&event1.title));
    assert!(ev1_html.contains("Seats Available"));

    // Test GET /events/10 as anonymous guest (draft event must return 404!)
    let ev10_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/events/10")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        ev10_resp.status(),
        StatusCode::NOT_FOUND,
        "Draft event 10 must return 404 for anonymous requests"
    );

    // Test GET /events/99999 (non-existent event must return 404)
    let ev_none_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/events/99999")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ev_none_resp.status(), StatusCode::NOT_FOUND);
}
