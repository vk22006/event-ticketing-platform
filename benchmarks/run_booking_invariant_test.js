const http = require('http');
const mysql = require('mysql2/promise');
const { spawn, execSync } = require('child_process');
const path = require('path');

const PROJECT_ROOT = path.resolve(__dirname, '..');
const CWD_DIR = path.resolve(PROJECT_ROOT, 'eventpro-rs');
const BINARY_PATH = path.join(CWD_DIR, 'target', 'release', 'eventpro-optimized.exe');
const PORT = 3001;

const DB_CONFIG = {
    host: '127.0.0.1',
    user: 'root',
    password: 'kishore2006',
    database: 'event_lifecycle',
    port: 3306,
};

function req(options, postData = null) {
    return new Promise((resolve, reject) => {
        const r = http.request(options, res => {
            let b = '';
            res.on('data', c => b += c);
            res.on('end', () => resolve({ statusCode: res.statusCode, headers: res.headers, body: b }));
        });
        r.on('error', reject);
        if (postData) r.write(postData);
        r.end();
    });
}

async function main() {
    console.log('========================================================================');
    console.log('Transactional Correctness & Capacity Invariant Verification (30 Cycles)');
    console.log('========================================================================\n');

    try { execSync('powershell -Command "Get-Process -Name \'eventpro*\' -ErrorAction SilentlyContinue | Stop-Process -Force"'); } catch (_) {}

    const conn = await mysql.createConnection(DB_CONFIG);
    const TEST_EVENT_ID = 97;

    await conn.query('UPDATE events SET available_seats = total_seats WHERE id = ?', [TEST_EVENT_ID]);
    const [[eventRowBefore]] = await conn.query('SELECT title, total_seats, available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);
    const initialSeats = eventRowBefore.available_seats;
    console.log(`Target Event: ID ${TEST_EVENT_ID} ("${eventRowBefore.title}")`);
    console.log(`Initial State: Total Seats = ${eventRowBefore.total_seats}, Available Seats = ${initialSeats}\n`);

    const child = spawn(BINARY_PATH, [], {
        cwd: CWD_DIR,
        env: {
            ...process.env,
            APP_ENV: 'production',
            PORT: PORT.toString(),
            DATABASE_URL: 'mysql://root:kishore2006@127.0.0.1:3306/event_lifecycle',
            SESSION_SECRET: 'benchmark_session_secret_at_least_16_chars_long',
            DB_POOL_MAX: '10',
            RUST_LOG: 'warn',
        },
        stdio: 'ignore',
    });

    await new Promise(r => setTimeout(r, 1200));

    try {
        // 1. Login attendee Charlie
        const loginPage = await req({ hostname: '127.0.0.1', port: PORT, path: '/auth/login', method: 'GET' });
        const initCookie = (loginPage.headers['set-cookie'] || [])[0].split(';')[0];
        const csrfToken = loginPage.body.match(/name="csrf_token"\s+value="([^"]+)"/)[1];

        const loginBody = `email=charlie%40mail.com&password=User%401234&csrf_token=${encodeURIComponent(csrfToken)}`;
        const loginRes = await req({
            hostname: '127.0.0.1', port: PORT, path: '/auth/login', method: 'POST',
            headers: {
                'Content-Type': 'application/x-www-form-urlencoded',
                'Content-Length': Buffer.byteLength(loginBody),
                'Cookie': initCookie,
            }
        }, loginBody);

        const authCookie = (loginRes.headers['set-cookie'] || [initCookie])[0].split(';')[0];
        console.log(`Authenticated attendee session initialized.`);

        let attempted = 0;
        let successful = 0;
        let failed = 0;
        const startTime = Date.now();
        const CYCLES = 30;

        for (let i = 1; i <= CYCLES; i++) {
            attempted++;
            try {
                // Step A: Read capacity before booking
                const [[beforeBooking]] = await conn.query('SELECT available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);

                // Step B: Fetch payment page for fresh CSRF token
                const paymentPage = await req({
                    hostname: '127.0.0.1', port: PORT,
                    path: `/bookings/payment?eventId=${TEST_EVENT_ID}&quantity=1`,
                    headers: { 'Cookie': authCookie }
                });
                const payCsrf = paymentPage.body.match(/name="csrf_token"\s+value="([^"]+)"/)[1];

                // Step C: Confirm booking
                const bookBody = `csrf_token=${encodeURIComponent(payCsrf)}&event_id=${TEST_EVENT_ID}&quantity=1`;
                const bookRes = await req({
                    hostname: '127.0.0.1', port: PORT, path: '/bookings/confirm', method: 'POST',
                    headers: {
                        'Content-Type': 'application/x-www-form-urlencoded',
                        'Content-Length': Buffer.byteLength(bookBody),
                        'Cookie': authCookie,
                    }
                }, bookBody);

                if (bookRes.statusCode !== 200) {
                    throw new Error(`Booking failed with HTTP ${bookRes.statusCode}`);
                }

                // Check capacity decremented by exactly 1
                const [[afterBooking]] = await conn.query('SELECT available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);
                if (afterBooking.available_seats !== beforeBooking.available_seats - 1) {
                    throw new Error(`Capacity invariant violated! Before=${beforeBooking.available_seats}, After=${afterBooking.available_seats}`);
                }

                // Step D: Find created booking ID to cancel
                const [[latestBooking]] = await conn.query(
                    'SELECT id FROM bookings WHERE user_id = 4 AND event_id = ? AND status = "confirmed" ORDER BY id DESC LIMIT 1',
                    [TEST_EVENT_ID]
                );
                if (!latestBooking) throw new Error('Created booking not found in DB');

                // Step E: Get cancellation CSRF token from booking detail or page
                const detailPage = await req({
                    hostname: '127.0.0.1', port: PORT,
                    path: `/bookings/${latestBooking.id}`,
                    headers: { 'Cookie': authCookie }
                });
                const cancelCsrfMatch = detailPage.body.match(/name="csrf_token"\s+value="([^"]+)"/);
                const cancelCsrf = cancelCsrfMatch ? cancelCsrfMatch[1] : payCsrf;

                // Step F: Cancel booking
                const cancelBody = `csrf_token=${encodeURIComponent(cancelCsrf)}`;
                const cancelRes = await req({
                    hostname: '127.0.0.1', port: PORT,
                    path: `/bookings/${latestBooking.id}/cancel`,
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/x-www-form-urlencoded',
                        'Content-Length': Buffer.byteLength(cancelBody),
                        'Cookie': authCookie,
                    }
                }, cancelBody);

                if (cancelRes.statusCode !== 303) {
                    throw new Error(`Cancellation failed with HTTP ${cancelRes.statusCode}`);
                }

                // Step G: Verify capacity incremented back by exactly 1
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

        // Final database invariant check
        const [[finalEventRow]] = await conn.query('SELECT available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);
        console.log(`\nFinal Capacity Check: Available Seats = ${finalEventRow.available_seats} (Initial = ${initialSeats})`);
        if (finalEventRow.available_seats === initialSeats) {
            console.log('Result: ZERO CAPACITY LEAK. Invariant strictly preserved (100% capacity restored).');
        } else {
            console.error('Result: CAPACITY LEAK DETECTED!');
        }

        // Clean up any test-created records for charlie on event 97
        await conn.query('DELETE FROM tickets WHERE event_id = ? AND booking_id IN (SELECT id FROM bookings WHERE user_id = 4 AND event_id = ?)', [TEST_EVENT_ID, TEST_EVENT_ID]);
        await conn.query('DELETE FROM bookings WHERE user_id = 4 AND event_id = ?', [TEST_EVENT_ID]);
        console.log('Cleanup: Test-created tickets and bookings cleanly deleted.');

    } finally {
        child.kill();
        await conn.end();
        try { execSync('powershell -Command "Get-Process -Name \'eventpro*\' -ErrorAction SilentlyContinue | Stop-Process -Force"'); } catch (_) {}
    }
}

main().catch(err => {
    console.error('Fatal test error:', err);
    process.exit(1);
});
