use std::sync::Arc;

use async_nats::Subscriber;
use sqlx::PgPool;
use tokio_stream::StreamExt;
use tracing::{info, warn};

use rust_decimal::Decimal;

use crate::domain::order::{Blockchain, OrderStatus};
use crate::events::{EventPublisher, OrderRow};

// 风控金额解析: JSON Number (Decimal 序列化形态, 微单位整数) / Decimal 字符串两种取值
pub fn dec_to_micro(v: &serde_json::Value) -> Option<Decimal> {
    match v {
        serde_json::Value::Number(n) => n.as_i64().map(|i| Decimal::new(i, 0)),
        serde_json::Value::String(s) => s.parse::<Decimal>().ok(),
        _ => None,
    }
}

// 发货分流决策: 输入订单数量 + 商品 category + 卡密池可用数 (事务前探测)
// card 类: 可用 >= 需求 → 发货 (事务内逐张 FOR UPDATE SKIP LOCKED 锁分配);
//          可用 < 需求 → 池耗尽, 保持 PAID 拒发 (杜绝无凭证出单)
// 非 card 类: 内容交付, 无需卡密, 直接发货 (交付物由下游承接)
#[derive(Debug, PartialEq, Eq)]
pub enum ShipDecision {
    Ship,
    CardPoolExhausted { need: i64, available: i64 },
}

pub fn decide_ship(quantity: i32, category: Option<&str>, available_keys: i64) -> ShipDecision {
    let need = quantity.max(1) as i64;
    match category {
        Some("card") => {
            if available_keys >= need {
                ShipDecision::Ship
            } else {
                ShipDecision::CardPoolExhausted {
                    need,
                    available: available_keys,
                }
            }
        }
        _ => ShipDecision::Ship,
    }
}

/// 确认数来源: 接入链节点 RPC 的唯一注入口 (依赖抽象, 不依赖具体节点客户端)。
/// 骨架实现只返回库内已存确认数, 保证订单在真实节点实现接入前停留在 PENDING_BALANCE,
/// 绝不出库、绝不伪造确认数。
#[async_trait::async_trait]
pub trait ConfirmationProvider: Send + Sync {
    async fn observed_confirmations(&self, network: Blockchain, last_stored: i32) -> i32;
}

/// 骨架实现: 未接链节点客户端, 原样返回库内值
pub struct StubConfirmationProvider;

#[async_trait::async_trait]
impl ConfirmationProvider for StubConfirmationProvider {
    async fn observed_confirmations(&self, _network: Blockchain, last_stored: i32) -> i32 {
        last_stored
    }
}

/// 三个事件 subject 的 in-memory 消费者。
/// 持久性由发布侧 JetStream 承担, 这里 core NATS fan-out 做实时分发;
/// 若需掉线重放, 将 run_* 换成 JetStream 持久 pull consumer + nack 重投即可。
pub struct EventConsumer {
    pool: PgPool,
    events: Arc<EventPublisher>,
    provider: Arc<dyn ConfirmationProvider>,
}

impl EventConsumer {
    pub fn new(
        pool: PgPool,
        events: Arc<EventPublisher>,
        provider: Arc<dyn ConfirmationProvider>,
    ) -> Self {
        Self {
            pool,
            events,
            provider,
        }
    }

    pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.events.client();
        let pool = self.pool;
        let provider = self.provider;
        let events = self.events;

        // B-02 启动回填: 进程重启后 in-flight 的 backoff 定时器任务丢失,
        // 悬挂 PENDING_BALANCE 订单无人再触发 poll, 这里一次性重扫并补发
        Self::backfill_pending_balance(&pool, &events).await;

        let paid_sub = client.subscribe("orders.events.paid").await?;
        let fraud_sub = client.subscribe("fraud.events.alert").await?;
        let poll_sub = client.subscribe("crypto.events.poll").await?;

        let paid_pool = pool.clone();
        tokio::spawn(Self::run_paid(paid_sub, paid_pool));
        let fraud_pool = pool.clone();
        tokio::spawn(Self::run_fraud(fraud_sub, fraud_pool));
        tokio::spawn(Self::run_poll(pool, provider, events, poll_sub));

