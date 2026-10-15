/**
 * Phase 1 Baseline Benchmark Suite
 * Evaluates the current Rust/Axum release backend across all specified workloads:
 * - GET /events (concurrencies 1, 10, 25, 50)
 * - GET /events/1 (concurrencies 1, 10, 25, 50)
 * - GET /dashboard (concurrencies 1, 10, 25, 50 with attendee session)
 * - GET /bookings/my (concurrencies 1, 10, 25, 50 with attendee session)
 * - Representative booking & cancellation workflow (transactional cycles)
 *
 * Captures: RPS, p50, p95, p99, errors, Working Set, CPU %,
 * pool acquisition timings, and SQL query timings.
 */

const { spawn, execSync } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const http = require('http');
const autocannon = require('autocannon');
const mysql = require('mysql2/promise');
const os = require('os');

const PROJECT_ROOT = path.resolve(__dirname, '..');
const CWD_DIR = path.resolve(PROJECT_ROOT, 'eventpro-rs');
const BINARY_PATH = path.join(CWD_DIR, 'target', 'release', 'eventpro.exe');
const BASE_URL = 'http://127.0.0.1:3001';
const DURATION_SECS = 10;
const WARMUP_SECS = 3;
const NUM_LOGICAL_CORES = os.cpus().length || 8;

const DB_CONFIG = {
    host: '127.0.0.1',
    port: 3306,
    user: 'root',
    password: 'kishore2006',
    database: 'event_lifecycle',
};

function killRunningEventpro() {
    try {
        execSync('taskkill /F /IM eventpro.exe 2>nul');
    } catch (_) {}
}

function computeSha256(filePath) {
    const data = fs.readFileSync(filePath);
    return crypto.createHash('sha256').update(data).digest('hex').toUpperCase();
}

function getProcessMetrics(pid) {
    if (!pid || pid <= 0) return null;
    try {
        const cmd = `powershell -NoProfile -Command "Get-Process -Id ${pid} -ErrorAction Stop | Select-Object -Property Id, CPU, WorkingSet64, PrivateMemorySize64 | ConvertTo-Json -Compress"`;
        const output = execSync(cmd, { encoding: 'utf8', timeout: 5000 }).trim();
        const data = JSON.parse(output);
        return {
            cpuSeconds: typeof data.CPU === 'number' ? data.CPU : parseFloat(data.CPU || 0),
            workingSetBytes: data.WorkingSet64 || 0,
            privateBytes: data.PrivateMemorySize64 || 0,
            timestamp: Date.now(),
        };
    } catch (_) {
        return null;
    }
}

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

async function waitForServerReady(timeoutMs = 15000) {
    const start = Date.now();
    await new Promise(r => setTimeout(r, 800));
    while (Date.now() - start < timeoutMs) {
        try {
            const resp = await httpRequest({
                hostname: '127.0.0.1',
                port: 3001,
                path: '/health',
                method: 'GET',
                timeout: 1000,
            });
            if (resp.statusCode === 200) {
                return true;
            }
        } catch (_) {}
        await new Promise(r => setTimeout(r, 200));
    }
    throw new Error('Server readiness probe timed out after ' + timeoutMs + ' ms');
}

async function resetDbTelemetry() {
    try {
        await httpRequest({
            hostname: '127.0.0.1',
            port: 3001,
            path: '/metrics/db/reset',
            method: 'POST',
        });
    } catch (e) {
        console.warn('Warning: Failed to reset DB telemetry:', e.message);
    }
}

async function getDbTelemetry() {
    try {
        const resp = await httpRequest({
            hostname: '127.0.0.1',
            port: 3001,
            path: '/metrics/db',
            method: 'GET',
        });
        return JSON.parse(resp.body);
    } catch (_) {
        return null;
    }
}

async function loginAttendee() {
    // 1. GET /auth/login for initial cookie and CSRF token
    const loginPage = await httpRequest({
        hostname: '127.0.0.1',
        port: 3001,
        path: '/auth/login',
        method: 'GET',
    });
    const initialCookie = (loginPage.headers['set-cookie'] || [])[0] || '';
    const csrfMatch = loginPage.body.match(/name="csrf_token"\s+value="([^"]+)"/);
    const csrfToken = csrfMatch ? csrfMatch[1] : '';

    // 2. POST /auth/login with charlie@mail.com credentials
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

