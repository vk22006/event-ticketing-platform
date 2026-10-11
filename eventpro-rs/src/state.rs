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
