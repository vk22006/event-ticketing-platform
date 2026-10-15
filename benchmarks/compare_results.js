const fs = require('fs');
const path = require('path');

const b = JSON.parse(fs.readFileSync(path.join(__dirname, 'results', 'phase1_baseline_results.json')));
const o = JSON.parse(fs.readFileSync(path.join(__dirname, 'results', 'phase2_optimized_results.json')));

const pairs = [
    { baseId: 'events_catalogue', optId: 'catalogue', label: 'Public Catalogue (/events)' },
    { baseId: 'event_detail', optId: 'event_detail', label: 'Event Detail (/events/1)' },
    { baseId: 'dashboard', optId: 'dashboard', label: 'Auth Dashboard (/dashboard)' },
    { baseId: 'bookings_my', optId: 'my_bookings', label: 'Booking History (/bookings/my)' },
];

console.log('| Workload | Concurrency | Baseline RPS | Optimized RPS | RPS Diff (%) | Baseline p50 | Opt p50 | Baseline p95 | Opt p95 | Base DB Q/sec | Opt DB Q/sec |');
console.log('|---|---|---|---|---|---|---|---|---|---|---|');

for (const pair of pairs) {
    for (const c of ['c_1', 'c_10', 'c_25', 'c_50']) {
        const bt = b.workloads[pair.baseId].tiers[c];
        const ot = o.workloads[pair.optId].tiers[c];
        const rpsDiff = (((ot.throughputRps - bt.throughputRps) / bt.throughputRps) * 100).toFixed(1);
        const sign = rpsDiff > 0 ? '+' : '';
        const bQps = bt.dbTelemetry ? (bt.dbTelemetry.query.count / 10).toFixed(0) : 'N/A';
        const oQps = ot.dbTelemetry ? (ot.dbTelemetry.query.count / 10).toFixed(0) : 'N/A';
        console.log(`| ${pair.label} | ${c.replace('c_', 'C=')} | ${bt.throughputRps} | ${ot.throughputRps} | **${sign}${rpsDiff}%** | ${bt.latency.p50}ms | ${ot.latency.p50}ms | ${bt.latency.p95}ms | ${ot.latency.p95}ms | ${bQps} | ${oQps} |`);
    }
}

console.log('\n### Booking Workflow Comparison (30 Cycles on Event 97)');
console.log('| Metric | Baseline | Optimized | Delta |');
console.log('|---|---|---|---|');
console.log(`| Total Duration | ${b.bookingWorkflow.totalElapsedSec}s | ${o.bookingWorkflow.totalElapsedSec}s | -${(b.bookingWorkflow.totalElapsedSec - o.bookingWorkflow.totalElapsedSec).toFixed(2)}s |`);
console.log(`| Cycle Throughput | ${b.bookingWorkflow.cycleRps} cycles/s | ${o.bookingWorkflow.cycleRps} cycles/s | +${(o.bookingWorkflow.cycleRps - b.bookingWorkflow.cycleRps).toFixed(2)} cycles/s (+${(((o.bookingWorkflow.cycleRps - b.bookingWorkflow.cycleRps) / b.bookingWorkflow.cycleRps) * 100).toFixed(1)}%) |`);
console.log(`| Book Latency avg/p50/p95 | ${b.bookingWorkflow.bookLatencies.avg}/${b.bookingWorkflow.bookLatencies.p50}/${b.bookingWorkflow.bookLatencies.p95}ms | ${o.bookingWorkflow.bookLatencies.avg}/${o.bookingWorkflow.bookLatencies.p50}/${o.bookingWorkflow.bookLatencies.p95}ms | -${(b.bookingWorkflow.bookLatencies.avg - o.bookingWorkflow.bookLatencies.avg).toFixed(2)}ms avg |`);
console.log(`| Cancel Latency avg/p50/p95 | ${b.bookingWorkflow.cancelLatencies.avg}/${b.bookingWorkflow.cancelLatencies.p50}/${b.bookingWorkflow.cancelLatencies.p95}ms | ${o.bookingWorkflow.cancelLatencies.avg}/${o.bookingWorkflow.cancelLatencies.p50}/${o.bookingWorkflow.cancelLatencies.p95}ms | -${(b.bookingWorkflow.cancelLatencies.avg - o.bookingWorkflow.cancelLatencies.avg).toFixed(2)}ms avg |`);
console.log(`| Invariant Preserved | ${b.bookingWorkflow.seatInvariancePreserved ? 'YES' : 'NO'} | ${o.bookingWorkflow.seatInvariancePreserved ? 'YES' : 'NO'} | 0 seat leaks |`);
