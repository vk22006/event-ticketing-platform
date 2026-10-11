use std::fmt;
use std::path::PathBuf;

#[derive(Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub session_secret: String,
    pub app_env: String,
    pub public_dir: PathBuf,
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database_url", &"[REDACTED]")
            .field("session_secret", &"[REDACTED]")
            .field("app_env", &self.app_env)
            .field("public_dir", &self.public_dir)
            .finish()
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ConfigError {
    #[error("Missing database configuration: set DATABASE_URL or DB_HOST, DB_USER, DB_NAME")]
    MissingDatabaseConfig,
    #[error("Invalid database URL: must start with 'mysql://'")]
    InvalidDatabaseUrl,
    #[error("Session secret is too short: must be at least 16 characters")]
    SessionSecretTooShort,
    #[error("Invalid port: {0}")]
    InvalidPort(String),
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

        // Default to port 3001 so Rust coexists with Express on 3000
        let port = match std::env::var("PORT") {
            Ok(p) => p.parse::<u16>().map_err(|_| ConfigError::InvalidPort(p))?,
            Err(_) => 3001,
        };

        let app_env = std::env::var("APP_ENV")
            .or_else(|_| std::env::var("NODE_ENV"))
            .unwrap_or_else(|_| "development".to_string());

        let database_url = Self::resolve_database_url()?;

        let session_secret = std::env::var("SESSION_SECRET")
            .unwrap_or_else(|_| "ev3nt_l1fecycle_s3cret_k3y_2024".to_string());

        let public_dir = std::env::var("PUBLIC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("../public"));

        let config = Self {
            host,
            port,
            database_url,
            session_secret,
            app_env,
            public_dir,
        };

        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.database_url.is_empty() || !self.database_url.starts_with("mysql://") {
            return Err(ConfigError::InvalidDatabaseUrl);
        }

        if self.session_secret.len() < 16 {
            return Err(ConfigError::SessionSecretTooShort);
        }

        Ok(())
    }

    fn resolve_database_url() -> Result<String, ConfigError> {
        if let Ok(url) = std::env::var("DATABASE_URL") {
            if !url.trim().is_empty() {
                return Ok(url);
            }
        }

        // Support fallback to Express .env format: DB_HOST, DB_PORT, DB_USER, DB_PASSWORD, DB_NAME
        let host = std::env::var("DB_HOST").unwrap_or_else(|_| "localhost".to_string());
        let port = std::env::var("DB_PORT").unwrap_or_else(|_| "3306".to_string());
        let user = std::env::var("DB_USER").unwrap_or_else(|_| "root".to_string());
        let password = std::env::var("DB_PASSWORD").unwrap_or_default();
        let db_name = std::env::var("DB_NAME").unwrap_or_else(|_| "event_lifecycle".to_string());

        if user.is_empty() || db_name.is_empty() {
            return Err(ConfigError::MissingDatabaseConfig);
        }

        if password.is_empty() {
            Ok(format!("mysql://{}@{}:{}/{}", user, host, port, db_name))
        } else {
            Ok(format!(
                "mysql://{}:{}@{}:{}/{}",
                user, password, host, port, db_name
            ))
        }
    }

    pub fn is_production(&self) -> bool {
        self.app_env.eq_ignore_ascii_case("production")
    }
}
