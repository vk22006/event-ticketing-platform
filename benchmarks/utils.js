/**
 * Shared Utilities for EventPro Benchmarks and Diagnostics (Axum Backend)
 */

const http = require('http');
const https = require('https');
const { spawn, execSync } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const mysql = require('mysql2/promise');
const autocannon = require('autocannon');

const PROJECT_ROOT = path.resolve(__dirname, '..');
const RS_ROOT = path.resolve(PROJECT_ROOT, 'eventpro-rs');

const DB_CONFIG = {
    host: process.env.DB_HOST || '127.0.0.1',
    user: process.env.DB_USER || 'root',
    password: process.env.DB_PASSWORD !== undefined ? process.env.DB_PASSWORD : 'kishore2006',
    port: parseInt(process.env.DB_PORT || '3306', 10),
};

function getDb(database = 'event_lifecycle') {
    return mysql.createConnection({ ...DB_CONFIG, database });
}

function computeSha256(filePath) {
    const data = fs.readFileSync(filePath);
    return crypto.createHash('sha256').update(data).digest('hex').toUpperCase();
}

function httpRequest(options, postData = null) {
    return new Promise((resolve, reject) => {
        const client = options.protocol === 'https:' ? https : http;
        const req = client.request(options, (res) => {
            let body = '';
            res.on('data', (chunk) => (body += chunk));
            res.on('end', () =>
                resolve({
                    statusCode: res.statusCode,
                    headers: res.headers,
                    body,
                    bytes: Buffer.byteLength(body, 'utf8'),
                })
            );
        });
        req.on('error', reject);
        if (options.timeout) {
            req.setTimeout(options.timeout, () => req.destroy(new Error('Request timeout')));
        }
        if (postData) req.write(postData);
        req.end();
    });
}

async function waitForUrl(url, maxMs = 8000, intervalMs = 100) {
    const parsed = new URL(url);
    const start = Date.now();
    while (Date.now() - start < maxMs) {
        try {
            const res = await httpRequest({
                protocol: parsed.protocol,
                hostname: parsed.hostname,
                port: parsed.port,
                path: parsed.pathname + parsed.search,
                method: 'GET',
                timeout: 1000,
            });
            if (res.statusCode >= 200 && res.statusCode < 400) return true;
        } catch (_) {}
        await new Promise((r) => setTimeout(r, intervalMs));
    }
    return false;
}

async function killProcess(child) {
    if (!child || !child.pid) return;
    try {
        execSync(`taskkill /F /T /PID ${child.pid} 2>nul`);
    } catch (_) {
        try { child.kill('SIGKILL'); } catch (_) {}
    }
    await new Promise((r) => setTimeout(r, 200));
}

function killNamedProcesses(names = ['eventpro.exe', 'eventpro-baseline.exe', 'eventpro-optimized.exe']) {
    for (const name of names) {
        try {
            execSync(`taskkill /F /IM ${name} 2>nul`);
        } catch (_) {}
    }
}

async function spawnServer(customOpts = {}) {
    const port = customOpts.port || 3001;
    const defaultExe = path.join(RS_ROOT, 'target', 'release', 'eventpro.exe');
    const binPath = customOpts.binPath || defaultExe;

    const env = {
        ...process.env,
        APP_ENV: 'production',
        PORT: port.toString(),
        DATABASE_URL: `mysql://${DB_CONFIG.user}:${DB_CONFIG.password}@${DB_CONFIG.host}:${DB_CONFIG.port}/event_lifecycle`,
        SESSION_SECRET: 'benchmark_session_secret_at_least_16_chars_long',
        DB_POOL_MAX: customOpts.poolMax || '10',
        RUST_LOG: 'warn',
        ...customOpts.env,
    };

    const child = spawn(binPath, [], { cwd: RS_ROOT, env, stdio: 'ignore' });
    const readyUrl = `http://127.0.0.1:${port}/events`;
    const ready = await waitForUrl(readyUrl, customOpts.timeout || 10000);
    if (!ready) {
        await killProcess(child);
        throw new Error(`Axum failed to start on port ${port}`);
    }

    return {
        child,
        port,
        baseUrl: `http://127.0.0.1:${port}`,
        stop: () => killProcess(child),
    };
}

async function loginAttendee(baseUrl) {
    const parsed = new URL(baseUrl);
    const port = parsed.port;

    const loginPage = await httpRequest({
        hostname: parsed.hostname,
        port,
        path: '/auth/login',
        method: 'GET',
    });
    const initCookie = ((loginPage.headers['set-cookie'] || [])[0] || '').split(';')[0];
    const match = loginPage.body.match(/name="csrf_token"\s+value="([^"]+)"/);
    const csrfToken = match ? match[1] : '';

    const body = `email=charlie%40mail.com&password=User%401234&csrf_token=${encodeURIComponent(csrfToken)}`;
    const res = await httpRequest(
        {
            hostname: parsed.hostname,
            port,
            path: '/auth/login',
            method: 'POST',
            headers: {
                'Content-Type': 'application/x-www-form-urlencoded',
                'Content-Length': Buffer.byteLength(body),
                Cookie: initCookie,
            },
        },
        body
    );

    const authCookie = ((res.headers['set-cookie'] || [initCookie])[0] || '').split(';')[0];
    return { cookie: authCookie, csrfToken, statusCode: res.statusCode };
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

function runAutocannon(url, concurrency, testDuration, headers = {}) {
    return new Promise((resolve, reject) => {
        const instance = autocannon(
            { url, connections: concurrency, duration: testDuration, pipelining: 1, headers, excludeErrorStats: false },
            (err, results) => {
                if (err) return reject(err);
                resolve(results);
            }
        );
        autocannon.track(instance, { renderProgressBar: false, renderResultsTable: false });
    });
}

async function truncateMysqlSessions() {
    try {
        const conn = await getDb('tower_sessions');
        await conn.query('DELETE FROM session');
        await conn.end();
    } catch (_) {}
}

async function getMysqlSessionCount() {
    try {
        const conn = await getDb('tower_sessions');
        const [[{ cnt }]] = await conn.query('SELECT COUNT(*) as cnt FROM session');
        await conn.end();
        return cnt;
    } catch (_) {
        return null;
    }
}

async function resetDbTelemetry(port = 3001) {
    try {
        await httpRequest({ hostname: '127.0.0.1', port, path: '/metrics/db/reset', method: 'POST' });
    } catch (_) {}
}

async function getDbTelemetry(port = 3001) {
    try {
        const resp = await httpRequest({ hostname: '127.0.0.1', port, path: '/metrics/db', method: 'GET' });
        return JSON.parse(resp.body);
    } catch (_) {
        return null;
    }
}

module.exports = {
    PROJECT_ROOT,
    RS_ROOT,
    DB_CONFIG,
    getDb,
    computeSha256,
    httpRequest,
    waitForUrl,
    killProcess,
    killNamedProcesses,
    spawnServer,
    loginAttendee,
    getProcessMetrics,
    runAutocannon,
    truncateMysqlSessions,
    getMysqlSessionCount,
    resetDbTelemetry,
    getDbTelemetry,
};
