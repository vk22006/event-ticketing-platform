use rust_decimal::Decimal;
use sqlx::MySqlPool;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, PartialEq)]
pub struct Event {
    pub id: i32,
    pub organizer_id: i32,
    pub category_id: Option<i32>,
    pub title: String,
    pub description: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub event_date: chrono::NaiveDateTime,
    pub registration_deadline: Option<chrono::NaiveDateTime>,
    pub total_seats: i32,
    pub available_seats: i32,
    pub ticket_price: Decimal,
    pub banner_image: Option<String>,
    pub status: String,
    pub created_at: Option<chrono::NaiveDateTime>,
    pub updated_at: Option<chrono::NaiveDateTime>,
    // Joined fields from users and categories
    pub organizer_name: String,
    pub organizer_email: Option<String>,
    pub category_name: Option<String>,
    pub category_icon: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EventFilterParams {
    pub status: Option<String>,
    pub category_id: Option<i32>,
    pub city: Option<String>,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl Event {
    /// Retrieve events matching filter criteria with parameterized SQL bindings
    pub async fn find_all(
        pool: &MySqlPool,
        filters: &EventFilterParams,
    ) -> Result<Vec<Event>, sqlx::Error> {
        let mut sql = String::from(
            "SELECT e.id, e.organizer_id, e.category_id, e.title, e.description, \
                    e.venue, e.city, e.event_date, e.registration_deadline, \
                    e.total_seats, e.available_seats, e.ticket_price, e.banner_image, \
                    e.status, e.created_at, e.updated_at, \
                    u.full_name AS organizer_name, NULL AS organizer_email, \
                    c.name AS category_name, c.icon AS category_icon \
             FROM events e \
             JOIN users u ON e.organizer_id = u.id \
             LEFT JOIN categories c ON e.category_id = c.id \
             WHERE 1=1",
        );

        let status = filters.status.as_deref().unwrap_or("published");
        sql.push_str(" AND e.status = ?");

        if filters.category_id.is_some() {
            sql.push_str(" AND e.category_id = ?");
        }
        if let Some(ref city) = filters.city {
            if !city.trim().is_empty() {
                sql.push_str(" AND e.city LIKE ?");
            }
        }
        if let Some(ref search) = filters.search {
            if !search.trim().is_empty() {
                sql.push_str(" AND (e.title LIKE ? OR e.description LIKE ?)");
            }
        }

        sql.push_str(" ORDER BY e.event_date ASC LIMIT ? OFFSET ?");

        let limit = filters.limit.unwrap_or(9);
        let offset = filters.offset.unwrap_or(0);

        let mut query = sqlx::query_as::<_, Event>(&sql).bind(status);

        if let Some(cat_id) = filters.category_id {
            query = query.bind(cat_id);
        }
        if let Some(ref city) = filters.city {
            if !city.trim().is_empty() {
                query = query.bind(format!("%{}%", city.trim()));
            }
        }
        if let Some(ref search) = filters.search {
            if !search.trim().is_empty() {
                let pattern = format!("%{}%", search.trim());
                query = query.bind(pattern.clone()).bind(pattern);
            }
        }

        query = query.bind(limit).bind(offset);

        query.fetch_all(pool).await
    }

    /// Retrieve full event details including organizer contact information
    pub async fn find_by_id(pool: &MySqlPool, id: i32) -> Result<Option<Event>, sqlx::Error> {
        let sql = "SELECT e.id, e.organizer_id, e.category_id, e.title, e.description, \
                          e.venue, e.city, e.event_date, e.registration_deadline, \
                          e.total_seats, e.available_seats, e.ticket_price, e.banner_image, \
                          e.status, e.created_at, e.updated_at, \
                          u.full_name AS organizer_name, u.email AS organizer_email, \
                          c.name AS category_name, c.icon AS category_icon \
                   FROM events e \
                   JOIN users u ON e.organizer_id = u.id \
                   LEFT JOIN categories c ON e.category_id = c.id \
                   WHERE e.id = ?";

        sqlx::query_as::<_, Event>(sql)
            .bind(id)
            .fetch_optional(pool)
            .await
    }
}
