const mysql = require('mysql2/promise');

async function showDbs() {
  const conn = await mysql.createConnection('mysql://root:kishore2006@127.0.0.1:3306');
  const [dbs] = await conn.query('SHOW DATABASES');
  console.log('Databases:', dbs.map(d => Object.values(d)[0]));
  for (const dbRow of dbs) {
    const dbName = Object.values(dbRow)[0];
    const [tables] = await conn.query(`SHOW TABLES FROM \`${dbName}\``);
    const tableNames = tables.map(t => Object.values(t)[0]);
    if (tableNames.some(t => t.includes('session'))) {
      console.log(`Found session in database: ${dbName} -> tables:`, tableNames);
    }
  }
  await conn.end();
}

showDbs().catch(console.error);