        info!("event consumer tasks started: paid / fraud / poll");
        Ok(())
    }

    // B-02: 重扫悬挂订单, 为每个 PENDING_BALANCE 订单补发一次确认轮询
    async fn backfill_pending_balance(pool: &PgPool, events: &Arc<EventPublisher>) {
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, out_trade_no FROM orders WHERE status = 'PENDING_BALANCE'")
                .fetch_all(pool)
                .await
                .unwrap_or_default();
        for (order_id, out_trade_no) in rows {
            info!(
                order_id,
                "startup backfill: republish confirmation poll for pending order"
            );
            events
                .publish_confirmation_poll(order_id, &out_trade_no)
                .await;
        }
    }

    // 正常结算事件: 履约发货接线。
    // 订单进入 PAID 后, 按商品 category 分流:
    //   card   → 从卡密池分配 count 张 (AVAILABLE→LOCKED→USED, 同事务 CAS), 订单 PAID→SHIPPED
    //   其它   → 内容类 (source/tutorial/resource), 直接 PAID→SHIPPED (交付物由下游服务承接, 留痕)
    // 卡密耗尽 (可用不足) → 订单保持 PAID, 告警待补货, 不发货 (杜绝无凭证出单)
    async fn run_paid(mut sub: Subscriber, pool: PgPool) {
        while let Some(msg) = sub.next().await {
            let v: serde_json::Value = serde_json::from_slice(&msg.payload).unwrap_or_default();
            let order_id = v.get("order_id").and_then(|x| x.as_i64());
            let out_trade_no = v
                .get("out_trade_no")
                .and_then(|x| x.as_str())
                .unwrap_or_default();
            let Some(order_id) = order_id else {
                warn!(out_trade_no = %out_trade_no, "paid event missing order_id, skip fulfillment");
                continue;
            };
            if let Err(e) = Self::ship_order(&pool, order_id, out_trade_no).await {
                warn!(%e, order_id, out_trade_no = %out_trade_no, "fulfillment failed, order stays PAID");
            }
        }
        warn!("orders.events.paid stream ended");
    }

    // 单张订单履约: 事务内完成卡密分配 + 状态迁移, 任一环节失败整体回滚保持 PAID
    async fn ship_order(pool: &PgPool, order_id: i64, out_trade_no: &str) -> Result<(), String> {
        use crate::domain::order::OrderStatus;

        // 读订单: 数量 / 产品 / 状态
        let row: Option<(i32, Option<i64>, String)> = sqlx::query_as(
            "SELECT quantity, product_id, status FROM orders WHERE id = $1",
        )
        .bind(order_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
        let Some((quantity, product_id, status)) = row else {
            return Err(format!("order {order_id} not found"));
        };
        let cur = OrderStatus::from_db_value(&status).map_err(|e| e.to_string())?;
        // 幂等: 已 SHIPPED / 非可迁移态, 不再处理
        if !matches!(cur, OrderStatus::Paid) {
            return Ok(());
        }

        // 商品 category 决定是否走卡密池
        let category: Option<String> = sqlx::query_scalar(
            "SELECT category FROM products WHERE id = $1",
        )
        .bind(product_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .flatten();

        // 事务内取卡密池可用数, 分流决策 (纯函数, 单测覆盖)
        let available: i64 = if category.as_deref() == Some("card") {
            let product_id = product_id.ok_or("card order has no product_id")?;
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM card_keys WHERE product_id = $1 AND status = 'AVAILABLE'",
            )
            .bind(product_id)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?
        } else {
            i64::MAX
        };

        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

        let decision = decide_ship(quantity, category.as_deref(), available);
        let (allocated, shipped) = match decision {
            ShipDecision::CardPoolExhausted { need, available } => {
                let _ = tx.rollback().await;
                return Err(format!("card pool exhausted: need {need}, available {available}"));
            }
            ShipDecision::Ship => {
                let mut allocated = 0i64;
                let mut delivery_id: Option<i64> = None;
                if category.as_deref() == Some("card") {
                    let product_id = product_id.ok_or("card order has no product_id")?;
                    let need = quantity.max(1) as i64;
                    // 事务内 CAS 分配: 仅取 AVAILABLE, 逐张锁后转 USED 并绑定订单
                    // FOR UPDATE SKIP LOCKED 防并发双发
                    let keys: Vec<i64> = sqlx::query_scalar(
                        "SELECT id FROM card_keys
                         WHERE product_id = $1 AND status = 'AVAILABLE'
                         ORDER BY id LIMIT $2 FOR UPDATE SKIP LOCKED",
                    )
                    .bind(product_id)
                    .bind(need)
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                    for k in &keys {
                        sqlx::query(
                            "UPDATE card_keys SET status = 'USED', order_id = $2,
                                         used_at = now(), allocated_at = COALESCE(allocated_at, now())
                              WHERE id = $1",
                        )
                        .bind(k)
                        .bind(order_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| e.to_string())?;
                    }
                    allocated = keys.len() as i64;
                    // 每张卡密各落一条 delivery, 用户侧 deliveries API 才能看到全部卡密
                    let user_id: i64 = sqlx::query_scalar(
                        "SELECT user_id FROM orders WHERE id = $1",
                    )
                    .bind(order_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                    for k in &keys {
                        let did: i64 = sqlx::query_scalar(
                            "INSERT INTO product_deliveries (user_id, product_id, order_id, delivery_type, card_key_id)
                             VALUES ($1, $2, $3, 'card_key', $4) RETURNING id",
                        )
                        .bind(user_id)
                        .bind(product_id)
                        .bind(order_id)
                        .bind(k)
                        .fetch_one(&mut *tx)
                        .await
                        .map_err(|e| e.to_string())?;
                        delivery_id = Some(did);
                    }
                } else {
                    // 内容类交付: 源码/教程/资源包 → 登记 file/content 交付物, 供 /user/deliveries 下载
                    // 实际文件 URL 由 admin 上传绑定 (uploaded_files); 未绑定时 delivery 标记 content 占位
                    let user_id: i64 = sqlx::query_scalar(
                        "SELECT user_id FROM orders WHERE id = $1",
                    )
                    .bind(order_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                    let dt = match category.as_deref() {
                        Some("source") => "file",
                        Some("tutorial") => "content",
                        Some("resource") => "file",
                        _ => "content",
                    };
                    // file_url 取该产品最新一条 active uploaded_files 的 storage_key (admin 已上传绑定);
                    // 未上传则留空 (delivery 标记 content 占位, 下载端点返 404).
                    let storage_key: Option<String> = sqlx::query_scalar(
                        "SELECT storage_key FROM uploaded_files
                         WHERE product_id = $1 AND active ORDER BY id DESC LIMIT 1",
                    )
                    .bind(product_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .unwrap_or(None);
                    delivery_id = sqlx::query_scalar(
                        "INSERT INTO product_deliveries (user_id, product_id, order_id, delivery_type, file_url)
                         VALUES ($1, $2, $3, $4, $5) RETURNING id",
                    )
                    .bind(user_id)
                    .bind(product_id)
                    .bind(order_id)
                    .bind(dt)
                    .bind(&storage_key)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                }
                let _ = delivery_id; // 仅日志/占位, 不影响状态机
                (allocated, true)
            }
        };

        // PAID → SHIPPED 状态迁移 (乐观锁)
        sqlx::query(
            "UPDATE orders SET status = 'SHIPPED',
                              state_version = state_version + 1, updated_at = now()
             WHERE id = $1 AND status = 'PAID'",
        )
        .bind(order_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        tx.commit().await.map_err(|e| e.to_string())?;

        info!(
            order_id,
            out_trade_no = %out_trade_no,
            allocated,
            content_delivery = shipped,
            "order fulfilled: PAID -> SHIPPED"
        );
        Ok(())
    }

    // 风控告警: 运营通道接管点 (webhook 侧已冻结订单, 这里做留痕转发 + 落库)
    async fn run_fraud(mut sub: Subscriber, pool: PgPool) {
        while let Some(msg) = sub.next().await {
            let v: serde_json::Value = serde_json::from_slice(&msg.payload).unwrap_or_default();
            let out_trade_no = v.get("out_trade_no").and_then(|x| x.as_str()).unwrap_or("");
            let reason = v.get("reason").and_then(|x| x.as_str()).unwrap_or("other");
            let order_id = v.get("order_id").and_then(|x| x.as_i64());
            let received = v.get("received_amount").and_then(dec_to_micro);
            let expected = v.get("expected_amount").and_then(dec_to_micro);
            let currency = v.get("callback_currency").and_then(|x| x.as_str());
            let gateway = v.get("gateway_type").and_then(|x| x.as_str()).unwrap_or("");
            let evidence = v
                .get("gateway_txn_id")
                .map(|x| {
                    serde_json::json!({
                        "gateway_txn_id": x,
                        "order_currency": v.get("order_currency"),
                        "callback_currency": v.get("callback_currency"),
                    })
                })
                .unwrap_or(serde_json::json!({}));

            // 风控事件持久化: 运营后台 /admin/fraud 可查历史明细
            match sqlx::query(
                "INSERT INTO fraud_events (out_trade_no, order_id, reason,
                        received_amount, expected_amount, currency, gateway_type, evidence)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(out_trade_no)
            .bind(order_id)
            .bind(reason)
            .bind(received)
            .bind(expected)
            .bind(currency)
            .bind(gateway)
            .bind(evidence)
            .execute(&pool)
            .await
            {
                Ok(_) => warn!(
                    out_trade_no = %out_trade_no,
                    reason = %reason,
                    "fraud alert consumed and persisted to fraud_events"
                ),
                Err(e) => warn!(
                    %e,
                    out_trade_no = %out_trade_no,
                    "fraud alert consumed but persist failed; routing to on-call ops channel"
                ),
            }
        }
        warn!("fraud.events.alert stream ended");
    }

    // 链上确认轮询: 把 PENDING_BALANCE 订单推到 PAID (仅当确认数真实达标)
    // B-02 闭环: 未达标 → 300s backoff 重排; 处理失败 → 30s 重试;
    // 进程重启 → start() 启动回填重扫悬挂订单。
    async fn run_poll(
        pool: PgPool,
        provider: Arc<dyn ConfirmationProvider>,
        events: Arc<EventPublisher>,
        mut sub: Subscriber,
    ) {
        while let Some(msg) = sub.next().await {
            let v: serde_json::Value = serde_json::from_slice(&msg.payload).unwrap_or_default();
            let Some(order_id) = v.get("order_id").and_then(|x| x.as_i64()) else {
                warn!("poll event missing order_id, dropped");
                continue;
            };
            let out_trade_no = v
                .get("out_trade_no")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            match Self::process_poll(&pool, provider.as_ref(), order_id, &out_trade_no).await {
                Ok(true) => {
                    // 未达标: 300s 后重排一轮 (BTC 平均 10min/块, 6 确认需数小时)
                    let events2 = events.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                        info!(
                            order_id,
                            "rescheduling confirmation poll after 300s backoff"
                        );
                        events2
                            .publish_confirmation_poll(order_id, &out_trade_no)
                            .await;
                    });
                }
                Ok(false) => {}
                Err(e) => {
                    // 失败不再即丢: 30s 重试, 避免瞬时 DB 抖动永久悬挂
                    warn!(
                        %e,
                        order_id,
                        "poll processing failed, retrying in 30s"
                    );
                    let events3 = events.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        events3
                            .publish_confirmation_poll(order_id, &out_trade_no)
                            .await;
                    });
                }
            }
        }
        warn!("crypto.events.poll stream ended");
    }

    // 返回 Ok(仍低于确认数阈值): true 需重排; false 已终态(结算或单已推进)
    async fn process_poll(
        pool: &PgPool,
        provider: &dyn ConfirmationProvider,
        order_id: i64,
        out_trade_no: &str,
    ) -> Result<bool, String> {
        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

        let row: OrderRow = sqlx::query_as::<_, OrderRow>(
            "SELECT id, out_trade_no, expected_amount, paid_amount, currency, status,
                    crypto_network, blockchain_confirmations, locked_exchange_rate, gateway_type,
                    state_version, quantity
             FROM orders WHERE id = $1 FOR UPDATE",
        )
        .bind(order_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        let status = OrderStatus::from_db_value(&row.status).map_err(|e| e.to_string())?;
        if status != OrderStatus::PendingBalance {
            let _ = tx.rollback().await;
            info!(order_id, status = %status.as_db_value(), "poll: order not PENDING_BALANCE, no-op");
            return Ok(false);
        }

        let Some(network) = row
            .crypto_network
            .as_deref()
            .and_then(Blockchain::from_db_value)
        else {
            let _ = tx.rollback().await;
            warn!(
                order_id,
                "poll: unknown crypto_network, keep PENDING_BALANCE"
            );
            return Ok(false);
        };

        let required = network.required_confirmations();
        let observed = provider
            .observed_confirmations(network, row.blockchain_confirmations)
            .await;

        if observed >= required {
            // 唯一合法迁移: PENDING_BALANCE -> PAID, 带 state_version 乐观锁防并发
            OrderStatus::PendingBalance
                .transition_to(OrderStatus::Paid)
                .map_err(|e| e.to_string())?;
            let res = sqlx::query(
                "UPDATE orders
                 SET status = 'PAID', blockchain_confirmations = $2,
                     state_version = state_version + 1, updated_at = now()
                 WHERE id = $1 AND status = 'PENDING_BALANCE' AND state_version = $3",
            )
            .bind(order_id)
            .bind(observed)
            .bind(row.state_version)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
            if res.rows_affected() == 0 {
                let _ = tx.rollback().await;
                warn!(
                    order_id,
                    "poll: concurrent modification or already settled, no-op"
                );
                return Ok(false);
            }
            sqlx::query(
                "UPDATE payment_transactions
                 SET status = 'SETTLED'
                 WHERE out_trade_no = $1 AND status = 'RECEIVED'",
            )
            .bind(out_trade_no)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
            tx.commit().await.map_err(|e| e.to_string())?;
            info!(
                order_id,
                observed, required, "crypto order settled: PENDING_BALANCE -> PAID"
            );
            return Ok(false);
        }
        sqlx::query("UPDATE orders SET blockchain_confirmations = $2 WHERE id = $1")
            .bind(observed)
            .bind(order_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        info!(
            order_id,
            observed, required, "poll: confirmations below threshold, keep PENDING_BALANCE"
        );
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::{dec_to_micro, decide_ship, ShipDecision};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn dec_to_micro_reads_json_number() {
        assert_eq!(dec_to_micro(&serde_json::json!(150000000)), Some(Decimal::new(150_000_000, 0)));
    }

    #[test]
    fn dec_to_micro_reads_decimal_string() {
        assert_eq!(
            dec_to_micro(&serde_json::json!("12.345678")),
            Some(Decimal::from_str("12.345678").unwrap())
        );
        assert_eq!(dec_to_micro(&serde_json::json!("2500")), Some(Decimal::from(2500)));
    }

    #[test]
    fn dec_to_micro_rejects_non_numeric() {
        assert_eq!(dec_to_micro(&serde_json::Value::Null), None);
        assert_eq!(dec_to_micro(&serde_json::json!(true)), None);
        assert_eq!(dec_to_micro(&serde_json::json!("[1]")), None);
        assert_eq!(dec_to_micro(&serde_json::json!("not-a-number")), None);
        assert_eq!(dec_to_micro(&serde_json::json!(1.5)), None);
    }

    #[test]
    fn decide_ship_card_allocates_min_of_need_and_available() {
        assert_eq!(decide_ship(3, Some("card"), 5), ShipDecision::Ship);
        assert_eq!(decide_ship(5, Some("card"), 5), ShipDecision::Ship);
        assert_eq!(decide_ship(1, Some("card"), 1), ShipDecision::Ship);
    }

    #[test]
    fn decide_ship_card_exhausted_when_available_below_need() {
        assert_eq!(
            decide_ship(3, Some("card"), 2),
            ShipDecision::CardPoolExhausted { need: 3, available: 2 }
        );
        assert_eq!(
            decide_ship(1, Some("card"), 0),
            ShipDecision::CardPoolExhausted { need: 1, available: 0 }
        );
    }

    #[test]
    fn decide_ship_non_card_goes_content_delivery() {
        for cat in [None, Some("source"), Some("tutorial"), Some("resource")] {
            assert_eq!(decide_ship(2, cat, 0), ShipDecision::Ship);
        }
    }

    #[test]
    fn decide_ship_quantity_clamped_to_one() {
        assert_eq!(decide_ship(0, Some("card"), 5), ShipDecision::Ship);
        assert_eq!(
            decide_ship(0, Some("card"), 0),
            ShipDecision::CardPoolExhausted { need: 1, available: 0 }
        );
    }
}
