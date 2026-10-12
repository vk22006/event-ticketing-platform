use crate::{
    csrf::{csrf_error_response, get_or_create_csrf_token, validate_csrf_token},
    middleware::{forbidden_response, AuthenticatedUser, OptionalUser},
    models::{
        Category, CreateEventParams, Event, EventFilterParams, SeatType, UpdateEventParams, User,
    },
    state::AppState,
};
use axum::{
    extract::{Form, Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

#[derive(Debug, serde::Deserialize, Default)]
pub struct EventListQuery {
    pub category_id: Option<String>,
    pub city: Option<String>,
    pub search: Option<String>,
    pub page: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Default, Clone)]
pub struct EventFormData {
    pub csrf_token: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub event_date: Option<String>,
    pub registration_deadline: Option<String>,
    pub total_seats: Option<String>,
    pub ticket_price: Option<String>,
    pub category_id: Option<String>,
    pub status: Option<String>,
    // Client-supplied organizer_id is ignored to prevent privilege escalation
    pub organizer_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct ActionFormData {
    pub csrf_token: Option<String>,
}

/// Helper to determine if a user has management permissions over an event.
/// Admins can manage any event; organizers may only manage events they own.
pub fn can_manage(event: &Event, user: &User) -> bool {
    user.role == "admin" || event.organizer_id == user.id
}

/// Parse date-time strings from HTML5 datetime-local inputs
fn parse_datetime(s: &str) -> Option<chrono::NaiveDateTime> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M"))
        .ok()
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

// ─── Organizer / Admin Event Management ──────────────────────────────────────

/// GET /events/create - Render event creation form
pub async fn show_create(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
) -> Response {
    let current_user = user.0;
    if current_user.role != "organizer" && current_user.role != "admin" {
        return forbidden_response(&state.jinja, None);
    }

    let categories = match Category::find_all(&state.db).await {
        Ok(c) => c,
        Err(err) => {
            tracing::error!(error = %err, "Failed to load categories for event creation");
            return render_500(&state, Some(current_user));
        }
    };

    let csrf_token = match get_or_create_csrf_token(&session).await {
        Ok(token) => token,
        Err(err) => {
            tracing::error!(error = %err, "Failed to generate CSRF token for event creation");
            return render_500(&state, Some(current_user));
        }
    };

    let context = minijinja::context! {
        title => "Create Event",
        categories => categories,
        error => None::<String>,
        user => current_user,
        csrf_token => csrf_token,
    };

    render_html(
        &state,
        "dashboard/create-event.html",
        context,
        StatusCode::OK,
    )
}

/// POST /events/create - Validate and create new event
pub async fn create_event(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
    Form(form): Form<EventFormData>,
) -> Response {
    let current_user = user.0;
    if current_user.role != "organizer" && current_user.role != "admin" {
        return forbidden_response(&state.jinja, None);
    }

    let token = form.csrf_token.as_deref().unwrap_or("");
    if !validate_csrf_token(&session, token).await {
        return csrf_error_response(&state.jinja);
    }

    let title = match form
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(t) if t.len() <= 255 => t.to_string(),
        Some(_) => {
            return render_create_error(
                &state,
                &session,
                &current_user,
                &form,
                "Title must be 255 characters or fewer.",
            )
            .await;
        }
        None => {
            return render_create_error(
                &state,
                &session,
                &current_user,
                &form,
                "Title, event date, and total seats are required.",
            )
            .await;
        }
    };

    let event_date = match form.event_date.as_deref().and_then(parse_datetime) {
        Some(dt) => dt,
        None => {
            return render_create_error(
                &state,
                &session,
                &current_user,
                &form,
                "Title, event date, and total seats are required.",
            )
            .await;
        }
    };

    let total_seats = match form
        .total_seats
        .as_deref()
        .and_then(|s| s.trim().parse::<i32>().ok())
    {
        Some(seats) if seats > 0 => seats,
        _ => {
            return render_create_error(
                &state,
                &session,
                &current_user,
                &form,
                "Total seats must be a positive number.",
            )
            .await;
        }
    };

    let registration_deadline = match form.registration_deadline.as_deref() {
        Some(s) if !s.trim().is_empty() => match parse_datetime(s) {
            Some(dt) => {
                if dt > event_date {
                    return render_create_error(
                        &state,
                        &session,
                        &current_user,
                        &form,
                        "Registration deadline cannot be after the event date.",
                    )
                    .await;
                }
                Some(dt)
            }
            None => {
                return render_create_error(
                    &state,
                    &session,
                    &current_user,
                    &form,
                    "Invalid registration deadline format.",
                )
                .await;
            }
        },
        _ => None,
    };

    let ticket_price = match form.ticket_price.as_deref() {
        Some(s) if !s.trim().is_empty() => match s.trim().parse::<rust_decimal::Decimal>() {
            Ok(price) if price >= rust_decimal::Decimal::ZERO => price,
            _ => {
                return render_create_error(
                    &state,
                    &session,
                    &current_user,
                    &form,
                    "Ticket price must be a valid non-negative amount.",
                )
                .await;
            }
        },
        _ => rust_decimal::Decimal::ZERO,
    };

    let category_id = match form.category_id.as_deref() {
        Some(s) if !s.trim().is_empty() => match s.trim().parse::<i32>() {
            Ok(cat_id) => match Category::find_by_id(&state.db, cat_id).await {
                Ok(Some(_)) => Some(cat_id),
                Ok(None) => {
                    return render_create_error(
                        &state,
                        &session,
                        &current_user,
                        &form,
                        "Selected category does not exist.",
                    )
                    .await;
                }
                Err(err) => {
                    tracing::error!(error = %err, "Database error validating category");
                    return render_500(&state, Some(current_user));
                }
            },
            Err(_) => {
                return render_create_error(
                    &state,
                    &session,
                    &current_user,
                    &form,
                    "Invalid category selected.",
                )
                .await;
            }
        },
        _ => None,
    };

    let status = match form.status.as_deref() {
        Some("published") => "published".to_string(),
        _ => "draft".to_string(),
    };

    let description = form
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let venue = form
        .venue
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let city = form
        .city
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    let params = CreateEventParams {
        category_id,
        title,
        description,
        venue,
        city,
        event_date,
        registration_deadline,
        total_seats,
        ticket_price,
        status,
        banner_image: None,
    };

    match Event::create(&state.db, current_user.id, &params).await {
        Ok(_) => Redirect::to("/dashboard").into_response(),
        Err(err) => {
            tracing::error!(error = %err, user_id = current_user.id, "Failed to create event");
            render_500(&state, Some(current_user))
        }
    }
}

/// GET /events/:id/edit - Render event edit form
pub async fn show_edit(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Response {
    let current_user = user.0;
    if current_user.role != "organizer" && current_user.role != "admin" {
        return forbidden_response(&state.jinja, None);
    }

    let event_id = match id_str.trim().parse::<i32>() {
        Ok(id) if id > 0 => id,
        _ => return render_404(&state, Some(current_user)),
    };

    let (event, categories) = match tokio::try_join!(
        Event::find_by_id(&state.db, event_id),
        Category::find_all(&state.db)
    ) {
        Ok((Some(e), c)) => (e, c),
        Ok((None, _)) => return render_404(&state, Some(current_user)),
        Err(err) => {
            tracing::error!(error = %err, event_id = event_id, "Failed to load event or categories");
            return render_500(&state, Some(current_user));
        }
    };

    if !can_manage(&event, &current_user) {
        return forbidden_response(
            &state.jinja,
            Some("You do not have permission to edit this event."),
        );
    }

    let csrf_token = match get_or_create_csrf_token(&session).await {
        Ok(token) => token,
        Err(err) => {
            tracing::error!(error = %err, "Failed to generate CSRF token for edit");
            return render_500(&state, Some(current_user));
        }
    };

    let context = minijinja::context! {
        title => "Edit Event",
        event => event,
        categories => categories,
        error => None::<String>,
        user => current_user,
        csrf_token => csrf_token,
    };

    render_html(&state, "dashboard/edit-event.html", context, StatusCode::OK)
}

/// POST /events/:id/edit - Update existing event
pub async fn update_event(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    Form(form): Form<EventFormData>,
) -> Response {
    let current_user = user.0;
    if current_user.role != "organizer" && current_user.role != "admin" {
        return forbidden_response(&state.jinja, None);
    }

    let token = form.csrf_token.as_deref().unwrap_or("");
    if !validate_csrf_token(&session, token).await {
        return csrf_error_response(&state.jinja);
    }

    let event_id = match id_str.trim().parse::<i32>() {
        Ok(id) if id > 0 => id,
        _ => return render_404(&state, Some(current_user)),
    };

    let event = match Event::find_by_id(&state.db, event_id).await {
        Ok(Some(e)) => e,
        Ok(None) => return render_404(&state, Some(current_user)),
        Err(err) => {
            tracing::error!(error = %err, event_id = event_id, "Failed to fetch event for update");
            return render_500(&state, Some(current_user));
        }
    };

    if !can_manage(&event, &current_user) {
        return forbidden_response(
            &state.jinja,
            Some("You do not have permission to edit this event."),
        );
    }

    let title = match form
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(t) if t.len() <= 255 => t.to_string(),
        Some(_) => {
            return render_edit_error(
                &state,
                &session,
                &current_user,
                &event,
                "Title must be 255 characters or fewer.",
            )
            .await;
        }
        None => {
            return render_edit_error(
                &state,
                &session,
                &current_user,
                &event,
                "Title, event date, and total seats are required.",
            )
            .await;
        }
    };

    let event_date = match form.event_date.as_deref().and_then(parse_datetime) {
        Some(dt) => dt,
        None => {
            return render_edit_error(
                &state,
                &session,
                &current_user,
                &event,
                "Title, event date, and total seats are required.",
            )
            .await;
        }
    };

    let total_seats = match form
        .total_seats
        .as_deref()
        .and_then(|s| s.trim().parse::<i32>().ok())
    {
        Some(seats) if seats > 0 => seats,
        _ => {
            return render_edit_error(
                &state,
                &session,
                &current_user,
                &event,
                "Total seats must be a positive number.",
            )
            .await;
        }
    };

    let registration_deadline = match form.registration_deadline.as_deref() {
        Some(s) if !s.trim().is_empty() => match parse_datetime(s) {
            Some(dt) => {
                if dt > event_date {
                    return render_edit_error(
                        &state,
                        &session,
                        &current_user,
                        &event,
                        "Registration deadline cannot be after the event date.",
                    )
                    .await;
                }
                Some(dt)
            }
            None => {
                return render_edit_error(
                    &state,
                    &session,
                    &current_user,
                    &event,
                    "Invalid registration deadline format.",
                )
                .await;
            }
        },
        _ => None,
    };

    let ticket_price = match form.ticket_price.as_deref() {
        Some(s) if !s.trim().is_empty() => match s.trim().parse::<rust_decimal::Decimal>() {
            Ok(price) if price >= rust_decimal::Decimal::ZERO => price,
            _ => {
                return render_edit_error(
                    &state,
                    &session,
                    &current_user,
                    &event,
                    "Ticket price must be a valid non-negative amount.",
                )
                .await;
            }
        },
        _ => rust_decimal::Decimal::ZERO,
    };

    let category_id = match form.category_id.as_deref() {
        Some(s) if !s.trim().is_empty() => match s.trim().parse::<i32>() {
            Ok(cat_id) => match Category::find_by_id(&state.db, cat_id).await {
                Ok(Some(_)) => Some(cat_id),
                Ok(None) => {
                    return render_edit_error(
                        &state,
                        &session,
                        &current_user,
                        &event,
                        "Selected category does not exist.",
                    )
                    .await;
                }
                Err(err) => {
                    tracing::error!(error = %err, "Database error validating category");
                    return render_500(&state, Some(current_user));
                }
            },
            Err(_) => {
                return render_edit_error(
                    &state,
                    &session,
                    &current_user,
                    &event,
                    "Invalid category selected.",
                )
                .await;
            }
        },
        _ => None,
    };

    let status = match form.status.as_deref() {
        Some(s @ ("draft" | "published" | "cancelled" | "completed")) => s.to_string(),
        _ => event.status.clone(),
    };

    let description = form
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let venue = form
        .venue
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let city = form
        .city
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    let params = UpdateEventParams {
        category_id,
        title,
        description,
        venue,
        city,
        event_date,
        registration_deadline,
        total_seats,
        ticket_price,
        status,
        banner_image: event.banner_image.clone(),
    };

    match Event::update(&state.db, event_id, &params).await {
        Ok(_) => Redirect::to("/dashboard").into_response(),
        Err(err) => {
            tracing::error!(error = %err, event_id = event_id, "Failed to update event in database");
            render_500(&state, Some(current_user))
        }
    }
}

/// POST /events/:id/toggle-status - Toggle event status between published and draft
pub async fn toggle_status(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    Form(form): Form<ActionFormData>,
) -> Response {
    let current_user = user.0;
    if current_user.role != "organizer" && current_user.role != "admin" {
        return forbidden_response(&state.jinja, None);
    }

    let token = form.csrf_token.as_deref().unwrap_or("");
    if !validate_csrf_token(&session, token).await {
        return csrf_error_response(&state.jinja);
    }

    let event_id = match id_str.trim().parse::<i32>() {
        Ok(id) if id > 0 => id,
        _ => return render_404(&state, Some(current_user)),
    };

    let event = match Event::find_by_id(&state.db, event_id).await {
        Ok(Some(e)) => e,
        Ok(None) => return render_404(&state, Some(current_user)),
        Err(err) => {
            tracing::error!(error = %err, event_id = event_id, "Database error retrieving event for toggle");
            return render_500(&state, Some(current_user));
        }
    };

    if !can_manage(&event, &current_user) {
        return forbidden_response(
            &state.jinja,
            Some("You do not have permission to update this event."),
        );
    }

    let new_status = if event.status == "published" {
        "draft"
    } else {
        "published"
    };

    match Event::update_status(&state.db, event_id, new_status).await {
        Ok(_) => Redirect::to("/dashboard").into_response(),
        Err(err) => {
            tracing::error!(error = %err, event_id = event_id, "Failed to update event status");
            render_500(&state, Some(current_user))
        }
    }
}

/// POST /events/:id/delete - Delete event by ID
pub async fn delete_event(
    user: AuthenticatedUser,
    session: Session,
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    Form(form): Form<ActionFormData>,
) -> Response {
    let current_user = user.0;
    if current_user.role != "organizer" && current_user.role != "admin" {
        return forbidden_response(&state.jinja, None);
    }

    let token = form.csrf_token.as_deref().unwrap_or("");
    if !validate_csrf_token(&session, token).await {
        return csrf_error_response(&state.jinja);
    }

    let event_id = match id_str.trim().parse::<i32>() {
        Ok(id) if id > 0 => id,
        _ => return render_404(&state, Some(current_user)),
    };

    let event = match Event::find_by_id(&state.db, event_id).await {
        Ok(Some(e)) => e,
        Ok(None) => return render_404(&state, Some(current_user)),
        Err(err) => {
            tracing::error!(error = %err, event_id = event_id, "Database error retrieving event for deletion");
            return render_500(&state, Some(current_user));
        }
    };

    if !can_manage(&event, &current_user) {
        return forbidden_response(
            &state.jinja,
            Some("You do not have permission to delete this event."),
        );
    }

    match Event::delete(&state.db, event_id).await {
        Ok(_) => Redirect::to("/dashboard").into_response(),
        Err(err) => {
            tracing::error!(error = %err, event_id = event_id, "Failed to delete event from database");
            render_500(&state, Some(current_user))
        }
    }
}

async fn render_create_error(
    state: &AppState,
    session: &Session,
    user: &User,
    form: &EventFormData,
    error_msg: &str,
) -> Response {
    let categories = match Category::find_all(&state.db).await {
        Ok(c) => c,
        Err(err) => {
            tracing::error!(error = %err, "Failed to fetch categories on create error");
            return render_500(state, Some(user.clone()));
        }
    };
    let csrf_token = get_or_create_csrf_token(session).await.unwrap_or_default();

    let context = minijinja::context! {
        title => "Create Event",
        categories => categories,
        error => error_msg,
        user => user,
        csrf_token => csrf_token,
        form => form,
    };

    render_html(
        state,
        "dashboard/create-event.html",
        context,
        StatusCode::BAD_REQUEST,
    )
}

async fn render_edit_error(
    state: &AppState,
    session: &Session,
    user: &User,
    event: &Event,
    error_msg: &str,
) -> Response {
    let categories = match Category::find_all(&state.db).await {
        Ok(c) => c,
        Err(err) => {
            tracing::error!(error = %err, "Failed to fetch categories on edit error");
            return render_500(state, Some(user.clone()));
        }
    };
    let csrf_token = get_or_create_csrf_token(session).await.unwrap_or_default();

    let context = minijinja::context! {
        title => "Edit Event",
        event => event,
        categories => categories,
        error => error_msg,
        user => user,
        csrf_token => csrf_token,
    };

    render_html(
        state,
        "dashboard/edit-event.html",
        context,
        StatusCode::BAD_REQUEST,
    )
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
