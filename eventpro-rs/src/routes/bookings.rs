use crate::{
    csrf::{csrf_error_response, get_or_create_csrf_token, validate_csrf_token},
    middleware::{forbidden_response, AuthenticatedUser},
    models::{
        booking::{Booking, BookingError, CancelError},
        event::Event,
        seat_type::SeatType,
    },
    routes::events::{render_404, render_500, render_html},
    state::AppState,
};
use axum::{
    extract::{Form, Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use rust_decimal::Decimal;
use tower_sessions::Session;

#[derive(Debug, serde::Deserialize, Default)]
pub struct PaymentQuery {
    #[serde(rename = "eventId")]
    pub event_id: Option<i32>,
    pub seat_type_id: Option<String>,
    pub quantity: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default, Clone)]
pub struct ConfirmBookingForm {
    pub csrf_token: Option<String>,
    pub _csrf: Option<String>,
    pub event_id: Option<String>,
    pub seat_type_id: Option<String>,
    pub quantity: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default, Clone)]
pub struct CancelBookingForm {
    pub csrf_token: Option<String>,
    pub _csrf: Option<String>,
}

/// GET /bookings/book/:id - Display seat and ticket selection form
pub async fn show_booking_form(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Response {
    let current_user = user.0;
    if current_user.role != "user" {
        return forbidden_response(
            &state.jinja,
            Some("Organizers and administrators cannot book tickets."),
        );
    }

    let event_id = match id_str.trim().parse::<i32>() {
        Ok(id) if id > 0 => id,
        _ => return render_404(&state, Some(current_user)),
    };

    let event_fut = Event::find_by_id(&state.db, event_id);
    let seats_fut = SeatType::find_by_event(&state.db, event_id);

    match tokio::try_join!(event_fut, seats_fut) {
        Ok((Some(event), seat_types)) => {
            if event.status != "published" {
                return render_404(&state, Some(current_user));
            }

            let error = if event.available_seats <= 0 {
                Some("Sorry, this event is sold out.")
            } else {
                None
            };

            let csrf_token = get_or_create_csrf_token(&session).await.unwrap_or_default();
            let context = minijinja::context! {
                title => "Book Tickets",
                event => event,
                seatTypes => seat_types,
                error => error,
                user => current_user,
                csrf_token => csrf_token,
            };

            render_html(&state, "booking/select.html", context, StatusCode::OK)
        }
        Ok((None, _)) => render_404(&state, Some(current_user)),
        Err(err) => {
            tracing::error!(error = %err, "Failed to load event for booking selection");
            render_500(&state, Some(current_user))
        }
    }
}

/// GET /bookings/payment - Display payment review stub
pub async fn show_payment(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
    Query(query): Query<PaymentQuery>,
) -> Response {
    let current_user = user.0;
    if current_user.role != "user" {
        return forbidden_response(
            &state.jinja,
            Some("Organizers and administrators cannot book tickets."),
        );
    }

    let event_id = match query.event_id {
        Some(id) if id > 0 => id,
        _ => return render_404(&state, Some(current_user)),
    };

    let qty = query
        .quantity
        .as_deref()
        .and_then(|s| s.trim().parse::<i32>().ok())
        .unwrap_or(1)
        .clamp(1, 10);

    let seat_type_id = query
        .seat_type_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.trim().parse::<i32>().ok());

    let event_fut = Event::find_by_id(&state.db, event_id);
    let seats_fut = SeatType::find_by_event(&state.db, event_id);

    match tokio::try_join!(event_fut, seats_fut) {
        Ok((Some(event), seat_types)) => {
            if event.status != "published" {
                return render_404(&state, Some(current_user));
            }

            let selected_seat =
                seat_type_id.and_then(|st_id| seat_types.iter().find(|s| s.id == st_id).cloned());

            if !seat_types.is_empty() && seat_type_id.is_none() {
                let context = minijinja::context! {
                    title => "Book Tickets",
                    event => event,
                    seatTypes => seat_types,
                    error => "Please select a seat tier.",
                    user => current_user,
                };
                return render_html(
                    &state,
                    "booking/select.html",
                    context,
                    StatusCode::BAD_REQUEST,
                );
            }

            if seat_types.is_empty() && seat_type_id.is_some() {
                return (StatusCode::BAD_REQUEST, Html("Invalid seat type selected"))
                    .into_response();
            }

            if !seat_types.is_empty() && seat_type_id.is_some() && selected_seat.is_none() {
                return (StatusCode::BAD_REQUEST, Html("Invalid seat type selected"))
                    .into_response();
            }

            let price = selected_seat
                .as_ref()
                .map(|s| s.price)
                .unwrap_or(event.ticket_price);
            let total = price * Decimal::from(qty);

            let csrf_token = match get_or_create_csrf_token(&session).await {
                Ok(token) => token,
                Err(err) => {
                    tracing::error!(error = %err, "Failed to obtain CSRF token for payment");
                    return render_500(&state, Some(current_user));
                }
            };

            let context = minijinja::context! {
                title => "Payment",
                event => event,
                seatType => selected_seat,
                quantity => qty,
                total => total,
                user => current_user,
                csrf_token => csrf_token,
            };

            render_html(&state, "booking/payment.html", context, StatusCode::OK)
        }
        Ok((None, _)) => render_404(&state, Some(current_user)),
        Err(err) => {
            tracing::error!(error = %err, "Database error loading payment review");
            render_500(&state, Some(current_user))
        }
    }
}

