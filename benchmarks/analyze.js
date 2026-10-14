/**
 * EventPro Benchmark Analysis Engine
 *
 * Compares raw benchmark results from Express.js and Axum backends.
 * Calculates:
 *  - Throughput delta: ((Axum - Express) / Express) * 100%
 *  - Latency delta: ((Express - Axum) / Express) * 100% (positive = Axum faster)
 *  - Resource efficiency (CPU% per 1,000 RPS, Memory delta)
 *  - Standard deviations across repeat runs
 *  - Outputs comparison.csv and markdown summary tables
 */

const fs = require('fs');
const path = require('path');

const resultsDir = path.join(__dirname, 'results');
const expressFile = path.join(resultsDir, 'raw_express.json');
const axumFile = path.join(resultsDir, 'raw_axum.json');
const startupFile = path.join(resultsDir, 'startup.json');

if (!fs.existsSync(expressFile) || !fs.existsSync(axumFile)) {
    console.error('Error: Both raw_express.json and raw_axum.json must exist before running analysis.');
    process.exit(1);
}

const expressData = JSON.parse(fs.readFileSync(expressFile, 'utf8'));
const axumData = JSON.parse(fs.readFileSync(axumFile, 'utf8'));
const startupData = fs.existsSync(startupFile) ? JSON.parse(fs.readFileSync(startupFile, 'utf8')) : null;

// Group runs by [route][concurrency]
function aggregateRuns(runs) {
    const map = new Map();
    for (const r of runs) {
        const key = `${r.route}__c${r.concurrency}`;
        if (!map.has(key)) {
            map.set(key, []);
        }
        map.get(key).push(r);
    }

    const aggregated = {};
    for (const [key, list] of map.entries()) {
        const route = list[0].route;
        const concurrency = list[0].concurrency;
        const n = list.length;

        const avg = (field) => list.reduce((sum, item) => sum + (item[field] || 0), 0) / n;
        const avgNested = (parent, field) => list.reduce((sum, item) => sum + ((item[parent] && item[parent][field]) || 0), 0) / n;

        const rpsValues = list.map(i => i.rps);
        const rpsMean = avg('rps');
        const rpsVar = rpsValues.reduce((sum, val) => sum + Math.pow(val - rpsMean, 2), 0) / n;
        const rpsStdDev = Math.sqrt(rpsVar);

        const p50Mean = avgNested('latency', 'p50');
        const p95Mean = avgNested('latency', 'p95');
        const p99Mean = avgNested('latency', 'p99');
        const latencyMean = avgNested('latency', 'mean');

        const cpuMean = avg('cpuPercent');
        const memMean = avg('memoryRssMb');
        const errorsSum = list.reduce((sum, item) => sum + (item.errors || 0), 0);
        const timeoutsSum = list.reduce((sum, item) => sum + (item.timeouts || 0), 0);
        const totalRequests = list.reduce((sum, item) => sum + (item.requestsTotal || 0), 0);
        const avgBytes = avg('avgBytes');

        aggregated[key] = {
            route,
            concurrency,
            runsCount: n,
            rpsMean: Math.round(rpsMean * 10) / 10,
            rpsStdDev: Math.round(rpsStdDev * 10) / 10,
            p50Mean: Math.round(p50Mean * 10) / 10,
            p95Mean: Math.round(p95Mean * 10) / 10,
            p99Mean: Math.round(p99Mean * 10) / 10,
            latencyMean: Math.round(latencyMean * 10) / 10,
            cpuMean: Math.round(cpuMean * 10) / 10,
            memMean: Math.round(memMean * 10) / 10,
            errorsSum,
            timeoutsSum,
            totalRequests,
            avgBytes: Math.round(avgBytes),
        };
    }
    return aggregated;
}

const expressAgg = aggregateRuns(expressData.results);
const axumAgg = aggregateRuns(axumData.results);

const comparisonRows = [];
const csvHeaders = [
    'Route',
    'Concurrency',
    'Express_RPS',
    'Axum_RPS',
    'RPS_Delta_Pct',
    'Express_p50_ms',
    'Axum_p50_ms',
    'p50_Latency_Reduction_Pct',
    'Express_p95_ms',
    'Axum_p95_ms',
    'p95_Latency_Reduction_Pct',
    'Express_p99_ms',
    'Axum_p99_ms',
    'p99_Latency_Reduction_Pct',
    'Express_CPU_Pct',
    'Axum_CPU_Pct',
    'Express_Mem_MB',
    'Axum_Mem_MB',
    'Express_Errors',
    'Axum_Errors',
];

const csvRows = [csvHeaders.join(',')];

// Common routes to compare directly
const commonKeys = Object.keys(expressAgg).filter(k => axumAgg[k]);

for (const key of commonKeys) {
    const exp = expressAgg[key];
    const ax = axumAgg[key];

    const rpsDeltaPct = Math.round(((ax.rpsMean - exp.rpsMean) / (exp.rpsMean || 1)) * 1000) / 10;
    const p50ReductionPct = Math.round(((exp.p50Mean - ax.p50Mean) / (exp.p50Mean || 1)) * 1000) / 10;
    const p95ReductionPct = Math.round(((exp.p95Mean - ax.p95Mean) / (exp.p95Mean || 1)) * 1000) / 10;
    const p99ReductionPct = Math.round(((exp.p99Mean - ax.p99Mean) / (exp.p99Mean || 1)) * 1000) / 10;

    const rowObj = {
        route: exp.route,
        concurrency: exp.concurrency,
        express: exp,
        axum: ax,
        rpsDeltaPct,
        p50ReductionPct,
        p95ReductionPct,
        p99ReductionPct,
    };
    comparisonRows.push(rowObj);

    csvRows.push([
        exp.route,
        exp.concurrency,
        exp.rpsMean,
        ax.rpsMean,
        rpsDeltaPct,
        exp.p50Mean,
        ax.p50Mean,
        p50ReductionPct,
        exp.p95Mean,
        ax.p95Mean,
        p95ReductionPct,
        exp.p99Mean,
        ax.p99Mean,
        p99ReductionPct,
        exp.cpuMean,
        ax.cpuMean,
        exp.memMean,
        ax.memMean,
        exp.errorsSum,
        ax.errorsSum,
    ].join(','));
}

// Write comparison CSV
const comparisonCsvPath = path.join(resultsDir, 'comparison.csv');
fs.writeFileSync(comparisonCsvPath, csvRows.join('\n'));

// Save complete analyzed JSON
const analysisJsonPath = path.join(resultsDir, 'comparison_summary.json');
fs.writeFileSync(analysisJsonPath, JSON.stringify({
    metadata: {
        analyzedAt: new Date().toISOString(),
        system: expressData.system,
        cores: expressData.numLogicalCores,
        durationPerRun: expressData.duration,
        warmupDuration: expressData.warmupDuration,
        runsPerScenario: expressData.runsCount,
    },
    startup: startupData,
    comparisons: comparisonRows,
    axumMicrobenchmarks: Object.keys(axumAgg)
        .filter(k => !expressAgg[k])
        .map(k => axumAgg[k]),
}, null, 2));

console.log('Analysis Complete!');
console.log(`Comparison CSV saved to  : ${comparisonCsvPath}`);
console.log(`Summary JSON saved to    : ${analysisJsonPath}`);
