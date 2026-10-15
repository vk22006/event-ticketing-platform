use crate::{
    csrf::get_or_create_csrf_token,
    middleware::AuthenticatedUser,
    models::{Booking, Event, UserBooking},
    state::AppState,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

/// GET /dashboard - Role-aware dashboard for organizers, admins, and attendees
pub async fn show_dashboard(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
) -> Response {
    let csrf_token = match get_or_create_csrf_token(&session).await {
        Ok(token) => token,
        Err(err) => {
            tracing::error!(error = %err, "Failed to get CSRF token on dashboard");
            return Redirect::to("/").into_response();
        }
    };

    let current_user = user.0;

    if current_user.role == "organizer" {
        match Event::get_by_organizer(&state.db, current_user.id).await {
            Ok(events) => {
                let context = minijinja::context! {
                    title => "Dashboard",
                    user => current_user,
                    events => events,
                    bookings => Vec::<UserBooking>::new(),
                    csrf_token => csrf_token,
                };
                render_html(&state, "dashboard/home.html", context, StatusCode::OK)
            }
            Err(err) => {
                tracing::error!(error = %err, user_id = current_user.id, "Failed to query organizer events");
                render_500(&state)
            }
        }
    } else if current_user.role == "admin" {
        match Event::find_all_unfiltered(&state.db).await {
            Ok(events) => {
                let context = minijinja::context! {
                    title => "Dashboard",
                    user => current_user,
                    events => events,
                    bookings => Vec::<UserBooking>::new(),
                    csrf_token => csrf_token,
                };
                render_html(&state, "dashboard/home.html", context, StatusCode::OK)
            }
            Err(err) => {
                tracing::error!(error = %err, "Failed to query admin events for dashboard");
                render_500(&state)
            }
        }
    } else {
        // Attendee view: list user's bookings
        match Booking::find_by_user(&state.db, current_user.id).await {
            Ok(bookings) => {
                let context = minijinja::context! {
                    title => "Dashboard",
                    user => current_user,
                    events => Vec::<Event>::new(),
                    bookings => bookings,
                    csrf_token => csrf_token,
                };
                render_html(&state, "dashboard/home.html", context, StatusCode::OK)
            }
            Err(err) => {
                tracing::error!(error = %err, user_id = current_user.id, "Failed to query user bookings");
                render_500(&state)
            }
        }
    }
}

fn render_500(state: &AppState) -> Response {
    let context = minijinja::context! {
        title => "500 – Server Error",
        message => "Our server encountered an unexpected error. Please try again shortly.",
    };
    render_html(
        state,
        "error/500.html",
        context,
        StatusCode::INTERNAL_SERVER_ERROR,
    )
}

fn render_html(
    state: &AppState,
    template_name: &str,
    context: minijinja::Value,
    status: StatusCode,
) -> Response {
    match state.jinja.get_template(template_name) {
        Ok(tmpl) => match tmpl.render(context) {
            Ok(body) => (status, Html(body)).into_response(),
            Err(err) => {
                tracing::error!(error = %err, template = %template_name, "Template rendering failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html("<h1>500 Internal Server Error</h1><p>Failed to render page.</p>"),
                )
                    .into_response()
            }
        },
        Err(err) => {
            tracing::error!(error = %err, template = %template_name, "Template not found");
            (
                StatusCode::NOT_FOUND,
                Html("<h1>404 Not Found</h1><p>Template not found.</p>"),
            )
                .into_response()
        }
    }
}
