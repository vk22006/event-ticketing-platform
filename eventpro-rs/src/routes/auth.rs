use crate::{
    csrf::{csrf_error_response, get_or_create_csrf_token, rotate_csrf_token, validate_csrf_token},
    middleware::RequireGuest,
    models::User,
    state::AppState,
};
use axum::{
    extract::State,
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use tower_sessions::Session;

#[derive(serde::Deserialize)]
pub struct LoginForm {
    pub email: Option<String>,
    pub password: Option<String>,
    pub csrf_token: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct RegisterForm {
    pub full_name: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub password: Option<String>,
    pub confirm_password: Option<String>,
    pub csrf_token: Option<String>,
}

/// GET /auth/login - Render login form for guests
pub async fn show_login(
    _guest: RequireGuest,
    session: Session,
    State(state): State<AppState>,
) -> Response {
    let csrf_token = match get_or_create_csrf_token(&session).await {
        Ok(token) => token,
        Err(err) => {
            tracing::error!(error = %err, "Failed to initialize CSRF token for login page");
            return Redirect::to("/health").into_response();
        }
    };

    let context = minijinja::context! {
        title => "Sign In – EventPro",
        error => None::<String>,
        email => "",
        csrf_token => csrf_token,
        user => None::<User>,
    };

    render_template(&state, "auth/login.html", context)
}

/// POST /auth/login - Authenticate user credentials and establish session
pub async fn login(
    _guest: RequireGuest,
    session: Session,
    State(state): State<AppState>,
    Form(payload): Form<LoginForm>,
) -> Response {
    let submitted_csrf = payload.csrf_token.as_deref().unwrap_or_default();
    if !validate_csrf_token(&session, submitted_csrf).await {
        tracing::warn!("Login attempt rejected due to invalid or missing CSRF token");
        return csrf_error_response(&state.jinja);
    }

    let email = payload.email.as_deref().unwrap_or("").trim();
    let password = payload.password.as_deref().unwrap_or("");

    if email.is_empty() || password.is_empty() {
        let csrf_token = get_or_create_csrf_token(&session).await.unwrap_or_default();
        let context = minijinja::context! {
            title => "Sign In – EventPro",
            error => "Email and password are required.",
            email => email,
            csrf_token => csrf_token,
            user => None::<User>,
        };
        return render_template(&state, "auth/login.html", context);
    }

    let user_opt = match User::find_by_email(&state.db, email).await {
        Ok(u) => u,
        Err(err) => {
            tracing::error!(error = %err, "Database lookup failed during login");
            let csrf_token = get_or_create_csrf_token(&session).await.unwrap_or_default();
            let context = minijinja::context! {
                title => "Sign In – EventPro",
                error => "Server error. Please try again.",
                email => email,
                csrf_token => csrf_token,
                user => None::<User>,
            };
            return render_template(&state, "auth/login.html", context);
        }
    };

    let user = match user_opt {
        Some(u) => u,
        None => {
            let csrf_token = get_or_create_csrf_token(&session).await.unwrap_or_default();
            let context = minijinja::context! {
                title => "Sign In – EventPro",
                error => "Invalid email or password.",
                email => email,
                csrf_token => csrf_token,
                user => None::<User>,
            };
            return render_template(&state, "auth/login.html", context);
        }
    };

    if !User::verify_password(password, &user.password) {
        let csrf_token = get_or_create_csrf_token(&session).await.unwrap_or_default();
        let context = minijinja::context! {
            title => "Sign In – EventPro",
            error => "Invalid email or password.",
            email => email,
            csrf_token => csrf_token,
            user => None::<User>,
        };
        return render_template(&state, "auth/login.html", context);
    }

    // Cycle session identifier to protect against session fixation attacks
    if let Err(err) = session.cycle_id().await {
        tracing::error!(error = %err, "Failed to cycle session after login");
        return Redirect::to("/auth/login").into_response();
    }

    // Refresh CSRF token for subsequent requests
    let _ = rotate_csrf_token(&session).await;

    // Populate session with authenticated identity
    let _ = session.insert("user_id", user.id).await;
    let _ = session.insert("userId", user.id).await;
    let _ = session.insert("role", &user.role).await;
    let _ = session.insert("name", &user.full_name).await;

    tracing::info!(user_id = user.id, role = %user.role, "User successfully logged in");

    let redirect_url = if user.role == "admin" {
        "/admin/dashboard"
    } else {
        "/dashboard"
    };

    Redirect::to(redirect_url).into_response()
}

/// GET /auth/register - Render registration form for guests
pub async fn show_register(
    _guest: RequireGuest,
    session: Session,
    State(state): State<AppState>,
) -> Response {
    let csrf_token = match get_or_create_csrf_token(&session).await {
        Ok(token) => token,
        Err(err) => {
            tracing::error!(error = %err, "Failed to initialize CSRF token for register page");
            return Redirect::to("/health").into_response();
        }
    };

    let context = minijinja::context! {
        title => "Create Account – EventPro",
        error => None::<String>,
        full_name => "",
        email => "",
        role => "user",
        csrf_token => csrf_token,
        user => None::<User>,
    };

    render_template(&state, "auth/register.html", context)
}

/// POST /auth/register - Create new user account and establish session
pub async fn register(
    _guest: RequireGuest,
    session: Session,
    State(state): State<AppState>,
    Form(payload): Form<RegisterForm>,
) -> Response {
    let submitted_csrf = payload.csrf_token.as_deref().unwrap_or_default();
    if !validate_csrf_token(&session, submitted_csrf).await {
        tracing::warn!("Registration attempt rejected due to invalid or missing CSRF token");
        return csrf_error_response(&state.jinja);
    }

    let full_name = payload.full_name.as_deref().unwrap_or("").trim();
    let email = payload.email.as_deref().unwrap_or("").trim();
    let password = payload.password.as_deref().unwrap_or("");
    let confirm_password = payload.confirm_password.as_deref().unwrap_or("");
    let role_input = payload.role.as_deref().unwrap_or("user");

    let safe_role = if role_input == "organizer" {
        "organizer"
    } else {
        "user"
    };

    macro_rules! render_err {
        ($err_msg:expr) => {{
            let csrf_token = get_or_create_csrf_token(&session).await.unwrap_or_default();
            let context = minijinja::context! {
                title => "Create Account – EventPro",
                error => $err_msg,
                full_name => full_name,
                email => email,
                role => safe_role,
                csrf_token => csrf_token,
                user => None::<User>,
            };
            return render_template(&state, "auth/register.html", context);
        }};
    }

    if full_name.is_empty() || email.is_empty() || password.is_empty() {
        render_err!("All fields are required.");
    }

    if password != confirm_password {
        render_err!("Passwords do not match.");
    }

    if password.len() < 6 {
        render_err!("Password must be at least 6 characters.");
    }

    // Check duplicate account
    match User::find_by_email(&state.db, email).await {
        Ok(Some(_)) => render_err!("An account with this email already exists."),
        Ok(None) => (),
        Err(err) => {
            tracing::error!(error = %err, "Database lookup failed during registration");
            render_err!("Server error. Please try again.");
        }
    }

    let hashed_password = match User::hash_password(password) {
        Ok(h) => h,
        Err(err) => {
            tracing::error!(error = %err, "Password hashing failed");
            render_err!("Server error. Please try again.");
        }
    };

    let user_id = match User::create(&state.db, full_name, email, &hashed_password, safe_role).await
    {
        Ok(id) => id,
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            render_err!("An account with this email already exists.");
        }
        Err(err) => {
            tracing::error!(error = %err, "Database user creation failed");
            render_err!("Server error. Please try again.");
        }
    };

    // Cycle session identifier to protect against session fixation attacks
    if let Err(err) = session.cycle_id().await {
        tracing::error!(error = %err, "Failed to cycle session after registration");
        return Redirect::to("/auth/login").into_response();
    }

    let _ = rotate_csrf_token(&session).await;

    // Populate session with authenticated identity
    let _ = session.insert("user_id", user_id).await;
    let _ = session.insert("userId", user_id).await;
    let _ = session.insert("role", safe_role).await;
    let _ = session.insert("name", full_name).await;

    tracing::info!(
        user_id = user_id,
        role = safe_role,
        "User registered successfully"
    );

    Redirect::to("/dashboard").into_response()
}

/// GET /auth/logout - Invalidate session and redirect to homepage
pub async fn logout(session: Session) -> Response {
    if let Err(err) = session.flush().await {
        tracing::error!(error = %err, "Error flushing session on logout");
    }
    Redirect::to("/").into_response()
}

fn render_template(state: &AppState, template_name: &str, context: minijinja::Value) -> Response {
    match state.jinja.get_template(template_name) {
        Ok(tmpl) => match tmpl.render(context) {
            Ok(body) => Html(body).into_response(),
            Err(err) => {
                tracing::error!(error = %err, template = %template_name, "Template rendering failed");
                Html("<h1>Template Error</h1><p>Failed to render page.</p>").into_response()
            }
        },
        Err(err) => {
            tracing::error!(error = %err, template = %template_name, "Template not found");
            Html("<h1>Not Found</h1><p>Page template not found.</p>").into_response()
        }
    }
}
