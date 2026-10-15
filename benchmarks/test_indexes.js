const mysql = require('mysql2/promise');

async function main() {
    const conn = await mysql.createConnection({
        host: '127.0.0.1',
        port: 3306,
        user: 'root',
        password: 'kishore2006',
        database: 'event_lifecycle',
    });

    console.log('=== BEFORE COMPOSITE INDEXES ===');

    console.log('\n--- EXPLAIN: Booking history by user ---');
    const [plan1Before] = await conn.query(
        'EXPLAIN SELECT b.id, b.user_id, b.event_id, b.seat_type_id, b.quantity, b.total_amount, b.status, b.payment_method, b.booked_at, e.title AS event_title, e.event_date, e.venue, e.city, e.banner_image, st.type_name AS seat_type FROM bookings b JOIN events e ON b.event_id = e.id LEFT JOIN seat_types st ON b.seat_type_id = st.id WHERE b.user_id = 4 ORDER BY b.booked_at DESC'
    );
    console.table(plan1Before.map(r => ({ table: r.table, type: r.type, possible_keys: r.possible_keys, key: r.key, Extra: r.Extra })));

    console.log('\n--- EXPLAIN: Events by organizer ---');
    const [plan2Before] = await conn.query(
        'EXPLAIN SELECT e.id, e.organizer_id, e.category_id, e.title, e.description, e.venue, e.city, e.event_date, e.registration_deadline, e.total_seats, e.available_seats, e.ticket_price, e.banner_image, e.status, e.created_at, e.updated_at, u.full_name AS organizer_name, u.email AS organizer_email, c.name AS category_name, c.icon AS category_icon FROM events e JOIN users u ON e.organizer_id = u.id LEFT JOIN categories c ON e.category_id = c.id WHERE e.organizer_id = 2 ORDER BY e.created_at DESC'
    );
    console.table(plan2Before.map(r => ({ table: r.table, type: r.type, possible_keys: r.possible_keys, key: r.key, Extra: r.Extra })));

    console.log('\n=== APPLYING COMPOSITE INDEXES ===');
    try {
        await conn.query('CREATE INDEX idx_bookings_user_booked ON bookings(user_id, booked_at DESC)');
        console.log('Created index: idx_bookings_user_booked ON bookings(user_id, booked_at DESC)');
    } catch (e) {
        console.log('Index idx_bookings_user_booked note:', e.message);
    }

    try {
        await conn.query('CREATE INDEX idx_events_organizer_created ON events(organizer_id, created_at DESC)');
        console.log('Created index: idx_events_organizer_created ON events(organizer_id, created_at DESC)');
    } catch (e) {
        console.log('Index idx_events_organizer_created note:', e.message);
    }

    console.log('\n=== AFTER COMPOSITE INDEXES ===');

    console.log('\n--- EXPLAIN: Booking history by user ---');
    const [plan1After] = await conn.query(
        'EXPLAIN SELECT b.id, b.user_id, b.event_id, b.seat_type_id, b.quantity, b.total_amount, b.status, b.payment_method, b.booked_at, e.title AS event_title, e.event_date, e.venue, e.city, e.banner_image, st.type_name AS seat_type FROM bookings b JOIN events e ON b.event_id = e.id LEFT JOIN seat_types st ON b.seat_type_id = st.id WHERE b.user_id = 4 ORDER BY b.booked_at DESC'
    );
    console.table(plan1After.map(r => ({ table: r.table, type: r.type, possible_keys: r.possible_keys, key: r.key, Extra: r.Extra })));

    console.log('\n--- EXPLAIN: Events by organizer ---');
    const [plan2After] = await conn.query(
        'EXPLAIN SELECT e.id, e.organizer_id, e.category_id, e.title, e.description, e.venue, e.city, e.event_date, e.registration_deadline, e.total_seats, e.available_seats, e.ticket_price, e.banner_image, e.status, e.created_at, e.updated_at, u.full_name AS organizer_name, u.email AS organizer_email, c.name AS category_name, c.icon AS category_icon FROM events e JOIN users u ON e.organizer_id = u.id LEFT JOIN categories c ON e.category_id = c.id WHERE e.organizer_id = 2 ORDER BY e.created_at DESC'
    );
    console.table(plan2After.map(r => ({ table: r.table, type: r.type, possible_keys: r.possible_keys, key: r.key, Extra: r.Extra })));

    await conn.end();
}

main().catch(console.error);
