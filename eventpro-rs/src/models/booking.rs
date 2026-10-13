use crate::models::ticket::Ticket;
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
pub struct BookingDetail {
    pub id: i32,
    pub user_id: i32,
    pub event_id: i32,
    pub seat_type_id: Option<i32>,
    pub quantity: i32,
    pub total_amount: Decimal,
    pub status: String,
    pub payment_method: Option<String>,
    pub booked_at: Option<chrono::NaiveDateTime>,
    // Joined event & user & seat fields
    pub event_title: String,
    pub event_date: chrono::NaiveDateTime,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub full_name: String,
    pub email: String,
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

#[derive(Debug, Clone)]
pub struct CreateBookingResult {
    pub booking_id: i32,
    pub total_amount: Decimal,
    pub tickets: Vec<Ticket>,
}

#[derive(Debug, thiserror::Error)]
pub enum BookingError {
    #[error("Event not found or not published")]
    EventNotFound,
    #[error("Event has already started")]
    EventExpired,
    #[error("Registration deadline has passed")]
    RegistrationClosed,
    #[error("Invalid quantity. Must be between 1 and 10")]
    InvalidQuantity,
    #[error("Invalid seat type selected")]
    InvalidSeatType,
    #[error("Not enough seats available for this event")]
    InsufficientEventCapacity,
    #[error("Not enough seats available in selected tier")]
    InsufficientSeatTypeCapacity,
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum CancelError {
    #[error("Booking not found")]
    NotFound,
    #[error("Unauthorized: only the booking owner may cancel")]
    Forbidden,
    #[error("Booking is not in a cancellable state")]
    InvalidStatus,
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(sqlx::FromRow)]
struct LockedEvent {
    pub id: i32,
    pub status: String,
    pub event_date: chrono::NaiveDateTime,
    pub registration_deadline: Option<chrono::NaiveDateTime>,
    pub available_seats: i32,
    pub ticket_price: Decimal,
}

#[derive(sqlx::FromRow)]
struct LockedSeatType {
    pub event_id: i32,
    pub price: Decimal,
    pub total_seats: i32,
    pub booked_seats: i32,
}

#[derive(sqlx::FromRow)]
struct BookingSnapshot {
    pub user_id: i32,
    pub event_id: i32,
    pub seat_type_id: Option<i32>,
}

#[derive(sqlx::FromRow)]
struct LockedBooking {
    pub user_id: i32,
    pub event_id: i32,
    pub seat_type_id: Option<i32>,
    pub quantity: i32,
    pub status: String,
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

