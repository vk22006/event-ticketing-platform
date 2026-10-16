use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::fmt;
use std::path::{Path, PathBuf};

/// RFC 3986 userinfo percent-encode set.
/// Encodes control characters plus URI-reserved delimiters:
/// ':', '@', '/', '?', '#', '%', '[', ']', '\\', ' ', '"'
const USERINFO_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b':')
    .add(b'@')
    .add(b'/')
    .add(b'?')
    .add(b'#')
    .add(b'%')
    .add(b'[')
    .add(b']')
    .add(b'\\')
    .add(b' ')
    .add(b'"');

/// Percent-encode user credentials to safely construct RFC 3986 connection URIs.
pub fn encode_credential(s: &str) -> String {
    utf8_percent_encode(s, USERINFO_ENCODE_SET).to_string()
}

/// Construct a standardized MySQL URL with percent-encoded credentials.
pub fn build_database_url(
    user: &str,
    password: &str,
    host: &str,
    port: &str,
    db_name: &str,
) -> String {
    let encoded_user = encode_credential(user);
    if password.is_empty() {
        format!("mysql://{}@{}:{}/{}", encoded_user, host, port, db_name)
    } else {
        let encoded_password = encode_credential(password);
        format!(
            "mysql://{}:{}@{}:{}/{}",
            encoded_user, encoded_password, host, port, db_name
        )
    }
}

/// Verify that a candidate path exists and contains the required subdirectories.
pub fn validate_public_dir(path: &Path) -> bool {
    path.is_dir()
        && path.join("css").is_dir()
        && path.join("js").is_dir()
        && path.join("uploads").is_dir()
}

/// Resolve the public asset directory reliably across execution working directories.
pub fn resolve_public_dir() -> Result<PathBuf, ConfigError> {
    // 1. Explicit override via environment variable
    if let Ok(dir_str) = std::env::var("PUBLIC_DIR") {
        let candidate = PathBuf::from(dir_str);
        if validate_public_dir(&candidate) {
            return std::fs::canonicalize(&candidate).map_err(|e| {
                ConfigError::InvalidPublicDir(format!(
                    "Failed to canonicalize PUBLIC_DIR {:?}: {}",
                    candidate, e
                ))
            });
        }
        return Err(ConfigError::InvalidPublicDir(format!(
            "Specified PUBLIC_DIR {:?} does not exist or lacks required subdirectories (css, js, uploads)",
            candidate
        )));
    }

    // 2. Candidate paths based on standard project layout
    let mut candidates = vec![
        PathBuf::from("../public"), // When launched from eventpro-rs/
        PathBuf::from("public"),    // When launched from repository root
    ];

    // Fallback: check relative to CARGO_MANIFEST_DIR if set
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        if let Some(parent) = Path::new(&manifest_dir).parent() {
            candidates.push(parent.join("public"));
        }
    }

    for candidate in &candidates {
        if validate_public_dir(candidate) {
            if let Ok(canonical) = std::fs::canonicalize(candidate) {
                return Ok(canonical);
            }
        }
    }

    Err(ConfigError::InvalidPublicDir(format!(
        "Public asset directory could not be resolved from candidates: {:?}",
        candidates
    )))
}

/// Resolve the upload directory, defaulting to public_dir/uploads if not explicitly configured.
pub fn resolve_upload_dir(public_dir: &Path) -> Result<PathBuf, ConfigError> {
    if let Ok(dir_str) = std::env::var("UPLOAD_DIR") {
        let candidate = PathBuf::from(dir_str);
        if candidate.is_dir() {
            return std::fs::canonicalize(&candidate).map_err(|e| {
                ConfigError::InvalidUploadDir(format!(
                    "Failed to canonicalize UPLOAD_DIR {:?}: {}",
                    candidate, e
                ))
            });
        }
        if std::fs::create_dir_all(&candidate).is_ok() {
            if let Ok(canonical) = std::fs::canonicalize(&candidate) {
                return Ok(canonical);
            }
        }
        return Err(ConfigError::InvalidUploadDir(format!(
            "Specified UPLOAD_DIR {:?} does not exist and could not be created",
            candidate
        )));
    }

    let default_uploads = public_dir.join("uploads");
    if default_uploads.is_dir() {
        if let Ok(canonical) = std::fs::canonicalize(&default_uploads) {
            return Ok(canonical);
        }
        return Ok(default_uploads);
    }

    if std::fs::create_dir_all(&default_uploads).is_ok() {
        if let Ok(canonical) = std::fs::canonicalize(&default_uploads) {
            return Ok(canonical);
        }
        return Ok(default_uploads);
    }

    Err(ConfigError::InvalidUploadDir(format!(
        "Upload directory could not be resolved or created at {:?}",
        default_uploads
    )))
}

#[derive(Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub session_secret: String,
    pub app_env: String,
    pub public_dir: PathBuf,
    pub upload_dir: PathBuf,
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
            .field("upload_dir", &self.upload_dir)
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
    #[error(
        "Public asset directory not found or missing required structure (css, js, uploads): {0}"
    )]
    InvalidPublicDir(String),
    #[error("Upload directory not found or cannot be created: {0}")]
    InvalidUploadDir(String),
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

        // Default to port 3001
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

        let public_dir = resolve_public_dir()?;
        let upload_dir = resolve_upload_dir(&public_dir)?;

        let config = Self {
            host,
            port,
            database_url,
            session_secret,
            app_env,
            public_dir,
            upload_dir,
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

        if !validate_public_dir(&self.public_dir) {
            return Err(ConfigError::InvalidPublicDir(format!(
                "Public directory {:?} is invalid or missing required structure (css, js, uploads)",
                self.public_dir
            )));
        }

        if !self.upload_dir.is_dir() {
            return Err(ConfigError::InvalidUploadDir(format!(
                "Upload directory {:?} is not a valid directory",
                self.upload_dir
            )));
        }

        Ok(())
    }

    fn resolve_database_url() -> Result<String, ConfigError> {
        if let Ok(url) = std::env::var("DATABASE_URL") {
            if !url.trim().is_empty() {
                return Ok(url);
            }
        }

        // Support fallback to discrete .env format: DB_HOST, DB_PORT, DB_USER, DB_PASSWORD, DB_NAME
        let host = std::env::var("DB_HOST").unwrap_or_else(|_| "localhost".to_string());
        let port = std::env::var("DB_PORT").unwrap_or_else(|_| "3306".to_string());
        let user = std::env::var("DB_USER").unwrap_or_else(|_| "root".to_string());
        let password = std::env::var("DB_PASSWORD").unwrap_or_default();
        let db_name = std::env::var("DB_NAME").unwrap_or_else(|_| "event_lifecycle".to_string());

        if user.is_empty() || db_name.is_empty() {
            return Err(ConfigError::MissingDatabaseConfig);
        }

        Ok(build_database_url(&user, &password, &host, &port, &db_name))
    }

    pub fn is_production(&self) -> bool {
        self.app_env.eq_ignore_ascii_case("production")
    }
}
