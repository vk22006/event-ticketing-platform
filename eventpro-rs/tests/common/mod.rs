use eventpro::config::build_database_url;
use sqlx::MySqlPool;
use std::time::Duration;

/// Returns true if strict MySQL integration test mode is enabled.
/// Supported environment flags: `STRICT_DB_TESTS`, `STRICT_TEST`, `STRICT_MYSQL`.
pub fn is_strict_mode() -> bool {
    std::env::var("STRICT_DB_TESTS")
        .or_else(|_| std::env::var("STRICT_TEST"))
        .or_else(|_| std::env::var("STRICT_MYSQL"))
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Resolves the test database URL and connects to MySQL with strict-mode enforcement.
///
/// Strict mode behavior:
/// - Panics if `DATABASE_URL` is missing, empty, or does not begin with `mysql://`.
/// - Panics if connection to MySQL cannot be established or times out.
/// - Never exposes database credentials (passwords, complete connection strings) in logs or panics.
///
/// Non-strict mode behavior:
/// - Prints an informative `SKIPPED:` message and returns `None` if `DATABASE_URL` is absent
///   or MySQL is unreachable, allowing fast and non-disruptive local development.
pub async fn get_test_pool() -> Option<MySqlPool> {
    let _ = dotenvy::dotenv();
    let _ = dotenvy::from_path("../.env");

    let strict = is_strict_mode();

    if let Ok(raw) = std::env::var("DATABASE_URL") {
        if raw.trim().is_empty() {
            if strict {
                panic!("STRICT MODE ERROR: DATABASE_URL is set but empty.");
            } else {
                println!(
                    "SKIPPED: Real MySQL integration tests skipped because DATABASE_URL is empty."
                );
                return None;
            }
        }
    }

    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => match (
            std::env::var("DB_USER"),
            std::env::var("DB_PASSWORD"),
            std::env::var("DB_HOST"),
            std::env::var("DB_PORT"),
            std::env::var("DB_NAME"),
        ) {
            (Ok(u), Ok(p), Ok(h), Ok(port), Ok(d)) => build_database_url(&u, &p, &h, &port, &d),
            _ => {
                if strict {
                    panic!(
                        "STRICT MODE ERROR: Required MySQL integration tests cannot execute: DATABASE_URL is missing, and fallback DB_* variables are not set."
                    );
                } else {
                    println!(
                        "SKIPPED: Real MySQL integration tests skipped because DATABASE_URL / DB_* environment variables are not set."
                    );
                    return None;
                }
            }
        },
    };

    if !db_url.starts_with("mysql://") {
        if strict {
            panic!("STRICT MODE ERROR: DATABASE_URL is invalid (must start with 'mysql://').");
        } else {
            println!(
                "SKIPPED: Real MySQL integration tests skipped because DATABASE_URL is invalid."
            );
            return None;
        }
    }

    match sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&db_url)
        .await
    {
        Ok(pool) => {
            println!("REAL MYSQL DETECTED: Connected to MySQL test database successfully.");
            Some(pool)
        }
        Err(err) => {
            if strict {
                panic!(
                    "STRICT MODE ERROR: Failed to connect to MySQL database: {}",
                    err
                );
            } else {
                println!(
                    "SKIPPED: Real MySQL integration tests skipped because database is unreachable: {}",
                    err
                );
                None
            }
        }
    }
}
