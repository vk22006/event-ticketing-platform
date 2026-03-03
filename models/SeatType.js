const db = require('../config/db');

class SeatType {
    static async findByEvent(event_id) {
        const [rows] = await db.query(
            'SELECT * FROM seat_types WHERE event_id = ?',
            [event_id]
        );
        return rows;
    }

    static async decrementBooked(id, qty = 1) {
        await db.query(
            'UPDATE seat_types SET booked_seats = booked_seats + ? WHERE id = ? AND (total_seats - booked_seats) >= ?',
            [qty, id, qty]
        );
    }

    static async incrementBooked(id, qty = 1) {
        await db.query(
            'UPDATE seat_types SET booked_seats = booked_seats - ? WHERE id = ?',
            [qty, id]
        );
    }
}

module.exports = SeatType;
