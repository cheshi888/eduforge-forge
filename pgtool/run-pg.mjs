import { startPg } from './pgserver.mjs';

const port = Number(process.env.PGPORT || 5432);
const { db } = await startPg(port);
const v = (await db.query('select version()')).rows[0].version;
console.log('[pglite] PostgreSQL via WASM ready on 127.0.0.1:' + port);
console.log('[pglite] ' + v);
setInterval(() => {}, 1 << 30); // 保持进程存活
