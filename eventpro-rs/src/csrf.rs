use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use subtle::ConstantTimeEq;
use tower_sessions::Session;

pub const CSRF_SESSION_KEY: &str = "csrf_token";

/// Generate a cryptographically secure random token (32 random bytes, hex-encoded to 64 chars).
pub fn generate_csrf_token() -> String {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Retrieve the existing CSRF token from the session or generate and store a new one.
pub async fn get_or_create_csrf_token(
    session: &Session,
) -> Result<String, tower_sessions::session::Error> {
    if let Some(token) = session.get::<String>(CSRF_SESSION_KEY).await? {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    let new_token = generate_csrf_token();
    session.insert(CSRF_SESSION_KEY, &new_token).await?;
    Ok(new_token)
}

/// Force generation of a fresh CSRF token (e.g. after login/register or session rotation).
pub async fn rotate_csrf_token(
    session: &Session,
) -> Result<String, tower_sessions::session::Error> {
    let new_token = generate_csrf_token();
    session.insert(CSRF_SESSION_KEY, &new_token).await?;
    Ok(new_token)
}

/// Perform constant-time verification between expected and submitted CSRF tokens.
pub fn verify_token_constant_time(expected: &str, submitted: &str) -> bool {
    if expected.is_empty() || submitted.is_empty() {
        return false;
    }

    let expected_bytes = expected.as_bytes();
    let submitted_bytes = submitted.as_bytes();

    if expected_bytes.len() != submitted_bytes.len() {
        return false;
    }

    expected_bytes.ct_eq(submitted_bytes).into()
}

/// Validate the submitted token against the current session's stored CSRF token.
pub async fn validate_csrf_token(session: &Session, submitted_token: &str) -> bool {
    let session_token = match session.get::<String>(CSRF_SESSION_KEY).await {
        Ok(Some(token)) => token,
        _ => return false,
    };

    verify_token_constant_time(&session_token, submitted_token)
}

/// Construct a safe 403 Forbidden HTML response for failed CSRF checks.
pub fn csrf_error_response(jinja: &minijinja::Environment) -> Response {
    let context = minijinja::context! {
        title => "Security Validation Failed – EventPro",
    };

    match jinja.get_template("error/csrf.html") {
        Ok(tmpl) => match tmpl.render(context) {
            Ok(rendered) => (StatusCode::FORBIDDEN, Html(rendered)).into_response(),
            Err(e) => {
                tracing::error!(error = %e, "Failed to render error/csrf.html");
                (
                    StatusCode::FORBIDDEN,
                    Html("<h1>403 Forbidden</h1><p>CSRF verification failed.</p>"),
                )
                    .into_response()
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "error/csrf.html template not found");
            (
                StatusCode::FORBIDDEN,
                Html("<h1>403 Forbidden</h1><p>CSRF verification failed.</p>"),
            )
                .into_response()
        }
    }
}
