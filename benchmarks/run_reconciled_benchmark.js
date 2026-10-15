// benchmarks/run_reconciled_benchmark.js
// Reconciles all benchmark harnesses, resolves experimental inconsistencies,
// and executes:
// 1. Alternating 5-trial controlled test of Baseline vs Optimized CSRF configurations (C=1 and C=10)
// 2. Multi-trial alternating pool tuning across sizes 10, 20, and 30 with configuration scope verification

const http = require('http');
const { spawn, execSync } = require('child_process');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');
const mysql = require('mysql2/promise');

const BASE_URL = 'http://127.0.0.1:3001';
const DURATION_S = 10;
const WARMUP_S = 3;
const EXE_PATH = path.resolve(__dirname, '..', 'eventpro-rs', 'target', 'release', 'eventpro.exe');
const CWD_DIR = path.resolve(__dirname, '..', 'eventpro-rs');

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function getFileSha256(filePath) {
  const fileBuffer = fs.readFileSync(filePath);
  const hashSum = crypto.createHash('sha256');
  hashSum.update(fileBuffer);
  return hashSum.digest('hex').toUpperCase();
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
    req.on('timeout', () => req.destroy(new Error('Request timeout')));
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
  throw new Error('Server failed to become ready within ' + maxWaitMs + 'ms');
}

function killRunningEventpro() {
  try {
    execSync('taskkill /F /IM eventpro.exe 2>nul');
  } catch (_) {}
}

async function getMysqlSessionCount() {
  try {
    const conn = await mysql.createConnection({
      host: 'localhost',
      user: 'root',
      password: 'kishore2006',
      database: 'tower_sessions',
    });
    const [rows] = await conn.query('SELECT COUNT(*) as cnt FROM session');
    await conn.end();
    return rows[0].cnt;
  } catch (e) {
    return null;
  }
}

async function truncateMysqlSessions() {
  try {
    const conn = await mysql.createConnection({
      host: 'localhost',
      user: 'root',
      password: 'kishore2006',
      database: 'tower_sessions',
    });
    await conn.query('DELETE FROM session');
    await conn.end();
  } catch (e) {
    console.error('Failed to clear sessions:', e.message);
  }
}

async function getMysqlThreadsConnected() {
  try {
    const conn = await mysql.createConnection({
      host: 'localhost',
      user: 'root',
      password: 'kishore2006',
      database: 'event_lifecycle',
    });
    const [rows] = await conn.query("SHOW GLOBAL STATUS LIKE 'Threads_connected'");
    await conn.end();
    return parseInt(rows[0].Value, 10);
  } catch (_) {
    return 0;
  }
}

function getProcessWorkingSet(pid) {
  if (!pid) return 0;
  try {
    const cmd = `powershell -NoProfile -Command "Get-Process -Id ${pid} | Select-Object -ExpandProperty WorkingSet64"`;
    const out = execSync(cmd, { encoding: 'utf8', timeout: 5000 }).trim();
    return Math.round((parseInt(out, 10) / (1024 * 1024)) * 100) / 100;
  } catch (_) {
    return 0;
  }
}

function runLoad(concurrency, durationSec, routePath = '/events') {
  return new Promise((resolve) => {
    const latencies = [];
    let errors = 0;
    let running = true;
    const url = new URL(`${BASE_URL}${routePath}`);

    const agent = new http.Agent({
      keepAlive: true,
      maxSockets: concurrency * 2,
    });

    const start = Date.now();
    const stopTime = start + (durationSec * 1000);

    function worker() {
      if (!running || Date.now() >= stopTime) return;
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
            if (running) setImmediate(worker);
          });
        }
      );

      req.on('error', () => {
        errors++;
        if (running) setImmediate(worker);
      });

      req.on('timeout', () => {
        req.destroy();
        errors++;
        if (running) setImmediate(worker);
      });
    }

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

