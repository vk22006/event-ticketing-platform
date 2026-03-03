const db = require('../config/db');

class Category {
    static async findAll() {
        const [rows] = await db.query('SELECT * FROM categories ORDER BY name ASC');
        return rows;
    }

    static async findById(id) {
        const [rows] = await db.query('SELECT * FROM categories WHERE id = ?', [id]);
        return rows[0] || null;
    }
}

module.exports = Category;
