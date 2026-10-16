mod common;

use axum::http::StatusCode;
use common::{
    create_test_app_with_pool, extract_cookie, extract_csrf_token, get_response, http_login,
    post_form_response, post_multipart_response, urlencode, MultipartBuilder,
};

// ─────────────────────────────────────────────────────────────────────────────
// Workflow 1: Guest Browsing & Discovery
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_e2e_workflow_1_guest_browsing_and_discovery() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };
    let (app, _upload) = create_test_app_with_pool(&pool);

    // 1. Root redirect: GET / -> 303 SEE_OTHER to /events
    let (status, headers, _) = get_response(&app, "/", None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers
            .get(axum::http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/events"
    );

    // 2. Events catalogue: GET /events -> 200 OK
    let (status, _, html) = get_response(&app, "/events", None).await;
    assert_eq!(status, StatusCode::OK);

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

    assert!(html.contains("form class=\"filter-bar\" method=\"GET\" action=\"/events\""));
    assert!(html.contains("name=\"search\""));
    assert!(html.contains("name=\"category_id\""));
    assert!(html.contains("name=\"city\""));

    assert!(html.contains("href=\"/events\">Browse Events</a>"));
    assert!(html.contains("href=\"/auth/register\">Join Now</a>"));

    // 3. View detail of seed event #1: GET /events/1 -> 200 OK
    let (status, _, detail_html) = get_response(&app, "/events/1", None).await;
    assert_eq!(status, StatusCode::OK);
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
    let (app, _upload) = create_test_app_with_pool(&pool);

    // 1. GET /auth/register -> 200 OK, obtain CSRF and cookie
    let (status, headers, reg_body) = get_response(&app, "/auth/register", None).await;
    assert_eq!(status, StatusCode::OK);
    let cookie_reg = extract_cookie(&headers).expect("Registration page cookie");
    let reg_csrf = extract_csrf_token(&reg_body).expect("CSRF token on register");

    // 2. POST /auth/register with new attendee credentials
    let test_email = format!("e2e_attendee_{}@example.com", rand::random::<u32>());
    let reg_form = format!(
        "full_name=E2E+Attendee&email={}&role=user&password=Password%40123&confirm_password=Password%40123&csrf_token={}",
        urlencode(&test_email),
        urlencode(&reg_csrf)
    );
    let (status, reg_post_headers, _) =
        post_form_response(&app, "/auth/register", Some(&cookie_reg), &reg_form).await;
    assert_eq!(
        status,
        StatusCode::SEE_OTHER,
        "Registration redirects to /dashboard"
    );
    assert_eq!(
        reg_post_headers
            .get(axum::http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/dashboard"
    );
    let auth_cookie = extract_cookie(&reg_post_headers).unwrap_or(cookie_reg);

    // 3. GET /dashboard with new session -> 200 OK
    let (status, _, dash_html) = get_response(&app, "/dashboard", Some(&auth_cookie)).await;
    assert_eq!(status, StatusCode::OK);

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
    let (status, _, _) = get_response(&app, "/auth/logout", Some(&auth_cookie)).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);

    // 4b. Verify POST /auth/logout without CSRF returns 403 Forbidden and preserves session
    let (status, _, _) = post_form_response(&app, "/auth/logout", Some(&auth_cookie), "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 4c. POST /auth/logout with valid CSRF -> 303 SEE_OTHER to /
    let (status, logout_headers, _) = post_form_response(
        &app,
        "/auth/logout",
        Some(&auth_cookie),
        &format!("csrf_token={}", logout_csrf),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        logout_headers
            .get(axum::http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/"
    );

    // 5. Subsequent access to /dashboard without session redirects to /auth/login
    let (status, anon_headers, _) = get_response(&app, "/dashboard", None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        anon_headers
            .get(axum::http::header::LOCATION)
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
    let (app, _upload) = create_test_app_with_pool(&pool);

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
    let (status, _, event_html) = get_response(
        &app,
        &format!("/events/{}", future_event_id),
        Some(&attendee_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        event_html.contains(&format!("href=\"/bookings/book/{}\"", future_event_id)),
        "Authenticated attendee sees Book Now link"
    );

    // 2. Select tickets: GET /bookings/book/{future_event_id}
    let (status, _, select_html) = get_response(
        &app,
        &format!("/bookings/book/{}", future_event_id),
        Some(&attendee_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(select_html.contains("action=\"/bookings/payment\" method=\"GET\""));
    assert!(select_html.contains(&format!("name=\"eventId\" value=\"{}\"", future_event_id)));

    // 3. Payment page: GET /bookings/payment?eventId={future_event_id}&quantity=1
    let pay_url = format!("/bookings/payment?eventId={}&quantity=1", future_event_id);
    let (status, _, pay_html) = get_response(&app, &pay_url, Some(&attendee_cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(pay_html.contains("This is a mock payment for demo purposes only"));
    assert!(pay_html.contains("action=\"/bookings/confirm\" method=\"POST\""));
    let confirm_csrf = extract_csrf_token(&pay_html).expect("CSRF token on payment page");

    // 4. Confirm booking: POST /bookings/confirm
    let confirm_form = format!(
        "event_id={}&seat_type_id=&quantity=1&csrf_token={}",
        future_event_id,
        urlencode(&confirm_csrf)
    );
    let (status, _, confirm_html) = post_form_response(
        &app,
        "/bookings/confirm",
        Some(&attendee_cookie),
        &confirm_form,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
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
    let (status, _, my_tickets_html) =
        get_response(&app, "/tickets/my", Some(&attendee_cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(my_tickets_html.contains(&format!("href=\"/tickets/{}\"", ticket_id)));
    assert!(my_tickets_html.contains(&format!("href=\"/tickets/{}/print\"", ticket_id)));

    // 6. View single ticket: GET /tickets/{ticket_id}
    let (status, _, view_ticket_html) = get_response(
        &app,
        &format!("/tickets/{}", ticket_id),
        Some(&attendee_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(view_ticket_html.contains(&ticket_code));

    // 7. Print ticket: GET /tickets/{ticket_id}/print
    let (status, _, print_html) = get_response(
        &app,
        &format!("/tickets/{}/print", ticket_id),
        Some(&attendee_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(print_html.contains("EventPro — Official Ticket"));
    assert!(print_html.contains(&ticket_code));

    // 8. My Bookings page: GET /bookings/my
    let (status, _, my_bookings_html) =
        get_response(&app, "/bookings/my", Some(&attendee_cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(my_bookings_html.contains(&format!("action=\"/bookings/{}/cancel\"", booking_id)));
    let cancel_csrf = extract_csrf_token(&my_bookings_html).expect("CSRF token on bookings page");

    // 9. Cancel booking: POST /bookings/{booking_id}/cancel
    let cancel_form = format!("csrf_token={}", urlencode(&cancel_csrf));
    let (status, cancel_headers, _) = post_form_response(
        &app,
        &format!("/bookings/{}/cancel", booking_id),
        Some(&attendee_cookie),
        &cancel_form,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        cancel_headers
            .get(axum::http::header::LOCATION)
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
    let (app, _upload) = create_test_app_with_pool(&pool);

    // Login as Alice Organizer (alice@events.com / Organizer@1)
    let (org_cookie, _) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // 1. GET /events/create -> 200 OK
    let (status, _, create_page_html) =
        get_response(&app, "/events/create", Some(&org_cookie)).await;
    assert_eq!(status, StatusCode::OK);
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

    let (status, resp_headers, _) = post_multipart_response(
        &app,
        "/events/create",
        Some(&org_cookie),
        &content_type,
        payload,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        resp_headers
            .get(axum::http::header::LOCATION)
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
    let (status, _, edit_html) = get_response(
        &app,
        &format!("/events/{}/edit", event_id),
        Some(&org_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
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

    let (status, _, _) = post_multipart_response(
        &app,
        &format!("/events/{}/edit", event_id),
        Some(&org_cookie),
        &ct_no_banner,
        payload_no_banner,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

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

    let (status, _, _) = post_multipart_response(
        &app,
        &format!("/events/{}/edit", event_id),
        Some(&org_cookie),
        &ct_replace,
        payload_replace,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

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
    let (_, _, dash_html) = get_response(&app, "/dashboard", Some(&org_cookie)).await;
    let toggle_csrf = extract_csrf_token(&dash_html).expect("CSRF token on dashboard");

    let toggle_form = format!("csrf_token={}", urlencode(&toggle_csrf));
    let (status, _, _) = post_form_response(
        &app,
        &format!("/events/{}/toggle-status", event_id),
        Some(&org_cookie),
        &toggle_form,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let event_after_toggle: (String,) = sqlx::query_as("SELECT status FROM events WHERE id = ?")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(event_after_toggle.0, "published");

    // 7. Delete event: POST /events/{id}/delete
    let delete_form = format!("csrf_token={}", urlencode(&toggle_csrf));
    let (status, _, _) = post_form_response(
        &app,
        &format!("/events/{}/delete", event_id),
        Some(&org_cookie),
        &delete_form,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

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
    let (app, _upload) = create_test_app_with_pool(&pool);

    // Login as Admin: admin@eventpro.com / Admin@1234
    let (admin_cookie, _) = http_login(&app, "admin@eventpro.com", "Admin@1234").await;

    // 1. GET /admin/dashboard -> 200 OK
    let (status, _, admin_html) = get_response(&app, "/admin/dashboard", Some(&admin_cookie)).await;
    assert_eq!(status, StatusCode::OK);

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
    assert!(
        admin_html.contains("href=\"/events/create\""),
        "Admin has link to Create Event"
    );
    assert!(
        admin_html.contains("href=\"/dashboard\""),
        "Admin has link to Dashboard Event Manager"
    );

    // 2. Admin access to /events/create -> 200 OK
    let (status, _, _) = get_response(&app, "/events/create", Some(&admin_cookie)).await;
    assert_eq!(status, StatusCode::OK);

    // 3. Admin access to /dashboard (all events management overview) -> 200 OK
    let (status, _, _) = get_response(&app, "/dashboard", Some(&admin_cookie)).await;
    assert_eq!(status, StatusCode::OK);
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
    let (app, _upload) = create_test_app_with_pool(&pool);

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
    let (status, _, _) = get_response(
        &app,
        &format!("/tickets/{}", diana_ticket_id),
        Some(&charlie_cookie),
    )
    .await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
        "Attendee cannot view another user's ticket"
    );

    // 2. Charlie attempts to cancel Diana's booking -> 403 or 404
    let (status, _, _) = post_form_response(
        &app,
        &format!("/bookings/{}/cancel", diana_booking_id),
        Some(&charlie_cookie),
        "csrf_token=dummy",
    )
    .await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
        "Attendee cannot cancel another user's booking"
    );

    // 3. Organizer Bob (id 3) attempts to edit Alice's event (id 1, organizer_id 2) -> 403
    let (bob_cookie, _) = http_login(&app, "bob@events.com", "Organizer@1").await;
    let (status, _, _) = get_response(&app, "/events/1/edit", Some(&bob_cookie)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 4. Attendee attempts to access /admin/dashboard -> 403
    let (status, _, _) = get_response(&app, "/admin/dashboard", Some(&charlie_cookie)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 5. Organizer attempts to access /admin/dashboard -> 403
    let (status, _, _) = get_response(&app, "/admin/dashboard", Some(&bob_cookie)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

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
    let (app, _upload) = create_test_app_with_pool(&pool);

    // 1. Nonexistent event: GET /events/999999 -> 404 with styled error page
    let (status, _, html_404) = get_response(&app, "/events/999999", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(html_404.contains("404"), "Includes 404 status code");
    assert!(html_404.contains("This page doesn't exist"));
    assert!(html_404.contains("href=\"/events\""));

    // 2. Search query returning empty result set -> empty state rendered
    let (status, _, html_empty) = get_response(
        &app,
        "/events?search=NonexistentSupercalifragilisticQuery123",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html_empty.contains("No events found"));
    assert!(html_empty.contains("Browse All Events"));

    // 3. Invalid CSRF token on state-changing endpoint -> 403 Forbidden / CSRF error response
    let (charlie_cookie, _) = http_login(&app, "charlie@mail.com", "User@1234").await;
    let (status, _, csrf_err_html) = post_form_response(
        &app,
        "/bookings/confirm",
        Some(&charlie_cookie),
        "event_id=1&quantity=1&csrf_token=invalid_forged_csrf_token",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
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
    let (app, _upload) = create_test_app_with_pool(&pool);

    // 1. Login as attendee
    let (cookie, _) = http_login(&app, "charlie@mail.com", "User@1234").await;

    // 2. Load /dashboard and verify both navbar and sidebar have POST logout forms with CSRF token
    let (status, _, html) = get_response(&app, "/dashboard", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        html.contains("<form action=\"/auth/logout\" method=\"POST\""),
        "Navbar logout form must exist with POST method"
    );
    assert!(
        html.contains("Logout</button>"),
        "Navbar logout button must be present"
    );
    assert!(
        html.contains("🚪 Logout</button>"),
        "Sidebar logout button must be present"
    );

    let csrf = extract_csrf_token(&html).expect("CSRF token present in dashboard HTML");
    assert!(!csrf.is_empty(), "Extracted CSRF token must not be empty");

    // 3. Perform logout using the form payload (navbar flow)
    let (status, logout_headers, _) = post_form_response(
        &app,
        "/auth/logout",
        Some(&cookie),
        &format!("csrf_token={}", csrf),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        logout_headers
            .get(axum::http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/"
    );

    // 4. Verify session is destroyed by accessing /dashboard with previous cookie
    let (status, post_logout_headers, _) = get_response(&app, "/dashboard", Some(&cookie)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        post_logout_headers
            .get(axum::http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/auth/login"
    );

    // 5. Test sidebar flow: login again as organizer
    let (org_cookie, _) = http_login(&app, "bob@events.com", "Organizer@1").await;
    let (status, _, org_html) = get_response(&app, "/dashboard", Some(&org_cookie)).await;
    assert_eq!(status, StatusCode::OK);

    let org_csrf =
        extract_csrf_token(&org_html).expect("CSRF token present in organizer dashboard HTML");
    let (status, sidebar_headers, _) = post_form_response(
        &app,
        "/auth/logout",
        Some(&org_cookie),
        &format!("csrf_token={}", org_csrf),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        sidebar_headers
            .get(axum::http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/"
    );
}