// Executes a single trial for a given configuration
async function runSingleTrial({ configName, envVars, concurrency, trialIndex }) {
  killRunningEventpro();
  await sleep(1000);
  await truncateMysqlSessions();

  const env = {
    ...process.env,
    RUST_LOG: 'warn',
    PORT: '3001',
    ...envVars,
  };

  const proc = spawn(EXE_PATH, [], { env, cwd: CWD_DIR, stdio: 'ignore' });
  const pid = proc.pid;
  await waitForReady();

  // Verify configuration scope via /metrics/db
  const initialMetrics = await httpJson(`${BASE_URL}/metrics/db`);
  const configuredMax = initialMetrics.pool ? initialMetrics.pool.configured_max : null;

  // Warmup
  await runLoad(concurrency, WARMUP_S);

  // Exact per-trial telemetry reset
  await httpJson(`${BASE_URL}/metrics/db/reset`, { method: 'POST' });

  // Load test
  const loadStats = await runLoad(concurrency, DURATION_S);
  const workingSetMb = getProcessWorkingSet(pid);

  // Collect per-trial telemetry immediately
  const trialMetrics = await httpJson(`${BASE_URL}/metrics/db`);
  const sessionCount = await getMysqlSessionCount();

  // Terminate process
  try {
    execSync(`taskkill /F /PID ${pid} 2>nul`);
  } catch (_) {}
  await sleep(1000);

  const trialResult = {
    trial: trialIndex,
    config: configName,
    concurrency,
    pid,
    configuredMax,
    load: loadStats,
    workingSetMb,
    dbTelemetry: trialMetrics,
    finalSessionCount: sessionCount,
  };

  console.log(`    [Trial ${trialIndex}] ${configName} @ C=${concurrency}: ${loadStats.rps} RPS | p50: ${loadStats.p50}ms | p95: ${loadStats.p95}ms | p99: ${loadStats.p99}ms | RAM: ${workingSetMb}MB | DB Acquire Avg: ${trialMetrics.acquire.avg_ms}ms (Max: ${trialMetrics.acquire.max_ms}ms) | DB Query Avg: ${trialMetrics.query.avg_ms}ms (Max: ${trialMetrics.query.max_ms}ms) | Sessions: ${sessionCount}`);

  return trialResult;
}

function computeSummary(trials) {
  const rpsList = trials.map(t => t.load.rps);
  const p50List = trials.map(t => t.load.p50);
  const p95List = trials.map(t => t.load.p95);
  const p99List = trials.map(t => t.load.p99);
  const ramList = trials.map(t => t.workingSetMb);
  const acqAvgList = trials.map(t => t.dbTelemetry.acquire.avg_ms);
  const acqMaxList = trials.map(t => t.dbTelemetry.acquire.max_ms);
  const qAvgList = trials.map(t => t.dbTelemetry.query.avg_ms);
  const qMaxList = trials.map(t => t.dbTelemetry.query.max_ms);

  const mean = arr => Math.round((arr.reduce((a, b) => a + b, 0) / arr.length) * 10) / 10;
  const stddev = (arr, m) => {
    const variance = arr.reduce((acc, val) => acc + Math.pow(val - m, 2), 0) / arr.length;
    return Math.round(Math.sqrt(variance) * 10) / 10;
  };

  const meanRps = mean(rpsList);
  return {
    meanRps,
    stdDevRps: stddev(rpsList, meanRps),
    meanP50: mean(p50List),
    meanP95: mean(p95List),
    meanP99: mean(p99List),
    meanWorkingSetMb: mean(ramList),
    meanAcquireWaitMs: mean(acqAvgList),
    maxAcquireWaitMs: Math.max(...acqMaxList),
    meanQueryExecMs: mean(qAvgList),
    maxQueryExecMs: Math.max(...qMaxList),
    totalErrors: trials.reduce((acc, t) => acc + t.load.errors, 0),
  };
}

