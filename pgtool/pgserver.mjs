import { PGlite } from '@electric-sql/pglite';
import { vector } from '@electric-sql/pglite-pgvector';
import { PGLiteSocketServer } from '@electric-sql/pglite-socket';

export async function startPg(port = 5432) {
  const db = new PGlite({
    extensions: { vector },
  });
  const server = new PGLiteSocketServer({
    db,
    port,
    host: '127.0.0.1',
    maxConnections: 200, // ⚠️ 默认 1，sqlx 连接池会开多个连接
  });
  await server.start();
  await db.exec('CREATE EXTENSION IF NOT EXISTS vector;');
  return { db, server };
}
