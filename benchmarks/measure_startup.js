/**
 * Measure Axum startup time from process invocation until first successful HTTP 200 response.
 * Conducts 5 clean trials for the Axum release binary and preserves historical comparison data.
 */

const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');
const { RS_ROOT, DB_CONFIG, waitForUrl, killProcess } = require('./utils');

const TRIALS = 5;
const TIMEOUT_MS = 10000;

async function measureSingleStartup() {
    const cmd = path.join(RS_ROOT, 'target', 'release', 'eventpro.exe');
    const port = 3001;
    const env = {
        ...process.env,
        APP_ENV: 'production',
        PORT: '3001',
        DATABASE_URL: `mysql://${DB_CONFIG.user}:${DB_CONFIG.password}@${DB_CONFIG.host}:${DB_CONFIG.port}/event_lifecycle`,
        SESSION_SECRET: 'benchmark_session_secret_at_least_16_chars_long',
        RUST_LOG: 'warn',
    };

    const checkUrl = `http://127.0.0.1:${port}/events`;
    const t0 = performance.now();
    const child = spawn(cmd, [], { cwd: RS_ROOT, env, stdio: 'ignore', windowsHide: true });

    let ready = false;
    let t1 = 0;
    try {
        ready = await waitForUrl(checkUrl, TIMEOUT_MS, 10);
        t1 = performance.now();
    } finally {
        await killProcess(child);
    }

    if (!ready) throw new Error(`Axum failed to become ready within ${TIMEOUT_MS}ms`);

    const startupMs = Math.round((t1 - t0) * 100) / 100;
    await new Promise((r) => setTimeout(r, 1000));
    return startupMs;
}

function calculateStats(numbers) {
    const n = numbers.length;
    const mean = numbers.reduce((a, b) => a + b, 0) / n;
    const variance = numbers.reduce((sum, val) => sum + Math.pow(val - mean, 2), 0) / n;
    return {
        mean: Math.round(mean * 100) / 100,
        stddev: Math.round(Math.sqrt(variance) * 100) / 100,
        min: Math.round(Math.min(...numbers) * 100) / 100,
        max: Math.round(Math.max(...numbers) * 100) / 100,
        trials: numbers,
    };
}

async function main() {
    console.log('====================================================');
    console.log(`Measuring Axum Process Startup-to-Readiness Latency (${TRIALS} trials)`);
    console.log('====================================================\n');

    const outDir = path.join(__dirname, 'results');
    if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true });
    const outPath = path.join(outDir, 'startup.json');

    let existing = {};
    if (fs.existsSync(outPath)) {
        try { existing = JSON.parse(fs.readFileSync(outPath, 'utf8')); } catch (_) {}
    }

    const times = [];
    for (let i = 1; i <= TRIALS; i++) {
        process.stdout.write(`  Axum Trial ${i}/${TRIALS}... `);
        const ms = await measureSingleStartup();
        times.push(ms);
        console.log(`${ms} ms`);
    }
    const axumStats = calculateStats(times);
    console.log(`\nAxum Mean Startup: ${axumStats.mean} ms (stddev: ${axumStats.stddev} ms)\n`);

    const results = {
        express: existing.express || null,
        axum: axumStats,
        comparison: existing.comparison || null,
    };

    if (results.express && results.express.mean) {
        const reductionPct = Math.round(((results.express.mean - axumStats.mean) / results.express.mean) * 10000) / 100;
        const speedupFactor = Math.round((results.express.mean / axumStats.mean) * 10) / 10;
        results.comparison = { startupReductionPercent: reductionPct, startupSpeedupFactor: speedupFactor };
        console.log('====================================================');
        console.log(`Historical Comparison vs Express (${results.express.mean} ms):`);
        console.log(`Startup Speedup Factor: ${speedupFactor}x faster (${reductionPct}% reduction)`);
        console.log('====================================================\n');
    }

    fs.writeFileSync(outPath, JSON.stringify(results, null, 2));
    console.log(`Startup results written to ${outPath}`);
}

main().catch((err) => {
    console.error('Fatal startup measurement error:', err);
    process.exit(1);
});
