const db = require('../config/db');

class Event {
    static async findAll({ status = 'published', category_id, city, search, limit = 20, offset = 0 } = {}) {
        let sql = `
      SELECT e.*, u.full_name AS organizer_name, c.name AS category_name, c.icon AS category_icon
      FROM events e
      JOIN users u ON e.organizer_id = u.id
      LEFT JOIN categories c ON e.category_id = c.id
      WHERE 1=1
    `;
        const params = [];

        if (status) { sql += ' AND e.status = ?'; params.push(status); }
        if (category_id) { sql += ' AND e.category_id = ?'; params.push(category_id); }
        if (city) { sql += ' AND e.city LIKE ?'; params.push(`%${city}%`); }
        if (search) { sql += ' AND (e.title LIKE ? OR e.description LIKE ?)'; params.push(`%${search}%`, `%${search}%`); }

        sql += ' ORDER BY e.event_date ASC LIMIT ? OFFSET ?';
        params.push(Number(limit), Number(offset));

        const [rows] = await db.query(sql, params);
        return rows;
    }

    static async findById(id) {
        const [rows] = await db.query(
            `SELECT e.*, u.full_name AS organizer_name, u.email AS organizer_email,
              c.name AS category_name, c.icon AS category_icon
       FROM events e
       JOIN users u ON e.organizer_id = u.id
       LEFT JOIN categories c ON e.category_id = c.id
       WHERE e.id = ?`,
            [id]
        );
        return rows[0] || null;
    }

    static async getByOrganizer(organizer_id) {
        const [rows] = await db.query(
            `SELECT e.*, c.name AS category_name
       FROM events e
       LEFT JOIN categories c ON e.category_id = c.id
       WHERE e.organizer_id = ?
       ORDER BY e.created_at DESC`,
            [organizer_id]
        );
        return rows;
    }

    static async create({ organizer_id, category_id, title, description, venue, city, event_date, registration_deadline, total_seats, ticket_price, status = 'draft', banner_image = null }) {
        const [result] = await db.query(
            `INSERT INTO events
         (organizer_id, category_id, title, description, venue, city, event_date, registration_deadline, total_seats, available_seats, ticket_price, status, banner_image)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
            [organizer_id, category_id, title, description, venue, city, event_date, registration_deadline,
                total_seats, total_seats, ticket_price, status, banner_image]
        );
        return result.insertId;
    }

    static async update(id, fields) {
        const allowed = ['title', 'description', 'venue', 'city', 'event_date', 'registration_deadline', 'total_seats', 'ticket_price', 'category_id', 'status', 'banner_image'];
        const sets = [];
        const vals = [];
        for (const [k, v] of Object.entries(fields)) {
            if (allowed.includes(k) && v !== undefined) {
                sets.push(`${k} = ?`);
                vals.push(v);
            }
        }
        if (!sets.length) return;
        vals.push(id);
        await db.query(`UPDATE events SET ${sets.join(', ')} WHERE id = ?`, vals);
    }

    static async delete(id) {
        await db.query('DELETE FROM events WHERE id = ?', [id]);
    }

    static async decrementSeats(id, qty = 1) {
        await db.query(
            'UPDATE events SET available_seats = available_seats - ? WHERE id = ? AND available_seats >= ?',
            [qty, id, qty]
        );
    }

    static async incrementSeats(id, qty = 1) {
        await db.query(
            'UPDATE events SET available_seats = available_seats + ? WHERE id = ?',
            [qty, id]
        );
    }

    static async count(status = null) {
        const sql = status
            ? 'SELECT COUNT(*) AS total FROM events WHERE status = ?'
            : 'SELECT COUNT(*) AS total FROM events';
        const [rows] = await db.query(sql, status ? [status] : []);
        return rows[0].total;
    }
}

module.exports = Event;
