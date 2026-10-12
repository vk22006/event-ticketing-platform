use sqlx::MySqlPool;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct Category {
    pub id: i32,
    pub name: String,
    pub icon: Option<String>,
}

impl Category {
    /// Retrieve all categories ordered alphabetically by name
    pub async fn find_all(pool: &MySqlPool) -> Result<Vec<Category>, sqlx::Error> {
        sqlx::query_as::<_, Category>("SELECT id, name, icon FROM categories ORDER BY name ASC")
            .fetch_all(pool)
            .await
    }

    /// Retrieve a single category by primary key ID
    pub async fn find_by_id(pool: &MySqlPool, id: i32) -> Result<Option<Category>, sqlx::Error> {
        sqlx::query_as::<_, Category>("SELECT id, name, icon FROM categories WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
    }
}
