mod common;

use axum::{
    body::to_bytes,
    extract::Request,
    http::{header, StatusCode},
    Router,
};
use eventpro::{
    config::{resolve_public_dir, AppConfig},
    routes::create_router_with_store,
    AppState,
};
use std::path::PathBuf;
use tower::ServiceExt;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

/// Temporary upload directory guard for isolated test runs.
struct TestUploadDir {
    path: PathBuf,
}

impl TestUploadDir {
    fn new() -> Self {
        let unique_id = format!(
            "eventpro_e2e_uploads_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        );
        let path = std::env::temp_dir().join(unique_id);
        std::fs::create_dir_all(&path).expect("failed to create temporary upload dir");
        Self { path }
    }

    fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for TestUploadDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn urlencode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

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

struct MultipartBuilder {
    boundary: String,
    body: Vec<u8>,
}

impl MultipartBuilder {
    fn new() -> Self {
        let boundary = format!("----E2EBoundary{}", rand::random::<u64>());
        Self {
            boundary,
            body: Vec::new(),
        }
    }

    fn text(mut self, name: &str, value: &str) -> Self {
        self.body
            .extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
        self.body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
        );
        self.body.extend_from_slice(value.as_bytes());
        self.body.extend_from_slice(b"\r\n");
        self
    }

    fn file(mut self, name: &str, filename: &str, mime: &str, data: &[u8]) -> Self {
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

    fn build(mut self) -> (String, Vec<u8>) {
        self.body
            .extend_from_slice(format!("--{}--\r\n", self.boundary).as_bytes());
        let content_type = format!("multipart/form-data; boundary={}", self.boundary);
        (content_type, self.body)
    }
}

fn create_test_app(state: AppState) -> Router {
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_name("eventpro_sid")
        .with_secure(false)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

    create_router_with_store(state, Some(session_layer))
}

async fn http_login(app: &Router, email: &str, password: &str) -> (String, String) {
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

// ─────────────────────────────────────────────────────────────────────────────
// Workflow 1: Guest Browsing & Discovery
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_e2e_workflow_1_guest_browsing_and_discovery() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let app = create_test_app(AppState::new(pool, config));

    // 1. Root redirect: GET / -> 303 SEE_OTHER to /events
    let req_root = Request::builder()
        .uri("/")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_root = app.clone().oneshot(req_root).await.unwrap();
    assert_eq!(resp_root.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp_root
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/events"
    );

    // 2. Events catalogue: GET /events -> 200 OK
    let req_events = Request::builder()
        .uri("/events")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_events = app.clone().oneshot(req_events).await.unwrap();
    assert_eq!(resp_events.status(), StatusCode::OK);

    let body = to_bytes(resp_events.into_body(), 256 * 1024).await.unwrap();
    let html = String::from_utf8_lossy(&body);

    // Verify guest navbar contents
    assert!(
        html.contains("href=\"/events\">Discover</a>"),
        "Navbar has Discover"
    );
    assert!(html.contains("href=\"/auth/login\""), "Navbar has Login");
    assert!(
        html.contains("href=\"/auth/register\""),
        "Navbar has Sign Up"
    );
    assert!(
        !html.contains("href=\"/events/create\""),
        "Guest must NOT see Create Event"
    );
    assert!(
        !html.contains("href=\"/admin/dashboard\""),
        "Guest must NOT see Admin"
    );
    assert!(!html.contains("/auth/logout"), "Guest must NOT see Logout");

    // Verify filter form
    assert!(html.contains("form class=\"filter-bar\" method=\"GET\" action=\"/events\""));
    assert!(html.contains("name=\"search\""));
    assert!(html.contains("name=\"category_id\""));
    assert!(html.contains("name=\"city\""));

    // Verify footer links
    assert!(html.contains("href=\"/events\">Browse Events</a>"));
    assert!(html.contains("href=\"/auth/register\">Join Now</a>"));

    // 3. View detail of seed event #1: GET /events/1 -> 200 OK
    let req_detail = Request::builder()
        .uri("/events/1")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_detail = app.clone().oneshot(req_detail).await.unwrap();
    assert_eq!(resp_detail.status(), StatusCode::OK);

    let detail_body = to_bytes(resp_detail.into_body(), 256 * 1024).await.unwrap();
    let detail_html = String::from_utf8_lossy(&detail_body);

    // Guest sees "Login to Book", but NOT "Book Now"
    assert!(
        detail_html.contains("href=\"/auth/login\""),
        "Guest on event detail should have Login to Book link"
    );
    assert!(
        !detail_html.contains("href=\"/bookings/book/1\""),
        "Guest should not have direct book link"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Workflow 2: Guest Registration, Login, and Session Lifecycle
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_e2e_workflow_2_guest_registration_and_auth() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let app = create_test_app(AppState::new(pool.clone(), config));

    // 1. GET /auth/register -> 200 OK, obtain CSRF and cookie
    let req_reg_page = Request::builder()
        .uri("/auth/register")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_reg_page = app.clone().oneshot(req_reg_page).await.unwrap();
    assert_eq!(resp_reg_page.status(), StatusCode::OK);
    let cookie_reg = extract_cookie(resp_reg_page.headers()).expect("Registration page cookie");
    let reg_body = to_bytes(resp_reg_page.into_body(), 64 * 1024)
        .await
        .unwrap();
    let reg_csrf =
        extract_csrf_token(&String::from_utf8_lossy(&reg_body)).expect("CSRF token on register");

    // 2. POST /auth/register with new attendee credentials
    let test_email = format!("e2e_attendee_{}@example.com", rand::random::<u32>());
    let reg_form = format!(
        "full_name=E2E+Attendee&email={}&role=user&password=Password%40123&confirm_password=Password%40123&csrf_token={}",
        urlencode(&test_email),
        urlencode(&reg_csrf)
    );
    let req_register = Request::builder()
        .method("POST")
        .uri("/auth/register")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &cookie_reg)
        .body(axum::body::Body::from(reg_form))
        .unwrap();
    let resp_register = app.clone().oneshot(req_register).await.unwrap();
    assert_eq!(
        resp_register.status(),
        StatusCode::SEE_OTHER,
        "Registration redirects to /dashboard"
    );
    assert_eq!(
        resp_register
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/dashboard"
    );

    let auth_cookie = extract_cookie(resp_register.headers()).unwrap_or(cookie_reg);

    // 3. GET /dashboard with new session -> 200 OK
    let req_dash = Request::builder()
        .uri("/dashboard")
        .header(header::COOKIE, &auth_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_dash = app.clone().oneshot(req_dash).await.unwrap();
    assert_eq!(resp_dash.status(), StatusCode::OK);
    let dash_html =
        String::from_utf8_lossy(&to_bytes(resp_dash.into_body(), 128 * 1024).await.unwrap())
            .to_string();

    // Verify role-specific visibility: attendee has user badge, My Bookings, My Tickets
    assert!(
        dash_html.contains("role-badge--user"),
        "Attendee has user role badge"
    );
    assert!(
        dash_html.contains("href=\"/bookings/my\""),
        "Sidebar has My Bookings"
    );
    assert!(
        dash_html.contains("href=\"/tickets/my\""),
        "Sidebar has My Tickets"
    );
    assert!(
        !dash_html.contains("href=\"/admin/dashboard\""),
        "Attendee cannot see Admin link"
    );

    // 4. Verify logout forms and behavior
    assert!(
        dash_html.contains("action=\"/auth/logout\""),
        "Logout form must be present in dashboard HTML"
    );
    let logout_csrf = extract_csrf_token(&dash_html).expect("CSRF token present in logout form");

    // 4a. Verify GET /auth/logout returns 405 Method Not Allowed and preserves session
    let req_get_logout = Request::builder()
        .uri("/auth/logout")
        .header(header::COOKIE, &auth_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_get_logout = app.clone().oneshot(req_get_logout).await.unwrap();
    assert_eq!(resp_get_logout.status(), StatusCode::METHOD_NOT_ALLOWED);

    // 4b. Verify POST /auth/logout without CSRF returns 403 Forbidden and preserves session
    let req_bad_logout = Request::builder()
        .method("POST")
        .uri("/auth/logout")
        .header(header::COOKIE, &auth_cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_bad_logout = app.clone().oneshot(req_bad_logout).await.unwrap();
    assert_eq!(resp_bad_logout.status(), StatusCode::FORBIDDEN);

    // 4c. POST /auth/logout with valid CSRF -> 303 SEE_OTHER to /
    let req_logout = Request::builder()
        .method("POST")
        .uri("/auth/logout")
        .header(header::COOKIE, &auth_cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(axum::body::Body::from(format!(
            "csrf_token={}",
            logout_csrf
        )))
        .unwrap();
    let resp_logout = app.clone().oneshot(req_logout).await.unwrap();
    assert_eq!(resp_logout.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp_logout
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/"
    );

    // 5. Subsequent access to /dashboard without session redirects to /auth/login
    let req_anon_dash = Request::builder()
        .uri("/dashboard")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_anon_dash = app.clone().oneshot(req_anon_dash).await.unwrap();
    assert_eq!(resp_anon_dash.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp_anon_dash
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/auth/login"
    );

    // 6. Login again with created account
    let (new_cookie, _) = http_login(&app, &test_email, "Password@123").await;
    assert!(
        !new_cookie.is_empty(),
        "Login succeeded with created account"
    );

    // Cleanup created user
    let _ = sqlx::query("DELETE FROM users WHERE email = ?")
        .bind(&test_email)
        .execute(&pool)
        .await;
}

// ─────────────────────────────────────────────────────────────────────────────
// Workflow 3: Attendee Booking, History, Tickets, and Cancellation
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_e2e_workflow_3_attendee_booking_lifecycle() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let app = create_test_app(AppState::new(pool.clone(), config));

    // Login as Charlie Brown (attendee: charlie@mail.com / User@1234)
    let (attendee_cookie, _) = http_login(&app, "charlie@mail.com", "User@1234").await;

    // Create a published future event for Alice Organizer (id 2) so booking passes deadline checks
    let future_title = format!("Future Festival {}", rand::random::<u32>());
    let insert_res = sqlx::query(
        "INSERT INTO events (organizer_id, category_id, title, description, venue, city, event_date, registration_deadline, total_seats, available_seats, ticket_price, status) \
         VALUES (2, 1, ?, 'Upcoming festival', 'Goa Beach', 'Goa', '2028-11-15 18:00:00', '2028-11-10 23:59:00', 100, 100, 500.00, 'published')"
    )
    .bind(&future_title)
    .execute(&pool)
    .await
    .unwrap();
    let future_event_id = insert_res.last_insert_id() as i32;

    // 1. View published event detail: GET /events/{future_event_id}
    let req_event = Request::builder()
        .uri(format!("/events/{}", future_event_id))
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_event = app.clone().oneshot(req_event).await.unwrap();
    assert_eq!(resp_event.status(), StatusCode::OK);
    let event_html =
        String::from_utf8_lossy(&to_bytes(resp_event.into_body(), 256 * 1024).await.unwrap())
            .to_string();
    assert!(
        event_html.contains(&format!("href=\"/bookings/book/{}\"", future_event_id)),
        "Authenticated attendee sees Book Now link"
    );

    // 2. Select tickets: GET /bookings/book/{future_event_id}
    let req_select = Request::builder()
        .uri(format!("/bookings/book/{}", future_event_id))
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_select = app.clone().oneshot(req_select).await.unwrap();
    assert_eq!(resp_select.status(), StatusCode::OK);
    let select_html =
        String::from_utf8_lossy(&to_bytes(resp_select.into_body(), 128 * 1024).await.unwrap())
            .to_string();
    assert!(select_html.contains("action=\"/bookings/payment\" method=\"GET\""));
    assert!(select_html.contains(&format!("name=\"eventId\" value=\"{}\"", future_event_id)));

    // 3. Payment page: GET /bookings/payment?eventId={future_event_id}&quantity=1
    let pay_url = format!("/bookings/payment?eventId={}&quantity=1", future_event_id);
    let req_pay = Request::builder()
        .uri(&pay_url)
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_pay = app.clone().oneshot(req_pay).await.unwrap();
    assert_eq!(resp_pay.status(), StatusCode::OK);
    let pay_html =
        String::from_utf8_lossy(&to_bytes(resp_pay.into_body(), 128 * 1024).await.unwrap())
            .to_string();

    assert!(pay_html.contains("This is a mock payment for demo purposes only"));
    assert!(pay_html.contains("action=\"/bookings/confirm\" method=\"POST\""));
    let confirm_csrf = extract_csrf_token(&pay_html).expect("CSRF token on payment page");

    // 4. Confirm booking: POST /bookings/confirm
    let confirm_form = format!(
        "event_id={}&seat_type_id=&quantity=1&csrf_token={}",
        future_event_id,
        urlencode(&confirm_csrf)
    );
    let req_confirm = Request::builder()
        .method("POST")
        .uri("/bookings/confirm")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::from(confirm_form))
        .unwrap();
    let resp_confirm = app.clone().oneshot(req_confirm).await.unwrap();
    assert_eq!(resp_confirm.status(), StatusCode::OK);
    let confirm_html = String::from_utf8_lossy(
        &to_bytes(resp_confirm.into_body(), 128 * 1024)
            .await
            .unwrap(),
    )
    .to_string();
    assert!(confirm_html.contains("Booking Confirmed!"));
    assert!(confirm_html.contains("href=\"/tickets/my\""));

    // Find the created booking in the database for Charlie (user id 4)
    let created_booking: (i32,) = sqlx::query_as(
        "SELECT id FROM bookings WHERE user_id = 4 AND event_id = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(future_event_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let booking_id = created_booking.0;

    // Find associated ticket
    let created_ticket: (i32, String) =
        sqlx::query_as("SELECT id, ticket_code FROM tickets WHERE booking_id = ? LIMIT 1")
            .bind(booking_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let ticket_id = created_ticket.0;
    let ticket_code = created_ticket.1;

    // 5. My Tickets page: GET /tickets/my
    let req_my_tickets = Request::builder()
        .uri("/tickets/my")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_my_tickets = app.clone().oneshot(req_my_tickets).await.unwrap();
    assert_eq!(resp_my_tickets.status(), StatusCode::OK);
    let my_tickets_html = String::from_utf8_lossy(
        &to_bytes(resp_my_tickets.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .to_string();
    assert!(my_tickets_html.contains(&format!("href=\"/tickets/{}\"", ticket_id)));
    assert!(my_tickets_html.contains(&format!("href=\"/tickets/{}/print\"", ticket_id)));

    // 6. View single ticket: GET /tickets/{ticket_id}
    let req_view_ticket = Request::builder()
        .uri(format!("/tickets/{}", ticket_id))
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_view_ticket = app.clone().oneshot(req_view_ticket).await.unwrap();
    assert_eq!(resp_view_ticket.status(), StatusCode::OK);
    let view_ticket_html = String::from_utf8_lossy(
        &to_bytes(resp_view_ticket.into_body(), 128 * 1024)
            .await
            .unwrap(),
    )
    .to_string();
    assert!(view_ticket_html.contains(&ticket_code));

    // 7. Print ticket: GET /tickets/{ticket_id}/print
    let req_print = Request::builder()
        .uri(format!("/tickets/{}/print", ticket_id))
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_print = app.clone().oneshot(req_print).await.unwrap();
    assert_eq!(resp_print.status(), StatusCode::OK);
    let print_html =
        String::from_utf8_lossy(&to_bytes(resp_print.into_body(), 128 * 1024).await.unwrap())
            .to_string();
    assert!(print_html.contains("EventPro — Official Ticket"));
    assert!(print_html.contains(&ticket_code));

    // 8. My Bookings page: GET /bookings/my
    let req_my_bookings = Request::builder()
        .uri("/bookings/my")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_my_bookings = app.clone().oneshot(req_my_bookings).await.unwrap();
    assert_eq!(resp_my_bookings.status(), StatusCode::OK);
    let my_bookings_html = String::from_utf8_lossy(
        &to_bytes(resp_my_bookings.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .to_string();
    assert!(my_bookings_html.contains(&format!("action=\"/bookings/{}/cancel\"", booking_id)));
    let cancel_csrf = extract_csrf_token(&my_bookings_html).expect("CSRF token on bookings page");

    // 9. Cancel booking: POST /bookings/{booking_id}/cancel
    let cancel_form = format!("csrf_token={}", urlencode(&cancel_csrf));
    let req_cancel = Request::builder()
        .method("POST")
        .uri(format!("/bookings/{}/cancel", booking_id))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &attendee_cookie)
        .body(axum::body::Body::from(cancel_form))
        .unwrap();
    let resp_cancel = app.clone().oneshot(req_cancel).await.unwrap();
    assert_eq!(resp_cancel.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp_cancel
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/bookings/my"
    );

    // Verify booking cancelled in database
    let updated_booking: (String,) = sqlx::query_as("SELECT status FROM bookings WHERE id = ?")
        .bind(booking_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(updated_booking.0, "cancelled");

    // Cleanup created test booking, tickets, and future event
    let _ = sqlx::query("DELETE FROM tickets WHERE booking_id = ?")
        .bind(booking_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM bookings WHERE id = ?")
        .bind(booking_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM events WHERE id = ?")
        .bind(future_event_id)
        .execute(&pool)
        .await;
}

// ─────────────────────────────────────────────────────────────────────────────
// Workflow 4: Organizer Event Lifecycle (Create, Banner, Edit, Toggle, Delete)
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_e2e_workflow_4_organizer_event_lifecycle() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let app = create_test_app(AppState::new(pool.clone(), config));

    // Login as Alice Organizer (alice@events.com / Organizer@1)
    let (org_cookie, _) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // 1. GET /events/create -> 200 OK
    let req_create_page = Request::builder()
        .uri("/events/create")
        .header(header::COOKIE, &org_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_create_page = app.clone().oneshot(req_create_page).await.unwrap();
    assert_eq!(resp_create_page.status(), StatusCode::OK);
    let create_page_html = String::from_utf8_lossy(
        &to_bytes(resp_create_page.into_body(), 128 * 1024)
            .await
            .unwrap(),
    )
    .to_string();
    assert!(create_page_html.contains("enctype=\"multipart/form-data\""));
    assert!(create_page_html.contains("name=\"banner_image\""));
    let create_csrf = extract_csrf_token(&create_page_html).expect("CSRF token on create page");

    // 2. POST /events/create with 1x1 valid PNG banner
    let valid_png_bytes = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89,
    ];
    let unique_title = format!("E2E Festival {}", rand::random::<u32>());
    let (content_type, payload) = MultipartBuilder::new()
        .text("csrf_token", &create_csrf)
        .text("title", &unique_title)
        .text("category_id", "1")
        .text("description", "A vibrant outdoor music gathering")
        .text("venue", "Beach Arena")
        .text("city", "Goa")
        .text("event_date", "2027-10-15T18:00")
        .text("total_seats", "200")
        .text("ticket_price", "750.00")
        .text("status", "draft")
        .file(
            "banner_image",
            "beach_banner.png",
            "image/png",
            &valid_png_bytes,
        )
        .build();

    let req_create = Request::builder()
        .method("POST")
        .uri("/events/create")
        .header(header::COOKIE, &org_cookie)
        .header(header::CONTENT_TYPE, content_type)
        .body(axum::body::Body::from(payload))
        .unwrap();
    let resp_create = app.clone().oneshot(req_create).await.unwrap();
    assert_eq!(resp_create.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp_create
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/dashboard"
    );

    // Fetch created event from DB
    let created_event: (i32, Option<String>, String) =
        sqlx::query_as("SELECT id, banner_image, status FROM events WHERE title = ?")
            .bind(&unique_title)
            .fetch_one(&pool)
            .await
            .unwrap();
    let event_id = created_event.0;
    let initial_banner = created_event.1.expect("Banner should be saved");
    assert_eq!(created_event.2, "draft");

    // 3. GET /events/{id}/edit -> 200 OK. Verify current banner preview
    let req_edit_page = Request::builder()
        .uri(format!("/events/{}/edit", event_id))
        .header(header::COOKIE, &org_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_edit_page = app.clone().oneshot(req_edit_page).await.unwrap();
    assert_eq!(resp_edit_page.status(), StatusCode::OK);
    let edit_html = String::from_utf8_lossy(
        &to_bytes(resp_edit_page.into_body(), 128 * 1024)
            .await
            .unwrap(),
    )
    .to_string();
    let banner_filename = initial_banner.strip_prefix("/uploads/").unwrap();
    assert!(
        edit_html.contains(banner_filename),
        "Edit page shows existing banner filename"
    );
    let edit_csrf = extract_csrf_token(&edit_html).expect("CSRF token on edit page");

    // 4. POST /events/{id}/edit without uploading replacement banner (preserves existing banner)
    let (ct_no_banner, payload_no_banner) = MultipartBuilder::new()
        .text("csrf_token", &edit_csrf)
        .text("title", &format!("{} (Updated)", unique_title))
        .text("category_id", "1")
        .text("venue", "Beach Arena Main Stage")
        .text("city", "Goa")
        .text("event_date", "2027-10-15T18:00")
        .text("total_seats", "250")
        .text("ticket_price", "800.00")
        .text("status", "draft")
        .build();

    let req_edit_no_banner = Request::builder()
        .method("POST")
        .uri(format!("/events/{}/edit", event_id))
        .header(header::COOKIE, &org_cookie)
        .header(header::CONTENT_TYPE, ct_no_banner)
        .body(axum::body::Body::from(payload_no_banner))
        .unwrap();
    let resp_edit_no_banner = app.clone().oneshot(req_edit_no_banner).await.unwrap();
    assert_eq!(resp_edit_no_banner.status(), StatusCode::SEE_OTHER);

    // Verify banner was preserved in database
    let event_after_edit1: (Option<String>,) =
        sqlx::query_as("SELECT banner_image FROM events WHERE id = ?")
            .bind(event_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        event_after_edit1.0.as_ref(),
        Some(&initial_banner),
        "Banner image must be preserved when no replacement file is submitted"
    );

    // 5. POST /events/{id}/edit with replacement banner (1x1 valid GIF)
    let valid_gif_bytes = [
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00,
        0x00, 0xFF, 0xFF, 0xFF, 0x21, 0xF9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2C, 0x00, 0x00,
        0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x01, 0x44, 0x00, 0x3B,
    ];
    let (ct_replace, payload_replace) = MultipartBuilder::new()
        .text("csrf_token", &edit_csrf)
        .text("title", &format!("{} (With GIF)", unique_title))
        .text("category_id", "1")
        .text("venue", "Beach Arena Main Stage")
        .text("city", "Goa")
        .text("event_date", "2027-10-15T18:00")
        .text("total_seats", "250")
        .text("ticket_price", "800.00")
        .text("status", "draft")
        .file(
            "banner_image",
            "new_banner.gif",
            "image/gif",
            &valid_gif_bytes,
        )
        .build();

    let req_edit_replace = Request::builder()
        .method("POST")
        .uri(format!("/events/{}/edit", event_id))
        .header(header::COOKIE, &org_cookie)
        .header(header::CONTENT_TYPE, ct_replace)
        .body(axum::body::Body::from(payload_replace))
        .unwrap();
    let resp_edit_replace = app.clone().oneshot(req_edit_replace).await.unwrap();
    assert_eq!(resp_edit_replace.status(), StatusCode::SEE_OTHER);

    let event_after_replace: (Option<String>,) =
        sqlx::query_as("SELECT banner_image FROM events WHERE id = ?")
            .bind(event_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let new_banner = event_after_replace.0.expect("New banner exists");
    assert_ne!(
        new_banner, initial_banner,
        "Banner should be updated to newly uploaded file"
    );

    // 6. Toggle status: POST /events/{id}/toggle-status
    let req_dash = Request::builder()
        .uri("/dashboard")
        .header(header::COOKIE, &org_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_dash = app.clone().oneshot(req_dash).await.unwrap();
    let dash_html =
        String::from_utf8_lossy(&to_bytes(resp_dash.into_body(), 128 * 1024).await.unwrap())
            .to_string();
    let toggle_csrf = extract_csrf_token(&dash_html).expect("CSRF token on dashboard");

    let toggle_form = format!("csrf_token={}", urlencode(&toggle_csrf));
    let req_toggle = Request::builder()
        .method("POST")
        .uri(format!("/events/{}/toggle-status", event_id))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &org_cookie)
        .body(axum::body::Body::from(toggle_form))
        .unwrap();
    let resp_toggle = app.clone().oneshot(req_toggle).await.unwrap();
    assert_eq!(resp_toggle.status(), StatusCode::SEE_OTHER);

    let event_after_toggle: (String,) = sqlx::query_as("SELECT status FROM events WHERE id = ?")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(event_after_toggle.0, "published");

    // 7. Delete event: POST /events/{id}/delete
    let delete_form = format!("csrf_token={}", urlencode(&toggle_csrf));
    let req_delete = Request::builder()
        .method("POST")
        .uri(format!("/events/{}/delete", event_id))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &org_cookie)
        .body(axum::body::Body::from(delete_form))
        .unwrap();
    let resp_delete = app.clone().oneshot(req_delete).await.unwrap();
    assert_eq!(resp_delete.status(), StatusCode::SEE_OTHER);

    // Verify event is removed from database
    let event_check: Option<(i32,)> = sqlx::query_as("SELECT id FROM events WHERE id = ?")
        .bind(event_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(event_check.is_none(), "Event was deleted from DB");
}

// ─────────────────────────────────────────────────────────────────────────────
// Workflow 5: Administrator Platform Management
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_e2e_workflow_5_admin_management() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let app = create_test_app(AppState::new(pool, config));

    // Login as Admin: admin@eventpro.com / Admin@1234
    let (admin_cookie, _) = http_login(&app, "admin@eventpro.com", "Admin@1234").await;

    // 1. GET /admin/dashboard -> 200 OK
    let req_admin = Request::builder()
        .uri("/admin/dashboard")
        .header(header::COOKIE, &admin_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_admin = app.clone().oneshot(req_admin).await.unwrap();
    assert_eq!(resp_admin.status(), StatusCode::OK);
    let admin_html =
        String::from_utf8_lossy(&to_bytes(resp_admin.into_body(), 128 * 1024).await.unwrap())
            .to_string();

    // Verify stats grid cards
    assert!(
        admin_html.contains("Total Users"),
        "Admin stats include Total Users"
    );
    assert!(
        admin_html.contains("Total Events"),
        "Admin stats include Total Events"
    );
    assert!(
        admin_html.contains("Confirmed Bookings"),
        "Admin stats include Confirmed Bookings"
    );
    assert!(
        admin_html.contains("Total Revenue"),
        "Admin stats include Total Revenue"
    );

    // Verify navigation links for admin
    assert!(
        admin_html.contains("href=\"/events/create\""),
        "Admin has link to Create Event"
    );
    assert!(
        admin_html.contains("href=\"/dashboard\""),
        "Admin has link to Dashboard Event Manager"
    );

    // 2. Admin access to /events/create -> 200 OK
    let req_create = Request::builder()
        .uri("/events/create")
        .header(header::COOKIE, &admin_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_create = app.clone().oneshot(req_create).await.unwrap();
    assert_eq!(resp_create.status(), StatusCode::OK);

    // 3. Admin access to /dashboard (all events management overview) -> 200 OK
    let req_dash = Request::builder()
        .uri("/dashboard")
        .header(header::COOKIE, &admin_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_dash = app.clone().oneshot(req_dash).await.unwrap();
    assert_eq!(resp_dash.status(), StatusCode::OK);
}

// ─────────────────────────────────────────────────────────────────────────────
// Workflow 6: Security and Cross-User Authorization Enforcement
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_e2e_workflow_6_cross_user_authorization() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let app = create_test_app(AppState::new(pool.clone(), config));

    // Charlie Brown (User id=4), Diana Prince (User id=5)
    let (charlie_cookie, _) = http_login(&app, "charlie@mail.com", "User@1234").await;
    let (_diana_cookie, _) = http_login(&app, "diana@mail.com", "User@1234").await;

    // Create a booking owned by Diana (user 5)
    let booking_result = sqlx::query(
        "INSERT INTO bookings (user_id, event_id, quantity, total_amount, status) VALUES (5, 1, 1, 1500.00, 'confirmed')"
    )
    .execute(&pool)
    .await
    .unwrap();
    let diana_booking_id = booking_result.last_insert_id() as i32;

    let ticket_code = format!("TCK-DIANA-{}", rand::random::<u32>());
    let ticket_result = sqlx::query(
        "INSERT INTO tickets (booking_id, user_id, event_id, ticket_code, status) VALUES (?, 5, 1, ?, 'active')"
    )
    .bind(diana_booking_id)
    .bind(&ticket_code)
    .execute(&pool)
    .await
    .unwrap();
    let diana_ticket_id = ticket_result.last_insert_id() as i32;

    // 1. Charlie attempts to view Diana's ticket -> 403 or 404
    let req_unauth_ticket = Request::builder()
        .uri(format!("/tickets/{}", diana_ticket_id))
        .header(header::COOKIE, &charlie_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_unauth_ticket = app.clone().oneshot(req_unauth_ticket).await.unwrap();
    assert!(
        resp_unauth_ticket.status() == StatusCode::FORBIDDEN
            || resp_unauth_ticket.status() == StatusCode::NOT_FOUND,
        "Attendee cannot view another user's ticket"
    );

    // 2. Charlie attempts to cancel Diana's booking -> 403 or 404
    let req_unauth_cancel = Request::builder()
        .method("POST")
        .uri(format!("/bookings/{}/cancel", diana_booking_id))
        .header(header::COOKIE, &charlie_cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(axum::body::Body::from("csrf_token=dummy"))
        .unwrap();
    let resp_unauth_cancel = app.clone().oneshot(req_unauth_cancel).await.unwrap();
    assert!(
        resp_unauth_cancel.status() == StatusCode::FORBIDDEN
            || resp_unauth_cancel.status() == StatusCode::NOT_FOUND,
        "Attendee cannot cancel another user's booking"
    );

    // 3. Organizer Bob (id 3) attempts to edit Alice's event (id 1, organizer_id 2) -> 403
    let (bob_cookie, _) = http_login(&app, "bob@events.com", "Organizer@1").await;
    let req_bob_edit = Request::builder()
        .uri("/events/1/edit")
        .header(header::COOKIE, &bob_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_bob_edit = app.clone().oneshot(req_bob_edit).await.unwrap();
    assert_eq!(resp_bob_edit.status(), StatusCode::FORBIDDEN);

    // 4. Attendee attempts to access /admin/dashboard -> 403
    let req_att_admin = Request::builder()
        .uri("/admin/dashboard")
        .header(header::COOKIE, &charlie_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_att_admin = app.clone().oneshot(req_att_admin).await.unwrap();
    assert_eq!(resp_att_admin.status(), StatusCode::FORBIDDEN);

    // 5. Organizer attempts to access /admin/dashboard -> 403
    let req_org_admin = Request::builder()
        .uri("/admin/dashboard")
        .header(header::COOKIE, &bob_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_org_admin = app.clone().oneshot(req_org_admin).await.unwrap();
    assert_eq!(resp_org_admin.status(), StatusCode::FORBIDDEN);

    // Cleanup Diana's test booking
    let _ = sqlx::query("DELETE FROM tickets WHERE id = ?")
        .bind(diana_ticket_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM bookings WHERE id = ?")
        .bind(diana_booking_id)
        .execute(&pool)
        .await;
}

// ─────────────────────────────────────────────────────────────────────────────
// Workflow 7: Edge Cases, Sensible UI Responses, and Error Pages
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_e2e_workflow_7_edge_cases_and_error_pages() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let app = create_test_app(AppState::new(pool, config));

    // 1. Nonexistent event: GET /events/999999 -> 404 with styled error page
    let req_404_event = Request::builder()
        .uri("/events/999999")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_404_event = app.clone().oneshot(req_404_event).await.unwrap();
    assert_eq!(resp_404_event.status(), StatusCode::NOT_FOUND);
    let html_404 = String::from_utf8_lossy(
        &to_bytes(resp_404_event.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .to_string();
    assert!(html_404.contains("404"), "Includes 404 status code");
    assert!(html_404.contains("This page doesn't exist"));
    assert!(html_404.contains("href=\"/events\""));

    // 2. Search query returning empty result set -> empty state rendered
    let req_empty_search = Request::builder()
        .uri("/events?search=NonexistentSupercalifragilisticQuery123")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_empty_search = app.clone().oneshot(req_empty_search).await.unwrap();
    assert_eq!(resp_empty_search.status(), StatusCode::OK);
    let html_empty = String::from_utf8_lossy(
        &to_bytes(resp_empty_search.into_body(), 128 * 1024)
            .await
            .unwrap(),
    )
    .to_string();
    assert!(html_empty.contains("No events found"));
    assert!(html_empty.contains("Browse All Events"));

    // 3. Invalid CSRF token on state-changing endpoint -> 403 Forbidden / CSRF error response
    let (charlie_cookie, _) = http_login(&app, "charlie@mail.com", "User@1234").await;
    let req_bad_csrf = Request::builder()
        .method("POST")
        .uri("/bookings/confirm")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &charlie_cookie)
        .body(axum::body::Body::from(
            "event_id=1&quantity=1&csrf_token=invalid_forged_csrf_token",
        ))
        .unwrap();
    let resp_bad_csrf = app.clone().oneshot(req_bad_csrf).await.unwrap();
    assert_eq!(resp_bad_csrf.status(), StatusCode::FORBIDDEN);
    let csrf_err_html = String::from_utf8_lossy(
        &to_bytes(resp_bad_csrf.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .to_string();
    assert!(
        csrf_err_html.contains("CSRF validation failed")
            || csrf_err_html.contains("Security Validation Failed")
    );
}

#[tokio::test]
async fn test_navbar_and_sidebar_logout_forms_e2e() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let app = create_test_app(AppState::new(pool, config));

    // 1. Login as attendee
    let (cookie, _) = http_login(&app, "charlie@mail.com", "User@1234").await;

    // 2. Load /dashboard and verify both navbar and sidebar have POST logout forms with CSRF token
    let req_dash = Request::builder()
        .uri("/dashboard")
        .header(header::COOKIE, &cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_dash = app.clone().oneshot(req_dash).await.unwrap();
    assert_eq!(resp_dash.status(), StatusCode::OK);
    let html = String::from_utf8_lossy(&to_bytes(resp_dash.into_body(), 128 * 1024).await.unwrap())
        .to_string();

    // Verify navbar logout form
    assert!(
        html.contains("<form action=\"/auth/logout\" method=\"POST\""),
        "Navbar logout form must exist with POST method"
    );
    assert!(
        html.contains("Logout</button>"),
        "Navbar logout button must be present"
    );

    // Verify sidebar logout form
    assert!(
        html.contains("🚪 Logout</button>"),
        "Sidebar logout button must be present"
    );

    // Verify CSRF tokens are injected
    let csrf = extract_csrf_token(&html).expect("CSRF token present in dashboard HTML");
    assert!(!csrf.is_empty(), "Extracted CSRF token must not be empty");

    // 3. Perform logout using the form payload (navbar flow)
    let req_logout = Request::builder()
        .method("POST")
        .uri("/auth/logout")
        .header(header::COOKIE, &cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(axum::body::Body::from(format!("csrf_token={}", csrf)))
        .unwrap();
    let resp_logout = app.clone().oneshot(req_logout).await.unwrap();
    assert_eq!(resp_logout.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp_logout
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/"
    );

    // 4. Verify session is destroyed by accessing /dashboard with previous cookie
    let req_dash_post_logout = Request::builder()
        .uri("/dashboard")
        .header(header::COOKIE, &cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_dash_post_logout = app.clone().oneshot(req_dash_post_logout).await.unwrap();
    assert_eq!(resp_dash_post_logout.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp_dash_post_logout
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/auth/login"
    );

    // 5. Test sidebar flow: login again as organizer
    let (org_cookie, _) = http_login(&app, "bob@events.com", "Organizer@1").await;
    let req_org_dash = Request::builder()
        .uri("/dashboard")
        .header(header::COOKIE, &org_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp_org_dash = app.clone().oneshot(req_org_dash).await.unwrap();
    assert_eq!(resp_org_dash.status(), StatusCode::OK);
    let org_html = String::from_utf8_lossy(
        &to_bytes(resp_org_dash.into_body(), 128 * 1024)
            .await
            .unwrap(),
    )
    .to_string();

    let org_csrf =
        extract_csrf_token(&org_html).expect("CSRF token present in organizer dashboard HTML");
    // Perform sidebar logout
    let req_sidebar_logout = Request::builder()
        .method("POST")
        .uri("/auth/logout")
        .header(header::COOKIE, &org_cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(axum::body::Body::from(format!("csrf_token={}", org_csrf)))
        .unwrap();
    let resp_sidebar_logout = app.clone().oneshot(req_sidebar_logout).await.unwrap();
    assert_eq!(resp_sidebar_logout.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp_sidebar_logout
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/"
    );
}
