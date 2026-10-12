use crate::{
    middleware::{forbidden_response, AuthenticatedUser},
    models::{Booking, Event, User},
    routes::events::render_500,
    state::AppState,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

/// GET /admin/dashboard - Platform overview for administrator accounts
pub async fn admin_dashboard(user: AuthenticatedUser, State(state): State<AppState>) -> Response {
    let current_user = user.0;
    if current_user.role != "admin" {
        return forbidden_response(&state.jinja, Some("Admin access required."));
    }

    let user_counts_future = User::count_by_role(&state.db);
    let total_events_future = Event::count(&state.db, None);
    let published_events_future = Event::count(&state.db, Some("published"));
    let total_bookings_future = Booking::count_total(&state.db);
    let revenue_future = Booking::total_revenue(&state.db);
    let recent_bookings_future = Booking::recent_bookings(&state.db, 8);

    match tokio::try_join!(
        user_counts_future,
        total_events_future,
        published_events_future,
        total_bookings_future,
        revenue_future,
        recent_bookings_future,
    ) {
        Ok((
            user_counts,
            total_events,
            published_events,
            total_bookings,
            revenue,
            recent_bookings,
        )) => {
            let organizers = *user_counts.get("organizer").unwrap_or(&0);
            let regular_users = *user_counts.get("user").unwrap_or(&0);
            let admin_users = *user_counts.get("admin").unwrap_or(&0);
            let total_users = organizers + regular_users + admin_users;

            let stats = minijinja::context! {
                total_users => total_users,
                totalUsers => total_users,
                organizers => organizers,
                regular_users => regular_users,
                regularUsers => regular_users,
                total_events => total_events,
                totalEvents => total_events,
                published_events => published_events,
                publishedEvents => published_events,
                total_bookings => total_bookings,
                totalBookings => total_bookings,
                revenue => revenue,
            };

            let context = minijinja::context! {
                title => "Admin Dashboard",
                user => current_user,
                stats => stats,
                recent_bookings => recent_bookings,
            };

            match state.jinja.get_template("dashboard/admin.html") {
                Ok(tmpl) => match tmpl.render(context) {
                    Ok(body) => (StatusCode::OK, Html(body)).into_response(),
                    Err(err) => {
                        tracing::error!(error = %err, "Failed to render dashboard/admin.html");
                        render_500(&state, Some(current_user))
                    }
                },
                Err(err) => {
                    tracing::error!(error = %err, "Template dashboard/admin.html not found");
                    render_500(&state, Some(current_user))
                }
            }
        }
        Err(err) => {
            tracing::error!(error = %err, "Failed to fetch stats for admin dashboard");
            render_500(&state, Some(current_user))
        }
    }
}
