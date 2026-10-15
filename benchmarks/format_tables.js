const fs = require('fs');
const path = require('path');

const axum = JSON.parse(fs.readFileSync(path.join(__dirname, 'results', 'raw_axum.json')));
const axumBefore = JSON.parse(fs.readFileSync(path.join(__dirname, 'results', 'raw_axum_before_fix.json')));
const express = JSON.parse(fs.readFileSync(path.join(__dirname, 'results', 'raw_express.json')));

function getStats(dataset) {
  const map = {};
  for (const r of dataset.results) {
    const key = `${r.route}__c${r.concurrency}`;
    if (!map[key]) map[key] = [];
    map[key].push(r);
  }
  const summary = {};
  for (const [key, runs] of Object.entries(map)) {
    const n = runs.length;
    const rpsValues = runs.map(r => r.rps);
    const meanRps = rpsValues.reduce((a, b) => a + b, 0) / n;
    const stdDevRps = Math.sqrt(rpsValues.reduce((a, b) => a + Math.pow(b - meanRps, 2), 0) / n);
    const p50 = runs.reduce((a, b) => a + b.latency.p50, 0) / n;
    const p95 = runs.reduce((a, b) => a + b.latency.p95, 0) / n;
    const p99 = runs.reduce((a, b) => a + b.latency.p99, 0) / n;
    const ws = runs.reduce((a, b) => a + b.memoryRssMb, 0) / n;
    const errors = runs.reduce((a, b) => a + b.errors, 0);
    summary[key] = {
      route: runs[0].route,
      concurrency: runs[0].concurrency,
      runs: runs.map(r => ({
        run: r.run,
        rps: r.rps,
        p50: r.latency.p50,
        p95: r.latency.p95,
        p99: r.latency.p99,
        ws: r.memoryRssMb,
        errors: r.errors
      })),
      meanRps: meanRps.toFixed(1),
      stdDevRps: stdDevRps.toFixed(1),
      p50: p50.toFixed(1),
      p95: p95.toFixed(1),
      p99: p99.toFixed(1),
      ws: ws.toFixed(1),
      errors
    };
  }
  return summary;
}

const axumStats = getStats(axum);
const axumBeforeStats = getStats(axumBefore);
const expressStats = getStats(express);

fs.writeFileSync(path.join(__dirname, 'results', 'formatted_stats.json'), JSON.stringify({ axumStats, axumBeforeStats, expressStats }, null, 2));
console.log('Saved formatted_stats.json');
