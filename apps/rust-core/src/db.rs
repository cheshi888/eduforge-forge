use sqlx::postgres::PgPoolOptions;
use sqlx::Executor;

// M2-04: HPA 上限 12 副本 × 8 连接 = 96, 不超 PostgreSQL 默认 max_connections=100;
// acquire_timeout 防止连接池饿死时回调请求无限挂起。
pub fn init(database_url: &str) -> Result<sqlx::PgPool, sqlx::Error> {
    // M2-04: HPA 上限 12 副本 × 8 连接 = 96, 不超 PostgreSQL 默认 max_connections=100;
    // acquire_timeout 防止连接池饿死时回调请求无限挂起。
    // 注: 本地 e2e 用 PGlite(WASM PG, 单 backend 多路复用) 时, sqlx 命名 prepared statement
    // 会跨连接冲突 ("prepared statement already exists")。max_connections 可由
    // FORGE_DB_MAX_CONNS 覆盖, 默认保持生产值 8。
    let max_conns: u32 = std::env::var("FORGE_DB_MAX_CONNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);
    let pool = PgPoolOptions::new()
        .max_connections(max_conns)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        // B4-05 根治: PGlite 单 backend 多路复用, prepared statement 命名空间全局共享,
        // 且连接断开时 PGlite 不清理残留 statement。sqlx 每个新连接的 next_statement_id
        // 都从 sqlx_s_1 重新计数, 会撞上旧连接残留的同名 statement。因此在每个新连接
        // 建立时 DEALLOCATE ALL 清空残留。这里用 Executor::execute 走 simple query
        // protocol (无参数), 不会创建新的命名 prepared statement。
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                conn.execute("DEALLOCATE ALL").await?;
                Ok(())
            })
        })
        .connect_lazy(database_url)?;
    Ok(pool)
}