/// POST /bookings/confirm - Process atomic transactional booking creation
pub async fn confirm_booking(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
    Form(form): Form<ConfirmBookingForm>,
) -> Response {
    let current_user = user.0;
    if current_user.role != "user" {
        return forbidden_response(
            &state.jinja,
            Some("Organizers and administrators cannot book tickets."),
        );
    }

    let submitted_token = form
        .csrf_token
        .as_deref()
        .or(form._csrf.as_deref())
        .unwrap_or("");

    if !validate_csrf_token(&session, submitted_token).await {
        return csrf_error_response(&state.jinja);
    }

    let event_id = match form
        .event_id
        .as_deref()
        .and_then(|s| s.trim().parse::<i32>().ok())
    {
        Some(id) if id > 0 => id,
        _ => return (StatusCode::BAD_REQUEST, Html("Invalid event ID")).into_response(),
    };

    let seat_type_id = form
        .seat_type_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.trim().parse::<i32>().ok());

    let quantity = match form
        .quantity
        .as_deref()
        .and_then(|s| s.trim().parse::<i32>().ok())
    {
        Some(qty) if (1..=10).contains(&qty) => qty,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Html("Quantity must be between 1 and 10"),
            )
                .into_response();
        }
    };

    match Booking::create_transactional(
        &state.db,
        current_user.id,
        event_id,
        seat_type_id,
        quantity,
    )
    .await
    {
        Ok(result) => {
            let event = Event::find_by_id(&state.db, event_id).await.ok().flatten();
            let booking = Booking::find_by_id(&state.db, result.booking_id)
                .await
                .ok()
                .flatten();

            let csrf_token = get_or_create_csrf_token(&session).await.unwrap_or_default();
            let context = minijinja::context! {
                title => "Booking Confirmed!",
                booking => booking,
                tickets => result.tickets,
                event => event,
                user => current_user,
                csrf_token => csrf_token,
            };

            render_html(&state, "booking/confirmation.html", context, StatusCode::OK)
        }
        Err(BookingError::EventNotFound) => render_404(&state, Some(current_user)),
        Err(BookingError::InvalidQuantity) => (
            StatusCode::BAD_REQUEST,
            Html("Quantity must be between 1 and 10"),
        )
            .into_response(),
        Err(BookingError::InsufficientEventCapacity)
        | Err(BookingError::InsufficientSeatTypeCapacity) => {
            let context = minijinja::context! {
                title => "Sold Out",
                message => "Not enough seats available.",
                user => current_user,
            };
            render_html(
                &state,
                "error/500.html",
                context,
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
        Err(BookingError::EventExpired) => {
            (StatusCode::BAD_REQUEST, Html("Event has already started")).into_response()
        }
        Err(BookingError::RegistrationClosed) => (
            StatusCode::BAD_REQUEST,
            Html("Registration deadline has passed"),
        )
            .into_response(),
        Err(BookingError::SeatTypeRequired) => (
            StatusCode::BAD_REQUEST,
            Html("Seat tier selection is required for this event"),
        )
            .into_response(),
        Err(BookingError::InvalidSeatType) => {
            (StatusCode::BAD_REQUEST, Html("Invalid seat type selected")).into_response()
        }
        Err(BookingError::TicketGenerationFailed) => {
            let context = minijinja::context! {
                title => "Booking Error",
                message => "Unable to complete booking due to a ticket code conflict. Please try again.",
                user => current_user,
            };
            render_html(
                &state,
                "error/500.html",
                context,
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
        Err(BookingError::Database(err)) => {
            tracing::error!(error = %err, "Database error during booking transaction");
            render_500(&state, Some(current_user))
        }
    }
}

/// GET /bookings/my - Display bookings for the current attendee
pub async fn my_bookings(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
) -> Response {
    let current_user = user.0;

    let csrf_token = match get_or_create_csrf_token(&session).await {
        Ok(token) => token,
        Err(err) => {
            tracing::error!(error = %err, "Failed to get CSRF token for bookings dashboard");
            return render_500(&state, Some(current_user));
        }
    };

    match Booking::find_by_user(&state.db, current_user.id).await {
        Ok(bookings) => {
            let context = minijinja::context! {
                title => "My Bookings",
                bookings => bookings,
                user => current_user,
                csrf_token => csrf_token,
            };

            render_html(
                &state,
                "dashboard/my-bookings.html",
                context,
                StatusCode::OK,
            )
        }
        Err(err) => {
            tracing::error!(error = %err, "Failed to fetch bookings for user");
            render_500(&state, Some(current_user))
        }
    }
}

/// POST /bookings/:id/cancel and POST /bookings/cancel/:id - Atomically cancel booking
pub async fn cancel_booking(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    Form(form): Form<CancelBookingForm>,
) -> Response {
    let current_user = user.0;

    let submitted_token = form
        .csrf_token
        .as_deref()
        .or(form._csrf.as_deref())
        .unwrap_or("");

    if !validate_csrf_token(&session, submitted_token).await {
        return csrf_error_response(&state.jinja);
    }

    let booking_id = match id_str.trim().parse::<i32>() {
        Ok(id) if id > 0 => id,
        _ => return render_404(&state, Some(current_user)),
    };

    let is_admin = current_user.role == "admin";

    match Booking::cancel_transactional(&state.db, booking_id, current_user.id, is_admin).await {
        Ok(()) => Redirect::to("/bookings/my").into_response(),
        Err(CancelError::NotFound) => render_404(&state, Some(current_user)),
        Err(CancelError::Forbidden) => forbidden_response(
            &state.jinja,
            Some("You are not authorized to cancel this booking."),
        ),
        Err(CancelError::InvalidStatus) => (
            StatusCode::BAD_REQUEST,
            Html("Booking cannot be cancelled in its current status"),
        )
            .into_response(),
        Err(CancelError::Database(err)) => {
            tracing::error!(error = %err, "Database error during cancellation");
            render_500(&state, Some(current_user))
        }
    }
}
