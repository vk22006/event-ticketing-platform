use crate::{
    middleware::OptionalUser,
    models::{Category, Event, EventFilterParams, SeatType, User},
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};

#[derive(Debug, serde::Deserialize, Default)]
pub struct EventListQuery {
    pub category_id: Option<String>,
    pub city: Option<String>,
    pub search: Option<String>,
    pub page: Option<String>,
}

/// GET / - Redirect to event catalogue
pub async fn root_redirect() -> Response {
    Redirect::to("/events").into_response()
}

/// GET /events - Public event catalogue with filters and search
pub async fn list_events(
    user: OptionalUser,
    State(state): State<AppState>,
    Query(query): Query<EventListQuery>,
) -> Response {
    // Parse filters safely without crashing on malformed query strings
    let category_id = query
        .category_id
        .as_deref()
        .and_then(|s| s.trim().parse::<i32>().ok());

    let city = query
        .city
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    let search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    // Maximum allowable page number to prevent integer overflow and excessive DB offsets
    const MAX_PAGE: i64 = 10_000;

    let page = query
        .page
        .as_deref()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(1)
        .clamp(1, MAX_PAGE);

    let limit = 9i64;
    let offset = (page - 1).checked_mul(limit).unwrap_or(0);

    let filters = EventFilterParams {
        status: Some("published".to_string()),
        category_id,
        city: city.clone(),
        search: search.clone(),
        limit: Some(limit),
        offset: Some(offset),
    };

    // Parallel execution of catalogue and categories queries to avoid sequential lag
    let events_future = Event::find_all(&state.db, &filters);
    let categories_future = Category::find_all(&state.db);

    match tokio::try_join!(events_future, categories_future) {
        Ok((events, categories)) => {
            let query_context = minijinja::context! {
                category_id => category_id,
                city => city.unwrap_or_default(),
                search => search.unwrap_or_default(),
                page => page,
            };

            let context = minijinja::context! {
                title => "Discover Events",
                events => events,
                categories => categories,
                query => query_context,
                user => user.0,
            };

            render_html(&state, "index.html", context, StatusCode::OK)
        }
        Err(err) => {
            tracing::error!(error = %err, "Database failure while querying event catalogue");
            render_500(&state, user.0)
        }
    }
}

/// GET /events/:id - Public event details with seat tier availability
pub async fn show_event(
    user: OptionalUser,
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Response {
    let event_id = match id_str.trim().parse::<i32>() {
        Ok(id) if id > 0 => id,
        _ => {
            tracing::debug!(id = %id_str, "Malformed or invalid event ID requested");
            return render_404(&state, user.0);
        }
    };

    let event_future = Event::find_by_id(&state.db, event_id);
    let seats_future = SeatType::find_by_event(&state.db, event_id);

    match tokio::try_join!(event_future, seats_future) {
        Ok((Some(event), seat_types)) => {
            // Visibility enforcement:
            // Published events are visible to all users (anonymous and authenticated).
            // Unpublished/draft events are visible only to the owning organizer or admins.
            let is_authorized = if event.status == "published" {
                true
            } else if let Some(ref current_user) = user.0 {
                current_user.role == "admin" || current_user.id == event.organizer_id
            } else {
                false
            };

            if !is_authorized {
                return render_404(&state, user.0);
            }

            let context = minijinja::context! {
                title => event.title,
                event => event,
                seatTypes => &seat_types,
                seat_types => &seat_types,
                user => user.0,
            };

            render_html(&state, "event-detail.html", context, StatusCode::OK)
        }
        Ok((None, _)) => render_404(&state, user.0),
        Err(err) => {
            tracing::error!(error = %err, event_id = event_id, "Database failure querying event details");
            render_500(&state, user.0)
        }
    }
}

pub fn render_404(state: &AppState, user: Option<User>) -> Response {
    let context = minijinja::context! {
        title => "404 – Not Found",
        user => user,
        message => "The page you're looking for has been moved, deleted, or never existed.",
    };
    render_html(state, "error/404.html", context, StatusCode::NOT_FOUND)
}

pub fn render_500(state: &AppState, user: Option<User>) -> Response {
    let context = minijinja::context! {
        title => "500 – Server Error",
        user => user,
        message => "Our server encountered an unexpected error. Please try again shortly.",
    };
    render_html(
        state,
        "error/500.html",
        context,
        StatusCode::INTERNAL_SERVER_ERROR,
    )
}

pub fn render_html(
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
