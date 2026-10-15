use rust_decimal::Decimal;
use sqlx::MySqlPool;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, PartialEq)]
pub struct SeatType {
    pub id: i32,
    pub event_id: i32,
    pub type_name: String,
    pub price: Decimal,
    pub total_seats: i32,
    pub booked_seats: i32,
}

impl SeatType {
    /// Retrieve all seat tiers for a specific event
    pub async fn find_by_event(
        pool: &MySqlPool,
        event_id: i32,
    ) -> Result<Vec<SeatType>, sqlx::Error> {
        let mut conn = crate::metrics::acquire_conn(pool).await?;
        let q_start = std::time::Instant::now();
        let res = sqlx::query_as::<_, SeatType>(
            "SELECT id, event_id, type_name, price, total_seats, booked_seats FROM seat_types WHERE event_id = ? ORDER BY price ASC",
        )
        .bind(event_id)
        .fetch_all(&mut *conn)
        .await;
        crate::metrics::record_query_duration(q_start.elapsed());
        res
    }

    /// Retrieve a specific seat tier by ID
    pub async fn find_by_id(pool: &MySqlPool, id: i32) -> Result<Option<SeatType>, sqlx::Error> {
        let mut conn = crate::metrics::acquire_conn(pool).await?;
        let q_start = std::time::Instant::now();
        let res = sqlx::query_as::<_, SeatType>(
            "SELECT id, event_id, type_name, price, total_seats, booked_seats FROM seat_types WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await;
        crate::metrics::record_query_duration(q_start.elapsed());
        res
    }
}
