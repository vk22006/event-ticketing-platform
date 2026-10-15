const mysql = require('mysql2/promise');

async function inspect() {
  const conn = await mysql.createConnection('mysql://root:kishore2006@127.0.0.1:3306');
  for (const db of ['event_lifecycle', 'tower_sessions']) {
    try {
      const [tables] = await conn.query(`SHOW TABLES FROM \`${db}\``);
      console.log(`Database ${db} tables:`, tables.map(t => Object.values(t)[0]));
      for (const t of tables) {
        const name = Object.values(t)[0];
        if (name.includes('session')) {
          const [[cnt]] = await conn.query(`SELECT COUNT(*) as c FROM \`${db}\`.\`${name}\``);
          console.log(`  -> ${name} has ${cnt.c} rows`);
        }
      }
    } catch (e) {
      console.log(`Error in ${db}:`, e.message);
    }
  }
  await conn.end();
}

inspect();
