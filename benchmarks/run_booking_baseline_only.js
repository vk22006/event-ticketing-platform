const { spawn, execSync } = require('child_process');
const path = require('path');
const fs = require('fs');
const http = require('http');
const mysql = require('mysql2/promise');

const PROJECT_ROOT = path.resolve(__dirname, '..');
const CWD_DIR = path.resolve(PROJECT_ROOT, 'eventpro-rs');
const BINARY_PATH = path.join(CWD_DIR, 'target', 'release', 'eventpro.exe');

const DB_CONFIG = {
    host: '127.0.0.1',
    port: 3306,
    user: 'root',
    password: 'kishore2006',
    database: 'event_lifecycle',
};

function httpRequest(options, postData = null) {
    return new Promise((resolve, reject) => {
        const req = http.request(options, (res) => {
            let body = '';
            res.on('data', chunk => body += chunk);
            res.on('end', () => resolve({ statusCode: res.statusCode, headers: res.headers, body }));
        });
        req.on('error', reject);
        if (postData) {
            req.write(postData);
        }
        req.end();
    });
}

async function loginAttendee() {
    const loginPage = await httpRequest({
        hostname: '127.0.0.1',
        port: 3001,
        path: '/auth/login',
        method: 'GET',
    });
    const initialCookie = (loginPage.headers['set-cookie'] || [])[0] || '';
    const csrfMatch = loginPage.body.match(/name="csrf_token"\s+value="([^"]+)"/);
    const csrfToken = csrfMatch ? csrfMatch[1] : '';

    const postBody = `email=charlie%40mail.com&password=User%401234&csrf_token=${encodeURIComponent(csrfToken)}`;
    const loginResp = await httpRequest({
        hostname: '127.0.0.1',
        port: 3001,
        path: '/auth/login',
        method: 'POST',
        headers: {
            'Content-Type': 'application/x-www-form-urlencoded',
            'Content-Length': Buffer.byteLength(postBody),
            'Cookie': initialCookie.split(';')[0],
        },
    }, postBody);

    const authCookie = (loginResp.headers['set-cookie'] || [])[0] || initialCookie;
    return authCookie.split(';')[0];
}

