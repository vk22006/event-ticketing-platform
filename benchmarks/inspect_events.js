const mysql = require('mysql2/promise');

async function main() {
    const conn = await mysql.createConnection('mysql://root:kishore2006@127.0.0.1:3306/event_lifecycle');
    const [events] = await conn.query('SELECT id, title, status, event_date, available_seats, total_seats FROM events WHERE status = "published" ORDER BY id LIMIT 5');
    console.table(events);
    if (events.length > 0) {
        const ids = events.map(e => e.id).join(',');
        const [seats] = await conn.query(`SELECT id, event_id, type_name, price, total_seats, booked_seats FROM seat_types WHERE event_id IN (${ids})`);
        console.table(seats);
    }
    await conn.end();
}

main().catch(console.error);
