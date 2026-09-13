use sqlx::postgres::PgPoolOptions;

// M2-04: HPA 上限 12 副本 × 8 连接 = 96, 不超 PostgreSQL 默认 max_connections=100;
// acquire_timeout 防止连接池饿死时回调请求无限挂起。
pub fn init(database_url: &str) -> Result<sqlx::PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_lazy(database_url)?;
    Ok(pool)
}
