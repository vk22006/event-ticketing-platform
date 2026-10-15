const fs = require('fs');
const { axumStats, axumBeforeStats, expressStats } = JSON.parse(fs.readFileSync('benchmarks/results/formatted_stats.json'));

console.log('=== AXUM BEFORE VS AFTER CSRF FIX ===');
console.log('Key'.padEnd(22) + ' | ' + 'Express'.padStart(8) + ' | ' + 'Before'.padStart(8) + ' | ' + 'Post-Fix'.padStart(8) + ' | ' + 'p50 Before -> Post' + ' | ' + 'Mem Before -> Post');
for (const key of Object.keys(axumStats)) {
  const b = axumBeforeStats[key];
  const a = axumStats[key];
  const e = expressStats[key];
  const eStr = e ? e.meanRps.padStart(8) : '     N/A';
  const bStr = b ? b.meanRps.padStart(8) : '     N/A';
  const aStr = a.meanRps.padStart(8);
  const p50Str = (b ? b.p50.padStart(5) : '  N/A') + ' -> ' + a.p50.padStart(5);
  const memStr = (b ? b.ws.padStart(5) : '  N/A') + ' -> ' + a.ws.padStart(5);
  console.log(key.padEnd(22) + ' | ' + eStr + ' | ' + bStr + ' | ' + aStr + ' | ' + p50Str + ' | ' + memStr);
}
