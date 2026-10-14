/**
 * EventPro Benchmark Harness
 * Cross-platform load testing and resource monitoring for Express.js vs Rust/Axum.
 *
 * Uses autocannon for high-performance HTTP load generation and PowerShell Get-Process
 * for accurate, process-isolated CPU and memory metrics on Windows.
 */

const autocannon = require('autocannon');
const http = require('http');
const https = require('https');
const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const os = require('os');

// Parse CLI arguments
const args = process.argv.slice(2);
function getArg(name, defaultValue) {
    const idx = args.indexOf(`--${name}`);
    if (idx !== -1 && args[idx + 1] && !args[idx + 1].startsWith('--')) {
        return args[idx + 1];
    }
    return defaultValue;
}

const target = getArg('target', 'express').toLowerCase();
const baseUrl = getArg('url', target === 'express' ? 'http://127.0.0.1:3000' : 'http://127.0.0.1:3001');
const targetPid = parseInt(getArg('pid', '0'), 10);
const concurrencyList = getArg('concurrencies', '1,10,25,50')
    .split(',')
    .map(c => parseInt(c.trim(), 10))
    .filter(c => !isNaN(c) && c > 0);
const duration = parseInt(getArg('duration', '10'), 10);
const warmupDuration = parseInt(getArg('warmup', '3'), 10);
const runsCount = parseInt(getArg('runs', '3'), 10);
const routeFilter = getArg('route', null);

const numLogicalCores = os.cpus().length || 8;

console.log('====================================================');
console.log(`EventPro Benchmark Harness: ${target.toUpperCase()}`);
console.log(`Base URL       : ${baseUrl}`);
console.log(`Target PID     : ${targetPid || 'Not specified (skipping resource metrics)'}`);
console.log(`Logical Cores  : ${numLogicalCores}`);
console.log(`Concurrencies  : ${concurrencyList.join(', ')}`);
console.log(`Duration / Run : ${duration}s (Warmup: ${warmupDuration}s, Runs: ${runsCount})`);
console.log('====================================================\n');

// Sampling process metrics
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

