/**
 * Controlled Benchmark Runner for Directive 3 & Directive 2
 * Tests /events at concurrency 1 and 10 across 5 trials each for:
 * 1. Baseline (with pre-fix session creation on public GETs: BASELINE_CSRF=1)
 * 2. Optimized (post-fix with zero session creation)
 *
 * Records:
 * - Throughput (RPS), Latencies (p50, p95, p99)
 * - Process Working Set memory
 * - Database pool acquisition wait time & distribution
 * - Database query duration & distribution
 * - Errors & timeouts
 */

const { spawn, execSync } = require('child_process');
const http = require('http');
const path = require('path');
const fs = require('fs');
const autocannon = require('autocannon');

const BASE_URL = 'http://127.0.0.1:3001';
const BINARY_PATH = path.resolve(__dirname, '..', 'eventpro-rs', 'target', 'release', 'eventpro.exe');
const DURATION = 10;
const WARMUP = 3;
const TRIALS = 5;

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function httpJson(urlStr, method = 'GET', postData = null) {
  return new Promise((resolve, reject) => {
    const urlObj = new URL(urlStr);
    const options = {
      hostname: urlObj.hostname,
      port: urlObj.port,
      path: urlObj.pathname,
      method,
      timeout: 4000,
      headers: {
        'Connection': 'close',
      },
    };
    if (postData) {
      options.headers['Content-Type'] = 'application/json';
      options.headers['Content-Length'] = Buffer.byteLength(postData);
    }
    const req = http.request(options, res => {
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
    req.on('timeout', () => {
      req.destroy(new Error('Request timed out'));
    });
    if (postData) {
      req.write(postData);
    }
    req.end();
  });
}

async function waitForReady(maxWaitMs = 20000) {
  const start = Date.now();
  await sleep(1500);
  while (Date.now() - start < maxWaitMs) {
    try {
      const res = await httpJson(`${BASE_URL}/health`);
      if (res && res.status === 'ok') {
        console.log(`[Process] Health check passed: ${JSON.stringify(res)}`);
        return true;
      }
    } catch (err) {
      console.log(`[Process] Waiting for server... (${err.message})`);
    }
    await sleep(1000);
  }
  throw new Error('Server failed to become ready within ' + maxWaitMs + 'ms');
}

function getProcessMetrics(pid) {
  if (!pid) return { cpuSeconds: 0, workingSetMb: 0 };
  try {
    const cmd = `powershell -NoProfile -Command "Get-Process -Id ${pid} | Select-Object -Property Id, CPU, WorkingSet64 | ConvertTo-Json -Compress"`;
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

function runAutocannon(endpoint, concurrency, durationSec) {
  return new Promise((resolve, reject) => {
    const instance = autocannon({
      url: `${BASE_URL}${endpoint}`,
      connections: concurrency,
      duration: durationSec,
      pipelining: 1,
      excludeErrorStats: false,
    }, (err, results) => {
      if (err) return reject(err);
      resolve(results);
    });
    autocannon.track(instance, { renderProgressBar: false, renderResultsTable: false });
  });
}

async function runScenario(scenarioName, envVars) {
  console.log(`\n================================================================`);
  console.log(`STARTING SCENARIO: ${scenarioName.toUpperCase()}`);
  console.log(`================================================================`);

  // Truncate tower_sessions before starting
  try {
    const mysql = require('mysql2/promise');
    const conn = await mysql.createConnection('mysql://root:kishore2006@127.0.0.1:3306/tower_sessions');
    await conn.query('TRUNCATE TABLE session');
    await conn.end();
    console.log('[DB] tower_sessions.session truncated to 0 rows.');
  } catch (e) {
    console.warn('[DB Warning]', e.message);
  }

  try {
    execSync('taskkill /F /IM eventpro.exe 2>nul');
  } catch (_) {}
  await sleep(1000);

  // Spawn server process
  const childEnv = Object.assign({}, process.env, {
    PORT: '3001',
    APP_ENV: 'production',
  }, envVars);

  const serverProc = spawn(BINARY_PATH, [], {
    cwd: path.resolve(__dirname, '..', 'eventpro-rs'),
    env: childEnv,
    stdio: 'inherit',
  });

  const pid = serverProc.pid;
  console.log(`[Process] Spawned ${path.basename(BINARY_PATH)} with PID ${pid}`);

  await waitForReady();
  console.log(`[Process] Backend is ready on port 3001.`);

  const scenarioResults = {
    scenario: scenarioName,
    pid,
    concurrency_1: {},
    concurrency_10: {},
  };

  const concurrencies = [1, 10];

  for (const c of concurrencies) {
    console.log(`\n--- Testing /events at Concurrency: ${c} VUs (${TRIALS} trials, ${DURATION}s each) ---`);

    // Warm-up run
    process.stdout.write(`  [Warmup ${WARMUP}s] ... `);
    await runAutocannon('/events', c, WARMUP);
    console.log('done.');

    // Reset DB metrics
    await httpJson(`${BASE_URL}/metrics/db/reset`, 'POST');

    const trials = [];

    for (let t = 1; t <= TRIALS; t++) {
      process.stdout.write(`  [Trial ${t}/${TRIALS} (${DURATION}s)] ... `);
      const beforeMem = getProcessMetrics(pid).workingSetMb;
      const res = await runAutocannon('/events', c, DURATION);
      const afterMem = getProcessMetrics(pid).workingSetMb;

      const rps = Math.round(res.requests.average * 10) / 10;
      const p50 = res.latency.p50;
      const p95 = typeof res.latency.p95 === 'number' ? res.latency.p95 : res.latency.p90;
      const p99 = res.latency.p99;
      const errors = res.errors || 0;

      trials.push({
        trial: t,
        rps,
        p50,
        p95,
        p99,
        workingSetMb: afterMem,
        errors,
      });

      console.log(`RPS: ${rps} | p50: ${p50}ms | p95: ${p95}ms | p99: ${p99}ms | RAM: ${afterMem}MB | Err: ${errors}`);
    }

    // Capture DB metrics for this concurrency tier
    const dbMetrics = await httpJson(`${BASE_URL}/metrics/db`);
    console.log(`  [DB Instrumentation] Pool Acquire Wait: avg=${dbMetrics.acquire.avg_ms}ms, max=${dbMetrics.acquire.max_ms}ms, count=${dbMetrics.acquire.count}`);
    console.log(`  [DB Instrumentation] Query Exec Duration: avg=${dbMetrics.query.avg_ms}ms, max=${dbMetrics.query.max_ms}ms, count=${dbMetrics.query.count}`);

    const rpsValues = trials.map(t => t.rps);
    const meanRps = Math.round((rpsValues.reduce((a, b) => a + b, 0) / TRIALS) * 10) / 10;
    const stdDevRps = Math.round(Math.sqrt(rpsValues.reduce((a, b) => a + Math.pow(b - meanRps, 2), 0) / TRIALS) * 10) / 10;
    const avgP50 = Math.round((trials.reduce((a, b) => a + b.p50, 0) / TRIALS) * 10) / 10;
    const avgP95 = Math.round((trials.reduce((a, b) => a + b.p95, 0) / TRIALS) * 10) / 10;
    const avgP99 = Math.round((trials.reduce((a, b) => a + b.p99, 0) / TRIALS) * 10) / 10;
    const avgMem = Math.round((trials.reduce((a, b) => a + b.workingSetMb, 0) / TRIALS) * 10) / 10;

    scenarioResults[`concurrency_${c}`] = {
      concurrency: c,
      trials,
      summary: {
        meanRps,
        stdDevRps,
        p50: avgP50,
        p95: avgP95,
        p99: avgP99,
        workingSetMb: avgMem,
        totalErrors: trials.reduce((a, b) => a + b.errors, 0),
      },
      dbMetrics,
    };
  }

  // Count session rows left in DB
  try {
    const mysql = require('mysql2/promise');
    const conn = await mysql.createConnection('mysql://root:kishore2006@127.0.0.1:3306/tower_sessions');
    const [[countRow]] = await conn.query('SELECT COUNT(*) as cnt FROM session');
    scenarioResults.finalSessionRows = countRow.cnt;
    await conn.end();
    console.log(`[DB] Final session rows in tower_sessions: ${countRow.cnt}`);
  } catch (e) {
    scenarioResults.finalSessionRows = null;
  }

  // Terminate server process
  serverProc.kill();
  try {
    execSync(`taskkill /F /PID ${pid} 2>nul`);
  } catch (_) {}
  console.log(`[Process] Terminated PID ${pid}. Waiting 2s before next scenario...`);
  await sleep(2000);

  return scenarioResults;
}

async function main() {
  console.log('EventPro Controlled Test: 5 Trials per Concurrency (1 & 10) on /events');
  console.log(`Duration: ${DURATION}s per trial | Warmup: ${WARMUP}s`);

  // Run 1: Baseline (pre-fix session creation: BASELINE_CSRF=1)
  const baselineResults = await runScenario('baseline_with_csrf_session_creation', {
    BASELINE_CSRF: '1',
  });

  // Run 2: Optimized (post-fix zero session creation)
  const optimizedResults = await runScenario('optimized_zero_session_creation', {});

  const finalReport = {
    timestamp: new Date().toISOString(),
    duration: DURATION,
    warmup: WARMUP,
    trialsCount: TRIALS,
    baseline: baselineResults,
    optimized: optimizedResults,
  };

  const resultsPath = path.join(__dirname, 'results', 'controlled_test_5trials.json');
  fs.writeFileSync(resultsPath, JSON.stringify(finalReport, null, 2));
  console.log(`\n================================================================`);
  console.log(`Controlled Test Complete! Results saved to: ${resultsPath}`);
  console.log(`================================================================`);
}

main().catch(console.error);
