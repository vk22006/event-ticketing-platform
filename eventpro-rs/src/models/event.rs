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

    /// Retrieve all events owned by a specific organizer ordered by created_at DESC
    pub async fn get_by_organizer(
        pool: &MySqlPool,
        organizer_id: i32,
    ) -> Result<Vec<Event>, sqlx::Error> {
        let sql = "SELECT e.id, e.organizer_id, e.category_id, e.title, e.description, \
                          e.venue, e.city, e.event_date, e.registration_deadline, \
                          e.total_seats, e.available_seats, e.ticket_price, e.banner_image, \
                          e.status, e.created_at, e.updated_at, \
                          u.full_name AS organizer_name, u.email AS organizer_email, \
                          c.name AS category_name, c.icon AS category_icon \
                   FROM events e \
                   JOIN users u ON e.organizer_id = u.id \
                   LEFT JOIN categories c ON e.category_id = c.id \
                   WHERE e.organizer_id = ? \
                   ORDER BY e.created_at DESC";

        sqlx::query_as::<_, Event>(sql)
            .bind(organizer_id)
            .fetch_all(pool)
            .await
    }

    /// Retrieve all events across all organizers and statuses for admin view
    pub async fn find_all_unfiltered(pool: &MySqlPool) -> Result<Vec<Event>, sqlx::Error> {
        let sql = "SELECT e.id, e.organizer_id, e.category_id, e.title, e.description, \
                          e.venue, e.city, e.event_date, e.registration_deadline, \
                          e.total_seats, e.available_seats, e.ticket_price, e.banner_image, \
                          e.status, e.created_at, e.updated_at, \
                          u.full_name AS organizer_name, u.email AS organizer_email, \
                          c.name AS category_name, c.icon AS category_icon \
                   FROM events e \
                   JOIN users u ON e.organizer_id = u.id \
                   LEFT JOIN categories c ON e.category_id = c.id \
                   ORDER BY e.created_at DESC";

        sqlx::query_as::<_, Event>(sql).fetch_all(pool).await
    }

    /// Insert a new event into the database
    pub async fn create(
        pool: &MySqlPool,
        organizer_id: i32,
        params: &CreateEventParams,
    ) -> Result<i32, sqlx::Error> {
        let sql = "INSERT INTO events \
                   (organizer_id, category_id, title, description, venue, city, \
                    event_date, registration_deadline, total_seats, available_seats, \
                    ticket_price, status, banner_image) \
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

        let res = sqlx::query(sql)
            .bind(organizer_id)
            .bind(params.category_id)
            .bind(&params.title)
            .bind(&params.description)
            .bind(&params.venue)
            .bind(&params.city)
            .bind(params.event_date)
            .bind(params.registration_deadline)
            .bind(params.total_seats)
            .bind(params.total_seats) // initial available_seats = total_seats
            .bind(params.ticket_price)
            .bind(&params.status)
            .bind(&params.banner_image)
            .execute(pool)
            .await?;

        Ok(res.last_insert_id() as i32)
    }

    /// Update an existing event in the database
    pub async fn update(
        pool: &MySqlPool,
        id: i32,
        params: &UpdateEventParams,
    ) -> Result<(), sqlx::Error> {
        let sql = "UPDATE events SET \
                   title = ?, description = ?, venue = ?, city = ?, \
                   event_date = ?, registration_deadline = ?, total_seats = ?, \
                   ticket_price = ?, category_id = ?, status = ?, banner_image = COALESCE(?, banner_image) \
                   WHERE id = ?";

        sqlx::query(sql)
            .bind(&params.title)
            .bind(&params.description)
            .bind(&params.venue)
            .bind(&params.city)
            .bind(params.event_date)
            .bind(params.registration_deadline)
            .bind(params.total_seats)
            .bind(params.ticket_price)
            .bind(params.category_id)
            .bind(&params.status)
            .bind(&params.banner_image)
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Update the status of an event (e.g. toggle between published and draft)
    pub async fn update_status(
        pool: &MySqlPool,
        id: i32,
        new_status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE events SET status = ? WHERE id = ?")
            .bind(new_status)
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Delete an event by ID
    pub async fn delete(pool: &MySqlPool, id: i32) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM events WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Count events with optional status filter for platform statistics
    pub async fn count(pool: &MySqlPool, status: Option<&str>) -> Result<i64, sqlx::Error> {
        let (sql, bind_status) = match status {
            Some(s) => ("SELECT COUNT(*) FROM events WHERE status = ?", Some(s)),
            None => ("SELECT COUNT(*) FROM events", None),
        };

        let mut query = sqlx::query_as::<_, (i64,)>(sql);
        if let Some(s) = bind_status {
            query = query.bind(s);
        }

        let row = query.fetch_one(pool).await?;
        Ok(row.0)
    }
}

#[derive(Debug, Clone)]
pub struct CreateEventParams {
    pub category_id: Option<i32>,
    pub title: String,
    pub description: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub event_date: chrono::NaiveDateTime,
    pub registration_deadline: Option<chrono::NaiveDateTime>,
    pub total_seats: i32,
    pub ticket_price: Decimal,
    pub status: String,
    pub banner_image: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateEventParams {
    pub category_id: Option<i32>,
    pub title: String,
    pub description: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub event_date: chrono::NaiveDateTime,
    pub registration_deadline: Option<chrono::NaiveDateTime>,
    pub total_seats: i32,
    pub ticket_price: Decimal,
    pub status: String,
    pub banner_image: Option<String>,
}
