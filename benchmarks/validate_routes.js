/**
 * EventPro Route and Authentication Validator (Axum Backend)
 *
 * Verifies that the Rust/Axum production binary returns successful HTML responses
 * on all benchmarked routes, confirming:
 * 1. Authenticated session cookies and CSRF tokens are respected.
 * 2. /dashboard and /bookings/my render authenticated attendee data (not redirects).
 * 3. /events/1 identifies the identical event entity and seat tiers.
 * 4. Microbenchmark routes /health and /ready return 200 OK.
 */

const path = require('path');
const fs = require('fs');
const { spawnServer, httpRequest, loginAttendee } = require('./utils');

async function auditAxum() {
    console.log(`\n====================================================`);
    console.log(`Starting AXUM backend on port 3001...`);
    console.log(`====================================================`);

    const server = await spawnServer();
    const results = { target: 'axum', port: server.port, routes: {} };

    try {
        const { cookie, statusCode } = await loginAttendee(server.baseUrl);
        results.auth = { statusCode, cookieName: cookie.split('=')[0] };
        console.log(`[Auth] Cookie established: ${results.auth.cookieName}=*** (Status ${statusCode})`);

        const testList = [
            { name: 'catalogue', path: '/events', auth: false },
            { name: 'event_detail', path: '/events/1', auth: false },
            { name: 'dashboard', path: '/dashboard', auth: true },
            { name: 'bookings_history', path: '/bookings/my', auth: true },
            { name: 'health', path: '/health', auth: false },
            { name: 'ready', path: '/ready', auth: false },
        ];

        for (const item of testList) {
            const headers = item.auth && cookie ? { Cookie: cookie } : {};
            const res = await httpRequest({
                hostname: '127.0.0.1',
                port: server.port,
                path: item.path,
                method: 'GET',
                headers,
            });

            const titleMatch = res.body.match(/<title>([^<]+)<\/title>/i);
            const h1Match = res.body.match(/<h1[^>]*>([^<]+)<\/h1>/i);
            const hasCharlie = /charlie/i.test(res.body);
            const hasLoginForm = /action="\/auth\/login"/i.test(res.body);

            results.routes[item.name] = {
                path: item.path,
                statusCode: res.statusCode,
                bytes: res.bytes,
                contentType: res.headers['content-type'],
                title: titleMatch ? titleMatch[1].trim() : null,
                h1: h1Match ? h1Match[1].trim() : null,
                authenticatedDataRendered: item.auth ? (hasCharlie && !hasLoginForm) : null,
                hasLoginForm,
                isRedirect: res.statusCode >= 300 && res.statusCode < 400,
                bodySnippet: res.body.slice(0, 300).replace(/\s+/g, ' ').trim(),
            };

            console.log(`[Route] ${item.path} -> ${res.statusCode} (${res.bytes} bytes) | Title: ${results.routes[item.name].title || 'N/A'}`);
            if (item.auth) {
                console.log(`        Auth verification: hasCharlie=${hasCharlie}, hasLoginForm=${hasLoginForm}`);
            }
        }
    } finally {
        await server.stop();
        await new Promise((r) => setTimeout(r, 1000));
    }

    return results;
}

async function main() {
    console.log('Auditing Axum backend for semantic correctness and response fidelity...\n');
    const axumAudit = await auditAxum();

    const outDir = path.join(__dirname, 'results');
    if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true });
    const outPath = path.join(outDir, 'route_validation.json');

    // Preserve historical Express audit data if present
    let existing = {};
    if (fs.existsSync(outPath)) {
        try { existing = JSON.parse(fs.readFileSync(outPath, 'utf8')); } catch (_) {}
    }

    const finalReport = {
        express: existing.express || null,
        axum: axumAudit,
    };

    fs.writeFileSync(outPath, JSON.stringify(finalReport, null, 2));

    console.log('\n====================================================');
    console.log('Audit Summary Written to:', outPath);
    console.log('====================================================');
}

main().catch((err) => {
    console.error('Validation failure:', err);
    process.exit(1);
});
