/**
 * Phase 3 Validation: Alternating Multi-Trial Benchmark Suite
 * Compares Baseline vs Optimized binary across 5 alternating rounds (AB, BA, AB, BA, AB)
 */

const fs = require('fs');
const path = require('path');
const os = require('os');
const {
    PROJECT_ROOT,
    RS_ROOT,
    computeSha256,
    getProcessMetrics,
    loginAttendee,
    runAutocannon,
    resetDbTelemetry,
    getDbTelemetry,
    spawnServer,
    killNamedProcesses,
} = require('./utils');

const BASELINE_BIN = path.join(RS_ROOT, 'target', 'release', 'eventpro-baseline.exe');
const OPTIMIZED_BIN = path.join(RS_ROOT, 'target', 'release', 'eventpro-optimized.exe');
const PORT = 3001;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const DURATION_SECS = 2;
const WARMUP_SECS = 1;
const NUM_LOGICAL_CORES = os.cpus().length || 8;

async function evaluateBinary(binPath, binLabel, workloads, concurrencyList) {
    killNamedProcesses();
    await new Promise((r) => setTimeout(r, 600));

    const server = await spawnServer('axum', { binPath, port: PORT });
    try {
        const { cookie } = await loginAttendee(BASE_URL);
        const runData = {};

        for (const wl of workloads) {
            runData[wl.id] = {};
            const headers = wl.requiresAuth ? { Cookie: cookie } : {};

            for (const c of concurrencyList) {
                await runAutocannon(wl.url, c, WARMUP_SECS, headers);
                await resetDbTelemetry(PORT);

                const startMetrics = getProcessMetrics(server.child.pid);
                const startTime = Date.now();
                process.stdout.write(`    [${binLabel.padEnd(9)}] ${wl.name.padEnd(16)} (C=${String(c).padEnd(2)})... `);
                const acRes = await runAutocannon(wl.url, c, DURATION_SECS, headers);

                const endTime = Date.now();
                const endMetrics = getProcessMetrics(server.child.pid);
                const dbTelemetry = await getDbTelemetry(PORT);
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
        await server.stop();
        killNamedProcesses();
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

    const concurrencies = [1, 10, 25, 50];
    const NUM_ROUNDS = 5;
    const allResults = {
        timestamp: new Date().toISOString(),
        baselineBinary: { path: BASELINE_BIN, sha256: baseHash },
        optimizedBinary: { path: OPTIMIZED_BIN, sha256: optHash },
        testParameters: { durationSecs: DURATION_SECS, warmupSecs: WARMUP_SECS, rounds: NUM_ROUNDS, concurrencies },
        rounds: [],
    };

    for (let r = 1; r <= NUM_ROUNDS; r++) {
        console.log(`\n======================================================`);
        console.log(`ROUND ${r} of ${NUM_ROUNDS} (Alternating Order)`);
        console.log(`======================================================`);

        const firstIsBase = r % 2 === 1;
        const order = firstIsBase
            ? [{ bin: BASELINE_BIN, label: 'baseline' }, { bin: OPTIMIZED_BIN, label: 'optimized' }]
            : [{ bin: OPTIMIZED_BIN, label: 'optimized' }, { bin: BASELINE_BIN, label: 'baseline' }];

        const roundResult = { round: r, order: order.map((o) => o.label), data: {} };
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

main().catch((err) => {
    console.error('Fatal benchmark execution error:', err);
    process.exit(1);
});
