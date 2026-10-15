// benchmarks/run_pool_tuning.js
// Tests SQLx connection pool limits: 10, 20, and 30
// Measures throughput, latency percentiles, pool acquisition queue wait time,
// query duration, MySQL active connections, and MySQL CPU.

const http = require('http');
const { spawn, execSync } = require('child_process');
const path = require('path');
const fs = require('fs');
const mysql = require('mysql2/promise');

const BASE_URL = 'http://127.0.0.1:3001';
const DURATION_S = 10;
const WARMUP_S = 3;
const CONCURRENCY = 10;
const POOL_LIMITS = [10, 20, 30];

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function httpJson(urlStr, options = {}, postData = null) {
  return new Promise((resolve, reject) => {
    const parsed = new URL(urlStr);
    const reqOpts = {
      protocol: parsed.protocol,
      hostname: parsed.hostname,
      port: parsed.port,
      path: parsed.pathname + parsed.search,
      method: options.method || 'GET',
      headers: options.headers || {},
      timeout: 10000,
    };
    const req = http.request(reqOpts, res => {
      let data = '';
      res.on('data', c => data += c);
      res.on('end', () => {
        try {
          resolve(JSON.parse(data));
        } catch (_) {
          resolve(data);
        }
      });
    });
    req.on('error', reject);
    req.on('timeout', () => req.destroy(new Error('Timeout')));
    if (postData) req.write(postData);
    req.end();
  });
}

async function waitForReady(maxWaitMs = 15000) {
  const start = Date.now();
  await sleep(1000);
  while (Date.now() - start < maxWaitMs) {
    try {
      const res = await httpJson(`${BASE_URL}/health`);
      if (res && res.status === 'ok') return true;
    } catch (_) {}
    await sleep(500);
  }
  throw new Error('Server failed to become ready');
}

function getMysqldCpu() {
  try {
    const cmd = `powershell -NoProfile -Command "Get-Process -Name mysqld -ErrorAction SilentlyContinue | Select-Object -Property Id, CPU, WorkingSet64 | ConvertTo-Json -Compress"`;
    const out = execSync(cmd, { encoding: 'utf8', timeout: 5000 }).trim();
    const data = JSON.parse(out);
    return {
      cpuSeconds: typeof data.CPU === 'number' ? data.CPU : parseFloat(data.CPU || 0),
      workingSetMb: Math.round(((data.WorkingSet64 || 0) / (1024 * 1024)) * 100) / 100,
    };
  } catch (_) {
    return { cpuSeconds: 0, workingSetMb: 0 };
  }
}

async function getMysqlStatus() {
  try {
    const conn = await mysql.createConnection({
      host: 'localhost',
      user: 'root',
      password: 'kishore2006',
      database: 'event_lifecycle',
    });
    const [rows] = await conn.query(`
      SHOW GLOBAL STATUS WHERE Variable_name IN (
        'Threads_connected',
        'Threads_running',
        'Max_used_connections',
        'Questions',
        'Slow_queries'
      )
    `);
    await conn.end();
    const status = {};
    for (const r of rows) {
      status[r.Variable_name] = parseInt(r.Value, 10);
    }
    return status;
  } catch (e) {
    return { error: e.message };
  }
}

function runLoad(concurrency, durationSec, routePath = '/events') {
  return new Promise((resolve) => {
    const latencies = [];
    let errors = 0;
    let completed = 0;
    let running = true;
    const url = new URL(`${BASE_URL}${routePath}`);

    const agent = new http.Agent({
      keepAlive: true,
      maxSockets: concurrency * 2,
    });

    const start = Date.now();
    const stopTime = start + (durationSec * 1000);

    function worker() {
      if (!running || Date.now() >= stopTime) {
        return;
      }
      const reqStart = process.hrtime.bigint();
      const req = http.get(
        {
          hostname: url.hostname,
          port: url.port,
          path: url.pathname + url.search,
          agent,
          timeout: 5000,
        },
        (res) => {
          res.on('data', () => {});
          res.on('end', () => {
            const elapsedMs = Number(process.hrtime.bigint() - reqStart) / 1e6;
            if (res.statusCode === 200) {
              latencies.push(elapsedMs);
            } else {
              errors++;
            }
            completed++;
            if (running) setImmediate(worker);
          });
        }
      );

      req.on('error', () => {
        errors++;
        completed++;
        if (running) setImmediate(worker);
      });

      req.on('timeout', () => {
        req.destroy();
        errors++;
        completed++;
        if (running) setImmediate(worker);
      });
    }

    // Launch VUs
    for (let i = 0; i < concurrency; i++) {
      worker();
    }

    const timer = setInterval(() => {
      if (Date.now() >= stopTime) {
        running = false;
        clearInterval(timer);
        agent.destroy();
        const totalDurationSec = (Date.now() - start) / 1000;
        latencies.sort((a, b) => a - b);
        const p50 = latencies[Math.floor(latencies.length * 0.50)] || 0;
        const p95 = latencies[Math.floor(latencies.length * 0.95)] || 0;
        const p99 = latencies[Math.floor(latencies.length * 0.99)] || 0;
        resolve({
          rps: Math.round((latencies.length / totalDurationSec) * 10) / 10,
          p50: Math.round(p50 * 10) / 10,
          p95: Math.round(p95 * 10) / 10,
          p99: Math.round(p99 * 10) / 10,
          requests: latencies.length,
          errors,
        });
      }
    }, 100);
  });
}

