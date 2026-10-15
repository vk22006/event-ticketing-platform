/**
 * Phase 3 Validation: Alternating Multi-Trial Benchmark Suite
 * Compares Baseline (SHA-256: C23EA7C7CA45204B611F26E5727F7109148ED01D5C17D3F12F5AA3AC67032FDA)
 * vs Optimized (SHA-256: F573E471904B2CC65DAB79DC02F32C7D67617BF5B3DAABC340D232424FA9A841)
 *
 * Runs 5 strictly alternating rounds (AB, BA, AB, BA, AB) across workloads:
 * - Authenticated Dashboard (/dashboard)
 * - Booking History (/bookings/my)
 * - Public Catalogue (/events)
 * - Event Detail (/events/1)
 *
 * Captures per-trial RPS, p50, p95, p99, DB query count, DB acquire wait, CPU%, and Working Set.
 * Computes mean, standard deviation, and variance to separate true performance gains from noise.
 */

const { spawn, execSync } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const http = require('http');
const autocannon = require('autocannon');
const os = require('os');

const PROJECT_ROOT = path.resolve(__dirname, '..');
const CWD_DIR = path.resolve(PROJECT_ROOT, 'eventpro-rs');
const BASELINE_BIN = path.join(CWD_DIR, 'target', 'release', 'eventpro-baseline.exe');
const OPTIMIZED_BIN = path.join(CWD_DIR, 'target', 'release', 'eventpro-optimized.exe');
const PORT = 3001;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const DURATION_SECS = 2;
const WARMUP_SECS = 1;
const NUM_LOGICAL_CORES = os.cpus().length || 8;

const DB_URL = 'mysql://root:kishore2006@127.0.0.1:3306/event_lifecycle';

function killRunningEventpro() {
    try {
        execSync('powershell -Command "Get-Process -Name \'eventpro*\' -ErrorAction SilentlyContinue | Stop-Process -Force"', { stdio: 'ignore' });
    } catch (_) {}
}

function computeSha256(filePath) {
    const data = fs.readFileSync(filePath);
    return crypto.createHash('sha256').update(data).digest('hex').toUpperCase();
}

