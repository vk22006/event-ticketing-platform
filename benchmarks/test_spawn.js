const { spawn } = require('child_process');
const http = require('http');
const path = require('path');

const proc = spawn(path.resolve(__dirname, '..', 'eventpro-rs', 'target', 'release', 'eventpro.exe'), [], {
  cwd: path.resolve(__dirname, '..', 'eventpro-rs'),
  env: Object.assign({}, process.env, { PORT: '3001', APP_ENV: 'production' }),
  stdio: 'inherit',
});

console.log('Spawned PID:', proc.pid);

setTimeout(() => {
  http.get('http://127.0.0.1:3001/health', (res) => {
    let d = '';
    res.on('data', c => d += c);
    res.on('end', () => {
      console.log('Response status:', res.statusCode, 'body:', d);
      proc.kill();
      process.exit(0);
    });
  }).on('error', (e) => {
    console.error('HTTP Error:', e.message);
    proc.kill();
    process.exit(1);
  });
}, 2000);
