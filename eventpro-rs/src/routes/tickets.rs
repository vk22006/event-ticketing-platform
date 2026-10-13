use crate::{
    middleware::{forbidden_response, AuthenticatedUser},
    models::ticket::Ticket,
    routes::events::{render_404, render_500, render_html},
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};

/// GET /tickets/my - Display all tickets belonging to the authenticated attendee
pub async fn my_tickets(user: AuthenticatedUser, State(state): State<AppState>) -> Response {
    let current_user = user.0;

    match Ticket::find_by_user(&state.db, current_user.id).await {
        Ok(tickets) => {
            let context = minijinja::context! {
                title => "My Tickets",
                tickets => tickets,
                user => current_user,
            };

            render_html(&state, "dashboard/my-tickets.html", context, StatusCode::OK)
        }
        Err(err) => {
            tracing::error!(error = %err, "Failed to fetch user tickets");
            render_500(&state, Some(current_user))
        }
    }
}

/// GET /tickets/:id - View single ticket details
pub async fn view_ticket(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Response {
    let current_user = user.0;

    let ticket_id = match id_str.trim().parse::<i32>() {
        Ok(id) if id > 0 => id,
        _ => return render_404(&state, Some(current_user)),
    };

    match Ticket::find_by_id(&state.db, ticket_id).await {
        Ok(Some(ticket)) => {
            if ticket.user_id != current_user.id && current_user.role != "admin" {
                return forbidden_response(
                    &state.jinja,
                    Some("You are not authorized to view this ticket."),
                );
            }

            let context = minijinja::context! {
                title => format!("Ticket – {}", ticket.ticket_code),
                ticket => ticket,
                user => current_user,
            };

            render_html(&state, "ticket/view.html", context, StatusCode::OK)
        }
        Ok(None) => render_404(&state, Some(current_user)),
        Err(err) => {
            tracing::error!(error = %err, "Failed to load ticket details");
            render_500(&state, Some(current_user))
        }
    }
}

/// GET /tickets/:id/print - View printable ticket without layout chrome
pub async fn print_ticket(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Response {
    let current_user = user.0;

    let ticket_id = match id_str.trim().parse::<i32>() {
        Ok(id) if id > 0 => id,
        _ => return render_404(&state, Some(current_user)),
    };

    match Ticket::find_by_id(&state.db, ticket_id).await {
        Ok(Some(ticket)) => {
            if ticket.user_id != current_user.id && current_user.role != "admin" {
                return forbidden_response(
                    &state.jinja,
                    Some("You are not authorized to view this ticket."),
                );
            }

            let context = minijinja::context! {
                title => format!("Print Ticket – {}", ticket.ticket_code),
                ticket => ticket,
            };

            render_html(&state, "ticket/print.html", context, StatusCode::OK)
        }
        Ok(None) => render_404(&state, Some(current_user)),
        Err(err) => {
            tracing::error!(error = %err, "Failed to load ticket for printing");
            render_500(&state, Some(current_user))
        }
    }
}
