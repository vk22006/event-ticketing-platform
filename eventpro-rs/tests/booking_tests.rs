mod common;

use axum::http::{header, StatusCode};
use common::{
    create_test_app, create_test_state, create_test_state_with_pool, extract_csrf_token,
    get_response, http_login, post_form_response, sample_event, sample_user, urlencode,
};
use eventpro::models::{
    booking::{Booking, BookingDetail, BookingError, UserBooking},
    seat_type::SeatType,
    ticket::{Ticket, TicketDetail, UserTicket},
    User,
};
use rust_decimal::Decimal;
use std::collections::HashSet;

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

    assert!(rendered.contains(&event.title));
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

    for uri in [
        "/bookings/book/1",
        "/bookings/payment?eventId=1",
        "/bookings/my",
        "/tickets/my",
    ] {
        let (status, headers, _) = get_response(&app, uri, None).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers.get(header::LOCATION).unwrap().to_str().unwrap(),
            "/auth/login"
        );
    }

    for (uri, body) in [
        ("/bookings/confirm", "event_id=1&quantity=1"),
        ("/bookings/1/cancel", ""),
    ] {
        let (status, headers, _) = post_form_response(&app, uri, None, body).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers.get(header::LOCATION).unwrap().to_str().unwrap(),
            "/auth/login"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Real MySQL Integration: Atomic Booking, Cancellation, Concurrency & Security
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_real_mysql_booking_workflow_lifecycle() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let state = create_test_state_with_pool(&pool);
    let app = create_test_app(state);

    let (attendee_cookie, _) = http_login(&app, "charlie@mail.com", "User@1234").await;
    let (organizer_cookie, organizer_csrf) =
        http_login(&app, "alice@events.com", "Organizer@1").await;
    let (admin_cookie, _) = http_login(&app, "admin@eventpro.com", "Admin@1234").await;

    let charlie = User::find_by_email(&pool, "charlie@mail.com")
        .await
        .unwrap()
        .expect("Charlie must exist in seed data");
    let charlie_id = charlie.id;

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

    // Obtain fresh CSRF token for the attendee session from the payment review page
    let (_, _, pay_get_body) = get_response(
        &app,
        &format!(
            "/bookings/payment?eventId={}&seat_type_id={}&quantity=1",
            event_id, seat_type_id
        ),
        Some(&attendee_cookie),
    )
    .await;
    let attendee_csrf = extract_csrf_token(&pay_get_body)
        .expect("CSRF token must be present in payment review page");

    // A. Role check: Organizers and Admins are FORBIDDEN from booking
    let (st, _, _) = get_response(
        &app,
        &format!("/bookings/book/{}", event_id),
        Some(&organizer_cookie),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "Organizers must not access booking selection form"
    );

    let (st, _, _) = get_response(
        &app,
        &format!("/bookings/book/{}", event_id),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "Admins must not access booking selection form"
    );

    let (st, _, _) = post_form_response(
        &app,
        "/bookings/confirm",
        Some(&organizer_cookie),
        &format!(
            "event_id={}&quantity=1&csrf_token={}",
            event_id,
            urlencode(&organizer_csrf)
        ),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "Organizers must be forbidden from confirming bookings"
    );

    // B. CSRF protection verification
    let (st, _, _) = post_form_response(
        &app,
        "/bookings/confirm",
        Some(&attendee_cookie),
        &format!(
            "event_id={}&quantity=2&csrf_token=INVALID_CSRF_TOKEN",
            event_id
        ),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "Confirming booking with invalid CSRF token must return 403"
    );

    // C. Validation: Quantity bounds (0 and 11)
    let (st, _, _) = post_form_response(
        &app,
        "/bookings/confirm",
        Some(&attendee_cookie),
        &format!(
            "event_id={}&quantity=0&csrf_token={}",
            event_id,
            urlencode(&attendee_csrf)
        ),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "Quantity 0 must be rejected with 400"
    );

    let (st, _, _) = post_form_response(
        &app,
        "/bookings/confirm",
        Some(&attendee_cookie),
        &format!(
            "event_id={}&quantity=11&csrf_token={}",
            event_id,
            urlencode(&attendee_csrf)
        ),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "Quantity > 10 must be rejected with 400"
    );

    // D. Validation: Invalid seat type belonging to another event
    let (st, _, _) = post_form_response(
        &app,
        "/bookings/confirm",
        Some(&attendee_cookie),
        &format!(
            "event_id={}&seat_type_id=999999&quantity=2&csrf_token={}",
            event_id,
            urlencode(&attendee_csrf)
        ),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "Seat type belonging to nonexistent or foreign event must be rejected"
    );

    // E. Successful Booking Creation (Quantity = 3)
    let (status, _, confirm_html) = post_form_response(
        &app,
        "/bookings/confirm",
        Some(&attendee_cookie),
        &format!(
            "event_id={}&seat_type_id={}&quantity=3&total=0.01&csrf_token={}",
            event_id,
            seat_type_id,
            urlencode(&attendee_csrf)
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "Valid booking confirmation must render confirmation page"
    );
    assert!(confirm_html.contains("Booking Confirmed!"));

    // Verify Database Invariants
    let booking_row: (i32, Decimal, String, i32) = sqlx::query_as(
        "SELECT id, total_amount, status, quantity FROM bookings WHERE event_id = ? AND user_id = ? ORDER BY id DESC LIMIT 1"
    )
    .bind(event_id)
    .bind(charlie_id)
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
    let (st, _, _) = get_response(
        &app,
        &format!("/tickets/{}", ticket_id_1),
        Some(&organizer_cookie),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "Non-owner cannot view another attendee's ticket"
    );

    let (st, _, _) = get_response(
        &app,
        &format!("/tickets/{}", ticket_id_1),
        Some(&attendee_cookie),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // G. Cancellation Authorization: Alien user cannot cancel Charlie's booking
    let (st, _, _) = post_form_response(
        &app,
        &format!("/bookings/{}/cancel", booking_id),
        Some(&organizer_cookie),
        &format!("csrf_token={}", urlencode(&organizer_csrf)),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "Alien user cannot cancel someone else's booking"
    );

    // H. Successful Cancellation by Owner
    let (st, _, _) = post_form_response(
        &app,
        &format!("/bookings/{}/cancel", booking_id),
        Some(&attendee_cookie),
        &format!("csrf_token={}", urlencode(&attendee_csrf)),
    )
    .await;
    assert_eq!(
        st,
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
    let (st, _, _) = post_form_response(
        &app,
        &format!("/bookings/{}/cancel", booking_id),
        Some(&attendee_cookie),
        &format!("csrf_token={}", urlencode(&attendee_csrf)),
    )
    .await;
    assert_eq!(
        st,
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
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
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

#[tokio::test]
async fn test_real_mysql_seat_type_consistency() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let future_date = chrono::Utc::now().naive_utc() + chrono::Duration::days(15);

    // 1. Create Event A (Tiered event)
    let title_a = format!("Tiered Event {}", rand::random::<u32>());
    let res_a = sqlx::query(
        "INSERT INTO events (organizer_id, title, venue, city, event_date, total_seats, available_seats, ticket_price, status) \
         VALUES (2, ?, 'Auditorium A', 'Bengaluru', ?, 100, 100, 500.00, 'published')"
    )
    .bind(&title_a)
    .bind(future_date)
    .execute(&pool)
    .await
    .expect("Creating Event A must succeed");
    let evt_a_id = res_a.last_insert_id() as i32;

    // Create seat tier for Event A
    let res_st_a = sqlx::query(
        "INSERT INTO seat_types (event_id, type_name, price, total_seats, booked_seats) \
         VALUES (?, 'VIP Tier', 1000.00, 20, 0)",
    )
    .bind(evt_a_id)
    .execute(&pool)
    .await
    .expect("Creating Seat Type A must succeed");
    let st_a_id = res_st_a.last_insert_id() as i32;

    // 2. Create Event B (Foreign event with its own tier)
    let title_b = format!("Foreign Tiered Event {}", rand::random::<u32>());
    let res_b = sqlx::query(
        "INSERT INTO events (organizer_id, title, venue, city, event_date, total_seats, available_seats, ticket_price, status) \
         VALUES (2, ?, 'Auditorium B', 'Delhi', ?, 50, 50, 300.00, 'published')"
    )
    .bind(&title_b)
    .bind(future_date)
    .execute(&pool)
    .await
    .expect("Creating Event B must succeed");
    let evt_b_id = res_b.last_insert_id() as i32;

    let res_st_b = sqlx::query(
        "INSERT INTO seat_types (event_id, type_name, price, total_seats, booked_seats) \
         VALUES (?, 'Balcony Tier', 400.00, 10, 0)",
    )
    .bind(evt_b_id)
    .execute(&pool)
    .await
    .expect("Creating Seat Type B must succeed");
    let st_b_id = res_st_b.last_insert_id() as i32;

    // 3. Create Event C (General Admission - NO seat tiers defined)
    let title_c = format!("General Admission Event {}", rand::random::<u32>());
    let res_c = sqlx::query(
        "INSERT INTO events (organizer_id, title, venue, city, event_date, total_seats, available_seats, ticket_price, status) \
         VALUES (2, ?, 'Open Ground', 'Mumbai', ?, 200, 200, 250.00, 'published')"
    )
    .bind(&title_c)
    .bind(future_date)
    .execute(&pool)
    .await
    .expect("Creating Event C must succeed");
    let evt_c_id = res_c.last_insert_id() as i32;

    // --- Subtest 1: Missing seat type on a tiered event MUST be rejected with SeatTypeRequired ---
    let missing_st_res = Booking::create_transactional(&pool, 1, evt_a_id, None, 2).await;
    match missing_st_res {
        Err(BookingError::SeatTypeRequired) => {}
        other => panic!(
            "Expected BookingError::SeatTypeRequired on tiered event with missing seat_type_id, got {:?}",
            other
        ),
    }

    // Invariants: No booking record, no tickets, no seat count changes
    let bkg_count_a: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bookings WHERE event_id = ?")
        .bind(evt_a_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        bkg_count_a.0, 0,
        "No bookings should be created on failed tier validation"
    );

    let tkt_count_a: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tickets WHERE event_id = ?")
        .bind(evt_a_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        tkt_count_a.0, 0,
        "No tickets should be created on failed tier validation"
    );

    let ev_a_seats: (i32,) = sqlx::query_as("SELECT available_seats FROM events WHERE id = ?")
        .bind(evt_a_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        ev_a_seats.0, 100,
        "Event available_seats must remain unchanged"
    );

    let st_a_booked: (i32,) = sqlx::query_as("SELECT booked_seats FROM seat_types WHERE id = ?")
        .bind(st_a_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        st_a_booked.0, 0,
        "SeatType booked_seats must remain unchanged"
    );

    // --- Subtest 2: Valid tier selection on a tiered event MUST succeed ---
    let valid_tier_res = Booking::create_transactional(&pool, 1, evt_a_id, Some(st_a_id), 2)
        .await
        .expect("Valid tier booking should succeed");
    assert_eq!(valid_tier_res.tickets.len(), 2);
    assert_eq!(valid_tier_res.total_amount, Decimal::new(200000, 2)); // 1000.00 * 2

    let ev_a_seats_after: (i32,) =
        sqlx::query_as("SELECT available_seats FROM events WHERE id = ?")
            .bind(evt_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        ev_a_seats_after.0, 98,
        "Event available_seats must be decremented by 2"
    );

    let st_a_booked_after: (i32,) =
        sqlx::query_as("SELECT booked_seats FROM seat_types WHERE id = ?")
            .bind(st_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        st_a_booked_after.0, 2,
        "SeatType booked_seats must be incremented by 2"
    );

    // --- Subtest 3: Foreign event's seat type MUST be rejected with InvalidSeatType ---
    let foreign_st_res = Booking::create_transactional(&pool, 1, evt_a_id, Some(st_b_id), 1).await;
    match foreign_st_res {
        Err(BookingError::InvalidSeatType) => {}
        other => panic!(
            "Expected BookingError::InvalidSeatType on foreign seat type, got {:?}",
            other
        ),
    }

    let ev_a_seats_after_foreign: (i32,) =
        sqlx::query_as("SELECT available_seats FROM events WHERE id = ?")
            .bind(evt_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        ev_a_seats_after_foreign.0, 98,
        "Event available_seats must not change on rejected foreign seat type"
    );

    let st_b_booked: (i32,) = sqlx::query_as("SELECT booked_seats FROM seat_types WHERE id = ?")
        .bind(st_b_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        st_b_booked.0, 0,
        "Foreign seat tier booked_seats must remain 0"
    );

    // --- Subtest 4: General admission event (no tiers) without seat_type_id MUST succeed ---
    let ga_res = Booking::create_transactional(&pool, 1, evt_c_id, None, 3)
        .await
        .expect("General admission booking should succeed without seat_type_id");
    assert_eq!(ga_res.tickets.len(), 3);
    assert_eq!(ga_res.total_amount, Decimal::new(75000, 2)); // 250.00 * 3

    let ev_c_seats: (i32,) = sqlx::query_as("SELECT available_seats FROM events WHERE id = ?")
        .bind(evt_c_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        ev_c_seats.0, 197,
        "GA event available_seats must be decremented by 3"
    );

    // --- Subtest 5: General admission event WITH seat_type_id MUST be rejected ---
    let ga_with_st_res = Booking::create_transactional(&pool, 1, evt_c_id, Some(st_a_id), 1).await;
    match ga_with_st_res {
        Err(BookingError::InvalidSeatType) => {}
        other => panic!(
            "Expected BookingError::InvalidSeatType on GA event with seat_type_id, got {:?}",
            other
        ),
    }

    // Clean up all dynamically created records
    let _ = sqlx::query("DELETE FROM tickets WHERE event_id IN (?, ?, ?)")
        .bind(evt_a_id)
        .bind(evt_b_id)
        .bind(evt_c_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM bookings WHERE event_id IN (?, ?, ?)")
        .bind(evt_a_id)
        .bind(evt_b_id)
        .bind(evt_c_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM seat_types WHERE event_id IN (?, ?)")
        .bind(evt_a_id)
        .bind(evt_b_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM events WHERE id IN (?, ?, ?)")
        .bind(evt_a_id)
        .bind(evt_b_id)
        .bind(evt_c_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn test_real_mysql_ticket_code_collision_retry_and_exhaustion() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let future_date = chrono::Utc::now().naive_utc() + chrono::Duration::days(20);

    // Create an event for deterministic collision tests
    let event_title = format!("Collision Retry Event {}", rand::random::<u32>());
    let res = sqlx::query(
        "INSERT INTO events (organizer_id, title, venue, city, event_date, total_seats, available_seats, ticket_price, status) \
         VALUES (2, ?, 'Coliseum', 'Hyderabad', ?, 50, 50, 150.00, 'published')"
    )
    .bind(&event_title)
    .bind(future_date)
    .execute(&pool)
    .await
    .expect("Creating collision test event must succeed");
    let evt_id = res.last_insert_id() as i32;

    // Create a base booking to hold a colliding ticket
    let bkg_res = sqlx::query(
        "INSERT INTO bookings (user_id, event_id, quantity, total_amount, status, payment_method, booked_at) \
         VALUES (1, ?, 1, 150.00, 'confirmed', 'mock_payment', NOW())"
    )
    .bind(evt_id)
    .execute(&pool)
    .await
    .expect("Creating baseline booking must succeed");
    let baseline_bkg_id = bkg_res.last_insert_id() as i32;

    // Pre-insert an existing ticket code that will trigger unique constraint error 1062
    let colliding_code = format!("COLLISION-TEST-CODE-{}", rand::random::<u32>());
    sqlx::query(
        "INSERT INTO tickets (booking_id, user_id, event_id, ticket_code, status, issued_at) \
         VALUES (?, 1, ?, ?, 'active', NOW())",
    )
    .bind(baseline_bkg_id)
    .bind(evt_id)
    .bind(&colliding_code)
    .execute(&pool)
    .await
    .expect("Pre-inserting colliding ticket code must succeed");

    // --- Subtest 1: Deterministic Collision Retry with Successful Recovery ---
    let attempt_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter_clone = attempt_counter.clone();
    let collision_target = colliding_code.clone();

    let recovering_generator = move |ev_id: i32, bkg_id: i32| {
        let count = counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if count == 0 {
            // First attempt: deliberately collide with pre-inserted code
            collision_target.clone()
        } else {
            // Second attempt: generate unique code
            format!(
                "RECOVERED-TICKET-EVT-{}-BKG-{}-{}",
                ev_id,
                bkg_id,
                rand::random::<u32>()
            )
        }
    };

    let booking_res = Booking::create_transactional_with_generator(
        &pool,
        1,
        evt_id,
        None,
        1,
        recovering_generator,
    )
    .await
    .expect("Booking should succeed after colliding code is regenerated");

    assert_eq!(
        attempt_counter.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "Generator should have been invoked exactly twice (1 collision + 1 retry success)"
    );
    assert_eq!(booking_res.tickets.len(), 1);
    assert!(
        booking_res.tickets[0]
            .ticket_code
            .starts_with("RECOVERED-TICKET-"),
        "Recovered ticket must have the regenerated code"
    );

    // --- Subtest 2: Deterministic Collision Retry Exhaustion & Complete Rollback ---
    let seats_before: (i32,) = sqlx::query_as("SELECT available_seats FROM events WHERE id = ?")
        .bind(evt_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let bkgs_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bookings WHERE event_id = ?")
        .bind(evt_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let tkts_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tickets WHERE event_id = ?")
        .bind(evt_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let exhaust_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let exhaust_clone = exhaust_counter.clone();
    let collision_exhaust_target = colliding_code.clone();

    let always_colliding_generator = move |_ev_id: i32, _bkg_id: i32| {
        exhaust_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        collision_exhaust_target.clone()
    };

    let exhaust_res = Booking::create_transactional_with_generator(
        &pool,
        1,
        evt_id,
        None,
        1,
        always_colliding_generator,
    )
    .await;

    match exhaust_res {
        Err(BookingError::TicketGenerationFailed) => {}
        other => panic!(
            "Expected BookingError::TicketGenerationFailed when collisions exhaust retries, got {:?}",
            other
        ),
    }

    assert_eq!(
        exhaust_counter.load(std::sync::atomic::Ordering::SeqCst),
        eventpro::models::booking::MAX_TICKET_CODE_RETRIES,
        "Generator must be called exactly MAX_TICKET_CODE_RETRIES times before aborting"
    );

    // Verify atomicity and total rollback
    let seats_after: (i32,) = sqlx::query_as("SELECT available_seats FROM events WHERE id = ?")
        .bind(evt_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        seats_after.0, seats_before.0,
        "Available seats must not be decremented when ticket retry fails"
    );

    let bkgs_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bookings WHERE event_id = ?")
        .bind(evt_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        bkgs_after.0, bkgs_before.0,
        "Booking record must be rolled back on retry exhaustion"
    );

    let tkts_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tickets WHERE event_id = ?")
        .bind(evt_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        tkts_after.0, tkts_before.0,
        "No tickets should remain after transaction rollback"
    );

    // Cleanup
    let _ = sqlx::query("DELETE FROM tickets WHERE event_id = ?")
        .bind(evt_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM bookings WHERE event_id = ?")
        .bind(evt_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM events WHERE id = ?")
        .bind(evt_id)
        .execute(&pool)
        .await;
}
