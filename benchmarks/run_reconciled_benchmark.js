// benchmarks/run_reconciled_benchmark.js
// Reconciles benchmark harnesses and executes:
// 1. Alternating 5-trial controlled test of Baseline vs Optimized CSRF configurations (C=1 and C=10)
// 2. Multi-trial alternating pool tuning across sizes 10, 20, and 30 with configuration scope verification

const http = require('http');
const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');
const {
  RS_ROOT,
  getDb,
  computeSha256,
  waitForUrl,
  httpRequest,
  killProcess,
  killNamedProcesses,
  truncateMysqlSessions,
  getMysqlSessionCount,
  resetDbTelemetry,
  getDbTelemetry,
  getProcessMetrics,
} = require('./utils');

const BASE_URL = 'http://127.0.0.1:3001';
const DURATION_S = 10;
const WARMUP_S = 3;
const EXE_PATH = path.resolve(RS_ROOT, 'target', 'release', 'eventpro.exe');

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function getMysqlThreadsConnected() {
  try {
    const conn = await getDb();
    const [rows] = await conn.query("SHOW GLOBAL STATUS LIKE 'Threads_connected'");
    await conn.end();
    return parseInt(rows[0].Value, 10);
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
    const agent = new http.Agent({ keepAlive: true, maxSockets: concurrency * 2 });
    const start = Date.now();
    const stopTime = start + durationSec * 1000;

    function worker() {
      if (!running || Date.now() >= stopTime) return;
      const reqStart = process.hrtime.bigint();
      const req = http.get(
        { hostname: url.hostname, port: url.port, path: url.pathname + url.search, agent, timeout: 5000 },
        (res) => {
          res.on('data', () => {});
          res.on('end', () => {
            const elapsedMs = Number(process.hrtime.bigint() - reqStart) / 1e6;
            if (res.statusCode === 200) latencies.push(elapsedMs);
            else errors++;
            if (running) setImmediate(worker);
          });
        }
      );
      req.on('error', () => { errors++; if (running) setImmediate(worker); });
      req.on('timeout', () => { req.destroy(); errors++; if (running) setImmediate(worker); });
    }

    for (let i = 0; i < concurrency; i++) worker();

    const timer = setInterval(() => {
      if (Date.now() >= stopTime) {
        running = false;
        clearInterval(timer);
        agent.destroy();
        const totalDurationSec = (Date.now() - start) / 1000;
        latencies.sort((a, b) => a - b);
        resolve({
          rps: Math.round((latencies.length / totalDurationSec) * 10) / 10,
          p50: Math.round((latencies[Math.floor(latencies.length * 0.5)] || 0) * 10) / 10,
          p95: Math.round((latencies[Math.floor(latencies.length * 0.95)] || 0) * 10) / 10,
          p99: Math.round((latencies[Math.floor(latencies.length * 0.99)] || 0) * 10) / 10,
          requests: latencies.length,
          errors,
        });
      }
    }, 100);
  });
}

async function runSingleTrial({ configName, envVars, concurrency, trialIndex }) {
  killNamedProcesses();
  await sleep(1000);
  await truncateMysqlSessions();

  const env = { ...process.env, RUST_LOG: 'warn', PORT: '3001', ...envVars };
  const proc = spawn(EXE_PATH, [], { env, cwd: RS_ROOT, stdio: 'ignore' });
  const pid = proc.pid;
  await waitForUrl(`${BASE_URL}/health`, 15000);

  const initialMetrics = await getDbTelemetry(3001);
  const configuredMax = initialMetrics && initialMetrics.pool ? initialMetrics.pool.configured_max : null;

  await runLoad(concurrency, WARMUP_S);
  await resetDbTelemetry(3001);

  const loadStats = await runLoad(concurrency, DURATION_S);
  const metrics = getProcessMetrics(pid);
  const workingSetMb = metrics ? Math.round((metrics.workingSetBytes / (1024 * 1024)) * 100) / 100 : 0;

  const trialMetrics = await getDbTelemetry(3001);
  const sessionCount = await getMysqlSessionCount();

  await killProcess(proc);
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

  console.log(`    [Trial ${trialIndex}] ${configName} @ C=${concurrency}: ${loadStats.rps} RPS | p50: ${loadStats.p50}ms | p95: ${loadStats.p95}ms | p99: ${loadStats.p99}ms | RAM: ${workingSetMb}MB | DB Acquire Avg: ${trialMetrics.acquire.avg_ms}ms | DB Query Avg: ${trialMetrics.query.avg_ms}ms | Sessions: ${sessionCount}`);
  return trialResult;
}