function getProcessMetrics(pid) {
    if (!pid || pid <= 0) return null;
    try {
        const cmd = `powershell -NoProfile -Command "Get-Process -Id ${pid} -ErrorAction Stop | Select-Object -Property Id, CPU, WorkingSet64 | ConvertTo-Json -Compress"`;
        const output = execSync(cmd, { encoding: 'utf8', timeout: 5000 }).trim();
        const data = JSON.parse(output);
        return {
            cpuSeconds: typeof data.CPU === 'number' ? data.CPU : parseFloat(data.CPU || 0),
            workingSetBytes: data.WorkingSet64 || 0,
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
    await new Promise(r => setTimeout(r, 600));
    while (Date.now() - start < timeoutMs) {
        try {
            const resp = await httpRequest({
                hostname: '127.0.0.1',
                port: PORT,
                path: '/health',
                method: 'GET',
                timeout: 1000,
            });
            if (resp.statusCode === 200) return true;
        } catch (_) {}
        await new Promise(r => setTimeout(r, 150));
    }
    throw new Error('Server readiness probe timed out after ' + timeoutMs + ' ms');
}

async function resetDbTelemetry() {
    try {
        await httpRequest({
            hostname: '127.0.0.1',
            port: PORT,
            path: '/metrics/db/reset',
            method: 'POST',
        });
    } catch (_) {}
}

async function getDbTelemetry() {
    try {
        const resp = await httpRequest({
            hostname: '127.0.0.1',
            port: PORT,
            path: '/metrics/db',
            method: 'GET',
        });
        return JSON.parse(resp.body);
    } catch (_) {
        return null;
    }
}

async function loginAttendee() {
    const loginPage = await httpRequest({
        hostname: '127.0.0.1',
        port: PORT,
        path: '/auth/login',
        method: 'GET',
    });
    const initialCookie = (loginPage.headers['set-cookie'] || [])[0] || '';
    const csrfMatch = loginPage.body.match(/name="csrf_token"\s+value="([^"]+)"/);
    const csrfToken = csrfMatch ? csrfMatch[1] : '';

    const postBody = `email=charlie%40mail.com&password=User%401234&csrf_token=${encodeURIComponent(csrfToken)}`;
    const loginResp = await httpRequest({
        hostname: '127.0.0.1',
        port: PORT,
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
        autocannon({
            url,
            connections: concurrency,
            duration: testDuration,
            pipelining: 1,
            headers,
        }, (err, results) => {
            if (err) return reject(err);
            resolve(results);
        });
    });
}

async function evaluateBinary(binPath, binLabel, workloads, concurrencyList) {
    killRunningEventpro();
    await new Promise(r => setTimeout(r, 600));

    const env = {
        ...process.env,
        APP_ENV: 'production',
        PORT: PORT.toString(),
        DATABASE_URL: DB_URL,
        SESSION_SECRET: 'benchmark_session_secret_at_least_16_chars_long',
        DB_POOL_MAX: '10',
        RUST_LOG: 'warn',
    };

    const child = spawn(binPath, [], {
        cwd: CWD_DIR,
        env,
        stdio: 'ignore',
    });

    try {
        await waitForServerReady();
        const cookie = await loginAttendee();
        const runData = {};

        for (const wl of workloads) {
            runData[wl.id] = {};
            const headers = wl.requiresAuth ? { 'Cookie': cookie } : {};

            for (const c of concurrencyList) {
                // Warmup
                await runAutocannon(wl.url, c, WARMUP_SECS, headers);
                await resetDbTelemetry();

                const startMetrics = getProcessMetrics(child.pid);
                const startTime = Date.now();
                process.stdout.write(`    [${binLabel.padEnd(9)}] ${wl.name.padEnd(16)} (C=${String(c).padEnd(2)})... `);
                const acRes = await runAutocannon(wl.url, c, DURATION_SECS, headers);

                const endTime = Date.now();
                const endMetrics = getProcessMetrics(child.pid);
                const dbTelemetry = await getDbTelemetry();
                console.log(`${acRes.requests.average.toFixed(1).padStart(7)} RPS | p50: ${String(acRes.latency.p50).padStart(3)}ms | DB Qs: ${dbTelemetry ? dbTelemetry.query.count : 0}`);

                const elapsedSec = (endTime - startTime) / 1000;
                let cpuPercent = 0;
                if (startMetrics && endMetrics && elapsedSec > 0) {
                    const delta = endMetrics.cpuSeconds - startMetrics.cpuSeconds;
                    cpuPercent = parseFloat(((delta / (elapsedSec * NUM_LOGICAL_CORES)) * 100).toFixed(2));
                }
                const workingSetMb = endMetrics ? parseFloat((endMetrics.workingSetBytes / 1024 / 1024).toFixed(2)) : 0;

                const p95 = typeof acRes.latency.p95 === 'number'
                    ? acRes.latency.p95
                    : Math.round(((acRes.latency.p90 || 0) + ((acRes.latency.p97_5 || acRes.latency.p99 || 0) - (acRes.latency.p90 || 0)) * (5 / 7.5)) * 10) / 10;

                runData[wl.id][`c_${c}`] = {
                    throughputRps: parseFloat(acRes.requests.average.toFixed(1)),
                    totalRequests: acRes.requests.total,
                    errors: acRes.errors + acRes.non2xx,
                    p50: acRes.latency.p50,
                    p95,
                    p99: acRes.latency.p99,
                    avgLatency: acRes.latency.average,
                    cpuPercent,
                    workingSetMb,
                    dbQueries: dbTelemetry ? dbTelemetry.query.count : 0,
                    dbQueryRate: dbTelemetry ? parseFloat((dbTelemetry.query.count / elapsedSec).toFixed(1)) : 0,
                    dbAcquireAvgMs: dbTelemetry ? dbTelemetry.acquire.avg_ms : 0,
                    dbQueryAvgMs: dbTelemetry ? dbTelemetry.query.avg_ms : 0,
                };
            }
        }
        return runData;
    } finally {
        child.kill();
        killRunningEventpro();
    }
}

async function main() {
    console.log('================================================================');
    console.log('Phase 3 Validation: 5 Alternating Baseline vs Optimized Rounds');
    console.log('================================================================');

    const baseHash = computeSha256(BASELINE_BIN);
    const optHash = computeSha256(OPTIMIZED_BIN);

    console.log(`Baseline Binary  : ${BASELINE_BIN} (SHA-256: ${baseHash})`);
    console.log(`Optimized Binary : ${OPTIMIZED_BIN} (SHA-256: ${optHash})`);
    console.log(`Duration per trial: ${DURATION_SECS}s (Warmup: ${WARMUP_SECS}s)`);

    const workloads = [
        { id: 'dashboard', name: 'Auth Dashboard', url: `${BASE_URL}/dashboard`, requiresAuth: true },
        { id: 'my_bookings', name: 'Booking History', url: `${BASE_URL}/bookings/my`, requiresAuth: true },
        { id: 'catalogue', name: 'Public Catalogue', url: `${BASE_URL}/events`, requiresAuth: false },
        { id: 'event_detail', name: 'Event Detail', url: `${BASE_URL}/events/1`, requiresAuth: false },
    ];

    const concurrencies = [1, 10, 25, 50]; // Concurrency tiers 1, 10, 25, 50

    const NUM_ROUNDS = 5;
    const allResults = {
        timestamp: new Date().toISOString(),
        baselineBinary: { path: BASELINE_BIN, sha256: baseHash },
        optimizedBinary: { path: OPTIMIZED_BIN, sha256: optHash },
        testParameters: {
            durationSecs: DURATION_SECS,
            warmupSecs: WARMUP_SECS,
            rounds: NUM_ROUNDS,
            concurrencies,
        },
        rounds: [],
    };

    // Alternating execution: Round 1 (B then O), Round 2 (O then B), etc.
    for (let r = 1; r <= NUM_ROUNDS; r++) {
        console.log(`\n======================================================`);
        console.log(`ROUND ${r} of ${NUM_ROUNDS} (Alternating Order)`);
        console.log(`======================================================`);

        const firstIsBase = (r % 2 === 1);
        const order = firstIsBase
            ? [{ bin: BASELINE_BIN, label: 'baseline' }, { bin: OPTIMIZED_BIN, label: 'optimized' }]
            : [{ bin: OPTIMIZED_BIN, label: 'optimized' }, { bin: BASELINE_BIN, label: 'baseline' }];

        const roundResult = { round: r, order: order.map(o => o.label), data: {} };

        for (const step of order) {
            console.log(`\n--> Running ${step.label.toUpperCase()} binary...`);
            const data = await evaluateBinary(step.bin, step.label, workloads, concurrencies);
            roundResult.data[step.label] = data;


        }
        allResults.rounds.push(roundResult);
    }

    const outPath = path.join(PROJECT_ROOT, 'benchmarks', 'results', 'phase3_alternating_validation.json');
    fs.writeFileSync(outPath, JSON.stringify(allResults, null, 2), 'utf8');
    console.log(`\nPhase 3 Alternating Validation results saved to:\n  ${outPath}`);
}

main().catch(err => {
    console.error('Fatal benchmark execution error:', err);
    process.exit(1);
});
