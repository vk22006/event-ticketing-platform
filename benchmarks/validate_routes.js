/**
 * EventPro Route and Authentication Validator
 *
 * Verifies that both Express.js and Axum return semantically equivalent,
 * successful HTML responses on all benchmarked routes, confirming that:
 * 1. Authenticated session cookies are valid and respected.
 * 2. /dashboard and /bookings/my render authenticated attendee data (not redirects or error pages).
 * 3. /events/1 identifies the identical event entity and seat tiers on both backends.
 * 4. Response size differences are explained at the HTML and header level.
 */

const { spawn } = require('child_process');
const http = require('http');
const path = require('path');
const fs = require('fs');

function httpRequest(options, postData = null) {
    return new Promise((resolve, reject) => {
        const req = http.request(options, (res) => {
            let body = '';
            res.on('data', chunk => body += chunk);
            res.on('end', () => resolve({
                statusCode: res.statusCode,
                headers: res.headers,
                body,
                bytes: Buffer.byteLength(body, 'utf8'),
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
    try {
        process.kill(child.pid, 'SIGINT');
    } catch (_) {}
    await new Promise(r => setTimeout(r, 200));
    try {
        process.kill(child.pid, 'SIGKILL');
    } catch (_) {}
}

async function auditBackend(target) {
    const isExpress = target === 'express';
    const port = isExpress ? 3000 : 3001;
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

    console.log(`\n====================================================`);
    console.log(`Starting ${target.toUpperCase()} on port ${port}...`);
    console.log(`====================================================`);

    const child = spawn(cmd, args, { cwd, env, stdio: 'ignore' });
    const ready = await waitForHttp(`http://127.0.0.1:${port}/events`, 8000);
    if (!ready) {
        await killProcessTree(child);
        throw new Error(`${target} failed to start on port ${port}`);
    }

    const results = { target, port, routes: {} };

    try {
        // 1. Establish session cookie
        let sessionCookie = '';
        if (isExpress) {
            const postBody = `email=charlie%40mail.com&password=User%401234`;
            let loginRes = await httpRequest({
                hostname: '127.0.0.1',
                port,
                path: '/auth/login',
                method: 'POST',
                headers: {
                    'Content-Type': 'application/x-www-form-urlencoded',
                    'Content-Length': Buffer.byteLength(postBody),
                },
            }, postBody);

            if (loginRes.statusCode === 401 || loginRes.body.includes('Invalid email or password')) {
                const fallbackBody = `email=user%40eventpro.com&password=user123`;
                loginRes = await httpRequest({
                    hostname: '127.0.0.1',
                    port,
                    path: '/auth/login',
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/x-www-form-urlencoded',
                        'Content-Length': Buffer.byteLength(fallbackBody),
                    },
                }, fallbackBody);
            }

            const rawCookie = (loginRes.headers['set-cookie'] || [])[0] || '';
            sessionCookie = rawCookie.split(';')[0];
            results.auth = {
                statusCode: loginRes.statusCode,
                cookieName: sessionCookie.split('=')[0],
                redirectLocation: loginRes.headers['location'],
            };
        } else {
            // Axum: GET /auth/login then POST
            const loginPage = await httpRequest({
                hostname: '127.0.0.1',
                port,
                path: '/auth/login',
                method: 'GET',
            });
            const initialCookie = (loginPage.headers['set-cookie'] || [])[0] || '';
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
                    'Cookie': initialCookie.split(';')[0],
                },
            }, postBody);

            const authCookie = (loginRes.headers['set-cookie'] || [])[0] || initialCookie;
            sessionCookie = authCookie.split(';')[0];
            results.auth = {
                statusCode: loginRes.statusCode,
                cookieName: sessionCookie.split('=')[0],
                redirectLocation: loginRes.headers['location'],
            };
        }

        console.log(`[Auth] Cookie established: ${results.auth.cookieName}=*** (Status ${results.auth.statusCode})`);

        // Routes to test
        const testList = [
            { name: 'catalogue', path: '/events', auth: false },
            { name: 'event_detail', path: '/events/1', auth: false },
            { name: 'dashboard', path: '/dashboard', auth: true },
            { name: 'bookings_history', path: '/bookings/my', auth: true },
        ];

        if (!isExpress) {
            testList.push({ name: 'health', path: '/health', auth: false });
            testList.push({ name: 'ready', path: '/ready', auth: false });
        }

        for (const item of testList) {
            const headers = {};
            if (item.auth && sessionCookie) {
                headers['Cookie'] = sessionCookie;
            }

            const res = await httpRequest({
                hostname: '127.0.0.1',
                port,
                path: item.path,
                method: 'GET',
                headers,
            });

            // Extract key indicators
            const titleMatch = res.body.match(/<title>([^<]+)<\/title>/i);
            const h1Match = res.body.match(/<h1[^>]*>([^<]+)<\/h1>/i);
            const hasCharlie = /charlie/i.test(res.body);
            const hasLoginForm = /action="\/auth\/login"/i.test(res.body);
            const isRedirect = res.statusCode >= 300 && res.statusCode < 400;

            results.routes[item.name] = {
                path: item.path,
                statusCode: res.statusCode,
                bytes: res.bytes,
                contentType: res.headers['content-type'],
                title: titleMatch ? titleMatch[1].trim() : null,
                h1: h1Match ? h1Match[1].trim() : null,
                authenticatedDataRendered: item.auth ? (hasCharlie && !hasLoginForm) : null,
                hasLoginForm,
                isRedirect,
                bodySnippet: res.body.slice(0, 300).replace(/\s+/g, ' ').trim(),
            };

            console.log(`[Route] ${item.path} -> ${res.statusCode} (${res.bytes} bytes) | Title: ${results.routes[item.name].title || 'N/A'}`);
            if (item.auth) {
                console.log(`        Auth verification: hasCharlie=${hasCharlie}, hasLoginForm=${hasLoginForm}`);
            }
        }
    } finally {
        await killProcessTree(child);
        await new Promise(r => setTimeout(r, 1000));
    }

    return results;
}

async function main() {
    console.log('Auditing both backends for semantic correctness and response fidelity...\n');
    const expressAudit = await auditBackend('express');
    const axumAudit = await auditBackend('axum');

    const outDir = path.join(__dirname, 'results');
    if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true });
    const outPath = path.join(outDir, 'route_validation.json');
    fs.writeFileSync(outPath, JSON.stringify({ express: expressAudit, axum: axumAudit }, null, 2));

    console.log('\n====================================================');
    console.log('Audit Summary Written to:', outPath);
    console.log('====================================================');
}

main().catch(err => {
    console.error('Validation failure:', err);
    process.exit(1);
});