function runAutocannon(url, concurrency, testDuration, headers = {}) {
    return new Promise((resolve, reject) => {
        const instance = autocannon({
            url,
            connections: concurrency,
            duration: testDuration,
            pipelining: 1,
            headers,
            excludeErrorStats: false,
        }, (err, results) => {
            if (err) return reject(err);
            resolve(results);
        });
        autocannon.track(instance, { renderProgressBar: false, renderResultsTable: false });
    });
}

async function runBookingWorkflowBenchmark(authCookie, cycles = 30) {
    console.log(`\n--- Running Representative Booking & Cancellation Workflow (${cycles} cycles) ---`);
    const latencies = { book: [], cancel: [] };
    let errors = 0;

    // Use Event 97 (published future event with available seats)
    const TEST_EVENT_ID = 97;
    const conn = await mysql.createConnection(DB_CONFIG);
    const [[initialSeat]] = await conn.query('SELECT available_seats FROM events WHERE id = ?', [TEST_EVENT_ID]);
    console.log(`Initial Event ${TEST_EVENT_ID} seats: available=${initialSeat.available_seats}`);

    const startTime = Date.now();

    for (let i = 0; i < cycles; i++) {
        try {
            // 1. Get payment page and fresh CSRF token
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
                throw new Error(`Booking failed with HTTP ${bookResp.statusCode}: ${bookResp.body.substring(0, 100)}`);
            }
            latencies.book.push(bookElapsed);

            // Extract booking ID from confirmation HTML
            const bookingIdMatch = bookResp.body.match(/<strong>#(\d+)<\/strong>/) ||
                                   bookResp.body.match(/Booking #(\d+)/i) || 
                                   bookResp.body.match(/name="booking_id"\s+value="(\d+)"/);
            
            let bookingId = bookingIdMatch ? parseInt(bookingIdMatch[1], 10) : null;
            if (!bookingId) {
                // Fallback: Query the latest booking by user 4 for this event
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

    const bookStats = getPercentiles(latencies.book);
    const cancelStats = getPercentiles(latencies.cancel);

    return {
        eventId: TEST_EVENT_ID,
        cycles,
        successfulCycles: cycles - errors,
        errors,
        totalElapsedSec: parseFloat(totalElapsedSec.toFixed(2)),
        cycleRps: parseFloat(((cycles - errors) / totalElapsedSec).toFixed(2)),
        seatInvariancePreserved,
        bookLatencies: bookStats,
        cancelLatencies: cancelStats,
    };
}

async function main() {
    console.log('================================================================');
    console.log('Phase 1: Comprehensive Baseline Benchmark');
    console.log('================================================================');

    const binaryHash = computeSha256(BINARY_PATH);
    console.log(`Binary Path : ${BINARY_PATH}`);
    console.log(`Binary SHA256: ${binaryHash}`);

    killRunningEventpro();
    await new Promise(r => setTimeout(r, 1000));

    // Spawn server process
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

    console.log(`Spawned EventPro server PID: ${child.pid}`);

    try {
        await waitForServerReady();
        console.log('Server is healthy and ready to receive traffic.\n');

        // Log in attendee
        const attendeeCookie = await loginAttendee();
        console.log(`Attendee session cookie obtained: ${attendeeCookie.substring(0, 24)}...\n`);

        const workloads = [
            {
                id: 'events_catalogue',
                name: 'GET /events',
                url: `${BASE_URL}/events`,
                headers: {},
                isAuth: false,
            },
            {
                id: 'event_detail',
                name: 'GET /events/1',
                url: `${BASE_URL}/events/1`,
                headers: {},
                isAuth: false,
            },
            {
                id: 'dashboard',
                name: 'GET /dashboard',
                url: `${BASE_URL}/dashboard`,
                headers: { 'Cookie': attendeeCookie },
                isAuth: true,
            },
            {
                id: 'bookings_my',
                name: 'GET /bookings/my',
                url: `${BASE_URL}/bookings/my`,
                headers: { 'Cookie': attendeeCookie },
                isAuth: true,
            },
        ];

        const concurrencies = [1, 10, 25, 50];
        const results = {
            metadata: {
                timestamp: new Date().toISOString(),
                binaryHash,
                binaryPath: BINARY_PATH,
                durationSeconds: DURATION_SECS,
                warmupSeconds: WARMUP_SECS,
                concurrencies,
                poolMax: 10,
                numLogicalCores: NUM_LOGICAL_CORES,
            },
            workloads: {},
        };

        for (const wl of workloads) {
            console.log(`================================================================`);
            console.log(`Benchmarking Workload: ${wl.name} (${wl.isAuth ? 'Authenticated' : 'Public'})`);
            console.log(`================================================================`);

            results.workloads[wl.id] = {
                name: wl.name,
                url: wl.url,
                isAuthenticated: wl.isAuth,
                tiers: {},
            };

            for (const c of concurrencies) {
                console.log(`--> Running Concurrency ${c} VU (Warmup: ${WARMUP_SECS}s, Test: ${DURATION_SECS}s)...`);

                // 1. Warmup
                await runAutocannon(wl.url, c, WARMUP_SECS, wl.headers);

                // 2. Reset telemetry
                await resetDbTelemetry();

                // 3. Baseline resource snapshot
                const startMetrics = getProcessMetrics(child.pid);
                const startTime = Date.now();

                // 4. Measured trial
                const acRes = await runAutocannon(wl.url, c, DURATION_SECS, wl.headers);

                // 5. Post trial resource snapshot & DB telemetry
                const endTime = Date.now();
                const endMetrics = getProcessMetrics(child.pid);
                const dbTelemetry = await getDbTelemetry();

                const elapsedSec = (endTime - startTime) / 1000;
                let cpuPercent = 0;
                if (startMetrics && endMetrics && elapsedSec > 0) {
                    const cpuDelta = endMetrics.cpuSeconds - startMetrics.cpuSeconds;
                    cpuPercent = parseFloat(((cpuDelta / (elapsedSec * NUM_LOGICAL_CORES)) * 100).toFixed(2));
                }
                const workingSetMb = endMetrics ? parseFloat((endMetrics.workingSetBytes / 1024 / 1024).toFixed(2)) : 0;

                const tierData = {
                    concurrency: c,
                    throughputRps: parseFloat(acRes.requests.average.toFixed(1)),
                    totalRequests: acRes.requests.total,
                    errors: acRes.errors,
                    timeouts: acRes.timeouts,
                    non2xx: acRes.non2xx,
                    latency: {
                        p50: acRes.latency.p50,
                        p95: typeof acRes.latency.p95 === 'number'
                            ? acRes.latency.p95
                            : Math.round(((acRes.latency.p90 || 0) + ((acRes.latency.p97_5 || acRes.latency.p99 || 0) - (acRes.latency.p90 || 0)) * (5 / 7.5)) * 10) / 10,
                        p99: acRes.latency.p99,
                        avg: acRes.latency.average,
                    },
                    resources: {
                        cpuPercent,
                        workingSetMb,
                    },
                    dbTelemetry,
                };

                results.workloads[wl.id].tiers[`c_${c}`] = tierData;

                console.log(`    Result: ${tierData.throughputRps} RPS | p50: ${tierData.latency.p50}ms | p95: ${tierData.latency.p95}ms | p99: ${tierData.latency.p99}ms | RAM: ${workingSetMb}MB | CPU: ${cpuPercent}% | Errors: ${tierData.errors + tierData.non2xx}`);
                if (dbTelemetry) {
                    console.log(`    DB: Acquire avg=${dbTelemetry.acquire.avg_ms}ms (max=${dbTelemetry.acquire.max_ms}ms) | Query avg=${dbTelemetry.query.avg_ms}ms (max=${dbTelemetry.query.max_ms}ms) | Queries=${dbTelemetry.query.count}`);
                }
            }
        }

        // Run representative booking & cancellation workflow
        const bookingWorkflow = await runBookingWorkflowBenchmark(attendeeCookie, 30);
        results.bookingWorkflow = bookingWorkflow;

        const outPath = path.join(PROJECT_ROOT, 'benchmarks', 'results', 'phase1_baseline_results.json');
        fs.writeFileSync(outPath, JSON.stringify(results, null, 2), 'utf8');
        console.log(`\nPhase 1 Baseline Benchmark results saved to:\n  ${outPath}`);

    } finally {
        console.log('\nShutting down EventPro server...');
        child.kill();
    }
}

main().catch(err => {
    console.error('Fatal benchmark execution error:', err);
    process.exit(1);
});
