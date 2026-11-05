import pg from 'pg';
import { readFileSync, readdirSync } from 'fs';
import { join } from 'path';

const { Client } = pg;
const client = new Client({
  host: '127.0.0.1',
  port: 5432,
  user: 'postgres',
  password: 'postgres',
  database: 'postgres',
});
await client.connect();
console.log('[schema] connected to PGlite');

const dir = process.env.SCHEMA_DIR;
if (!dir) {
  console.error('SCHEMA_DIR not set');
  process.exit(1);
}

// 按文件名排序应用
const files = readdirSync(dir).filter((f) => f.endsWith('.sql')).sort();
for (const f of files) {
  let sql = readFileSync(join(dir, f), 'utf8');
  // 跳过 CREATE EXTENSION（服务器端已加载 vector）
  sql = sql.replace(/CREATE EXTENSION IF NOT EXISTS vector;/gi, '');
  try {
    await client.query(sql);
    console.log(`[schema] ✓ ${f}`);
  } catch (e) {
    console.error(`[schema] ✗ ${f}: ${e.message}`);
    process.exit(1);
  }
}

// 验证表
const tables = await client.query(
  "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename"
);
console.log('[schema] tables: ' + tables.rows.map((r) => r.tablename).join(', '));
await client.end();
