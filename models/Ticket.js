const db = require('../config/db');
const { v4: uuidv4 } = require('uuid');

class Ticket {
    static generateCode(eventId, bookingId) {
        const rand = uuidv4().split('-')[0].toUpperCase();
        return `EVT-${eventId}-BKG-${bookingId}-${rand}`;
    }

    static async create({ booking_id, user_id, event_id, seat_number = null }) {
        const ticket_code = Ticket.generateCode(event_id, booking_id);
        const [result] = await db.query(
            `INSERT INTO tickets (booking_id, user_id, event_id, ticket_code, seat_number)
       VALUES (?, ?, ?, ?, ?)`,
            [booking_id, user_id, event_id, ticket_code, seat_number]
        );
        return { id: result.insertId, ticket_code };
    }

    static async findByBooking(booking_id) {
        const [rows] = await db.query(
            `SELECT t.*, e.title AS event_title, e.event_date, e.venue, e.city,
              u.full_name, u.email
       FROM tickets t
       JOIN events e ON t.event_id = e.id
       JOIN users u ON t.user_id = u.id
       WHERE t.booking_id = ?`,
            [booking_id]
        );
        return rows;
    }

    static async findByUser(user_id) {
        const [rows] = await db.query(
            `SELECT t.*, e.title AS event_title, e.event_date, e.venue, e.city, e.banner_image,
              b.total_amount, b.quantity, b.status AS booking_status
       FROM tickets t
       JOIN events e ON t.event_id = e.id
       JOIN bookings b ON t.booking_id = b.id
       WHERE t.user_id = ?
       ORDER BY t.issued_at DESC`,
            [user_id]
        );
        return rows;
    }

    static async findByCode(ticket_code) {
        const [rows] = await db.query(
            `SELECT t.*, e.title AS event_title, e.event_date, e.venue, e.city,
              u.full_name, u.email, b.quantity, b.total_amount
       FROM tickets t
       JOIN events e ON t.event_id = e.id
       JOIN users u ON t.user_id = u.id
       JOIN bookings b ON t.booking_id = b.id
       WHERE t.ticket_code = ?`,
            [ticket_code]
        );
        return rows[0] || null;
    }

    static async findById(id) {
        const [rows] = await db.query(
            `SELECT t.*, e.title AS event_title, e.event_date, e.venue, e.city,
              u.full_name, u.email, b.quantity, b.total_amount, b.status AS booking_status
       FROM tickets t
       JOIN events e ON t.event_id = e.id
       JOIN users u ON t.user_id = u.id
       JOIN bookings b ON t.booking_id = b.id
       WHERE t.id = ?`,
            [id]
        );
        return rows[0] || null;
    }

    static async markUsed(id) {
        await db.query(`UPDATE tickets SET status='used' WHERE id=?`, [id]);
    }

    static async cancelByBooking(booking_id) {
        await db.query(`UPDATE tickets SET status='cancelled' WHERE booking_id=?`, [booking_id]);
    }
}

module.exports = Ticket;
