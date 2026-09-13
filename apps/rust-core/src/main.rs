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
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,eduforge_core=debug")),
        )
        .json()
        .init();

    let cfg = config::Config::from_env()?;
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

    let state = AppState {
        pool,
        events: events.clone(),
        cfg,
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
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .route("/readyz", axum::routing::get(readyz))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    info!(addr = %state.cfg.bind_addr, "eduforge-core listening");
    let listener = tokio::net::TcpListener::bind(&state.cfg.bind_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
