use rust_decimal::Decimal;
use sqlx::MySqlPool;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, PartialEq)]
pub struct UserBooking {
    pub id: i32,
    pub user_id: i32,
    pub event_id: i32,
    pub seat_type_id: Option<i32>,
    pub quantity: i32,
    pub total_amount: Decimal,
    pub status: String,
    pub payment_method: Option<String>,
    pub booked_at: Option<chrono::NaiveDateTime>,
    // Joined event fields
    pub event_title: String,
    pub event_date: chrono::NaiveDateTime,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub banner_image: Option<String>,
    pub seat_type: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, PartialEq)]
pub struct RecentBooking {
    pub id: i32,
    pub user_id: i32,
    pub event_id: i32,
    pub total_amount: Decimal,
    pub status: String,
    pub booked_at: Option<chrono::NaiveDateTime>,
    pub full_name: String,
    pub email: String,
    pub event_title: String,
}

pub struct Booking;

impl Booking {
    /// Retrieve all bookings made by a specific user with joined event and seat details
    pub async fn find_by_user(
        pool: &MySqlPool,
        user_id: i32,
    ) -> Result<Vec<UserBooking>, sqlx::Error> {
        let sql = "SELECT b.id, b.user_id, b.event_id, b.seat_type_id, b.quantity, \
                          b.total_amount, b.status, b.payment_method, b.booked_at, \
                          e.title AS event_title, e.event_date, e.venue, e.city, \
                          e.banner_image, st.type_name AS seat_type \
                   FROM bookings b \
                   JOIN events e ON b.event_id = e.id \
                   LEFT JOIN seat_types st ON b.seat_type_id = st.id \
                   WHERE b.user_id = ? \
                   ORDER BY b.booked_at DESC";

        sqlx::query_as::<_, UserBooking>(sql)
            .bind(user_id)
            .fetch_all(pool)
            .await
    }

    /// Count total confirmed bookings for platform statistics
    pub async fn count_total(pool: &MySqlPool) -> Result<i64, sqlx::Error> {
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM bookings WHERE status = 'confirmed'")
                .fetch_one(pool)
                .await?;
        Ok(row.0)
    }

    /// Calculate total revenue from confirmed bookings
    pub async fn total_revenue(pool: &MySqlPool) -> Result<Decimal, sqlx::Error> {
        let row: (Option<Decimal>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(total_amount), 0.00) FROM bookings WHERE status = 'confirmed'",
        )
        .fetch_one(pool)
        .await?;
        Ok(row.0.unwrap_or(Decimal::ZERO))
    }

    /// Retrieve recent bookings with buyer and event information
    pub async fn recent_bookings(
        pool: &MySqlPool,
        limit: i64,
    ) -> Result<Vec<RecentBooking>, sqlx::Error> {
        let sql = "SELECT b.id, b.user_id, b.event_id, b.total_amount, b.status, b.booked_at, \
                          u.full_name, u.email, e.title AS event_title \
                   FROM bookings b \
                   JOIN users u ON b.user_id = u.id \
                   JOIN events e ON b.event_id = e.id \
                   ORDER BY b.booked_at DESC \
                   LIMIT ?";

        sqlx::query_as::<_, RecentBooking>(sql)
            .bind(limit)
            .fetch_all(pool)
            .await
    }
}