    /// Retrieve detailed booking information by ID
    pub async fn find_by_id(
        pool: &MySqlPool,
        id: i32,
    ) -> Result<Option<BookingDetail>, sqlx::Error> {
        let sql = "SELECT b.id, b.user_id, b.event_id, b.seat_type_id, b.quantity, \
                          b.total_amount, b.status, b.payment_method, b.booked_at, \
                          e.title AS event_title, e.event_date, e.venue, e.city, \
                          u.full_name, u.email, st.type_name AS seat_type \
                   FROM bookings b \
                   JOIN events e ON b.event_id = e.id \
                   JOIN users u ON b.user_id = u.id \
                   LEFT JOIN seat_types st ON b.seat_type_id = st.id \
                   WHERE b.id = ?";

        sqlx::query_as::<_, BookingDetail>(sql)
            .bind(id)
            .fetch_optional(pool)
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

    /// Atomically book seats within a transaction using row-level locking.
    ///
    /// Lock order:
    /// 1. Lock `events` record (`FOR UPDATE`).
    /// 2. Lock `seat_types` record (`FOR UPDATE`, if seat_type_id is provided).
    ///
    /// Validates publication status, timing, deadline, and capacity invariants.
    /// Inserts booking, generates tickets, and updates remaining seat counters.
    pub async fn create_transactional(
        pool: &MySqlPool,
        user_id: i32,
        event_id: i32,
        seat_type_id: Option<i32>,
        quantity: i32,
    ) -> Result<CreateBookingResult, BookingError> {
        if !(1..=10).contains(&quantity) {
            return Err(BookingError::InvalidQuantity);
        }

        let mut tx = pool.begin().await?;

        // 1. Lock event row
        let event_row: Option<LockedEvent> = sqlx::query_as(
            "SELECT id, status, event_date, registration_deadline, available_seats, ticket_price \
             FROM events WHERE id = ? FOR UPDATE",
        )
        .bind(event_id)
        .fetch_optional(&mut *tx)
        .await?;

        let ev = match event_row {
            Some(r) => r,
            None => return Err(BookingError::EventNotFound),
        };

        if ev.status != "published" {
            return Err(BookingError::EventNotFound);
        }

        let now = chrono::Utc::now().naive_utc();
        if ev.event_date <= now {
            return Err(BookingError::EventExpired);
        }
        if let Some(deadline) = ev.registration_deadline {
            if deadline < now {
                return Err(BookingError::RegistrationClosed);
            }
        }

        if ev.available_seats < quantity {
            return Err(BookingError::InsufficientEventCapacity);
        }

        // 2. Lock seat_type row if seat_type_id is provided
        let unit_price = if let Some(st_id) = seat_type_id {
            let seat_row: Option<LockedSeatType> = sqlx::query_as(
                "SELECT event_id, price, total_seats, booked_seats FROM seat_types WHERE id = ? FOR UPDATE",
            )
            .bind(st_id)
            .fetch_optional(&mut *tx)
            .await?;

            let s = match seat_row {
                Some(s) => s,
                None => return Err(BookingError::InvalidSeatType),
            };

            if s.event_id != ev.id {
                return Err(BookingError::InvalidSeatType);
            }

            if (s.total_seats - s.booked_seats) < quantity {
                return Err(BookingError::InsufficientSeatTypeCapacity);
            }

            s.price
        } else {
            ev.ticket_price
        };

        let total_amount = unit_price * Decimal::from(quantity);

        // 3. Insert booking record
        let res = sqlx::query(
            "INSERT INTO bookings (user_id, event_id, seat_type_id, quantity, total_amount, status, payment_method, booked_at) \
             VALUES (?, ?, ?, ?, ?, 'confirmed', 'mock_payment', NOW())",
        )
        .bind(user_id)
        .bind(ev.id)
        .bind(seat_type_id)
        .bind(quantity)
        .bind(total_amount)
        .execute(&mut *tx)
        .await?;

        let booking_id = res.last_insert_id() as i32;

        // 4. Generate and insert ticket records
        let mut tickets = Vec::with_capacity(quantity as usize);
        for _ in 0..quantity {
            let ticket_code = Ticket::generate_code(ev.id, booking_id);
            let t_res = sqlx::query(
                "INSERT INTO tickets (booking_id, user_id, event_id, ticket_code, status, issued_at) \
                 VALUES (?, ?, ?, ?, 'active', NOW())",
            )
            .bind(booking_id)
            .bind(user_id)
            .bind(ev.id)
            .bind(&ticket_code)
            .execute(&mut *tx)
            .await?;

            let ticket_id = t_res.last_insert_id() as i32;
            tickets.push(Ticket {
                id: ticket_id,
                booking_id,
                user_id,
                event_id: ev.id,
                ticket_code,
                seat_number: None,
                status: "active".to_string(),
                issued_at: Some(chrono::Utc::now().naive_utc()),
            });
        }

        // 5. Update remaining capacity counters
        sqlx::query("UPDATE events SET available_seats = available_seats - ? WHERE id = ?")
            .bind(quantity)
            .bind(ev.id)
            .execute(&mut *tx)
            .await?;

        if let Some(st_id) = seat_type_id {
            sqlx::query("UPDATE seat_types SET booked_seats = booked_seats + ? WHERE id = ?")
                .bind(quantity)
                .bind(st_id)
                .execute(&mut *tx)
                .await?;
        }

        // 6. Commit atomic transaction
        tx.commit().await?;

        Ok(CreateBookingResult {
            booking_id,
            total_amount,
            tickets,
        })
    }

    /// Atomically cancel a booking within a transaction.
    ///
    /// Lock order:
    /// 1. Lock `events` record (`FOR UPDATE`).
    /// 2. Lock `seat_types` record (`FOR UPDATE`, if present).
    /// 3. Lock `bookings` record (`FOR UPDATE`).
    ///
    /// Validates ownership, ensures booking is 'confirmed', marks booking & tickets
    /// as 'cancelled', and restores seat counters.
    pub async fn cancel_transactional(
        pool: &MySqlPool,
        booking_id: i32,
        user_id: i32,
        is_admin: bool,
    ) -> Result<(), CancelError> {
        // Read uncommitted snapshot to discover event_id and seat_type_id
        let snapshot: Option<BookingSnapshot> =
            sqlx::query_as("SELECT user_id, event_id, seat_type_id FROM bookings WHERE id = ?")
                .bind(booking_id)
                .fetch_optional(pool)
                .await?;

        let snap = match snapshot {
            Some(s) => s,
            None => return Err(CancelError::NotFound),
        };

        if !is_admin && snap.user_id != user_id {
            return Err(CancelError::Forbidden);
        }

        let mut tx = pool.begin().await?;

        // 1. Lock events record
        let ev_exists: Option<(i32,)> =
            sqlx::query_as("SELECT id FROM events WHERE id = ? FOR UPDATE")
                .bind(snap.event_id)
                .fetch_optional(&mut *tx)
                .await?;

        if ev_exists.is_none() {
            return Err(CancelError::NotFound);
        }

        // 2. Lock seat_types record if applicable
        if let Some(st_id) = snap.seat_type_id {
            let _st_exists: Option<(i32,)> =
                sqlx::query_as("SELECT id FROM seat_types WHERE id = ? FOR UPDATE")
                    .bind(st_id)
                    .fetch_optional(&mut *tx)
                    .await?;
        }

        // 3. Lock bookings record
        let locked_b: Option<LockedBooking> = sqlx::query_as(
            "SELECT user_id, event_id, seat_type_id, quantity, status \
             FROM bookings WHERE id = ? FOR UPDATE",
        )
        .bind(booking_id)
        .fetch_optional(&mut *tx)
        .await?;

        let b = match locked_b {
            Some(b) => b,
            None => return Err(CancelError::NotFound),
        };

        if !is_admin && b.user_id != user_id {
            return Err(CancelError::Forbidden);
        }

        if b.status != "confirmed" {
            return Err(CancelError::InvalidStatus);
        }

        // 4. Update booking status to 'cancelled'
        sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
            .bind(booking_id)
            .execute(&mut *tx)
            .await?;

        // 5. Update tickets to 'cancelled'
        sqlx::query("UPDATE tickets SET status = 'cancelled' WHERE booking_id = ?")
            .bind(booking_id)
            .execute(&mut *tx)
            .await?;

        // 6. Restore counters
        sqlx::query("UPDATE events SET available_seats = available_seats + ? WHERE id = ?")
            .bind(b.quantity)
            .bind(b.event_id)
            .execute(&mut *tx)
            .await?;

        if let Some(st_id) = b.seat_type_id {
            sqlx::query("UPDATE seat_types SET booked_seats = booked_seats - ? WHERE id = ?")
                .bind(b.quantity)
                .bind(st_id)
                .execute(&mut *tx)
                .await?;
        }

        // 7. Commit transaction
        tx.commit().await?;

        Ok(())
    }
}
