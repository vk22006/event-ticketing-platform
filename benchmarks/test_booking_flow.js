const http = require('http');
const { spawn, execSync } = require('child_process');
const path = require('path');

const CWD_DIR = path.resolve(__dirname, '..', 'eventpro-rs');
const BINARY_PATH = path.join(CWD_DIR, 'target', 'release', 'eventpro.exe');

function req(options, postData = null) {
    return new Promise((resolve, reject) => {
        const r = http.request(options, res => {
            let b = '';
            res.on('data', c => b += c);
            res.on('end', () => resolve({ statusCode: res.statusCode, headers: res.headers, body: b }));
        });
        r.on('error', reject);
        if (postData) r.write(postData);
        r.end();
    });
}

async function main() {
    try { execSync('taskkill /F /IM eventpro.exe 2>nul'); } catch (_) {}
    const child = spawn(BINARY_PATH, [], {
        cwd: CWD_DIR,
        env: { ...process.env, APP_ENV: 'production', PORT: '3001', RUST_LOG: 'info' },
    });
    child.stdout.on('data', d => console.log('[STDOUT]', d.toString().trim()));
    child.stderr.on('data', d => console.log('[STDERR]', d.toString().trim()));

    await new Promise(r => setTimeout(r, 1500));

    try {
        const loginPage = await req({ hostname: '127.0.0.1', port: 3001, path: '/auth/login' });
        const cookie = loginPage.headers['set-cookie'][0].split(';')[0];
        const csrfMatch = loginPage.body.match(/name="csrf_token"\s+value="([^"]+)"/);
        const csrf = csrfMatch[1];

        const loginBody = `email=charlie%40mail.com&password=User%401234&csrf_token=${csrf}`;
        const loginRes = await req({
            hostname: '127.0.0.1', port: 3001, path: '/auth/login', method: 'POST',
            headers: {
                'Content-Type': 'application/x-www-form-urlencoded',
                'Content-Length': Buffer.byteLength(loginBody),
                'Cookie': cookie,
            }
        }, loginBody);

        const authCookie = (loginRes.headers['set-cookie'] || [cookie])[0].split(';')[0];
        console.log('Auth cookie:', authCookie);

        const paymentPage = await req({
            hostname: '127.0.0.1', port: 3001,
            path: '/bookings/payment?eventId=97&quantity=1',
            headers: { Cookie: authCookie }
        });

        console.log('Payment status:', paymentPage.statusCode);
        const payCsrfMatch = paymentPage.body.match(/name="csrf_token"\s+value="([^"]+)"/);
        console.log('Payment CSRF:', payCsrfMatch ? payCsrfMatch[1] : 'NONE');

        if (payCsrfMatch) {
            const bookCsrf = payCsrfMatch[1];
            const bookBody = `csrf_token=${encodeURIComponent(bookCsrf)}&event_id=97&quantity=1`;
            const bookRes = await req({
                hostname: '127.0.0.1', port: 3001, path: '/bookings/confirm', method: 'POST',
                headers: {
                    'Content-Type': 'application/x-www-form-urlencoded',
                    'Content-Length': Buffer.byteLength(bookBody),
                    'Cookie': authCookie,
                }
            }, bookBody);

            console.log('Book response status:', bookRes.statusCode);
            console.log('Book response headers:', bookRes.headers);
            console.log('Book response body snippet:', bookRes.body.substring(0, 400));
        }

    } finally {
        child.kill();
    }
}

main().catch(console.error);
