/**
 * Measure startup time from process invocation until first successful HTTP 200 response.
 * Conducts 5 clean trials for both Express (Node.js) and Axum (Rust release).
 */

const { spawn } = require('child_process');
const http = require('http');
const path = require('path');
const fs = require('fs');

const TRIALS = 5;
const POLL_INTERVAL_MS = 10;
const TIMEOUT_MS = 10000;

function pollUrl(url) {
    return new Promise((resolve) => {
        const req = http.get(url, (res) => {
            if (res.statusCode >= 200 && res.statusCode < 400) {
                resolve(true);
            } else {
                resolve(false);
            }
        });
        req.on('error', () => resolve(false));
        req.setTimeout(500, () => {
            req.destroy();
            resolve(false);
        });
    });
}

async function waitForReady(url, maxTimeoutMs) {
    const start = Date.now();
    while (Date.now() - start < maxTimeoutMs) {
        const ok = await pollUrl(url);
        if (ok) return true;
        await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
    }
    return false;
}

async function killProcessTree(child) {
    return new Promise((resolve) => {
        if (!child || !child.pid) return resolve();
        try {
            process.kill(child.pid, 'SIGINT');
        } catch (_) {}
        setTimeout(() => {
            try {
                process.kill(child.pid, 'SIGKILL');
            } catch (_) {}
            resolve();
        }, 300);
    });
}

async function measureSingleStartup(type) {
    const isExpress = type === 'express';
    const cwd = isExpress
        ? path.resolve(__dirname, '..')
        : path.resolve(__dirname, '..', 'eventpro-rs');

    const cmd = isExpress ? 'node' : path.join(cwd, 'target', 'release', 'eventpro.exe');
    const args = isExpress ? ['app.js'] : [];
    const env = Object.assign({}, process.env, isExpress ? {
        NODE_ENV: 'production',
        PORT: '3000',
    } : {
        APP_ENV: 'production',
        PORT: '3001',
    });

    const checkUrl = isExpress ? 'http://127.0.0.1:3000/events' : 'http://127.0.0.1:3001/events';

    const t0 = performance.now();
    const child = spawn(cmd, args, {
        cwd,
        env,
        stdio: 'ignore',
        windowsHide: true,
    });

    let ready = false;
    let t1 = 0;

    try {
        ready = await waitForReady(checkUrl, TIMEOUT_MS);
        t1 = performance.now();
    } finally {
        await killProcessTree(child);
    }

    if (!ready) {
        throw new Error(`${type} failed to become ready within ${TIMEOUT_MS}ms`);
    }

    const startupMs = Math.round((t1 - t0) * 100) / 100;
    // Grace period for OS socket teardown
    await new Promise((r) => setTimeout(r, 1200));
    return startupMs;
}

function calculateStats(numbers) {
    const n = numbers.length;
    const mean = numbers.reduce((a, b) => a + b, 0) / n;
    const variance = numbers.reduce((sum, val) => sum + Math.pow(val - mean, 2), 0) / n;
    const stddev = Math.sqrt(variance);
    const min = Math.min(...numbers);
    const max = Math.max(...numbers);
    return {
        mean: Math.round(mean * 100) / 100,
        stddev: Math.round(stddev * 100) / 100,
        min: Math.round(min * 100) / 100,
        max: Math.round(max * 100) / 100,
        trials: numbers,
    };
}

async function main() {
    console.log('====================================================');
    console.log('Measuring Process Startup-to-Readiness Latency');
    console.log(`Trials per target: ${TRIALS}`);
    console.log('====================================================\n');

    const results = {};

    console.log('--- Benchmarking Express.js Startup ---');
    const expressTimes = [];
    for (let i = 1; i <= TRIALS; i++) {
        process.stdout.write(`  Express Trial ${i}/${TRIALS}... `);
        const ms = await measureSingleStartup('express');
        expressTimes.push(ms);
        console.log(`${ms} ms`);
    }
    results.express = calculateStats(expressTimes);
    console.log(`Express Mean Startup: ${results.express.mean} ms (stddev: ${results.express.stddev} ms)\n`);

    console.log('--- Benchmarking Axum (Rust Release) Startup ---');
    const axumTimes = [];
    for (let i = 1; i <= TRIALS; i++) {
        process.stdout.write(`  Axum Trial ${i}/${TRIALS}... `);
        const ms = await measureSingleStartup('axum');
        axumTimes.push(ms);
        console.log(`${ms} ms`);
    }
    results.axum = calculateStats(axumTimes);
    console.log(`Axum Mean Startup: ${results.axum.mean} ms (stddev: ${results.axum.stddev} ms)\n`);

    const reductionPct = Math.round(((results.express.mean - results.axum.mean) / results.express.mean) * 10000) / 100;
    const speedupFactor = Math.round((results.express.mean / results.axum.mean) * 10) / 10;
    results.comparison = {
        startupReductionPercent: reductionPct,
        startupSpeedupFactor: speedupFactor,
    };

    console.log('====================================================');
    console.log(`Startup Speedup Factor: ${speedupFactor}x faster (${reductionPct}% reduction)`);
    console.log('====================================================\n');

    const outDir = path.join(__dirname, 'results');
    if (!fs.existsSync(outDir)) {
        fs.mkdirSync(outDir, { recursive: true });
    }
    const outPath = path.join(outDir, 'startup.json');
    fs.writeFileSync(outPath, JSON.stringify(results, null, 2));
    console.log(`Startup results written to ${outPath}`);
}

main().catch((err) => {
    console.error('Fatal startup measurement error:', err);
    process.exit(1);
});
