const fs = require('fs');
const path = require('path');

const dataFile = path.join(__dirname, 'results', 'phase3_alternating_validation.json');
const raw = JSON.parse(fs.readFileSync(dataFile, 'utf8'));

const workloads = [
    { id: 'dashboard', name: 'Auth Dashboard (/dashboard)' },
    { id: 'my_bookings', name: 'Booking History (/bookings/my)' },
    { id: 'catalogue', name: 'Public Catalogue (/events)' },
    { id: 'event_detail', name: 'Event Detail (/events/1)' },
];

const concurrencies = [1, 10, 25, 50];

function calcStats(arr) {
    if (arr.length === 0) return { mean: 0, stdev: 0, min: 0, max: 0 };
    const mean = arr.reduce((a, b) => a + b, 0) / arr.length;
    const variance = arr.reduce((a, b) => a + Math.pow(b - mean, 2), 0) / (arr.length > 1 ? (arr.length - 1) : 1);
    const stdev = Math.sqrt(variance);
    return {
        mean: parseFloat(mean.toFixed(1)),
        stdev: parseFloat(stdev.toFixed(1)),
        min: Math.min(...arr),
        max: Math.max(...arr),
    };
}

console.log('========================================================================================================');
console.log('PHASE 3 ALTERNATING MULTI-TRIAL BENCHMARK ANALYSIS (5 Alternating Rounds, N=5 per config)');
console.log('========================================================================================================\n');

const summaryTable = [];

for (const wl of workloads) {
    console.log(`### Workload: ${wl.name}`);
    console.log('| C  | Baseline Mean ± Stdev (RPS) | Optimized Mean ± Stdev (RPS) | Throughput Δ (%) | Base p50/p95 | Opt p50/p95 | Base Qs | Opt Qs | Q Reduc (%) |');
    console.log('|----|-----------------------------|------------------------------|------------------|--------------|-------------|---------|--------|-------------|');

    for (const c of concurrencies) {
        const key = `c_${c}`;
        const baseRps = [];
        const optRps = [];
        const baseP50 = [];
        const optP50 = [];
        const baseP95 = [];
        const optP95 = [];
        const baseQs = [];
        const optQs = [];

        for (const round of raw.rounds) {
            const b = round.data.baseline[wl.id][key];
            const o = round.data.optimized[wl.id][key];
            baseRps.push(b.throughputRps);
            optRps.push(o.throughputRps);
            baseP50.push(b.p50);
            optP50.push(o.p50);
            baseP95.push(b.p95);
            optP95.push(o.p95);
            baseQs.push(b.dbQueries);
            optQs.push(o.dbQueries);
        }

        const bStats = calcStats(baseRps);
        const oStats = calcStats(optRps);
        const bP50Avg = (baseP50.reduce((a, b) => a + b, 0) / baseP50.length).toFixed(1);
        const oP50Avg = (optP50.reduce((a, b) => a + b, 0) / optP50.length).toFixed(1);
        const bP95Avg = (baseP95.reduce((a, b) => a + b, 0) / baseP95.length).toFixed(1);
        const oP95Avg = (optP95.reduce((a, b) => a + b, 0) / optP95.length).toFixed(1);
        const bQsAvg = Math.round(baseQs.reduce((a, b) => a + b, 0) / baseQs.length);
        const oQsAvg = Math.round(optQs.reduce((a, b) => a + b, 0) / optQs.length);

        const deltaPct = (((oStats.mean - bStats.mean) / bStats.mean) * 100).toFixed(1);
        const qReducPct = bQsAvg > 0 ? (((bQsAvg - oQsAvg) / bQsAvg) * 100).toFixed(1) : '0.0';

        console.log(`| ${String(c).padEnd(2)} | ${String(bStats.mean).padStart(7)} ± ${String(bStats.stdev).padEnd(5)} (${bStats.min}-${bStats.max}) | ${String(oStats.mean).padStart(7)} ± ${String(oStats.stdev).padEnd(5)} (${oStats.min}-${oStats.max}) | ${deltaPct.padStart(6)}% | ${bP50Avg}ms / ${bP95Avg}ms | ${oP50Avg}ms / ${oP95Avg}ms | ${String(bQsAvg).padStart(7)} | ${String(oQsAvg).padStart(6)} | ${qReducPct.padStart(6)}% |`);

        summaryTable.push({
            workload: wl.id,
            concurrency: c,
            baseMean: bStats.mean,
            baseStdev: bStats.stdev,
            optMean: oStats.mean,
            optStdev: oStats.stdev,
            deltaPct: parseFloat(deltaPct),
            baseP50: parseFloat(bP50Avg),
            optP50: parseFloat(oP50Avg),
            baseP95: parseFloat(bP95Avg),
            optP95: parseFloat(oP95Avg),
            baseQs: bQsAvg,
            optQs: oQsAvg,
            qReducPct: parseFloat(qReducPct),
        });
    }
    console.log('\n');
}

fs.writeFileSync(path.join(__dirname, 'results', 'phase3_summary.json'), JSON.stringify(summaryTable, null, 2), 'utf8');
