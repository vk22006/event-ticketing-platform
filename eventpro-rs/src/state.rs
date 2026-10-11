use crate::config::AppConfig;
use minijinja::{Environment, Value};
use sqlx::MySqlPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: MySqlPool,
    pub jinja: Arc<Environment<'static>>,
    pub config: Arc<AppConfig>,
}

impl AppState {
    pub fn new(db: MySqlPool, config: AppConfig) -> Self {
        let mut jinja = Environment::new();

        // Register formatting filters needed for EventPro templates
        jinja.add_filter("inr", filter_inr);
        jinja.add_filter("date_in", filter_date_in);

        // Register core Phase 2 templates
        jinja
            .add_template(
                "partials/navbar.html",
                include_str!("../templates/partials/navbar.html"),
            )
            .expect("Failed to register partials/navbar.html");
        jinja
            .add_template(
                "partials/footer.html",
                include_str!("../templates/partials/footer.html"),
            )
            .expect("Failed to register partials/footer.html");
        jinja
            .add_template(
                "auth/login.html",
                include_str!("../templates/auth/login.html"),
            )
            .expect("Failed to register auth/login.html");
        jinja
            .add_template(
                "auth/register.html",
                include_str!("../templates/auth/register.html"),
            )
            .expect("Failed to register auth/register.html");
        jinja
            .add_template(
                "error/csrf.html",
                include_str!("../templates/error/csrf.html"),
            )
            .expect("Failed to register error/csrf.html");
        jinja
            .add_template(
                "error/403.html",
                include_str!("../templates/error/403.html"),
            )
            .expect("Failed to register error/403.html");

        Self {
            db,
            jinja: Arc::new(jinja),
            config: Arc::new(config),
        }
    }
}

// Indian Rupee currency formatter filter for templates
fn filter_inr(value: Value) -> Result<String, minijinja::Error> {
    if let Ok(num) = f64::try_from(value.clone()) {
        if num == 0.0 {
            return Ok("Free".to_string());
        }
        Ok(format!("₹{:.2}", num))
    } else {
        Ok(format!("₹{}", value))
    }
}

// Date formatter filter
fn filter_date_in(value: Value) -> Result<String, minijinja::Error> {
    Ok(value.to_string())
}
