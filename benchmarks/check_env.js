const { execSync } = require('child_process');
const out = execSync('powershell "Get-Process | Where-Object { $_.ProcessName -match \'node|eventpro|mysqld|cargo|rust|MsMpEng\' } | Select-Object Id, ProcessName, CPU, WorkingSet64 | ConvertTo-Json"').toString();
console.log(out);
