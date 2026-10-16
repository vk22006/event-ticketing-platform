mod common;

use axum::http::StatusCode;
use common::{create_test_app, create_test_state, get_response, sample_event, sample_user};
use eventpro::models::{Category, Event, EventFilterParams, SeatType, User};

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
    assert!(rendered.contains("1499"));
}

#[tokio::test]
async fn test_empty_catalogue_behavior() {
    let state = create_test_state();
    let template = state.jinja.get_template("index.html").expect("index.html");

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
    let template = state
        .jinja
        .get_template("event-detail.html")
        .expect("event-detail.html");

    let event = sample_event(1, "published", 2);
    let seat_types = vec![SeatType {
        id: 10,
        event_id: 1,
        type_name: "VIP".to_string(),
        price: rust_decimal::Decimal::new(299900, 2),
        total_seats: 50,
        booked_seats: 10,
    }];

    let rendered = template
        .render(minijinja::context! {
            title => event.title.clone(),
            event => event,
            seatTypes => &seat_types,
            seat_types => &seat_types,
            user => None::<User>,
            authenticated_user => None::<User>,
            is_organizer => false,
            is_admin => false,
            can_edit => false,
            can_book => true,
            csrf_token => "dummy_csrf_token",
            error => None::<String>,
        })
        .expect("rendering should succeed");

    assert!(rendered.contains("Global Tech Summit 2025"));
    assert!(rendered.contains("VIP"));
    assert!(rendered.contains("Convention Center Hall A"));
    assert!(rendered.contains("Login to Book"));
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. XSS Auto-Escaping Security Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_html_auto_escaping_untrusted_event_content() {
    let state = create_test_state();
    let template = state.jinja.get_template("index.html").expect("index.html");

    let mut event = sample_event(1, "published", 2);
    event.title = "Tech Conference <script>alert('xss')</script>".to_string();
    event.venue = Some("<b onmouseover=evil()>Mumbai</b>".to_string());

    let rendered = template
        .render(minijinja::context! {
            title => "Discover Events",
            events => vec![event],
            categories => Vec::<Category>::new(),
            query => minijinja::context! { search => "", category_id => None::<i32>, city => "", page => 1 },
            user => None::<User>,
        })
        .expect("rendering should succeed");

    assert!(!rendered.contains("<script>alert('xss')</script>"));
    assert!(!rendered.contains("<script>alert("));
    assert!(!rendered.contains("<b onmouseover=evil()>"));
    assert!(rendered.contains("&lt;script&gt;alert"));
    assert!(rendered.contains("&lt;b onmouseover=evil()&gt;Mumbai"));
}

#[tokio::test]
async fn test_html_auto_escaping_in_event_detail() {
    let state = create_test_state();
    let template = state
        .jinja
        .get_template("event-detail.html")
        .expect("event-detail.html");

    let mut event = sample_event(1, "published", 2);
    event.description = Some("<script src='evil.js'></script>".to_string());

    let rendered = template
        .render(minijinja::context! {
            title => "Detail",
            event => event,
            seatTypes => Vec::<SeatType>::new(),
            seat_types => Vec::<SeatType>::new(),
            user => None::<User>,
            csrf_token => "token",
        })
        .expect("rendering should succeed");

    assert!(!rendered.contains("<script src='evil.js'>"));
    assert!(rendered.contains("&lt;script src="));
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Navbar Rendering by Role (Consolidated)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_navbar_rendering_roles() {
    let state = create_test_state();
    let template = state
        .jinja
        .get_template("partials/navbar.html")
        .expect("navbar.html");

    // Anonymous
    let anon = template
        .render(minijinja::context! { user => None::<User> })
        .unwrap();
    assert!(anon.contains("href=\"/auth/login\""));
    assert!(!anon.contains("/auth/logout"));

    // Attendee
    let att = template
        .render(minijinja::context! { user => sample_user(3, "attendee"), csrf_token => "tok1" })
        .unwrap();
    assert!(att.contains("role-badge--attendee"));
    assert!(att.contains("Dashboard"));
    assert!(att.contains("/auth/logout"));
    assert!(!att.contains("Create Event"));
    assert!(!att.contains("/admin/dashboard"));

    // Organizer
    let org = template
        .render(minijinja::context! { user => sample_user(2, "organizer"), csrf_token => "tok2" })
        .unwrap();
    assert!(org.contains("role-badge--organizer"));
    assert!(org.contains("href=\"/events/create\""));
    assert!(!org.contains("/admin/dashboard"));

    // Admin
    let adm = template
        .render(minijinja::context! { user => sample_user(1, "admin"), csrf_token => "tok3" })
        .unwrap();
    assert!(adm.contains("role-badge--admin"));
    assert!(adm.contains("href=\"/events/create\""));
    assert!(adm.contains("href=\"/admin/dashboard\""));
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Event Visibility Rules Logic
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

    let attendee = sample_user(15, "attendee");
    let owner_organizer = sample_user(organizer_id, "organizer");
    let other_organizer = sample_user(99, "organizer");
    let admin = sample_user(1, "admin");

    assert!(is_visible(&published_event, None));
    assert!(is_visible(&published_event, Some(&attendee)));
    assert!(is_visible(&published_event, Some(&owner_organizer)));
    assert!(is_visible(&published_event, Some(&other_organizer)));
    assert!(is_visible(&published_event, Some(&admin)));

    assert!(!is_visible(&draft_event, None));
    assert!(!is_visible(&draft_event, Some(&attendee)));
    assert!(!is_visible(&draft_event, Some(&other_organizer)));
    assert!(is_visible(&draft_event, Some(&owner_organizer)));
    assert!(is_visible(&draft_event, Some(&admin)));
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. HTTP Router & Error Handling Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_root_redirects_to_events() {
    let app = create_test_app(create_test_state());
    let (status, headers, _) = get_response(&app, "/", None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers.get(axum::http::header::LOCATION).unwrap(),
        "/events"
    );
}

#[tokio::test]
async fn test_malformed_event_id_returns_404() {
    let app = create_test_app(create_test_state());
    for malformed_path in &["/events/abc", "/events/-1", "/events/0", "/events/12.34"] {
        let (status, _, body) = get_response(&app, malformed_path, None).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "Path {} should be 404",
            malformed_path
        );
        assert!(body.contains("404"));
    }
}

#[tokio::test]
async fn test_database_error_produces_safe_500_response() {
    let app = create_test_app(create_test_state());
    let (status, _, body) = get_response(&app, "/events", None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body.contains("500"));
    assert!(body.contains("Something went wrong"));
    assert!(!body.contains("mysql://"));
    assert!(!body.contains("SQLSTATE"));
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
}

#[tokio::test]
async fn test_pagination_bounds_and_overflow_protection() {
    let app = create_test_app(create_test_state());
    for extreme_uri in &[
        "/events?page=999999999999999999",
        "/events?page=-100",
        "/events?page=0",
        "/events?page=not_a_number",
    ] {
        let (status, _, _) = get_response(&app, extreme_uri, None).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}

#[tokio::test]
async fn test_optional_user_db_failure_propagates_500() {
    let state = create_test_state();
    let session_store = tower_sessions::MemoryStore::default();
    let session_layer =
        tower_sessions::SessionManagerLayer::new(session_store).with_name("eventpro_sid");

    let app = axum::Router::new()
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

    let (login_status, login_headers, _) = get_response(&app, "/login-session", None).await;
    assert_eq!(login_status, StatusCode::OK);
    let cookie = common::extract_cookie(&login_headers).expect("cookie should be set");

    let (status, _, body) = get_response(&app, "/test-user", Some(&cookie)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body.contains("500"));
    assert!(!body.contains("Anonymous"));
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Real MySQL Integration Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_real_mysql_catalogue_and_detail_integration() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let default_filters = EventFilterParams::default();
    let published_events = Event::find_all(&pool, &default_filters)
        .await
        .expect("Event::find_all query failed");

    assert!(!published_events.is_empty());
    for ev in &published_events {
        assert_eq!(ev.status, "published");
    }

    let draft_in_catalogue = published_events.iter().any(|ev| ev.id == 10);
    assert!(
        !draft_in_catalogue,
        "Draft event 10 must NOT appear in public published catalogue"
    );

    let categories = Category::find_all(&pool)
        .await
        .expect("Category::find_all failed");
    assert!(!categories.is_empty());

    let event1 = Event::find_by_id(&pool, 1)
        .await
        .unwrap()
        .expect("Event 1 must exist");
    assert_eq!(event1.id, 1);
    assert_eq!(event1.status, "published");

    let seat_types1 = SeatType::find_by_event(&pool, 1).await.unwrap();
    assert!(!seat_types1.is_empty());

    let public_dir = eventpro::config::resolve_public_dir().expect("public dir");
    let upload_dir = public_dir.join("uploads");
    let config = eventpro::config::AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://[REDACTED]".to_string(),
        session_secret: "integration_test_secret_key_12345678".to_string(),
        app_env: "test".to_string(),
        public_dir,
        upload_dir,
    };
    let app = create_test_app(eventpro::state::AppState::new(pool.clone(), config));

    let (status, headers, body) = get_response(&app, "/events", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(common::extract_cookie(&headers).is_none());
    assert!(body.contains("Discover Amazing Events"));

    let (ev1_status, _, ev1_body) = get_response(&app, "/events/1", None).await;
    assert_eq!(ev1_status, StatusCode::OK);
    assert!(ev1_body.contains(&event1.title));

    let (ev10_status, _, _) = get_response(&app, "/events/10", None).await;
    assert_eq!(ev10_status, StatusCode::NOT_FOUND);

    let (ev_none_status, _, _) = get_response(&app, "/events/99999", None).await;
    assert_eq!(ev_none_status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_anonymous_catalogue_does_not_create_session() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };
    let public_dir = eventpro::config::resolve_public_dir().expect("public dir");
    let config = eventpro::config::AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://[REDACTED]".to_string(),
        session_secret: "integration_test_secret_key_12345678".to_string(),
        app_env: "test".to_string(),
        public_dir: public_dir.clone(),
        upload_dir: public_dir.join("uploads"),
    };
    let app = create_test_app(eventpro::state::AppState::new(pool, config));

    let (s1, h1, _) = get_response(&app, "/events", None).await;
    assert_eq!(s1, StatusCode::OK);
    assert!(common::extract_cookie(&h1).is_none());

    let (s2, h2, _) = get_response(&app, "/events/1", None).await;
    assert_eq!(s2, StatusCode::OK);
    assert!(common::extract_cookie(&h2).is_none());
}
