const { execSync } = require('child_process');

console.log('=== LISTENING PORTS ===');
try {
  const netstat = execSync('netstat -ano | findstr "LISTENING" | findstr "3000 3001 3306"').toString();
  console.log(netstat);
} catch (e) {
  console.log('Error/None:', e.message);
}

console.log('=== MYSQL PROCESSES ===');
try {
  const wmic = execSync('wmic process where "name like \'%mysql%\'" get ProcessId,CommandLine /format:list').toString();
  console.log(wmic);
} catch (e) {
  console.log(e.message);
}

console.log('=== DEFENDER ACTIVITY ===');
try {
  const defender = execSync('powershell "Get-MpComputerStatus | Select-Object AntivirusEnabled, RealTimeProtectionEnabled, AntivirusSignatureAge"').toString();
  console.log(defender);
} catch (e) {
  console.log(e.message);
}
