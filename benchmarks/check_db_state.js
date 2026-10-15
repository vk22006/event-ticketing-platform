const mysql = require('mysql2/promise');

async function checkDb() {
  const conn = await mysql.createConnection('mysql://root:kishore2006@127.0.0.1:3306/event_lifecycle');
  const [tables] = await conn.query('SHOW TABLES');
  console.log('Tables in event_lifecycle:');
  for (const row of tables) {
    const tableName = Object.values(row)[0];
    const [[countRow]] = await conn.query(`SELECT COUNT(*) as cnt FROM \`${tableName}\``);
    console.log(` - ${tableName}: ${countRow.cnt} rows`);
  }
  const [[maxConn]] = await conn.query("SHOW VARIABLES LIKE 'max_connections'");
  console.log(`MySQL ${maxConn.Variable_name} = ${maxConn.Value}`);
  const [[threadsConnected]] = await conn.query("SHOW STATUS LIKE 'Threads_connected'");
  console.log(`MySQL ${threadsConnected.Variable_name} = ${threadsConnected.Value}`);
  const [[threadsRunning]] = await conn.query("SHOW STATUS LIKE 'Threads_running'");
  console.log(`MySQL ${threadsRunning.Variable_name} = ${threadsRunning.Value}`);
  await conn.end();
}

checkDb().catch(console.error);
