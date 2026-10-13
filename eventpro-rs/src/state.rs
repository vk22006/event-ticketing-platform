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
        jinja.add_filter("date_short", filter_date_short);
        jinja.add_filter("date_long", filter_date_long);
        jinja.add_filter("time_format", filter_time_format);
        jinja.add_filter("date_deadline", filter_date_deadline);
        jinja.add_filter("datetime_local", filter_datetime_local);
        jinja.add_filter("initial", filter_initial);
        jinja.add_filter("round", filter_round);

        // Register core templates
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
        jinja
            .add_template("index.html", include_str!("../templates/index.html"))
            .expect("Failed to register index.html");
        jinja
            .add_template(
                "event-detail.html",
                include_str!("../templates/event-detail.html"),
            )
            .expect("Failed to register event-detail.html");
        jinja
            .add_template(
                "error/404.html",
                include_str!("../templates/error/404.html"),
            )
            .expect("Failed to register error/404.html");
        jinja
            .add_template(
                "error/500.html",
                include_str!("../templates/error/500.html"),
            )
            .expect("Failed to register error/500.html");
        jinja
            .add_template(
                "dashboard/_sidebar.html",
                include_str!("../templates/dashboard/_sidebar.html"),
            )
            .expect("Failed to register dashboard/_sidebar.html");
        jinja
            .add_template(
                "dashboard/home.html",
                include_str!("../templates/dashboard/home.html"),
            )
            .expect("Failed to register dashboard/home.html");
        jinja
            .add_template(
                "dashboard/create-event.html",
                include_str!("../templates/dashboard/create-event.html"),
            )
            .expect("Failed to register dashboard/create-event.html");
        jinja
            .add_template(
                "dashboard/edit-event.html",
                include_str!("../templates/dashboard/edit-event.html"),
            )
            .expect("Failed to register dashboard/edit-event.html");
        jinja
            .add_template(
                "dashboard/admin.html",
                include_str!("../templates/dashboard/admin.html"),
            )
            .expect("Failed to register dashboard/admin.html");
        jinja
            .add_template(
                "booking/select.html",
                include_str!("../templates/booking/select.html"),
            )
            .expect("Failed to register booking/select.html");
        jinja
            .add_template(
                "booking/payment.html",
                include_str!("../templates/booking/payment.html"),
            )
            .expect("Failed to register booking/payment.html");
        jinja
            .add_template(
                "booking/confirmation.html",
                include_str!("../templates/booking/confirmation.html"),
            )
            .expect("Failed to register booking/confirmation.html");
        jinja
            .add_template(
                "dashboard/my-bookings.html",
                include_str!("../templates/dashboard/my-bookings.html"),
            )
            .expect("Failed to register dashboard/my-bookings.html");
        jinja
            .add_template(
                "dashboard/my-tickets.html",
                include_str!("../templates/dashboard/my-tickets.html"),
            )
            .expect("Failed to register dashboard/my-tickets.html");
        jinja
            .add_template(
                "ticket/view.html",
                include_str!("../templates/ticket/view.html"),
            )
            .expect("Failed to register ticket/view.html");
        jinja
            .add_template(
                "ticket/print.html",
                include_str!("../templates/ticket/print.html"),
            )
            .expect("Failed to register ticket/print.html");

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
        let s = value.to_string();
        if s == "0" || s == "0.00" {
            Ok("Free".to_string())
        } else {
            Ok(format!("₹{}", s))
        }
    }
}

// Date formatter filter
fn filter_date_in(value: Value) -> Result<String, minijinja::Error> {
    filter_date_short(value)
}

fn parse_datetime_val(value: &Value) -> Option<chrono::NaiveDateTime> {
    let s = value.as_str()?;
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

fn filter_date_short(value: Value) -> Result<String, minijinja::Error> {
    if let Some(dt) = parse_datetime_val(&value) {
        Ok(dt.format("%a, %d %b %Y").to_string())
    } else {
        Ok(value.to_string())
    }
}

fn filter_date_long(value: Value) -> Result<String, minijinja::Error> {
    if let Some(dt) = parse_datetime_val(&value) {
        Ok(dt.format("%A, %d %B %Y").to_string())
    } else {
        Ok(value.to_string())
    }
}

fn filter_time_format(value: Value) -> Result<String, minijinja::Error> {
    if let Some(dt) = parse_datetime_val(&value) {
        Ok(dt.format("%I:%M %p").to_string())
    } else {
        Ok(value.to_string())
    }
}

fn filter_date_deadline(value: Value) -> Result<String, minijinja::Error> {
    if let Some(dt) = parse_datetime_val(&value) {
        Ok(dt.format("%d/%m/%Y").to_string())
    } else {
        Ok(value.to_string())
    }
}

fn filter_round(value: Value) -> Result<i64, minijinja::Error> {
    if let Ok(num) = f64::try_from(value) {
        Ok(num.round() as i64)
    } else {
        Ok(0)
    }
}

fn filter_datetime_local(value: Value) -> Result<String, minijinja::Error> {
    if let Some(dt) = parse_datetime_val(&value) {
        Ok(dt.format("%Y-%m-%dT%H:%M").to_string())
    } else {
        Ok(value.to_string())
    }
}

fn filter_initial(value: Value) -> Result<String, minijinja::Error> {
    let s = value.as_str().unwrap_or("");
    Ok(s.chars().next().unwrap_or('?').to_uppercase().to_string())
}
