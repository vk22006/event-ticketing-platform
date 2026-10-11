use sqlx::MySqlPool;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i32,
    pub full_name: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub password: String,
    pub role: String,
    pub avatar: Option<String>,
    pub bio: Option<String>,
    pub created_at: Option<chrono::NaiveDateTime>,
    pub updated_at: Option<chrono::NaiveDateTime>,
}

impl User {
    /// Find a user by their unique email address
    pub async fn find_by_email(pool: &MySqlPool, email: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, full_name, email, password, role, avatar, bio, created_at, updated_at FROM users WHERE email = ?"
        )
        .bind(email)
        .fetch_optional(pool)
        .await
    }

    /// Find a user by their primary key id
    pub async fn find_by_id(pool: &MySqlPool, id: i32) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, full_name, email, password, role, avatar, bio, created_at, updated_at FROM users WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    /// Insert a new user into the database, returning their generated user ID
    pub async fn create(
        pool: &MySqlPool,
        full_name: &str,
        email: &str,
        hashed_password: &str,
        role: &str,
    ) -> Result<i32, sqlx::Error> {
        let result =
            sqlx::query("INSERT INTO users (full_name, email, password, role) VALUES (?, ?, ?, ?)")
                .bind(full_name)
                .bind(email)
                .bind(hashed_password)
                .bind(role)
                .execute(pool)
                .await?;

        Ok(result.last_insert_id() as i32)
    }

    /// Verify a plain-text password against a bcrypt hash (supports $2b$, $2a$, $2y$)
    pub fn verify_password(plain: &str, hashed: &str) -> bool {
        bcrypt::verify(plain, hashed).unwrap_or(false)
    }

    /// Hash a plain-text password using bcrypt with cost 10 (matching Express bcryptjs)
    pub fn hash_password(plain: &str) -> Result<String, bcrypt::BcryptError> {
        bcrypt::hash(plain, 10)
    }
}