async function main() {
  console.log('================================================================');
  console.log('EVENTPRO RECONCILED BENCHMARK & EXPERIMENTAL INTEGRITY SUITE');
  console.log('================================================================');

  const binaryHash = getFileSha256(EXE_PATH);
  console.log(`Binary Hash (SHA-256) : ${binaryHash}`);
  console.log(`Binary Path           : ${EXE_PATH}`);
  console.log(`Duration per Trial    : ${DURATION_S}s | Warmup: ${WARMUP_S}s`);
  console.log(`Logging Overhead Mode : RUST_LOG=warn, stdio: ignore (isolated)`);
  console.log(`Telemetry Mode        : Isolated Per-Trial Resets (/metrics/db/reset)`);

  const results = {
    metadata: {
      timestamp: new Date().toISOString(),
      binaryHash,
      binaryPath: EXE_PATH,
      durationSeconds: DURATION_S,
      warmupSeconds: WARMUP_S,
      host: {
        platform: 'win32-arm64',
        cpu: 'Snapdragon X Elite (8 cores)',
        database: 'MySQL 8.0.44 port 3306',
      },
    },
    part1_csrf_controlled: {},
    part2_pool_tuning: {},
  };

  // -------------------------------------------------------------
  // PART 1: Controlled Experiment (Alternating Order across 5 Trials)
  // -------------------------------------------------------------
  console.log('\n----------------------------------------------------------------');
  console.log('PART 1: Controlled Experiment (5 Trials Alternating Order)');
  console.log('----------------------------------------------------------------');

  const baselineConfig = { configName: 'baseline_with_csrf', envVars: { BASELINE_CSRF: '1', DB_POOL_MAX: '10' } };
  const optimizedConfig = { configName: 'optimized_zero_csrf', envVars: { BASELINE_CSRF: '0', DB_POOL_MAX: '10' } };

  for (const concurrency of [1, 10]) {
    console.log(`\n>>> Testing Concurrency = ${concurrency} VUs (5 alternating trials) <<<`);
    const baselineTrials = [];
    const optimizedTrials = [];

    for (let trial = 1; trial <= 5; trial++) {
      console.log(`  Trial ${trial}/5:`);
      // Alternate execution order to eliminate thermal or sequence drift
      if (trial % 2 === 1) {
        // Order: Baseline first, then Optimized
        const b = await runSingleTrial({ ...baselineConfig, concurrency, trialIndex: trial });
        baselineTrials.push(b);
        const o = await runSingleTrial({ ...optimizedConfig, concurrency, trialIndex: trial });
        optimizedTrials.push(o);
      } else {
        // Order: Optimized first, then Baseline
        const o = await runSingleTrial({ ...optimizedConfig, concurrency, trialIndex: trial });
        optimizedTrials.push(o);
        const b = await runSingleTrial({ ...baselineConfig, concurrency, trialIndex: trial });
        baselineTrials.push(b);
      }
    }

    results.part1_csrf_controlled[`concurrency_${concurrency}`] = {
      concurrency,
      baseline: {
        trials: baselineTrials,
        summary: computeSummary(baselineTrials),
      },
      optimized: {
        trials: optimizedTrials,
        summary: computeSummary(optimizedTrials),
      },
    };
  }

  // -------------------------------------------------------------
  // PART 2: Pool Size Tuning Retest (10, 20, 30 across 3 Alternating Rounds)
  // -------------------------------------------------------------
  console.log('\n----------------------------------------------------------------');
  console.log('PART 2: Pool Size Retest (Limits 10, 20, 30 in Alternating Rounds)');
  console.log('----------------------------------------------------------------');

  const poolSizes = [10, 20, 30];
  const poolTrials = { 10: [], 20: [], 30: [] };

  // Three alternating rounds:
  // Round 1: 10, 20, 30
  // Round 2: 30, 20, 10
  // Round 3: 20, 30, 10
  const orders = [
    [10, 20, 30],
    [30, 20, 10],
    [20, 30, 10],
  ];

  for (let round = 0; round < orders.length; round++) {
    const roundOrder = orders[round];
    console.log(`\n  Round ${round + 1}/3 - Test Order: [${roundOrder.join(', ')}]`);
    for (const size of roundOrder) {
      console.log(`    Testing Pool Limit = ${size}...`);
      const trialRes = await runSingleTrial({
        configName: `pool_size_${size}`,
        envVars: { BASELINE_CSRF: '0', DB_POOL_MAX: String(size) },
        concurrency: 10,
        trialIndex: round + 1,
      });

      // Verify effective setting
      if (trialRes.configuredMax !== size) {
        console.error(`CONFIGURATION SCOPE ERROR: Expected configured_max=${size}, got ${trialRes.configuredMax}`);
      }

      trialRes.mysqlThreadsConnected = await getMysqlThreadsConnected();
      poolTrials[size].push(trialRes);
    }
  }

  results.part2_pool_tuning = {
    concurrency: 10,
    pool_10: {
      limit: 10,
      trials: poolTrials[10],
      summary: computeSummary(poolTrials[10]),
    },
    pool_20: {
      limit: 20,
      trials: poolTrials[20],
      summary: computeSummary(poolTrials[20]),
    },
    pool_30: {
      limit: 30,
      trials: poolTrials[30],
      summary: computeSummary(poolTrials[30]),
    },
  };

  const outPath = path.join(__dirname, 'results', 'reconciled_benchmark_results.json');
  fs.writeFileSync(outPath, JSON.stringify(results, null, 2));
  console.log('\n================================================================');
  console.log(`Reconciled Benchmark Completed! Results saved to: ${outPath}`);
  console.log('================================================================');
}

main().catch(err => {
  console.error('Fatal error in reconciled benchmark:', err);
  process.exit(1);
});
