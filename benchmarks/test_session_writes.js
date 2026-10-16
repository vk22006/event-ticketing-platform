/**
 * Diagnostic tool: Instrument session store read/write behavior and pool acquisition
 */

const { getDb, spawnServer, httpRequest, loginAttendee } = require('./utils');

async function main() {
    console.log('Connecting to MySQL (tower_sessions)...');
    const db = await getDb('tower_sessions');

    console.log('Starting Axum release binary...');
    const server = await spawnServer('axum');

    try {
        const [[{ initialCount }]] = await db.query('SELECT COUNT(*) as initialCount FROM session');
        console.log(`Initial session row count: ${initialCount}`);

        // 1. Test unauthenticated GET /events
        await httpRequest({ hostname: '127.0.0.1', port: server.port, path: '/events', method: 'GET' });
        const [[{ countAfterEvents }]] = await db.query('SELECT COUNT(*) as countAfterEvents FROM session');
        console.log(`Session count after GET /events (no cookie): ${countAfterEvents} (delta: +${countAfterEvents - initialCount})`);

        // 2. Test unauthenticated GET /events/1
        await httpRequest({ hostname: '127.0.0.1', port: server.port, path: '/events/1', method: 'GET' });
        const [[{ countAfterDetail }]] = await db.query('SELECT COUNT(*) as countAfterDetail FROM session');
        console.log(`Session count after GET /events/1 (no cookie): ${countAfterDetail} (delta: +${countAfterDetail - countAfterEvents})`);

        // 3. Establish authenticated session
        const { cookie } = await loginAttendee(server.baseUrl);
        console.log(`Logged in. Cookie: ${cookie.slice(0, 20)}...`);

        const sidMatch = cookie.match(/eventpro_sid=([^;]+)/);
        const sidPrefix = sidMatch ? sidMatch[1].slice(0, 10) : '';
        console.log(`Cookie SID prefix: ${sidPrefix}...`);

        const [recentSessions] = await db.query('SELECT id, expiry_date FROM session ORDER BY expiry_date DESC LIMIT 5');
        console.log('Most recent sessions in DB:');
        for (const s of recentSessions) {
            console.log(`  id: ${s.id.slice(0, 15)}... | expiry: ${s.expiry_date}`);
        }

        const [[targetSession]] = await db.query('SELECT id, expiry_date FROM session WHERE id LIKE ? LIMIT 1', [`${sidPrefix}%`]);
        const matchedSid = targetSession ? targetSession.id : (recentSessions[0] ? recentSessions[0].id : null);

        // 4. Test GET /dashboard (Read 1)
        console.log('\nSending GET /dashboard with session cookie (Request 1)...');
        const [[before1]] = await db.query('SELECT expiry_date FROM session WHERE id = ?', [matchedSid]);
        await httpRequest({
            hostname: '127.0.0.1',
            port: server.port,
            path: '/dashboard',
            method: 'GET',
            headers: { Cookie: cookie },
        });
        const [[after1]] = await db.query('SELECT expiry_date FROM session WHERE id = ?', [matchedSid]);
        console.log(`Expiry before: ${before1.expiry_date}`);
        console.log(`Expiry after : ${after1.expiry_date}`);
        console.log(`Did expiry_date update in DB on GET /dashboard? ${before1.expiry_date.getTime() !== after1.expiry_date.getTime() ? 'YES' : 'NO'}`);

        // 5. Test GET /dashboard (Read 2 after delay)
        console.log('\nSending GET /dashboard with session cookie (Request 2, after 1000ms)...');
        await new Promise((r) => setTimeout(r, 1000));
        await httpRequest({
            hostname: '127.0.0.1',
            port: server.port,
            path: '/dashboard',
            method: 'GET',
            headers: { Cookie: cookie },
        });
        const [[after2]] = await db.query('SELECT expiry_date FROM session WHERE id = ?', [matchedSid]);
        console.log(`Expiry after req 2: ${after2.expiry_date}`);
        console.log(`Did expiry_date update again? ${after1.expiry_date.getTime() !== after2.expiry_date.getTime() ? 'YES' : 'NO'}`);
    } finally {
        await server.stop();
        await db.end();
    }
}

main().catch((err) => {
    console.error('Session test failed:', err);
    process.exit(1);
});
