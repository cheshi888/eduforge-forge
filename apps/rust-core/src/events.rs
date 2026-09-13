use sqlx::{FromRow, PgPool};
use tracing::{debug, error, warn};

// 整行映射: 字段与 orders 表列一一对应, 供 query_as 与审计使用, 未被当前逻辑读取的列保留
#[allow(dead_code)]
#[derive(Debug, Clone, FromRow)]
pub struct OrderRow {
    pub id: i64,
    pub out_trade_no: String,
    pub expected_amount: rust_decimal::Decimal,
    pub paid_amount: Option<rust_decimal::Decimal>,
    pub currency: String,
    pub status: String,
    pub crypto_network: Option<String>,
    pub blockchain_confirmations: i32,
    pub locked_exchange_rate: Option<rust_decimal::Decimal>,
    pub gateway_type: Option<String>,
    pub state_version: i32,
    // H2-05: 结算扣库存需订单数量
    pub quantity: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FraudAlert {
    // H-02: 告警归类 — "currency_mismatch" / "amount_mismatch" / "orphan_callback"
    pub reason: &'static str,
    pub order_id: Option<i64>,
    pub out_trade_no: String,
    pub expected_amount: Option<rust_decimal::Decimal>,
    pub received_amount: Option<rust_decimal::Decimal>,
    pub gateway_type: String,
    pub gateway_txn_id: String,
    // C-01: 币种核对证据 (金额不一致分支两侧币种必然相同); 孤儿回调无订单行, 置 None
    pub order_currency: Option<String>,
    pub callback_currency: String,
}

pub struct EventPublisher {
    js: async_nats::jetstream::Context,
    // M2-02: 事务级 outbox — 发布前先落库, NATS 故障期事件不丢, 恢复后 flusher 补发
    db: PgPool,
}

impl EventPublisher {
    pub async fn connect(
        url: &str,
        db: PgPool,
    ) -> Result<Self, async_nats::ConnectError> {
        let client = async_nats::connect(url).await?;
        let js = async_nats::jetstream::new(client);
        Ok(Self { js, db })
    }

    pub fn client(&self) -> async_nats::Client {
        self.js.client()
    }

    pub async fn publish_order_paid(&self, order_id: i64, out_trade_no: &str) {
        let subject = "orders.events.paid";
        let payload = serde_json::json!({ "order_id": order_id, "out_trade_no": out_trade_no });
        self.publish(subject, payload).await;
        debug!(%subject, %out_trade_no, "published order paid event");
    }

    pub async fn publish_fraud_alert(&self, alert: &FraudAlert) {
        let subject = "fraud.events.alert";
        let payload = serde_json::to_value(alert).unwrap_or_default();
        self.publish(subject, payload).await;
        warn!(alert.out_trade_no = %alert.out_trade_no, "fraud alert published");
    }

    pub async fn publish_confirmation_poll(&self, order_id: i64, out_trade_no: &str) {
        let subject = "crypto.events.poll";
        let payload = serde_json::json!({ "order_id": order_id, "out_trade_no": out_trade_no });
        self.publish(subject, payload).await;
    }

    // M2-02: 先写 outbox (独立短事务, 与调用方 commit 解耦), 再尝试 NATS;
    // NATS 失败仅 warn, 事件已持久化, flusher 扫描 pending 补发
    async fn publish(&self, subject: &str, payload: serde_json::Value) {
        let body = payload.to_string();
        if let Err(e) = sqlx::query(
            "INSERT INTO event_outbox (subject, payload) VALUES ($1, $2)",
        )
        .bind(subject)
        .bind(&body)
        .execute(&self.db)
        .await
        {
            // 落库失败属严重数据一致性告警 (outbox 是事件兜底的地基)
            error!(%e, %subject, "outbox insert failed, event will be lost");
        }
        let bytes = bytes::Bytes::from(body.into_bytes());
        if let Err(e) = self.js.publish(subject.to_string(), bytes).await {
            warn!(%e, %subject, "jetstream publish failed; outbox row retained for resend");
        }
    }

    // M2-02: flusher — 取最早 pending 批, 逐条补发 JetStream, 成功后标 sent.
    // 由 main 周期驱动; 失败保留 pending 下轮重试 (at-least-once, 消费端幂等去重).
    pub async fn flush_outbox(&self, limit: i64) -> i64 {
        let rows: Vec<OutboxRow> = match sqlx::query_as(
            "SELECT id, subject, payload FROM event_outbox
             WHERE status = 'pending' ORDER BY id LIMIT $1 FOR UPDATE SKIP LOCKED",
        )
        .bind(limit)
        .fetch_all(&self.db)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!(%e, "outbox flush read failed");
                return 0;
            }
        };
        let mut sent = 0i64;
        for r in rows {
            let bytes = bytes::Bytes::from(r.payload.clone().into_bytes());
            match self.js.publish(r.subject.to_string(), bytes).await {
                Ok(_) => {
                    let _ = sqlx::query(
                        "UPDATE event_outbox SET status = 'sent', sent_at = now() WHERE id = $1",
                    )
                    .bind(r.id)
                    .execute(&self.db)
                    .await;
                    sent += 1;
                }
                Err(e) => {
                    warn!(%e, outbox_id = r.id, %r.subject, "outbox resend failed, keep pending");
                    break; // 连接不可用则本批终止, 下轮重试
                }
            }
        }
        sent
    }
}

#[derive(sqlx::FromRow)]
struct OutboxRow {
    id: i64,
    subject: String,
    payload: String,
}
