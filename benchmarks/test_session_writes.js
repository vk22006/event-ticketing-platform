/**
 * Diagnostic tool: Instrument session store read/write behavior and pool acquisition
 */

const { spawn } = require('child_process');
const http = require('http');
const path = require('path');
const mysql = require('mysql2/promise');
require('dotenv').config();

function httpRequest(options, postData = null) {
    return new Promise((resolve, reject) => {
        const req = http.request(options, (res) => {
            let body = '';
            res.on('data', chunk => body += chunk);
            res.on('end', () => resolve({
                statusCode: res.statusCode,
                headers: res.headers,
                body,
            }));
        });
        req.on('error', reject);
        if (postData) req.write(postData);
        req.end();
    });
}

async function waitForHttp(url, maxMs = 5000) {
    const start = Date.now();
    while (Date.now() - start < maxMs) {
        try {
            const res = await httpRequest({
                hostname: '127.0.0.1',
                port: new URL(url).port,
                path: new URL(url).pathname,
                method: 'GET',
            });
            if (res.statusCode >= 200 && res.statusCode < 400) return true;
        } catch (_) {}
        await new Promise(r => setTimeout(r, 100));
    }
    return false;
}

async function killProcessTree(child) {
    if (!child || !child.pid) return;
    try { process.kill(child.pid, 'SIGINT'); } catch (_) {}
    await new Promise(r => setTimeout(r, 200));
    try { process.kill(child.pid, 'SIGKILL'); } catch (_) {}
}

async function main() {
    console.log('Connecting to MySQL...');
    const db = await mysql.createConnection({
        host: process.env.DB_HOST || 'localhost',
        user: process.env.DB_USER || 'root',
        password: process.env.DB_PASSWORD || '',
        database: 'tower_sessions',
    });

    const port = 3001;
    const cwd = path.resolve(__dirname, '..', 'eventpro-rs');
    const cmd = path.join(cwd, 'target', 'release', 'eventpro.exe');
    const env = Object.assign({}, process.env, { APP_ENV: 'production', PORT: '3001' });

    console.log('Starting Axum release binary...');
    const child = spawn(cmd, [], { cwd, env, stdio: 'ignore' });
    const ready = await waitForHttp(`http://127.0.0.1:${port}/events`, 8000);
    if (!ready) {
        await killProcessTree(child);
        throw new Error('Axum failed to start');
    }

    try {
        const [[{ initialCount }]] = await db.query('SELECT COUNT(*) as initialCount FROM session');
        console.log(`Initial session row count: ${initialCount}`);

        // 1. Test unauthenticated GET /events
        await httpRequest({ hostname: '127.0.0.1', port, path: '/events', method: 'GET' });
        const [[{ countAfterEvents }]] = await db.query('SELECT COUNT(*) as countAfterEvents FROM session');
        console.log(`Session count after GET /events (no cookie): ${countAfterEvents} (delta: +${countAfterEvents - initialCount})`);

        // 2. Test unauthenticated GET /events/1
        await httpRequest({ hostname: '127.0.0.1', port, path: '/events/1', method: 'GET' });
        const [[{ countAfterDetail }]] = await db.query('SELECT COUNT(*) as countAfterDetail FROM session');
        console.log(`Session count after GET /events/1 (no cookie): ${countAfterDetail} (delta: +${countAfterDetail - countAfterEvents})`);

        // 3. Establish authenticated session
        const loginPage = await httpRequest({ hostname: '127.0.0.1', port, path: '/auth/login', method: 'GET' });
        const initCookie = (loginPage.headers['set-cookie'] || [])[0] || '';
        const csrfMatch = loginPage.body.match(/name="csrf_token"\s+value="([^"]+)"/);
        const csrfToken = csrfMatch ? csrfMatch[1] : '';

        const postBody = `email=charlie%40mail.com&password=User%401234&csrf_token=${encodeURIComponent(csrfToken)}`;
        const loginRes = await httpRequest({
            hostname: '127.0.0.1',
            port,
            path: '/auth/login',
            method: 'POST',
            headers: {
                'Content-Type': 'application/x-www-form-urlencoded',
                'Content-Length': Buffer.byteLength(postBody),
                'Cookie': initCookie.split(';')[0],
            },
        }, postBody);

        const authCookie = (loginRes.headers['set-cookie'] || [])[0] || initCookie;
        const cookieHeader = authCookie.split(';')[0];
        console.log(`Logged in. Cookie: ${cookieHeader.split('=')[0]}=...`);

        // Extract session ID (tower-sessions encodes session ID in cookie)
        const rawSid = cookieHeader.split('=')[1];
        console.log(`Cookie SID prefix: ${rawSid.slice(0, 10)}...`);

        // Find the active session in DB
        const [rows] = await db.query('SELECT id, expiry_date FROM session ORDER BY expiry_date DESC LIMIT 5');
        console.log('Most recent sessions in DB:');
        rows.forEach(r => console.log(`  id: ${r.id.slice(0, 15)}... | expiry: ${r.expiry_date}`));

        const targetSessionId = rows[0].id;
        const targetExpiry1 = rows[0].expiry_date;

        // 4. Request /dashboard with session cookie
        console.log('\nSending GET /dashboard with session cookie (Request 1)...');
        await new Promise(r => setTimeout(r, 500));
        await httpRequest({
            hostname: '127.0.0.1',
            port,
            path: '/dashboard',
            method: 'GET',
            headers: { 'Cookie': cookieHeader },
        });

        const [[res1]] = await db.query('SELECT expiry_date FROM session WHERE id = ?', [targetSessionId]);
        console.log(`Expiry before: ${targetExpiry1}`);
        console.log(`Expiry after : ${res1.expiry_date}`);
        const changed1 = res1.expiry_date.getTime() !== targetExpiry1.getTime();
        console.log(`Did expiry_date update in DB on GET /dashboard? ${changed1 ? 'YES (WRITE OCCURRED)' : 'NO'}`);

        // 5. Request /dashboard again after 1 second
        console.log('\nSending GET /dashboard with session cookie (Request 2, after 1000ms)...');
        await new Promise(r => setTimeout(r, 1000));
        await httpRequest({
            hostname: '127.0.0.1',
            port,
            path: '/dashboard',
            method: 'GET',
            headers: { 'Cookie': cookieHeader },
        });

        const [[res2]] = await db.query('SELECT expiry_date FROM session WHERE id = ?', [targetSessionId]);
        console.log(`Expiry after req 2: ${res2.expiry_date}`);
        const changed2 = res2.expiry_date.getTime() !== res1.expiry_date.getTime();
        console.log(`Did expiry_date update again? ${changed2 ? 'YES (WRITE OCCURRED)' : 'NO'}`);

    } finally {
        await killProcessTree(child);
        await db.end();
    }
}

main().catch(err => {
    console.error('Diagnostic error:', err);
    process.exit(1);
});