async function testPoolSize(limit) {
  console.log(`\n================================================================`);
  console.log(`TESTING SQLx POOL LIMIT = ${limit} (Concurrency = ${CONCURRENCY}, Duration = ${DURATION_S}s)`);
  console.log(`================================================================`);

  // Kill existing processes on port 3001
  try {
    execSync('taskkill /F /IM eventpro.exe 2>nul');
  } catch (_) {}
  await sleep(1500);

  const env = {
    ...process.env,
    RUST_LOG: 'warn',
    PORT: '3001',
    DB_POOL_MAX: String(limit),
  };

  const exePath = path.resolve(__dirname, '..', 'eventpro-rs', 'target', 'release', 'eventpro.exe');
  const serverProc = spawn(exePath, [], {
    env,
    cwd: path.resolve(__dirname, '..', 'eventpro-rs'),
    stdio: 'ignore',
  });

  const pid = serverProc.pid;
  console.log(`[Process] Spawned eventpro.exe (PID ${pid}) with DB_POOL_MAX=${limit}`);
  await waitForReady();

  // Reset metrics
  await httpJson(`${BASE_URL}/metrics/db/reset`, { method: 'POST' });

  // Initial MySQL CPU & status
  const initialMysqlCpu = getMysqldCpu();
  const initialStatus = await getMysqlStatus();

  // Warmup
  console.log(`  [Warmup ${WARMUP_S}s] ...`);
  await runLoad(CONCURRENCY, WARMUP_S);
  await httpJson(`${BASE_URL}/metrics/db/reset`, { method: 'POST' });

  // Load test
  console.log(`  [Load Test ${DURATION_S}s at C=${CONCURRENCY}] ...`);
  const loadResult = await runLoad(CONCURRENCY, DURATION_S);

  // Final MySQL status and CPU
  const finalMysqlCpu = getMysqldCpu();
  const finalStatus = await getMysqlStatus();
  const dbMetrics = await httpJson(`${BASE_URL}/metrics/db`);

  // Terminate process
  try {
    execSync(`taskkill /F /PID ${pid} 2>nul`);
  } catch (_) {}
  await sleep(1500);

  const mysqlCpuDelta = Math.round((finalMysqlCpu.cpuSeconds - initialMysqlCpu.cpuSeconds) * 100) / 100;

  const result = {
    poolLimit: limit,
    concurrency: CONCURRENCY,
    durationSec: DURATION_S,
    load: loadResult,
    dbInstrumentation: dbMetrics,
    mysql: {
      initialStatus,
      finalStatus,
      threadsConnected: finalStatus.Threads_connected,
      threadsRunning: finalStatus.Threads_running,
      maxUsedConnections: finalStatus.Max_used_connections,
      mysqlCpuDeltaSeconds: mysqlCpuDelta,
    },
  };

  console.log(`  RPS: ${loadResult.rps} | p50: ${loadResult.p50}ms | p95: ${loadResult.p95}ms | p99: ${loadResult.p99}ms | Errors: ${loadResult.errors}`);
  console.log(`  [Pool Acquire] Avg: ${dbMetrics.acquire.avg_ms}ms | Max: ${dbMetrics.acquire.max_ms}ms | >=25ms wait: ${
    (dbMetrics.acquire.distribution['25-50ms'] || 0) +
    (dbMetrics.acquire.distribution['50-100ms'] || 0) +
    (dbMetrics.acquire.distribution['>=100ms'] || 0)
  }`);
  console.log(`  [Query Exec]   Avg: ${dbMetrics.query.avg_ms}ms | Max: ${dbMetrics.query.max_ms}ms`);
  console.log(`  [MySQL Status] Threads_connected: ${finalStatus.Threads_connected} | Max_used: ${finalStatus.Max_used_connections} | CPU Δ: ${mysqlCpuDelta}s`);

  return result;
}

async function main() {
  const allResults = [];
  for (const limit of POOL_LIMITS) {
    const res = await testPoolSize(limit);
    allResults.push(res);
  }

  const outPath = path.join(__dirname, 'results', 'pool_tuning_results.json');
  fs.writeFileSync(outPath, JSON.stringify(allResults, null, 2));
  console.log(`\n================================================================`);
  console.log(`Pool Tuning Completed! Results saved to: ${outPath}`);
  console.log(`================================================================`);
}

main().catch(console.error);
