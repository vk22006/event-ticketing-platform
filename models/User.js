const db = require('../config/db');
const bcrypt = require('bcryptjs');

class User {
    static async findByEmail(email) {
        const [rows] = await db.query('SELECT * FROM users WHERE email = ?', [email]);
        return rows[0] || null;
    }

    static async findById(id) {
        const [rows] = await db.query(
            'SELECT id, full_name, email, role, avatar, bio, created_at FROM users WHERE id = ?',
            [id]
        );
        return rows[0] || null;
    }

    static async create({ full_name, email, password, role = 'user' }) {
        const hashed = await bcrypt.hash(password, 10);
        const [result] = await db.query(
            'INSERT INTO users (full_name, email, password, role) VALUES (?, ?, ?, ?)',
            [full_name, email, hashed, role]
        );
        return result.insertId;
    }

    static async verifyPassword(plain, hashed) {
        return bcrypt.compare(plain, hashed);
    }

    static async updateProfile(id, { full_name, bio, avatar }) {
        await db.query(
            'UPDATE users SET full_name=?, bio=?, avatar=? WHERE id=?',
            [full_name, bio, avatar, id]
        );
    }

    static async findAll() {
        const [rows] = await db.query(
            'SELECT id, full_name, email, role, created_at FROM users ORDER BY created_at DESC'
        );
        return rows;
    }

    static async countByRole() {
        const [rows] = await db.query(
            `SELECT role, COUNT(*) AS total FROM users GROUP BY role`
        );
        return rows;
    }
}

module.exports = User;
