const { getDb } = require('./utils');

async function main() {
    const conn = await getDb();

    const planCols = (rows) => rows.map((r) => ({ table: r.table, type: r.type, possible_keys: r.possible_keys, key: r.key, Extra: r.Extra }));
    const sqlBookings = 'EXPLAIN SELECT b.id, b.user_id, b.event_id, b.seat_type_id, b.quantity, b.total_amount, b.status, b.payment_method, b.booked_at, e.title AS event_title, e.event_date, e.venue, e.city, e.banner_image, st.type_name AS seat_type FROM bookings b JOIN events e ON b.event_id = e.id LEFT JOIN seat_types st ON b.seat_type_id = st.id WHERE b.user_id = 4 ORDER BY b.booked_at DESC';
    const sqlEvents = 'EXPLAIN SELECT e.id, e.organizer_id, e.category_id, e.title, e.description, e.venue, e.city, e.event_date, e.registration_deadline, e.total_seats, e.available_seats, e.ticket_price, e.banner_image, e.status, e.created_at, e.updated_at, u.full_name AS organizer_name, u.email AS organizer_email, c.name AS category_name, c.icon AS category_icon FROM events e JOIN users u ON e.organizer_id = u.id LEFT JOIN categories c ON e.category_id = c.id WHERE e.organizer_id = 2 ORDER BY e.created_at DESC';

    console.log('=== BEFORE COMPOSITE INDEXES ===');
    console.log('\n--- EXPLAIN: Booking history by user ---');
    const [plan1] = await conn.query(sqlBookings);
    console.table(planCols(plan1));

    console.log('\n--- EXPLAIN: Events by organizer ---');
    const [plan2] = await conn.query(sqlEvents);
    console.table(planCols(plan2));

    console.log('\n=== APPLYING COMPOSITE INDEXES ===');
    for (const [name, ddl] of [
        ['idx_bookings_user_booked', 'CREATE INDEX idx_bookings_user_booked ON bookings(user_id, booked_at DESC)'],
        ['idx_events_organizer_created', 'CREATE INDEX idx_events_organizer_created ON events(organizer_id, created_at DESC)'],
    ]) {
        try {
            await conn.query(ddl);
            console.log(`Created index: ${name}`);
        } catch (e) {
            console.log(`Index ${name} note: ${e.message}`);
        }
    }

    console.log('\n=== AFTER COMPOSITE INDEXES ===');
    console.log('\n--- EXPLAIN: Booking history by user ---');
    const [plan1After] = await conn.query(sqlBookings);
    console.table(planCols(plan1After));

    console.log('\n--- EXPLAIN: Events by organizer ---');
    const [plan2After] = await conn.query(sqlEvents);
    console.table(planCols(plan2After));

    await conn.end();
}

main().catch(console.error);