// Simple HTTP client helper for authentication setup
function httpRequest(options, postData = null) {
    return new Promise((resolve, reject) => {
        const client = options.protocol === 'https:' ? https : http;
        const req = client.request(options, (res) => {
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

// Establish attendee session cookie
async function getAttendeeCookie() {
    const urlObj = new URL(baseUrl);
    console.log(`[Auth] Establishing attendee session at ${baseUrl}...`);

    try {
        if (target === 'axum') {
            // 1. GET /auth/login for initial cookie and CSRF token
            const loginPage = await httpRequest({
                protocol: urlObj.protocol,
                hostname: urlObj.hostname,
                port: urlObj.port,
                path: '/auth/login',
                method: 'GET',
            });

            const initialCookie = (loginPage.headers['set-cookie'] || [])[0] || '';
            const csrfMatch = loginPage.body.match(/name="csrf_token"\s+value="([^"]+)"/);
            const csrfToken = csrfMatch ? csrfMatch[1] : '';

            // 2. POST /auth/login with credentials
            const postBody = `email=charlie%40mail.com&password=User%401234&csrf_token=${encodeURIComponent(csrfToken)}`;
            const loginResp = await httpRequest({
                protocol: urlObj.protocol,
                hostname: urlObj.hostname,
                port: urlObj.port,
                path: '/auth/login',
                method: 'POST',
                headers: {
                    'Content-Type': 'application/x-www-form-urlencoded',
                    'Content-Length': Buffer.byteLength(postBody),
                    'Cookie': initialCookie.split(';')[0],
                },
            }, postBody);

            const authCookie = (loginResp.headers['set-cookie'] || [])[0] || initialCookie;
            const cookieValue = authCookie.split(';')[0];
            console.log(`[Auth] Axum attendee session established.`);
            return cookieValue;
        } else {
            // Express: POST /auth/login
            const postBody = `email=charlie%40mail.com&password=User%401234`;
            let loginResp = await httpRequest({
                protocol: urlObj.protocol,
                hostname: urlObj.hostname,
                port: urlObj.port,
                path: '/auth/login',
                method: 'POST',
                headers: {
                    'Content-Type': 'application/x-www-form-urlencoded',
                    'Content-Length': Buffer.byteLength(postBody),
                },
            }, postBody);

            if (loginResp.statusCode === 401 || (loginResp.body && loginResp.body.includes('Invalid email or password'))) {
                // Fallback to legacy password
                const fallbackBody = `email=user%40eventpro.com&password=user123`;
                loginResp = await httpRequest({
                    protocol: urlObj.protocol,
                    hostname: urlObj.hostname,
                    port: urlObj.port,
                    path: '/auth/login',
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/x-www-form-urlencoded',
                        'Content-Length': Buffer.byteLength(fallbackBody),
                    },
                }, fallbackBody);
            }

            const authCookie = (loginResp.headers['set-cookie'] || [])[0] || '';
            const cookieValue = authCookie.split(';')[0];
            console.log(`[Auth] Express attendee session established.`);
            return cookieValue;
        }
    } catch (err) {
        console.warn(`[Auth Warning] Failed to log in: ${err.message}. Authenticated routes may return redirects.`);
        return '';
    }
}

// Workload routes definition
function getWorkloads(cookie) {
    const list = [
        {
            name: 'catalogue',
            path: '/events',
            description: 'Public catalogue (2 DB queries: events + categories)',
            headers: {},
        },
        {
            name: 'event_detail',
            path: '/events/1',
            description: 'Event detail page (2 DB queries: event + seat tiers)',
            headers: {},
        },
        {
            name: 'dashboard',
            path: '/dashboard',
            description: 'Attendee dashboard (requires session cookie)',
            headers: cookie ? { 'Cookie': cookie } : {},
        },
        {
            name: 'bookings_history',
            path: '/bookings/my',
            description: 'Attendee booking history (requires session cookie)',
            headers: cookie ? { 'Cookie': cookie } : {},
        },
    ];

    if (target === 'axum') {
        list.push({
            name: 'health',
            path: '/health',
            description: 'Microbenchmark: In-memory liveness probe',
            headers: {},
        });
        list.push({
            name: 'ready',
            path: '/ready',
            description: 'Microbenchmark: Database readiness probe',
            headers: {},
        });
    }

    return list;
}

// Run single autocannon trial
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

        // Do not flood stdout with tick logs
        autocannon.track(instance, { renderProgressBar: false, renderResultsTable: false });
    });
}

