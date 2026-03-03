const db = require('../config/db');

class Booking {
    static async create({ user_id, event_id, seat_type_id, quantity, total_amount }) {
        const [result] = await db.query(
            `INSERT INTO bookings (user_id, event_id, seat_type_id, quantity, total_amount, status)
       VALUES (?, ?, ?, ?, ?, 'confirmed')`,
            [user_id, event_id, seat_type_id || null, quantity, total_amount]
        );
        return result.insertId;
    }

    static async findByUser(user_id) {
        const [rows] = await db.query(
            `SELECT b.*, e.title AS event_title, e.event_date, e.venue, e.city,
              e.banner_image, st.type_name AS seat_type
       FROM bookings b
       JOIN events e ON b.event_id = e.id
       LEFT JOIN seat_types st ON b.seat_type_id = st.id
       WHERE b.user_id = ?
       ORDER BY b.booked_at DESC`,
            [user_id]
        );
        return rows;
    }

    static async findByEvent(event_id) {
        const [rows] = await db.query(
            `SELECT b.*, u.full_name, u.email, st.type_name AS seat_type
       FROM bookings b
       JOIN users u ON b.user_id = u.id
       LEFT JOIN seat_types st ON b.seat_type_id = st.id
       WHERE b.event_id = ?
       ORDER BY b.booked_at DESC`,
            [event_id]
        );
        return rows;
    }

    static async findById(id) {
        const [rows] = await db.query(
            `SELECT b.*, e.title AS event_title, e.event_date, e.venue, e.city,
              u.full_name, u.email, st.type_name AS seat_type
       FROM bookings b
       JOIN events e ON b.event_id = e.id
       JOIN users u ON b.user_id = u.id
       LEFT JOIN seat_types st ON b.seat_type_id = st.id
       WHERE b.id = ?`,
            [id]
        );
        return rows[0] || null;
    }

    static async cancel(id) {
        await db.query(`UPDATE bookings SET status='cancelled' WHERE id=?`, [id]);
    }

    static async checkCapacity(event_id, quantity) {
        const [rows] = await db.query(
            'SELECT available_seats FROM events WHERE id = ?',
            [event_id]
        );
        if (!rows[0]) return false;
        return rows[0].available_seats >= quantity;
    }

    static async countTotal() {
        const [rows] = await db.query('SELECT COUNT(*) AS total FROM bookings WHERE status="confirmed"');
        return rows[0].total;
    }

    static async totalRevenue() {
        const [rows] = await db.query(
            'SELECT SUM(total_amount) AS revenue FROM bookings WHERE status="confirmed"'
        );
        return rows[0].revenue || 0;
    }

    static async recentBookings(limit = 10) {
        const [rows] = await db.query(
            `SELECT b.*, u.full_name, u.email, e.title AS event_title
       FROM bookings b
       JOIN users u ON b.user_id = u.id
       JOIN events e ON b.event_id = e.id
       ORDER BY b.booked_at DESC
       LIMIT ?`,
            [limit]
        );
        return rows;
    }
}

module.exports = Booking;
