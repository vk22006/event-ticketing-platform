use axum::{
    body::to_bytes,
    http::{header, Request, StatusCode},
    Router,
};
use eventpro::{
    config::{resolve_public_dir, AppConfig},
    models::{
        booking::{Booking, BookingDetail, BookingError, UserBooking},
        event::Event,
        seat_type::SeatType,
        ticket::{Ticket, TicketDetail, UserTicket},
        User,
    },
    state::AppState,
};
use rust_decimal::Decimal;
use std::collections::HashSet;
use std::time::Duration;
use tower::ServiceExt;
use tower_sessions::{Expiry, SessionManagerLayer};

fn urlencode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn create_test_state() -> AppState {
    let public_dir = resolve_public_dir().expect("public directory should resolve");
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://dummy_user:dummy_pass@127.0.0.1:9999/dummy_db".to_string(),
        session_secret: "phase5_test_session_secret_123456789".to_string(),
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

fn sample_event(id: i32, status: &str, available_seats: i32) -> Event {
    Event {
        id,
        organizer_id: 1,
        category_id: Some(1),
        title: "Phase 5 Test Event".to_string(),
        description: Some("Comprehensive booking tests".to_string()),
        venue: Some("Auditorium Hall".to_string()),
        city: Some("Bengaluru".to_string()),
        event_date: chrono::NaiveDate::from_ymd_opt(2026, 12, 1)
            .unwrap()
            .and_hms_opt(18, 0, 0)
            .unwrap(),
        registration_deadline: Some(
            chrono::NaiveDate::from_ymd_opt(2026, 11, 30)
                .unwrap()
                .and_hms_opt(23, 59, 59)
                .unwrap(),
        ),
        total_seats: 100,
        available_seats,
        ticket_price: Decimal::new(29900, 2),
        banner_image: None,
        status: status.to_string(),
        created_at: None,
        updated_at: None,
        organizer_name: "Event Organizer".to_string(),
        organizer_email: Some("org@example.com".to_string()),
        category_name: Some("Technology".to_string()),
        category_icon: Some("💻".to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Template Rendering & Ticket Code Unit Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_ticket_code_generation_format_and_uniqueness() {
    let mut codes = HashSet::new();
    let event_id = 42;
    let booking_id = 108;

    for _ in 0..1000 {
        let code = Ticket::generate_code(event_id, booking_id);
        assert!(
            code.starts_with(&format!("EVT-{}-BKG-{}-", event_id, booking_id)),
            "Ticket code must have prefix EVT-{{event_id}}-BKG-{{booking_id}}-"
        );

        let parts: Vec<&str> = code.split('-').collect();
        assert_eq!(
            parts.len(),
            5,
            "Ticket code must have 5 hyphen-separated segments"
        );
        assert_eq!(parts[0], "EVT");
        assert_eq!(parts[1], "42");
        assert_eq!(parts[2], "BKG");
        assert_eq!(parts[3], "108");

        let hex_suffix = parts[4];
        assert_eq!(
            hex_suffix.len(),
            8,
            "Ticket code suffix must be exactly 8 characters"
        );
        assert!(
            hex_suffix
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()),
            "Ticket code suffix must be uppercase hexadecimal"
        );

        let is_unique = codes.insert(code);
        assert!(
            is_unique,
            "Ticket code collision detected in 1000 generations"
        );
    }
}

#[tokio::test]
async fn test_booking_select_template_rendering() {
    let state = create_test_state();
    let template = state.jinja.get_template("booking/select.html").unwrap();

    let event = sample_event(10, "published", 25);
    let seat_types = vec![
        SeatType {
            id: 1,
            event_id: 10,
            type_name: "Silver Tier".to_string(),
            price: Decimal::new(19900, 2),
            total_seats: 50,
            booked_seats: 30,
        },
        SeatType {
            id: 2,
            event_id: 10,
            type_name: "Gold VIP".to_string(),
            price: Decimal::new(49900, 2),
            total_seats: 20,
            booked_seats: 5,
        },
    ];

    let rendered = template
        .render(minijinja::context! {
            title => "Book Tickets",
            event => event,
            seatTypes => seat_types,
            error => None::<String>,
            user => sample_user(1, "user"),
        })
        .expect("Select template should render cleanly");

    assert!(rendered.contains("Phase 5 Test Event"));
    assert!(rendered.contains("Silver Tier"));
    assert!(rendered.contains("Gold VIP"));
    assert!(rendered.contains("20 left")); // 50 - 30
    assert!(rendered.contains("15 left")); // 20 - 5
    assert!(rendered.contains("/bookings/payment"));
    assert!(rendered.contains("Continue to Payment →"));
}

#[tokio::test]
async fn test_booking_payment_template_rendering() {
    let state = create_test_state();
    let template = state.jinja.get_template("booking/payment.html").unwrap();

    let event = sample_event(10, "published", 25);
    let seat_type = SeatType {
        id: 1,
        event_id: 10,
        type_name: "Gold VIP".to_string(),
        price: Decimal::new(49900, 2),
        total_seats: 20,
        booked_seats: 5,
    };

    let rendered = template
        .render(minijinja::context! {
            title => "Payment",
            event => event,
            seatType => Some(seat_type),
            quantity => 2,
            total => Decimal::new(99800, 2),
            user => sample_user(1, "user"),
            csrf_token => "mock_csrf_payment_token_123",
        })
        .expect("Payment template should render cleanly");

    assert!(rendered.contains("Complete Payment"));
    assert!(rendered.contains("Gold VIP"));
    assert!(rendered.contains("2×"));
    assert!(rendered.contains("998"));
    assert!(rendered.contains("mock_csrf_payment_token_123"));
    assert!(rendered.contains("/bookings/confirm"));
}

#[tokio::test]
async fn test_booking_confirmation_template_rendering() {
    let state = create_test_state();
    let template = state
        .jinja
        .get_template("booking/confirmation.html")
        .unwrap();

    let event = sample_event(10, "published", 25);
    let booking = BookingDetail {
        id: 99,
        user_id: 1,
        event_id: 10,
        seat_type_id: Some(1),
        quantity: 2,
        total_amount: Decimal::new(99800, 2),
        status: "confirmed".to_string(),
        payment_method: Some("mock_payment".to_string()),
        booked_at: Some(chrono::Utc::now().naive_utc()),
        event_title: event.title.clone(),
        event_date: event.event_date,
        venue: event.venue.clone(),
        city: event.city.clone(),
        full_name: "John Attendee".to_string(),
        email: "john@example.com".to_string(),
        seat_type: Some("Gold VIP".to_string()),
    };

    let tickets = vec![
        Ticket {
            id: 101,
            booking_id: 99,
            user_id: 1,
            event_id: 10,
            ticket_code: "EVT-10-BKG-99-A1B2C3D4".to_string(),
            seat_number: None,
            status: "active".to_string(),
            issued_at: Some(chrono::Utc::now().naive_utc()),
        },
        Ticket {
            id: 102,
            booking_id: 99,
            user_id: 1,
            event_id: 10,
            ticket_code: "EVT-10-BKG-99-E5F60718".to_string(),
            seat_number: None,
            status: "active".to_string(),
            issued_at: Some(chrono::Utc::now().naive_utc()),
        },
    ];

    let rendered = template
        .render(minijinja::context! {
            title => "Booking Confirmed!",
            booking => booking,
            tickets => tickets,
            event => event,
            user => sample_user(1, "user"),
        })
        .expect("Confirmation template should render cleanly");

    assert!(rendered.contains("Booking Confirmed!"));
    assert!(rendered.contains("#99"));
    assert!(rendered.contains("EVT-10-BKG-99-A1B2C3D4"));
    assert!(rendered.contains("EVT-10-BKG-99-E5F60718"));
    assert!(rendered.contains("/tickets/my"));
}

#[tokio::test]
async fn test_my_bookings_and_my_tickets_templates_rendering() {
    let state = create_test_state();
    let b_template = state
        .jinja
        .get_template("dashboard/my-bookings.html")
        .unwrap();

    let bookings = vec![UserBooking {
        id: 77,
        user_id: 1,
        event_id: 10,
        seat_type_id: None,
        quantity: 1,
        total_amount: Decimal::new(29900, 2),
        status: "confirmed".to_string(),
        payment_method: Some("mock_payment".to_string()),
        booked_at: Some(chrono::Utc::now().naive_utc()),
        event_title: "Tech Conference".to_string(),
        event_date: chrono::NaiveDate::from_ymd_opt(2026, 12, 1)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap(),
        venue: Some("Main Hall".to_string()),
        city: Some("Bengaluru".to_string()),
        banner_image: None,
        seat_type: None,
    }];

    let b_rendered = b_template
        .render(minijinja::context! {
            title => "My Bookings",
            bookings => bookings,
            user => sample_user(1, "user"),
            csrf_token => "my_bookings_csrf_123",
        })
        .expect("My Bookings template should render cleanly");

    assert!(b_rendered.contains("My Bookings 📋"));
    assert!(b_rendered.contains("#77"));
    assert!(b_rendered.contains("Tech Conference"));
    assert!(b_rendered.contains("/bookings/77/cancel"));
    assert!(b_rendered.contains("my_bookings_csrf_123"));

    // My tickets template
    let t_template = state
        .jinja
        .get_template("dashboard/my-tickets.html")
        .unwrap();
    let tickets = vec![UserTicket {
        id: 55,
        booking_id: 77,
        user_id: 1,
        event_id: 10,
        ticket_code: "EVT-10-BKG-77-99AA88BB".to_string(),
        seat_number: None,
        status: "active".to_string(),
        issued_at: Some(chrono::Utc::now().naive_utc()),
        event_title: "Tech Conference".to_string(),
        event_date: chrono::NaiveDate::from_ymd_opt(2026, 12, 1)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap(),
        venue: Some("Main Hall".to_string()),
        city: Some("Bengaluru".to_string()),
        banner_image: None,
        total_amount: Decimal::new(29900, 2),
        quantity: 1,
        booking_status: "confirmed".to_string(),
    }];

    let t_rendered = t_template
        .render(minijinja::context! {
            title => "My Tickets",
            tickets => tickets,
            user => sample_user(1, "user"),
        })
        .expect("My Tickets template should render cleanly");

    assert!(t_rendered.contains("My Tickets 🎟️"));
    assert!(t_rendered.contains("EVT-10-BKG-77-99AA88BB"));
    assert!(t_rendered.contains("/tickets/55"));
    assert!(t_rendered.contains("/tickets/55/print"));
}

#[tokio::test]
async fn test_ticket_view_and_print_templates_rendering() {
    let state = create_test_state();
    let v_template = state.jinja.get_template("ticket/view.html").unwrap();
    let p_template = state.jinja.get_template("ticket/print.html").unwrap();

    let ticket = TicketDetail {
        id: 12,
        booking_id: 34,
        user_id: 1,
        event_id: 10,
        ticket_code: "EVT-10-BKG-34-FFEEDDCC".to_string(),
        seat_number: None,
        status: "active".to_string(),
        issued_at: Some(chrono::Utc::now().naive_utc()),
        event_title: "Rock Festival".to_string(),
        event_date: chrono::NaiveDate::from_ymd_opt(2026, 12, 5)
            .unwrap()
            .and_hms_opt(19, 0, 0)
            .unwrap(),
        venue: Some("Open Grounds".to_string()),
        city: Some("Mumbai".to_string()),
        full_name: "Alice Smith".to_string(),
        email: "alice@example.com".to_string(),
        quantity: 1,
        total_amount: Decimal::new(49900, 2),
        booking_status: "confirmed".to_string(),
    };

    let v_rendered = v_template
        .render(minijinja::context! {
            title => "Ticket – EVT-10-BKG-34-FFEEDDCC",
            ticket => ticket,
            user => sample_user(1, "user"),
        })
        .expect("Ticket view template should render cleanly");

    assert!(v_rendered.contains("Rock Festival"));
    assert!(v_rendered.contains("EVT-10-BKG-34-FFEEDDCC"));
    assert!(v_rendered.contains("Alice Smith"));
    assert!(v_rendered.contains("/tickets/12/print"));

    let p_rendered = p_template
        .render(minijinja::context! {
            title => "Ticket – EVT-10-BKG-34-FFEEDDCC",
            ticket => ticket,
        })
        .expect("Print template should render cleanly");

    assert!(p_rendered.contains("EventPro — Official Ticket"));
    assert!(p_rendered.contains("EVT-10-BKG-34-FFEEDDCC"));
    assert!(p_rendered.contains("window.print()"));
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. HTTP Routing & Authorization Verification
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_unauthenticated_booking_access_rejected() {
    let state = create_test_state();
    let app = create_test_app(state);

    // GET /bookings/book/1 without session -> 303 Redirect to login
    let book_req = Request::builder()
        .uri("/bookings/book/1")
        .body(axum::body::Body::empty())
        .unwrap();
    let book_resp = app.clone().oneshot(book_req).await.unwrap();
    assert_eq!(book_resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        book_resp
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/auth/login"
    );

    // GET /bookings/payment without session -> 303 Redirect to login
    let pay_req = Request::builder()
        .uri("/bookings/payment?eventId=1")
        .body(axum::body::Body::empty())
        .unwrap();
    let pay_resp = app.clone().oneshot(pay_req).await.unwrap();
    assert_eq!(pay_resp.status(), StatusCode::SEE_OTHER);

    // POST /bookings/confirm without session -> 303 Redirect to login
    let conf_req = Request::builder()
        .method("POST")
        .uri("/bookings/confirm")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(axum::body::Body::from("event_id=1&quantity=1"))
        .unwrap();
    let conf_resp = app.clone().oneshot(conf_req).await.unwrap();
    assert_eq!(conf_resp.status(), StatusCode::SEE_OTHER);

    // GET /bookings/my without session -> 303 Redirect to login
    let my_req = Request::builder()
        .uri("/bookings/my")
        .body(axum::body::Body::empty())
        .unwrap();
    let my_resp = app.clone().oneshot(my_req).await.unwrap();
    assert_eq!(my_resp.status(), StatusCode::SEE_OTHER);

    // POST /bookings/1/cancel without session -> 303 Redirect to login
    let cancel_req = Request::builder()
        .method("POST")
        .uri("/bookings/1/cancel")
        .body(axum::body::Body::empty())
        .unwrap();
    let cancel_resp = app.clone().oneshot(cancel_req).await.unwrap();
    assert_eq!(cancel_resp.status(), StatusCode::SEE_OTHER);

    // GET /tickets/my without session -> 303 Redirect to login
    let t_req = Request::builder()
        .uri("/tickets/my")
        .body(axum::body::Body::empty())
        .unwrap();
    let t_resp = app.clone().oneshot(t_req).await.unwrap();
    assert_eq!(t_resp.status(), StatusCode::SEE_OTHER);
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Real MySQL Integration: Atomic Booking, Cancellation, Concurrency & Security
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_real_mysql_booking_workflow_lifecycle() {
    let _ = dotenvy::dotenv();
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => url,
        _ => {
            eprintln!("Skipping real MySQL booking tests: DATABASE_URL not configured");
            return;
        }
    };

    let pool = match sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&db_url)
        .await
    {
        Ok(p) => p,
        Err(err) => {
            eprintln!(
                "Skipping real MySQL tests: cannot connect to database: {}",
                err
            );
            return;
        }
    };

    let public_dir = resolve_public_dir().expect("public directory should resolve");
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: db_url.clone(),
        session_secret: "phase5_integration_session_secret_123456".to_string(),
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
        assert_eq!(login_resp.status(), StatusCode::SEE_OTHER);
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

    // Authenticate users
    // Charlie is attendee ("user"), Alice is "organizer", Admin is "admin"
    let (attendee_cookie, attendee_csrf) = http_login(&app, "charlie@mail.com", "User@1234").await;
    let (organizer_cookie, organizer_csrf) =
        http_login(&app, "alice@events.com", "Organizer@1").await;
    let (admin_cookie, _admin_csrf) = http_login(&app, "admin@eventpro.com", "Admin@1234").await;

    // --- Dynamic Fixture Setup ---
    let event_title = format!("Phase5 Test Event {}", rand::random::<u32>());
    let future_date = chrono::Utc::now().naive_utc() + chrono::Duration::days(30);
    let deadline = chrono::Utc::now().naive_utc() + chrono::Duration::days(25);

    let insert_event_res = sqlx::query(
        "INSERT INTO events (organizer_id, title, description, venue, city, event_date, registration_deadline, total_seats, available_seats, ticket_price, status) \
         VALUES (2, ?, 'Isolated event for Phase 5 tests', 'Test Venue', 'Bengaluru', ?, ?, 20, 20, 350.00, 'published')"
    )
    .bind(&event_title)
    .bind(future_date)
    .bind(deadline)
    .execute(&pool)
    .await
    .expect("Creating test event must succeed");

    let event_id = insert_event_res.last_insert_id() as i32;

    let insert_seat_res = sqlx::query(
        "INSERT INTO seat_types (event_id, type_name, price, total_seats, booked_seats) \
         VALUES (?, 'VIP Balcony', 500.00, 10, 0)",
    )
    .bind(event_id)
    .execute(&pool)
    .await
    .expect("Creating test seat type must succeed");

    let seat_type_id = insert_seat_res.last_insert_id() as i32;

    // A. Role check: Organizers and Admins are FORBIDDEN from booking
    let org_book_req = Request::builder()
        .uri(format!("/bookings/book/{}", event_id))
        .header(header::COOKIE, &organizer_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let org_book_resp = app.clone().oneshot(org_book_req).await.unwrap();
    assert_eq!(
        org_book_resp.status(),
        StatusCode::FORBIDDEN,
        "Organizers must not access booking selection form"
    );

    let admin_book_req = Request::builder()
        .uri(format!("/bookings/book/{}", event_id))
        .header(header::COOKIE, &admin_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let admin_book_resp = app.clone().oneshot(admin_book_req).await.unwrap();
    assert_eq!(
        admin_book_resp.status(),
        StatusCode::FORBIDDEN,
        "Admins must not access booking selection form"
    );

    let org_confirm_req = Request::builder()
        .method("POST")
        .uri("/bookings/confirm")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &organizer_cookie)
        .body(axum::body::Body::from(format!(
            "event_id={}&quantity=1&csrf_token={}",
            event_id,
            urlencode(&organizer_csrf)
        )))
        .unwrap();
    let org_confirm_resp = app.clone().oneshot(org_confirm_req).await.unwrap();
    assert_eq!(
        org_confirm_resp.status(),
        StatusCode::FORBIDDEN,
        "Organizers must be forbidden from confirming bookings"
    );

    // B. CSRF protection verification
    let bad_csrf_req = Request::builder()
        .method("POST")
        .uri("/bookings/confirm")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::from(format!(
            "event_id={}&quantity=2&csrf_token=INVALID_CSRF_TOKEN",
            event_id
        )))
        .unwrap();
    let bad_csrf_resp = app.clone().oneshot(bad_csrf_req).await.unwrap();
    assert_eq!(
        bad_csrf_resp.status(),
        StatusCode::FORBIDDEN,
        "Confirming booking with invalid CSRF token must return 403"
    );

    // C. Validation: Quantity bounds (0 and 11)
    let qty_zero_req = Request::builder()
        .method("POST")
        .uri("/bookings/confirm")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::from(format!(
            "event_id={}&quantity=0&csrf_token={}",
            event_id,
            urlencode(&attendee_csrf)
        )))
        .unwrap();
    let qty_zero_resp = app.clone().oneshot(qty_zero_req).await.unwrap();
    assert_eq!(
        qty_zero_resp.status(),
        StatusCode::BAD_REQUEST,
        "Quantity 0 must be rejected with 400"
    );

    let qty_eleven_req = Request::builder()
        .method("POST")
        .uri("/bookings/confirm")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::from(format!(
            "event_id={}&quantity=11&csrf_token={}",
            event_id,
            urlencode(&attendee_csrf)
        )))
        .unwrap();
    let qty_eleven_resp = app.clone().oneshot(qty_eleven_req).await.unwrap();
    assert_eq!(
        qty_eleven_resp.status(),
        StatusCode::BAD_REQUEST,
        "Quantity > 10 must be rejected with 400"
    );

    // D. Validation: Invalid seat type belonging to another event
    let alien_seat_req = Request::builder()
        .method("POST")
        .uri("/bookings/confirm")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::from(format!(
            "event_id={}&seat_type_id=999999&quantity=2&csrf_token={}",
            event_id,
            urlencode(&attendee_csrf)
        )))
        .unwrap();
    let alien_seat_resp = app.clone().oneshot(alien_seat_req).await.unwrap();
    assert_eq!(
        alien_seat_resp.status(),
        StatusCode::BAD_REQUEST,
        "Seat type belonging to nonexistent or foreign event must be rejected"
    );

    // E. Successful Booking Creation (Quantity = 3)
    // Server must calculate price from database (500.00 * 3 = 1500.00), ignoring any tampered price
    let valid_booking_req = Request::builder()
        .method("POST")
        .uri("/bookings/confirm")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::from(format!(
            "event_id={}&seat_type_id={}&quantity=3&total=0.01&csrf_token={}",
            event_id,
            seat_type_id,
            urlencode(&attendee_csrf)
        )))
        .unwrap();
    let valid_booking_resp = app.clone().oneshot(valid_booking_req).await.unwrap();
    assert_eq!(
        valid_booking_resp.status(),
        StatusCode::OK,
        "Valid booking confirmation must render confirmation page"
    );
    let confirm_body = to_bytes(valid_booking_resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let confirm_html = String::from_utf8_lossy(&confirm_body);
    assert!(confirm_html.contains("Booking Confirmed!"));

    // Verify Database Invariants
    let booking_row: (i32, Decimal, String, i32) = sqlx::query_as(
        "SELECT id, total_amount, status, quantity FROM bookings WHERE event_id = ? AND user_id = 3 ORDER BY id DESC LIMIT 1"
    )
    .bind(event_id)
    .fetch_one(&pool)
    .await
    .expect("Booking must exist in DB");

    let booking_id = booking_row.0;
    assert_eq!(
        booking_row.1,
        Decimal::new(150000, 2),
        "Total amount must be server-calculated (500.00 * 3 = 1500.00), ignoring tampered 0.01"
    );
    assert_eq!(booking_row.2, "confirmed");
    assert_eq!(booking_row.3, 3);

    // Verify Tickets generated
    let tickets: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT id, ticket_code, status FROM tickets WHERE booking_id = ? ORDER BY id ASC",
    )
    .bind(booking_id)
    .fetch_all(&pool)
    .await
    .expect("Tickets must exist in DB");

    assert_eq!(
        tickets.len(),
        3,
        "Exactly 3 tickets must be created for quantity 3"
    );
    for t in &tickets {
        assert_eq!(t.2, "active", "Ticket status must be 'active'");
        assert!(t
            .1
            .starts_with(&format!("EVT-{}-BKG-{}-", event_id, booking_id)));
    }
    let ticket_id_1 = tickets[0].0;

    // Verify Counter Invariants:
    // Event available seats: 20 - 3 = 17
    // Seat type booked seats: 0 + 3 = 3
    let ev_seats: (i32,) = sqlx::query_as("SELECT available_seats FROM events WHERE id = ?")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        ev_seats.0, 17,
        "Event available_seats must be decremented by 3"
    );

    let st_seats: (i32,) = sqlx::query_as("SELECT booked_seats FROM seat_types WHERE id = ?")
        .bind(seat_type_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        st_seats.0, 3,
        "Seat type booked_seats must be incremented by 3"
    );

    // F. Attendee Privacy & Ticket Authorization
    // Alice (Organizer user_id 2) attempting to view Charlie's ticket -> 403 Forbidden
    let spy_ticket_req = Request::builder()
        .uri(format!("/tickets/{}", ticket_id_1))
        .header(header::COOKIE, &organizer_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let spy_ticket_resp = app.clone().oneshot(spy_ticket_req).await.unwrap();
    assert_eq!(
        spy_ticket_resp.status(),
        StatusCode::FORBIDDEN,
        "Non-owner cannot view another attendee's ticket"
    );

    // Charlie viewing his own ticket -> 200 OK
    let own_ticket_req = Request::builder()
        .uri(format!("/tickets/{}", ticket_id_1))
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let own_ticket_resp = app.clone().oneshot(own_ticket_req).await.unwrap();
    assert_eq!(own_ticket_resp.status(), StatusCode::OK);

    // G. Cancellation Authorization: Alien user cannot cancel Charlie's booking
    let alien_cancel_req = Request::builder()
        .method("POST")
        .uri(format!("/bookings/{}/cancel", booking_id))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &organizer_cookie)
        .body(axum::body::Body::from(format!(
            "csrf_token={}",
            urlencode(&organizer_csrf)
        )))
        .unwrap();
    let alien_cancel_resp = app.clone().oneshot(alien_cancel_req).await.unwrap();
    assert_eq!(
        alien_cancel_resp.status(),
        StatusCode::FORBIDDEN,
        "Alien user cannot cancel someone else's booking"
    );

    // H. Successful Cancellation by Owner
    let own_cancel_req = Request::builder()
        .method("POST")
        .uri(format!("/bookings/{}/cancel", booking_id))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::from(format!(
            "csrf_token={}",
            urlencode(&attendee_csrf)
        )))
        .unwrap();
    let own_cancel_resp = app.clone().oneshot(own_cancel_req).await.unwrap();
    assert_eq!(
        own_cancel_resp.status(),
        StatusCode::SEE_OTHER,
        "Successful cancellation redirects to /bookings/my"
    );

    // Verify DB state after cancellation:
    let cancelled_booking: (String,) = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
        .bind(booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cancelled_booking.0, "cancelled");

    // Tickets updated to 'cancelled'
    let cancelled_tickets: Vec<(String,)> =
        sqlx::query_as("SELECT status FROM tickets WHERE booking_id = ?")
            .bind(booking_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    for t in cancelled_tickets {
        assert_eq!(
            t.0, "cancelled",
            "All associated tickets must be marked 'cancelled'"
        );
    }

    // Counters restored:
    let restored_ev: (i32,) = sqlx::query_as("SELECT available_seats FROM events WHERE id = ?")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(restored_ev.0, 20, "available_seats must be restored to 20");

    let restored_st: (i32,) = sqlx::query_as("SELECT booked_seats FROM seat_types WHERE id = ?")
        .bind(seat_type_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        restored_st.0, 0,
        "booked_seats must be decremented back to 0"
    );

    // I. Repeated Cancellation Rejection (Invariant: Cannot restore counters twice)
    let repeat_cancel_req = Request::builder()
        .method("POST")
        .uri(format!("/bookings/{}/cancel", booking_id))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::from(format!(
            "csrf_token={}",
            urlencode(&attendee_csrf)
        )))
        .unwrap();
    let repeat_cancel_resp = app.clone().oneshot(repeat_cancel_req).await.unwrap();
    assert_eq!(
        repeat_cancel_resp.status(),
        StatusCode::BAD_REQUEST,
        "Cannot cancel already-cancelled booking"
    );

    // Ensure counters remain unchanged after repeat attempt
    let ev_after_repeat: (i32,) = sqlx::query_as("SELECT available_seats FROM events WHERE id = ?")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        ev_after_repeat.0, 20,
        "Repeated cancellation must not inflate available_seats"
    );

    let st_after_repeat: (i32,) =
        sqlx::query_as("SELECT booked_seats FROM seat_types WHERE id = ?")
            .bind(seat_type_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        st_after_repeat.0, 0,
        "Repeated cancellation must not decrement booked_seats twice"
    );

    // Clean up dynamically created test records safely
    let _ = sqlx::query("DELETE FROM tickets WHERE booking_id = ?")
        .bind(booking_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM bookings WHERE id = ?")
        .bind(booking_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM seat_types WHERE id = ?")
        .bind(seat_type_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM events WHERE id = ?")
        .bind(event_id)
        .execute(&pool)
        .await;
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Concurrency Test: Preventing Overselling Under Concurrent Requests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_real_mysql_concurrent_booking_prevents_overselling() {
    let _ = dotenvy::dotenv();
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => url,
        _ => {
            eprintln!("Skipping real MySQL concurrency test: DATABASE_URL not configured");
            return;
        }
    };

    let pool = match sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(15)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&db_url)
        .await
    {
        Ok(p) => p,
        Err(err) => {
            eprintln!(
                "Skipping real MySQL concurrency test: cannot connect: {}",
                err
            );
            return;
        }
    };

    // Create an event with exactly 5 available seats
    let event_title = format!("Concurrency Test Event {}", rand::random::<u32>());
    let future_date = chrono::Utc::now().naive_utc() + chrono::Duration::days(10);

    let insert_res = sqlx::query(
        "INSERT INTO events (organizer_id, title, venue, city, event_date, total_seats, available_seats, ticket_price, status) \
         VALUES (2, ?, 'Arena', 'Bengaluru', ?, 5, 5, 100.00, 'published')"
    )
    .bind(&event_title)
    .bind(future_date)
    .execute(&pool)
    .await
    .expect("Creating concurrency test event must succeed");

    let event_id = insert_res.last_insert_id() as i32;

    // Spawn 10 concurrent booking attempts, each for quantity 1
    let mut handles = Vec::new();
    for _ in 0..10 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            Booking::create_transactional(&pool_clone, 3, event_id, None, 1).await
        });
        handles.push(handle);
    }

    let mut successful_bookings = 0;
    let mut rejected_bookings = 0;

    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => successful_bookings += 1,
            Err(BookingError::InsufficientEventCapacity) => rejected_bookings += 1,
            Err(other) => panic!("Unexpected error during concurrent booking: {:?}", other),
        }
    }

    assert_eq!(
        successful_bookings, 5,
        "Exactly 5 concurrent bookings must succeed for 5 available seats"
    );
    assert_eq!(
        rejected_bookings, 5,
        "Exactly 5 concurrent bookings must be rejected due to capacity limit"
    );

    // Verify database state: available_seats must be exactly 0
    let final_seats: (i32,) = sqlx::query_as("SELECT available_seats FROM events WHERE id = ?")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        final_seats.0, 0,
        "Final available_seats must be exactly 0 (no overselling)"
    );

    let total_tickets: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tickets WHERE event_id = ?")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        total_tickets.0, 5,
        "Exactly 5 tickets must exist in database"
    );

    // Clean up dynamic records safely
    let _ = sqlx::query("DELETE FROM tickets WHERE event_id = ?")
        .bind(event_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM bookings WHERE event_id = ?")
        .bind(event_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM events WHERE id = ?")
        .bind(event_id)
        .execute(&pool)
        .await;
}