function computeSummary(trials) {
  const rpsList = trials.map((t) => t.load.rps);
  const p50List = trials.map((t) => t.load.p50);
  const p95List = trials.map((t) => t.load.p95);
  const p99List = trials.map((t) => t.load.p99);
  const ramList = trials.map((t) => t.workingSetMb);
  const acqAvgList = trials.map((t) => t.dbTelemetry.acquire.avg_ms);
  const acqMaxList = trials.map((t) => t.dbTelemetry.acquire.max_ms);
  const qAvgList = trials.map((t) => t.dbTelemetry.query.avg_ms);
  const qMaxList = trials.map((t) => t.dbTelemetry.query.max_ms);

  const mean = (arr) => Math.round((arr.reduce((a, b) => a + b, 0) / arr.length) * 10) / 10;
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

  const binaryHash = computeSha256(EXE_PATH);
  console.log(`Binary Hash (SHA-256) : ${binaryHash}`);
  console.log(`Binary Path           : ${EXE_PATH}`);
  console.log(`Duration per Trial    : ${DURATION_S}s | Warmup: ${WARMUP_S}s`);

  const results = {
    metadata: {
      timestamp: new Date().toISOString(),
      binaryHash,
      binaryPath: EXE_PATH,
      durationSeconds: DURATION_S,
      warmupSeconds: WARMUP_S,
      host: { platform: 'win32-arm64', cpu: 'Snapdragon X Elite (8 cores)', database: 'MySQL 8.0.44 port 3306' },
    },
    part1_csrf_controlled: {},
    part2_pool_tuning: {},
  };

  // PART 1: Controlled Experiment
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
      if (trial % 2 === 1) {
        baselineTrials.push(await runSingleTrial({ ...baselineConfig, concurrency, trialIndex: trial }));
        optimizedTrials.push(await runSingleTrial({ ...optimizedConfig, concurrency, trialIndex: trial }));
      } else {
        optimizedTrials.push(await runSingleTrial({ ...optimizedConfig, concurrency, trialIndex: trial }));
        baselineTrials.push(await runSingleTrial({ ...baselineConfig, concurrency, trialIndex: trial }));
      }
    }

    results.part1_csrf_controlled[`concurrency_${concurrency}`] = {
      concurrency,
      baseline: { trials: baselineTrials, summary: computeSummary(baselineTrials) },
      optimized: { trials: optimizedTrials, summary: computeSummary(optimizedTrials) },
    };
  }

  // PART 2: Pool Size Tuning Retest
  console.log('\n----------------------------------------------------------------');
  console.log('PART 2: Pool Size Retest (Limits 10, 20, 30 in Alternating Rounds)');
  console.log('----------------------------------------------------------------');

  const poolTrials = { 10: [], 20: [], 30: [] };
  const orders = [[10, 20, 30], [30, 20, 10], [20, 30, 10]];

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

      if (trialRes.configuredMax !== size) {
        console.error(`CONFIGURATION SCOPE ERROR: Expected configured_max=${size}, got ${trialRes.configuredMax}`);
      }
      trialRes.mysqlThreadsConnected = await getMysqlThreadsConnected();
      poolTrials[size].push(trialRes);
    }
  }

  results.part2_pool_tuning = {
    concurrency: 10,
    pool_10: { limit: 10, trials: poolTrials[10], summary: computeSummary(poolTrials[10]) },
    pool_20: { limit: 20, trials: poolTrials[20], summary: computeSummary(poolTrials[20]) },
    pool_30: { limit: 30, trials: poolTrials[30], summary: computeSummary(poolTrials[30]) },
  };

  const outPath = path.join(__dirname, 'results', 'reconciled_benchmark_results.json');
  fs.writeFileSync(outPath, JSON.stringify(results, null, 2));
  console.log('\n================================================================');
  console.log(`Reconciled Benchmark Completed! Results saved to: ${outPath}`);
  console.log('================================================================');
}

main().catch((err) => {
  console.error('Fatal error in reconciled benchmark:', err);
  process.exit(1);
});
