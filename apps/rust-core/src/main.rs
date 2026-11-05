use axum::extract::State;
use axum::http::StatusCode;
use axum::Router;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod api;
mod config;
mod consumer;
mod db;
mod domain;
mod events;
mod gateway;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub events: std::sync::Arc<events::EventPublisher>,
    pub cfg: config::Config,
    // 交付文件本地存储根目录 (上传/下载接线); 缺省 <cwd>/uploads
    pub uploads_dir: Option<std::path::PathBuf>,
}

// M2-08: 就绪探针 — DB + NATS 双依赖探活; K8s readiness 打 /readyz,
// liveness 保留 /healthz (仅进程存活), DB/NATS 故障时停止接新流量而非重启。
async fn readyz(State(state): State<AppState>) -> StatusCode {
    if sqlx::query("SELECT 1").execute(&state.pool).await.is_err() {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    // 非 JetStream 裸 publish 做 NATS 探活 (无流约束, 连接不可用即报错)
    if state
        .events
        .client()
        .publish("eduforge.health.probe", bytes::Bytes::from_static(b"p"))
        .await
        .is_err()
    {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    StatusCode::OK
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // B4-05 根治: 无论从 cargo run / 直接 exe / 脚本启动, 都自动加载 <cwd>/.env。
    // 否则 PGlite(单 session) 下 sqlx 命名 prepared statement 会跨连接冲突
    // ("prepared statement already exists"), 需要 statement-cache-capacity=0 + 单连接。
    // 已存在的系统环境变量优先于 .env (dotenvy 默认不覆盖)。
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,eduforge_core=debug")),
        )
        .json()
        .init();

    let cfg = config::Config::from_env()?;
    info!(
        db_url = %cfg.database_url,
        db_max_conns = %std::env::var("FORGE_DB_MAX_CONNS").unwrap_or_default(),
        "db config loaded"
    );
    let pool = db::init(&cfg.database_url)?;
    let events = events::EventPublisher::connect(&cfg.nats_url, pool.clone()).await?;
    let events = std::sync::Arc::new(events);

    // M2-02: outbox flusher — 每 10s 补发 NATS 故障期滞留的 pending 事件
    {
        let fl = events.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                let n = fl.flush_outbox(50).await;
                if n > 0 {
                    info!(resent = n, "outbox resent pending events");
                }
            }
        });
    }

    let uploads_dir = std::env::var("FORGE_UPLOADS_DIR")
        .ok()
        .map(std::path::PathBuf::from);
    let state = AppState {
        pool,
        events: events.clone(),
        cfg,
        uploads_dir,
    };

    consumer::EventConsumer::new(
        state.pool.clone(),
        events,
        std::sync::Arc::new(consumer::StubConfirmationProvider),
    )
    .start()
    .await?;

    let app = Router::new()
        .route(
            "/webhooks/:gateway",
            axum::routing::post(api::webhook::handle_webhook),
        )
        .merge(api::internal::routes())
        .merge(api::admin::routes())
        .merge(api::membership::routes())
        .merge(api::auth::routes())
        .merge(api::catalog::routes())
        .merge(api::promotion::routes())
        .merge(api::community::routes())
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .route("/readyz", axum::routing::get(readyz))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    info!(addr = %state.cfg.bind_addr, "eduforge-core listening");
    let listener = tokio::net::TcpListener::bind(&state.cfg.bind_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
