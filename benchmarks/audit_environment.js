const { execSync } = require('child_process');
const mysql = require('mysql2/promise');
const fs = require('fs');
const path = require('path');

async function auditEnvironment() {
  const audit = {
    timestamp: new Date().toISOString(),
    host: {
      platform: process.platform,
      arch: process.arch,
      nodeVersion: process.version,
    },
    mysql: {},
    defender: {},
    backgroundProcesses: []
  };

  // 1. MySQL Server Audit
  try {
    const conn = await mysql.createConnection('mysql://root:kishore2006@127.0.0.1:3306/event_lifecycle');
    const [[ver]] = await conn.query('SELECT VERSION() as v');
    audit.mysql.version = ver.v;
    const [[maxConn]] = await conn.query("SHOW VARIABLES LIKE 'max_connections'");
    audit.mysql.maxConnections = parseInt(maxConn.Value, 10);
    const [[poolSize]] = await conn.query("SHOW VARIABLES LIKE 'innodb_buffer_pool_size'");
    audit.mysql.bufferPoolSizeBytes = parseInt(poolSize.Value, 10);
    audit.mysql.bufferPoolSizeMb = Math.round(audit.mysql.bufferPoolSizeBytes / (1024 * 1024));
    const [[threadsConn]] = await conn.query("SHOW STATUS LIKE 'Threads_connected'");
    audit.mysql.threadsConnected = parseInt(threadsConn.Value, 10);
    const [[threadsRun]] = await conn.query("SHOW STATUS LIKE 'Threads_running'");
    audit.mysql.threadsRunning = parseInt(threadsRun.Value, 10);
    await conn.end();
  } catch (e) {
    audit.mysql.error = e.message;
  }

  // 2. Windows Defender
  try {
    const defOut = execSync('powershell "Get-MpComputerStatus | Select-Object AntivirusEnabled, RealTimeProtectionEnabled | ConvertTo-Json"').toString();
    audit.defender = JSON.parse(defOut);
  } catch (e) {
    audit.defender.error = e.message;
  }

  // 3. Competing Processes
  try {
    const psOut = execSync('powershell "Get-Process | Where-Object { $_.ProcessName -match \'node|eventpro|mysqld\' } | Select-Object Id, ProcessName, CPU, WorkingSet64 | ConvertTo-Json"').toString();
    audit.backgroundProcesses = JSON.parse(psOut);
  } catch (e) {
    audit.backgroundProcessesError = e.message;
  }

  // 4. Port Check
  try {
    const ports = execSync('netstat -ano | findstr "LISTENING" | findstr "3000 3001 3306"').toString().trim().split('\n').map(l => l.trim());
    audit.listeningPorts = ports;
  } catch (e) {
    audit.listeningPorts = [];
  }

  const outPath = path.join(__dirname, 'results', 'environment_audit.json');
  fs.writeFileSync(outPath, JSON.stringify(audit, null, 2));
  console.log('Environment audit saved to:', outPath);
  console.log(JSON.stringify(audit, null, 2));
}

auditEnvironment();
