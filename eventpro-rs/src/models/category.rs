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
        let mut conn = crate::metrics::acquire_conn(pool).await?;
        let q_start = std::time::Instant::now();
        let res = sqlx::query_as::<_, Category>(
            "SELECT id, name, icon FROM categories ORDER BY name ASC",
        )
        .fetch_all(&mut *conn)
        .await;
        crate::metrics::record_query_duration(q_start.elapsed());
        res
    }

    /// Retrieve a single category by primary key ID
    pub async fn find_by_id(pool: &MySqlPool, id: i32) -> Result<Option<Category>, sqlx::Error> {
        let mut conn = crate::metrics::acquire_conn(pool).await?;
        let q_start = std::time::Instant::now();
        let res =
            sqlx::query_as::<_, Category>("SELECT id, name, icon FROM categories WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await;
        crate::metrics::record_query_duration(q_start.elapsed());
        res
    }
}
