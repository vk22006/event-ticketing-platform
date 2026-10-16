/**
 * Transactional Correctness & Capacity Invariant Verification
 */

const { getDb, spawnServer, httpRequest, loginAttendee, killNamedProcesses } = require('./utils');

async function main() {
    console.log('========================================================================');
    console.log('Transactional Correctness & Capacity Invariant Verification (30 Cycles)');
    console.log('========================================================================\n');

    killNamedProcesses();
    const conn = await getDb();
    const TEST_EVENT_ID = 97;

    await conn.query('UPDATE events SET available_seats = total_seats WHERE id = ?', [TEST_EVENT_ID]);
    const [[eventRowBefore]] = await conn.query('SELECT title, total_seats, available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);
    const initialSeats = eventRowBefore.available_seats;
    console.log(`Target Event: ID ${TEST_EVENT_ID} ("${eventRowBefore.title}")`);
    console.log(`Initial State: Total Seats = ${eventRowBefore.total_seats}, Available Seats = ${initialSeats}\n`);

    const server = await spawnServer('axum');

    try {
        const { cookie: authCookie } = await loginAttendee(server.baseUrl);
        console.log('Authenticated attendee session initialized.');

        let attempted = 0, successful = 0, failed = 0;
        const startTime = Date.now();
        const CYCLES = 30;

        for (let i = 1; i <= CYCLES; i++) {
            attempted++;
            try {
                const [[beforeBooking]] = await conn.query('SELECT available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);

                // 1. Fetch payment page for fresh CSRF token
                const paymentPage = await httpRequest({
                    hostname: '127.0.0.1',
                    port: server.port,
                    path: `/bookings/payment?eventId=${TEST_EVENT_ID}&quantity=1`,
                    headers: { Cookie: authCookie },
                });
                const payCsrf = paymentPage.body.match(/name="csrf_token"\s+value="([^"]+)"/)[1];

                // 2. Confirm booking
                const bookBody = `csrf_token=${encodeURIComponent(payCsrf)}&event_id=${TEST_EVENT_ID}&quantity=1`;
                const bookRes = await httpRequest(
                    {
                        hostname: '127.0.0.1',
                        port: server.port,
                        path: '/bookings/confirm',
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/x-www-form-urlencoded',
                            'Content-Length': Buffer.byteLength(bookBody),
                            Cookie: authCookie,
                        },
                    },
                    bookBody
                );

                if (bookRes.statusCode !== 200) throw new Error(`Booking failed with HTTP ${bookRes.statusCode}`);

                const [[afterBooking]] = await conn.query('SELECT available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);
                if (afterBooking.available_seats !== beforeBooking.available_seats - 1) {
                    throw new Error(`Capacity invariant violated! Before=${beforeBooking.available_seats}, After=${afterBooking.available_seats}`);
                }

                // 3. Find created booking ID to cancel
                const [[latestBooking]] = await conn.query(
                    'SELECT id FROM bookings WHERE user_id = 4 AND event_id = ? AND status = "confirmed" ORDER BY id DESC LIMIT 1',
                    [TEST_EVENT_ID]
                );
                if (!latestBooking) throw new Error('Created booking not found in DB');

                // 4. Get cancel CSRF token and cancel
                const detailPage = await httpRequest({
                    hostname: '127.0.0.1',
                    port: server.port,
                    path: `/bookings/${latestBooking.id}`,
                    headers: { Cookie: authCookie },
                });
                const cancelMatch = detailPage.body.match(/name="csrf_token"\s+value="([^"]+)"/);
                const cancelCsrf = cancelMatch ? cancelMatch[1] : payCsrf;

                const cancelBody = `csrf_token=${encodeURIComponent(cancelCsrf)}`;
                const cancelRes = await httpRequest(
                    {
                        hostname: '127.0.0.1',
                        port: server.port,
                        path: `/bookings/${latestBooking.id}/cancel`,
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/x-www-form-urlencoded',
                            'Content-Length': Buffer.byteLength(cancelBody),
                            Cookie: authCookie,
                        },
                    },
                    cancelBody
                );

                if (cancelRes.statusCode !== 303) throw new Error(`Cancellation failed with HTTP ${cancelRes.statusCode}`);

                const [[afterCancel]] = await conn.query('SELECT available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);
                if (afterCancel.available_seats !== beforeBooking.available_seats) {
                    throw new Error(`Capacity not restored! Before=${beforeBooking.available_seats}, AfterCancel=${afterCancel.available_seats}`);
                }

                successful++;
                if (i % 5 === 0 || i === CYCLES) {
                    console.log(`  Cycle ${String(i).padStart(2)}/${CYCLES}: Booked (-1) & Cancelled (+1) -> Available Seats = ${afterCancel.available_seats} (Verified invariant)`);
                }
            } catch (err) {
                failed++;
                console.error(`  Cycle ${i} FAILED:`, err.message);
            }
        }

        const elapsedSec = ((Date.now() - startTime) / 1000).toFixed(2);
        console.log('\n--- Transactional Results Summary ---');
        console.log(`Total Attempted Cycles : ${attempted}`);
        console.log(`Successful Cycles      : ${successful}`);
        console.log(`Failed Cycles          : ${failed}`);
        console.log(`Elapsed Time           : ${elapsedSec}s (${(CYCLES / elapsedSec).toFixed(1)} cycles/sec)`);

        const [[finalEventRow]] = await conn.query('SELECT available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);
        console.log(`\nFinal Capacity Check: Available Seats = ${finalEventRow.available_seats} (Initial = ${initialSeats})`);
        if (finalEventRow.available_seats === initialSeats) {
            console.log('Result: ZERO CAPACITY LEAK. Invariant strictly preserved (100% capacity restored).');
        } else {
            console.error('Result: CAPACITY LEAK DETECTED!');
        }

        await conn.query('DELETE FROM tickets WHERE event_id = ? AND booking_id IN (SELECT id FROM bookings WHERE user_id = 4 AND event_id = ?)', [TEST_EVENT_ID, TEST_EVENT_ID]);
        await conn.query('DELETE FROM bookings WHERE user_id = 4 AND event_id = ?', [TEST_EVENT_ID]);
        console.log('Cleanup: Test-created tickets and bookings cleanly deleted.');
    } finally {
        await server.stop();
        await conn.end();
        killNamedProcesses();
    }
}

main().catch((err) => {
    console.error('Fatal test error:', err);
    process.exit(1);
});
