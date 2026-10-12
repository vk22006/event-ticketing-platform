use crate::{error::AppError, models::User, state::AppState};
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

/// Wrapper around an authenticated User loaded from the database.
/// If unauthenticated, the extractor terminates with a redirect to `/auth/login`.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser(pub User);

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = parts
            .extensions
            .get::<Session>()
            .ok_or_else(|| Redirect::to("/auth/login").into_response())?;

        let user_id = get_session_user_id(session).await;

        match user_id {
            Some(id) => match User::find_by_id(&state.db, id).await {
                Ok(Some(user)) => Ok(AuthenticatedUser(user)),
                Ok(None) => {
                    // Stale session pointing to deleted user
                    let _ = session.flush().await;
                    Err(Redirect::to("/auth/login").into_response())
                }
                Err(err) => {
                    tracing::error!(error = %err, "Database lookup failed in auth extractor");
                    Err(AppError::Database(err).into_response())
                }
            },
            None => Err(Redirect::to("/auth/login").into_response()),
        }
    }
}

/// Extractor for optional user identification (e.g. for dynamic navbar rendering).
#[derive(Debug, Clone)]
pub struct OptionalUser(pub Option<User>);

#[async_trait]
impl FromRequestParts<AppState> for OptionalUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = match parts.extensions.get::<Session>() {
            Some(s) => s,
            None => return Ok(OptionalUser(None)),
        };

        let user_id = get_session_user_id(session).await;

        if let Some(id) = user_id {
            match User::find_by_id(&state.db, id).await {
                Ok(Some(user)) => Ok(OptionalUser(Some(user))),
                Ok(None) => {
                    // Stale session pointing to deleted user
                    let _ = session.flush().await;
                    Ok(OptionalUser(None))
                }
                Err(err) => {
                    tracing::error!(
                        error = %err,
                        user_id = id,
                        "Database lookup failed in OptionalUser extractor"
                    );
                    Err(AppError::Database(err).into_response())
                }
            }
        } else {
            Ok(OptionalUser(None))
        }
    }
}

/// Extractor that rejects logged-in users and redirects them to `/dashboard`.
/// Equivalent to Express `requireGuest` middleware.
#[derive(Debug, Clone)]
pub struct RequireGuest;

#[async_trait]
impl FromRequestParts<AppState> for RequireGuest {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = parts.extensions.get::<Session>();

        if let Some(session) = session {
            if get_session_user_id(session).await.is_some() {
                return Err(Redirect::to("/dashboard").into_response());
            }
        }

        Ok(RequireGuest)
    }
}

/// Helper to extract user ID from session supporting both "user_id" and "userId".
pub async fn get_session_user_id(session: &Session) -> Option<i32> {
    if let Ok(Some(id)) = session.get::<i32>("user_id").await {
        return Some(id);
    }
    if let Ok(Some(id)) = session.get::<i32>("userId").await {
        return Some(id);
    }
    None
}

/// Verify that an authenticated user has one of the allowed roles.
pub fn verify_role(user: &User, allowed_roles: &[&str]) -> Result<(), AppError> {
    if allowed_roles.contains(&user.role.as_str()) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "You are not authorized to access this page.".into(),
        ))
    }
}

/// Render a 403 Forbidden response using the error/403.html template.
pub fn forbidden_response(jinja: &minijinja::Environment, message: Option<&str>) -> Response {
    let context = minijinja::context! {
        title => "Access Denied – EventPro",
        message => message.unwrap_or("You are not authorized to access this page."),
    };

    match jinja.get_template("error/403.html") {
        Ok(tmpl) => match tmpl.render(context) {
            Ok(rendered) => (StatusCode::FORBIDDEN, Html(rendered)).into_response(),
            Err(_) => (
                StatusCode::FORBIDDEN,
                Html("<h1>403 Forbidden</h1><p>Access denied.</p>"),
            )
                .into_response(),
        },
        Err(_) => (
            StatusCode::FORBIDDEN,
            Html("<h1>403 Forbidden</h1><p>Access denied.</p>"),
        )
            .into_response(),
    }
}