async function main() {
    try { execSync('taskkill /F /IM eventpro.exe 2>nul'); } catch (_) {}
    await new Promise(r => setTimeout(r, 1000));

    const child = spawn(BINARY_PATH, [], {
        cwd: CWD_DIR,
        env: {
            ...process.env,
            APP_ENV: 'production',
            PORT: '3001',
            RUST_LOG: 'warn',
            DB_POOL_MAX: '10',
            BASELINE_CSRF: '0',
        },
        stdio: 'ignore',
    });

    await new Promise(r => setTimeout(r, 1500));

    try {
        const authCookie = await loginAttendee();
        console.log('Logged in attendee:', authCookie.substring(0, 20));

        const TEST_EVENT_ID = 97;
        const cycles = 30;
        const latencies = { book: [], cancel: [] };
        let errors = 0;

        const conn = await mysql.createConnection(DB_CONFIG);
        const [[initialSeat]] = await conn.query('SELECT available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);
        console.log(`Initial Event ${TEST_EVENT_ID} seats: available=${initialSeat.available_seats}`);

        const startTime = Date.now();

        for (let i = 0; i < cycles; i++) {
            try {
                // 1. Get payment review and fresh CSRF token
                const paymentPage = await httpRequest({
                    hostname: '127.0.0.1',
                    port: 3001,
                    path: `/bookings/payment?eventId=${TEST_EVENT_ID}&quantity=1`,
                    method: 'GET',
                    headers: { 'Cookie': authCookie },
                });

                const csrfMatch = paymentPage.body.match(/name="csrf_token"\s+value="([^"]+)"/);
                const csrfToken = csrfMatch ? csrfMatch[1] : '';
                if (!csrfToken) throw new Error('Failed to extract CSRF token on payment review');

                // 2. POST /bookings/confirm
                const bookStart = Date.now();
                const bookBody = `csrf_token=${encodeURIComponent(csrfToken)}&event_id=${TEST_EVENT_ID}&quantity=1`;
                const bookResp = await httpRequest({
                    hostname: '127.0.0.1',
                    port: 3001,
                    path: '/bookings/confirm',
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/x-www-form-urlencoded',
                        'Content-Length': Buffer.byteLength(bookBody),
                        'Cookie': authCookie,
                    },
                }, bookBody);
                const bookElapsed = Date.now() - bookStart;

                if (bookResp.statusCode !== 200) {
                    throw new Error(`Booking failed with HTTP ${bookResp.statusCode}`);
                }
                latencies.book.push(bookElapsed);

                const bookingIdMatch = bookResp.body.match(/<strong>#(\d+)<\/strong>/) ||
                                       bookResp.body.match(/Booking #(\d+)/i) || 
                                       bookResp.body.match(/name="booking_id"\s+value="(\d+)"/);
                
                let bookingId = bookingIdMatch ? parseInt(bookingIdMatch[1], 10) : null;
                if (!bookingId) {
                    const [[latestBkg]] = await conn.query('SELECT id FROM bookings WHERE user_id = 4 AND event_id = ? ORDER BY id DESC LIMIT 1', [TEST_EVENT_ID]);
                    bookingId = latestBkg ? latestBkg.id : null;
                }
                if (!bookingId) throw new Error('Failed to determine created booking ID');

                // 3. POST /bookings/:id/cancel
                const cancelStart = Date.now();
                const cancelBody = `csrf_token=${encodeURIComponent(csrfToken)}`;
                const cancelResp = await httpRequest({
                    hostname: '127.0.0.1',
                    port: 3001,
                    path: `/bookings/${bookingId}/cancel`,
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/x-www-form-urlencoded',
                        'Content-Length': Buffer.byteLength(cancelBody),
                        'Cookie': authCookie,
                    },
                }, cancelBody);
                const cancelElapsed = Date.now() - cancelStart;

                if (cancelResp.statusCode !== 303 && cancelResp.statusCode !== 302 && cancelResp.statusCode !== 200) {
                    throw new Error(`Cancellation failed with HTTP ${cancelResp.statusCode}`);
                }
                latencies.cancel.push(cancelElapsed);
            } catch (err) {
                errors++;
                console.error(`Cycle ${i + 1} error:`, err.message);
            }
        }

        const totalElapsedSec = (Date.now() - startTime) / 1000;
        const [[finalSeat]] = await conn.query('SELECT available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);
        await conn.end();

        console.log(`Final Event ${TEST_EVENT_ID} seats  : available=${finalSeat.available_seats}`);
        const seatInvariancePreserved = (initialSeat.available_seats === finalSeat.available_seats);
        console.log(`Seat invariance check: ${seatInvariancePreserved ? 'PASSED (0 leaks)' : 'FAILED'}`);

        function getPercentiles(arr) {
            if (!arr.length) return { p50: 0, p95: 0, p99: 0, avg: 0 };
            const sorted = [...arr].sort((a, b) => a - b);
            return {
                p50: sorted[Math.floor(sorted.length * 0.50)],
                p95: sorted[Math.floor(sorted.length * 0.95)],
                p99: sorted[Math.floor(sorted.length * 0.99)],
                avg: parseFloat((sorted.reduce((a, b) => a + b, 0) / sorted.length).toFixed(2)),
            };
        }

        const bookingWorkflow = {
            eventId: TEST_EVENT_ID,
            cycles,
            successfulCycles: cycles - errors,
            errors,
            totalElapsedSec: parseFloat(totalElapsedSec.toFixed(2)),
            cycleRps: parseFloat(((cycles - errors) / totalElapsedSec).toFixed(2)),
            seatInvariancePreserved,
            bookLatencies: getPercentiles(latencies.book),
            cancelLatencies: getPercentiles(latencies.cancel),
        };

        console.log('Booking workflow stats:', JSON.stringify(bookingWorkflow, null, 2));

        // Update JSON file
        const outPath = path.join(PROJECT_ROOT, 'benchmarks', 'results', 'phase1_baseline_results.json');
        if (fs.existsSync(outPath)) {
            const data = JSON.parse(fs.readFileSync(outPath, 'utf8'));
            data.bookingWorkflow = bookingWorkflow;
            fs.writeFileSync(outPath, JSON.stringify(data, null, 2), 'utf8');
            console.log('Updated phase1_baseline_results.json with bookingWorkflow');
        }

    } finally {
        child.kill();
    }
}

main().catch(console.error);
