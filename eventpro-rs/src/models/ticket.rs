use chrono::NaiveDateTime;
use rand::RngCore;
use rust_decimal::Decimal;
use sqlx::MySqlPool;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, PartialEq)]
pub struct Ticket {
    pub id: i32,
    pub booking_id: i32,
    pub user_id: i32,
    pub event_id: i32,
    pub ticket_code: String,
    pub seat_number: Option<String>,
    pub status: String,
    pub issued_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, PartialEq)]
pub struct UserTicket {
    pub id: i32,
    pub booking_id: i32,
    pub user_id: i32,
    pub event_id: i32,
    pub ticket_code: String,
    pub seat_number: Option<String>,
    pub status: String,
    pub issued_at: Option<NaiveDateTime>,
    // Joined event & booking fields
    pub event_title: String,
    pub event_date: NaiveDateTime,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub banner_image: Option<String>,
    pub total_amount: Decimal,
    pub quantity: i32,
    pub booking_status: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow, PartialEq)]
pub struct TicketDetail {
    pub id: i32,
    pub booking_id: i32,
    pub user_id: i32,
    pub event_id: i32,
    pub ticket_code: String,
    pub seat_number: Option<String>,
    pub status: String,
    pub issued_at: Option<NaiveDateTime>,
    // Joined event, user & booking fields
    pub event_title: String,
    pub event_date: NaiveDateTime,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub full_name: String,
    pub email: String,
    pub quantity: i32,
    pub total_amount: Decimal,
    pub booking_status: String,
}

impl Ticket {
    /// Generate a unique ticket code following the established format:
    /// `EVT-{event_id}-BKG-{booking_id}-{8 uppercase hex characters}`
    pub fn generate_code(event_id: i32, booking_id: i32) -> String {
        let mut bytes = [0u8; 4];
        rand::thread_rng().fill_bytes(&mut bytes);
        let hex_suffix = bytes
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<String>();
        format!("EVT-{}-BKG-{}-{}", event_id, booking_id, hex_suffix)
    }

    /// Retrieve all tickets for a specific booking
    pub async fn find_by_booking(
        pool: &MySqlPool,
        booking_id: i32,
    ) -> Result<Vec<Ticket>, sqlx::Error> {
        sqlx::query_as::<_, Ticket>(
            "SELECT id, booking_id, user_id, event_id, ticket_code, seat_number, status, issued_at \
             FROM tickets WHERE booking_id = ? ORDER BY id ASC",
        )
        .bind(booking_id)
        .fetch_all(pool)
        .await
    }

    /// Retrieve all tickets for a user with joined event and booking info
    pub async fn find_by_user(
        pool: &MySqlPool,
        user_id: i32,
    ) -> Result<Vec<UserTicket>, sqlx::Error> {
        let sql = "SELECT t.id, t.booking_id, t.user_id, t.event_id, t.ticket_code, \
                          t.seat_number, t.status, t.issued_at, \
                          e.title AS event_title, e.event_date, e.venue, e.city, e.banner_image, \
                          b.total_amount, b.quantity, b.status AS booking_status \
                   FROM tickets t \
                   JOIN events e ON t.event_id = e.id \
                   JOIN bookings b ON t.booking_id = b.id \
                   WHERE t.user_id = ? \
                   ORDER BY t.issued_at DESC, t.id DESC";

        sqlx::query_as::<_, UserTicket>(sql)
            .bind(user_id)
            .fetch_all(pool)
            .await
    }

    /// Retrieve detailed ticket information by ticket ID
    pub async fn find_by_id(
        pool: &MySqlPool,
        id: i32,
    ) -> Result<Option<TicketDetail>, sqlx::Error> {
        let sql = "SELECT t.id, t.booking_id, t.user_id, t.event_id, t.ticket_code, \
                          t.seat_number, t.status, t.issued_at, \
                          e.title AS event_title, e.event_date, e.venue, e.city, \
                          u.full_name, u.email, \
                          b.quantity, b.total_amount, b.status AS booking_status \
                   FROM tickets t \
                   JOIN events e ON t.event_id = e.id \
                   JOIN users u ON t.user_id = u.id \
                   JOIN bookings b ON t.booking_id = b.id \
                   WHERE t.id = ?";

        sqlx::query_as::<_, TicketDetail>(sql)
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    /// Retrieve detailed ticket information by ticket code
    pub async fn find_by_code(
        pool: &MySqlPool,
        ticket_code: &str,
    ) -> Result<Option<TicketDetail>, sqlx::Error> {
        let sql = "SELECT t.id, t.booking_id, t.user_id, t.event_id, t.ticket_code, \
                          t.seat_number, t.status, t.issued_at, \
                          e.title AS event_title, e.event_date, e.venue, e.city, \
                          u.full_name, u.email, \
                          b.quantity, b.total_amount, b.status AS booking_status \
                   FROM tickets t \
                   JOIN events e ON t.event_id = e.id \
                   JOIN users u ON t.user_id = u.id \
                   JOIN bookings b ON t.booking_id = b.id \
                   WHERE t.ticket_code = ?";

        sqlx::query_as::<_, TicketDetail>(sql)
            .bind(ticket_code)
            .fetch_optional(pool)
            .await
    }
}
