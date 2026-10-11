use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Template rendering error: {0}")]
    Template(#[from] minijinja::Error),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Database(_) | Self::Template(_) | Self::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    pub fn client_message(&self) -> String {
        match self {
            Self::NotFound(msg) => msg.clone(),
            Self::Unauthorized(msg) => msg.clone(),
            Self::Forbidden(msg) => msg.clone(),
            Self::BadRequest(msg) => msg.clone(),
            Self::Database(_) => "A database error occurred. Please try again later.".to_string(),
            Self::Template(_) => "A template rendering error occurred.".to_string(),
            Self::Internal(msg) => msg.clone(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let message = self.client_message();

        // Log internal errors with tracing
        if status.is_server_error() {
            tracing::error!(status = %status, error = %self, "Application error occurred");
        } else {
            tracing::warn!(status = %status, message = %message, "Client request rejected");
        }

        // Return a clean fallback HTML page suitable for browser users
        let html_body = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{code} – EventPro</title>
    <link rel="stylesheet" href="/css/main.css">
    <style>
        body {{
            background: #0b0f19;
            color: #f1f5f9;
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            display: flex;
            align-items: center;
            justify-content: center;
            min-height: 100vh;
            margin: 0;
            padding: 1.5rem;
            box-sizing: border-box;
        }}
        .error-card {{
            background: #111827;
            border: 1px solid #1f2937;
            border-radius: 12px;
            padding: 2.5rem;
            max-width: 480px;
            width: 100%;
            text-align: center;
            box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.5);
        }}
        .error-code {{
            font-size: 3.5rem;
            font-weight: 800;
            color: #6366f1;
            margin: 0 0 0.5rem 0;
            line-height: 1;
        }}
        .error-title {{
            font-size: 1.25rem;
            font-weight: 600;
            margin: 0 0 1rem 0;
            color: #f8fafc;
        }}
        .error-msg {{
            color: #94a3b8;
            font-size: 0.95rem;
            margin-bottom: 2rem;
            line-height: 1.5;
        }}
        .btn-home {{
            display: inline-block;
            background: #6366f1;
            color: #ffffff;
            text-decoration: none;
            padding: 0.65rem 1.5rem;
            border-radius: 8px;
            font-weight: 600;
            transition: background 0.2s;
        }}
        .btn-home:hover {{
            background: #4f46e5;
        }}
    </style>
</head>
<body>
    <div class="error-card">
        <div class="error-code">{code}</div>
        <div class="error-title">{title}</div>
        <div class="error-msg">{msg}</div>
        <a href="/events" class="btn-home">Browse Events</a>
    </div>
</body>
</html>"#,
            code = status.as_u16(),
            title = status.canonical_reason().unwrap_or("Error"),
            msg = html_escape(&message)
        );

        (status, Html(html_body)).into_response()
    }
}

// Helper to escape basic HTML characters
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