// Main execution flow
async function main() {
    const resultsDir = path.join(__dirname, 'results');
    if (!fs.existsSync(resultsDir)) {
        fs.mkdirSync(resultsDir, { recursive: true });
    }

    const sessionCookie = await getAttendeeCookie();
    let workloads = getWorkloads(sessionCookie);

    if (routeFilter) {
        workloads = workloads.filter(w => w.name === routeFilter || w.path === routeFilter);
    }

    const allRunResults = [];
    const csvRows = [
        'Target,Route,Concurrency,Run,Throughput_RPS,Latency_p50_ms,Latency_p95_ms,Latency_p99_ms,Latency_Mean_ms,Status_2xx,Errors,Timeouts,CPU_Percent,Memory_RSS_MB,Response_Bytes_Avg'
    ];

    for (const workload of workloads) {
        const fullUrl = `${baseUrl}${workload.path}`;
        console.log(`\n====================================================`);
        console.log(`Workload: ${workload.name.toUpperCase()} -> ${workload.path}`);
        console.log(`Description: ${workload.description}`);
        console.log(`====================================================`);

        for (const concurrency of concurrencyList) {
            console.log(`\n--- Concurrency: ${concurrency} virtual users ---`);

            // Warm-up run
            if (warmupDuration > 0) {
                process.stdout.write(`  [Warmup ${warmupDuration}s] ... `);
                await runAutocannon(fullUrl, concurrency, warmupDuration, workload.headers);
                console.log('done.');
            }

            for (let run = 1; run <= runsCount; run++) {
                process.stdout.write(`  [Run ${run}/${runsCount} (${duration}s)] ... `);

                const beforeMetrics = getProcessMetrics(targetPid);
                const startTime = Date.now();

                const stats = await runAutocannon(fullUrl, concurrency, duration, workload.headers);
                const endTime = Date.now();
                const afterMetrics = getProcessMetrics(targetPid);

                const elapsedSec = (endTime - startTime) / 1000;
                let cpuPct = 0;
                let memRssMb = 0;

                if (beforeMetrics && afterMetrics && elapsedSec > 0) {
                    const cpuDelta = Math.max(0, afterMetrics.cpuSeconds - beforeMetrics.cpuSeconds);
                    cpuPct = Math.round(((cpuDelta / elapsedSec) / numLogicalCores) * 10000) / 100;
                    memRssMb = Math.round((afterMetrics.workingSetBytes / (1024 * 1024)) * 100) / 100;
                }

                const rps = Math.round(stats.requests.average * 100) / 100;
                const p50 = stats.latency.p50;
                const p95 = typeof stats.latency.p95 === 'number' ? stats.latency.p95 : Math.round(stats.latency.p90 + (stats.latency.p97_5 - stats.latency.p90) * (5 / 7.5));
                const p99 = stats.latency.p99;
                const meanLatency = Math.round(stats.latency.average * 100) / 100;
                const errors = stats.errors || 0;
                const timeouts = stats.timeouts || 0;
                const status2xx = stats['2xx'] || 0;
                const avgBytes = stats.throughput ? Math.round(stats.throughput.average / (rps || 1)) : 0;

                console.log(`RPS: ${rps} | p50: ${p50}ms | p95: ${p95}ms | p99: ${p99}ms | Err: ${errors} | CPU: ${cpuPct}% | Mem: ${memRssMb}MB`);

                allRunResults.push({
                    target,
                    route: workload.name,
                    path: workload.path,
                    concurrency,
                    run,
                    rps,
                    latency: {
                        p50,
                        p95,
                        p99,
                        mean: meanLatency,
                        stddev: stats.latency.stddev,
                    },
                    requestsTotal: stats.requests.total,
                    status2xx,
                    errors,
                    timeouts,
                    cpuPercent: cpuPct,
                    memoryRssMb: memRssMb,
                    avgBytes,
                });

                csvRows.push([
                    target,
                    workload.name,
                    concurrency,
                    run,
                    rps,
                    p50,
                    p95,
                    p99,
                    meanLatency,
                    status2xx,
                    errors,
                    timeouts,
                    cpuPct,
                    memRssMb,
                    avgBytes,
                ].join(','));

                // Brief cool-down between runs
                await new Promise(r => setTimeout(r, 1000));
            }
        }
    }

    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const rawOutPath = path.join(resultsDir, `raw_${target}.json`);
    const csvOutPath = path.join(resultsDir, `summary_${target}.csv`);

    fs.writeFileSync(rawOutPath, JSON.stringify({
        target,
        baseUrl,
        concurrencyList,
        duration,
        warmupDuration,
        runsCount,
        numLogicalCores,
        system: {
            platform: os.platform(),
            arch: os.arch(),
            release: os.release(),
            totalMemoryMb: Math.round(os.totalmem() / (1024 * 1024)),
        },
        results: allRunResults,
    }, null, 2));

    fs.writeFileSync(csvOutPath, csvRows.join('\n'));

    console.log(`\n====================================================`);
    console.log(`Benchmark Complete for ${target.toUpperCase()}`);
    console.log(`Raw Results saved to : ${rawOutPath}`);
    console.log(`CSV Summary saved to : ${csvOutPath}`);
    console.log(`====================================================\n`);
}

main().catch(err => {
    console.error('Fatal harness error:', err);
    process.exit(1);
});
