use std::str::FromStr;

use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::Response;
use bytes::Bytes;
use rust_decimal::Decimal;
use sqlx::Transaction;
use tracing::{error, info, warn};

use crate::domain::order::OrderStatus;
use crate::events::{FraudAlert, OrderRow};
use crate::gateway::{self, GatewayTransaction, GatewayType};
use crate::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutcome {
    Handled,
    // 幂等命中: 同一 (gateway_txn_id, gateway_type) 的重复回调, 直接 ACK
    Duplicate,
    // Crypto 未达确认数: 金额记 PENDING_BALANCE, 异步轮询节点
    PendingBalance,
    // 三道校验失败: 订单进入 FRAUD_SUSPENDED, 告警已发出
    FraudSuspended,
    // 基础设施故障: 返回 5xx 让网关按策略重试
    Failed,
}

pub async fn handle_webhook(
    state: State<AppState>,
    Path(gateway_str): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let state = &state;
    let gateway = match GatewayType::from_str_v(&gateway_str) {
        Ok(g) => g,
        Err(e) => {
            warn!(%e, "unknown gateway route");
            return reject_response(&format!("unknown gateway: {gateway_str}"));
        }
    };

    // 校验 0: 入口层 (Envoy/Nginx) 附加的自定义 HMAC-SHA256 签名头, 防伪造回调
    let hmac_header = headers
        .get(gateway::FORGE_HMAC_HEADER)
        .and_then(|v| v.to_str().ok());
    if let Err(e) = gateway::verify_ingress_hmac(&state.cfg.hmac_secret, &body, hmac_header) {
        warn!(%e, gw = %gateway.as_db_value(), "ingress hmac rejected");
        return reject_response("bad hmac");
    }

    // 校验 1: 网关自身验签 + 载荷解析
    let txn = match gateway::parse_and_verify(&state.cfg, gateway, &headers, &body) {
        Ok(t) => t,
        Err(e) => {
            warn!(%e, gw = %gateway.as_db_value(), "gateway signature or payload rejected");
            return reject_response("bad request");
        }
    };

    let outcome = process_transaction(state, gateway, &txn).await;
    info!(
        gw = %gateway.as_db_value(),
        txn = %txn.gateway_txn_id,
        out_trade_no = %txn.out_trade_no,
        ?outcome,
        "webhook processed"
    );

    match outcome {
        ProcessOutcome::Handled | ProcessOutcome::Duplicate | ProcessOutcome::PendingBalance => {
            gateway.ack("processed")
        }
        ProcessOutcome::FraudSuspended => gateway.ack("fraud-suspended"),
        ProcessOutcome::Failed => {
            error!(out_trade_no = %txn.out_trade_no, "processing failed, return 500 to trigger gateway retry");
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body("internal error".into())
                .unwrap()
        }
    }
}

fn reject_response(msg: &str) -> Response {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header("Content-Type", "text/plain")
        .body(msg.to_string().into())
        .unwrap()
}

async fn process_transaction(
    state: &AppState,
    gateway: GatewayType,
    txn: &GatewayTransaction,
) -> ProcessOutcome {
    let mut db: Transaction<'static, sqlx::Postgres> = match state.pool.begin().await {
        Ok(db) => db,
        Err(e) => {
            error!(%e, "db begin failed");
            return ProcessOutcome::Failed;
        }
    };

    // !!! CRITICAL: 幂等性校验 (三道死锁之第 3 道) !!!
    // (gateway_txn_id + gateway_type) 联合唯一索引是防重放攻击的终极防线。
    // ON CONFLICT DO NOTHING 原子认领: 影响行数为 0 即该网关流水已入库,
    // 判定为重复通知, 直接回 ACK, 绝不二次入账、绝不再次发货。
    let claimed = sqlx::query(
        r"INSERT INTO payment_transactions
            (gateway_txn_id, gateway_type, out_trade_no, amount, currency, confirmations, raw_payload, status)
           VALUES ($1, $2, $3, $4, $5, $6, $7, 'RECEIVED')
           ON CONFLICT (gateway_txn_id, gateway_type) DO NOTHING",
    )
    .bind(&txn.gateway_txn_id)
    .bind(gateway.as_db_value())
    .bind(&txn.out_trade_no)
    .bind(&txn.amount)
    .bind(&txn.currency)
    .bind(txn.confirmations)
    .bind(txn.raw.clone())
    .execute(&mut *db)
    .await;

    let rows = match claimed {
        Ok(r) => r.rows_affected(),
        Err(e) => {
            error!(%e, "idempotency insert failed");
            let _ = db.rollback().await;
            return ProcessOutcome::Failed;
        }
    };
    if rows == 0 {
        let _ = db.rollback().await;
        warn!(txn = %txn.gateway_txn_id, "duplicate notification dropped");
        return ProcessOutcome::Duplicate;
    }

    // !!! CRITICAL: 悲观锁 + 本地金额比对 (三道死锁之第 1 道) !!!
    // SELECT ... FOR UPDATE 锁定订单行, 将同一 out_trade_no 的并发回调串行化。
    // 金额只信任本地 expected_amount (下单时刻落库值), 与回调金额做 Decimal 精确相等比较;
    // 出现任何偏差 (哪怕 0.01) 立即冻结订单为 FRAUD_SUSPENDED 并推送告警。
    let order = sqlx::query_as::<_, OrderRow>(
        "SELECT id, out_trade_no, expected_amount, paid_amount, currency, status,
                crypto_network, blockchain_confirmations, locked_exchange_rate, gateway_type,
                state_version, quantity
          FROM orders WHERE out_trade_no = $1 FOR UPDATE",
    )
    .bind(&txn.out_trade_no)
    .fetch_optional(&mut *db)
    .await;

    let order = match order {
        Ok(Some(row)) => row,
        Ok(None) => {
            warn!(
                out_trade_no = %txn.out_trade_no,
                "order not found, marking rejected and raising fraud alert"
            );
            mark_txn(&mut db, gateway, "REJECTED", txn).await;
            if db.commit().await.is_err() {
                return ProcessOutcome::Failed;
            }
            // H-02: 孤儿回调 (单号库内不存在) 也推风控告警, 阻断"批量打不存在单号无告警"盲区;
            // 订单相关字段无来源, 置 None 不伪造
            state
                .events
                .publish_fraud_alert(&FraudAlert {
                    reason: "orphan_callback",
                    order_id: None,
                    out_trade_no: txn.out_trade_no.clone(),
                    expected_amount: None,
                    received_amount: Decimal::from_str(&txn.amount).ok(),
                    gateway_type: gateway.as_db_value().to_string(),
                    gateway_txn_id: txn.gateway_txn_id.clone(),
                    order_currency: None,
                    callback_currency: txn.currency.clone(),
                })
                .await;
            return ProcessOutcome::FraudSuspended;
        }
        Err(e) => {
            error!(%e, "order lock read failed");
            let _ = db.rollback().await;
            return ProcessOutcome::Failed;
        }
    };

    let cb_amount: Decimal = match Decimal::from_str(&txn.amount) {
        Ok(d) => d,
        Err(e) => {
            error!(%e, "callback amount unparseable as Decimal");
            mark_txn(&mut db, gateway, "REJECTED", txn).await;
            let _ = db.commit().await;
            return ProcessOutcome::FraudSuspended;
        }
    };

    // C-01 + H2-01 (方案 B): 币种核对先于金额核对.
    // - 同币种: 直接精确相等比较 (原语义).
    // - 跨币种且订单锁定美元锚定汇率: 期望金额 × rate 折算到回调币种后比较 (FX 折算).
    // - 跨币种但无锁汇: 冻结为 fraud (杜绝无依据跨币种同额入账, 原 C-01 语义).
    let fx_cross = match fx_comparable(&order.currency, &txn.currency, order.locked_exchange_rate) {
        Ok(direct) => !direct,
        Err(reason) => {
            error!(
                order_currency = %order.currency,
                callback_currency = %txn.currency,
                reason,
                "callback currency mismatch with order, order frozen as fraud-suspended",
            );
            sqlx::query(
                "UPDATE orders
                 SET status = 'FRAUD_SUSPENDED', state_version = state_version + 1, updated_at = now()
                 WHERE id = $1",
            )
            .bind(order.id)
            .execute(&mut *db)
            .await
            .ok();
            mark_txn(&mut db, gateway, "CURRENCY_MISMATCH", txn).await;
            if db.commit().await.is_err() {
                return ProcessOutcome::Failed;
            }
            state
                .events
                .publish_fraud_alert(&FraudAlert {
                    reason: "currency_mismatch",
                    order_id: Some(order.id),
                    out_trade_no: order.out_trade_no.clone(),
                    expected_amount: Some(order.expected_amount),
                    received_amount: Some(cb_amount),
                    gateway_type: gateway.as_db_value().to_string(),
                    gateway_txn_id: txn.gateway_txn_id.clone(),
                    order_currency: Some(order.currency.clone()),
                    callback_currency: txn.currency.clone(),
                })
                .await;
            return ProcessOutcome::FraudSuspended;
        }
    };

    // H2-01 (方案 B): 期望值统一折算到回调币种. 同币种系数 1, 跨币种用锁定汇率.
    let expected_in_cb_currency = if fx_cross {
        order.expected_amount * order.locked_exchange_rate.unwrap()
    } else {
        order.expected_amount
    };

    if cb_amount != expected_in_cb_currency {
        error!(
            expected_in_cb_currency = %expected_in_cb_currency,
            received = %cb_amount,
            fx_cross,
            diff = %cb_amount - expected_in_cb_currency,
            "amount mismatch, order frozen as fraud-suspended"
        );
        sqlx::query(
            "UPDATE orders
             SET status = 'FRAUD_SUSPENDED', paid_amount = $2,
                 state_version = state_version + 1, updated_at = now()
             WHERE id = $1",
        )
        .bind(order.id)
        .bind(cb_amount)
        .execute(&mut *db)
        .await
        .ok();
        mark_txn(&mut db, gateway, "MISMATCH", txn).await;
        if db.commit().await.is_err() {
            return ProcessOutcome::Failed;
        }
        state
            .events
            .publish_fraud_alert(&FraudAlert {
                reason: "amount_mismatch",
                order_id: Some(order.id),
                out_trade_no: order.out_trade_no.clone(),
                // H2-01: 证据侧记折算到回调币种后的期望值, 便于风控直接比对
                expected_amount: Some(expected_in_cb_currency),
                received_amount: Some(cb_amount),
                gateway_type: gateway.as_db_value().to_string(),
                gateway_txn_id: txn.gateway_txn_id.clone(),
                order_currency: Some(order.currency.clone()),
                callback_currency: txn.currency.clone(),
            })
            .await;
        return ProcessOutcome::FraudSuspended;
    }

    // !!! CRITICAL: 区块链确认数校验 (三道死锁之第 2 道) !!!
    // 未达确认数阈值 (BTC >= 6, TRC-20/ERC-20 >= 1) 的交易只入账 PENDING_BALANCE,
    // 不可发货; 由 NATS 消费者异步轮询节点补齐确认数后再走结算。
    if let (Some(conf), Some(network)) = (txn.confirmations, txn.crypto_network) {
        let required = network.required_confirmations();
        if conf < required {
            sqlx::query(
                "UPDATE orders
                 SET status = 'PENDING_BALANCE', blockchain_confirmations = $2,
                     paid_amount = $3, state_version = state_version + 1, updated_at = now()
                 WHERE id = $1",
            )
            .bind(order.id)
            .bind(conf)
            .bind(cb_amount)
            .execute(&mut *db)
            .await
            .ok();
            mark_txn(&mut db, gateway, "RECEIVED", txn).await;
            if db.commit().await.is_err() {
                return ProcessOutcome::Failed;
            }
            state
                .events
                .publish_confirmation_poll(order.id, &order.out_trade_no)
                .await;
            info!(
                conf,
                required, "below confirmation threshold, parked in PENDING_BALANCE"
            );
            return ProcessOutcome::PendingBalance;
        }
        sqlx::query("UPDATE orders SET blockchain_confirmations = $2 WHERE id = $1")
            .bind(order.id)
            .bind(conf)
            .execute(&mut *db)
            .await
            .ok();
    }

    // 正常结算: 状态机迁移 PENDING -> PAID
    let from = match OrderStatus::from_db_value(&order.status) {
        Ok(s) => s,
        Err(e) => {
            error!(%e, "invalid stored status");
            let _ = db.rollback().await;
            return ProcessOutcome::Failed;
        }
    };
    let to = match from.transition_to(OrderStatus::Paid) {
        Ok(s) => s,
        Err(e) => {
            // M2-03: 状态机阻断 (如重复 CAPTURE / Adyen 已终态) 时此事件并未结算,
            // 标 SETTLED 会失真; 用 schema L60 既有的 DUPLICATE 词 (L2-01 一并收口)
            warn!(%e, "state machine blocked settlement, mark duplicate");
            mark_txn(&mut db, gateway, "DUPLICATE", txn).await;
            if db.commit().await.is_err() {
                return ProcessOutcome::Failed;
            }
            return ProcessOutcome::Duplicate;
        }
    };
    // H2-05 (方案 A): 结算前 DB CAS 扣减库存, 堵住并发超卖。
    // 无库存列约束的既有库应保证 stock >= 0; CAS 命中 0 行 = 库存不足, 冻结订单拒付。
    let cas = sqlx::query(
        "UPDATE products SET stock = stock - $2
         WHERE id = (SELECT product_id FROM orders WHERE id = $1) AND stock >= $2",
    )
    .bind(order.id)
    .bind(order.quantity as i64)
    .execute(&mut *db)
    .await
    .ok();
    if cas.map(|r| r.rows_affected() == 0).unwrap_or(true) {
        // 库存不足 (或子查询无产品): 回滚, 订单不动作, 事件标 CURRENCY_MISMATCH 之外的库存拒因
        error!(order_id = order.id, "insufficient stock, settlement blocked");
        let _ = db.rollback().await;
        return ProcessOutcome::Failed;
    }

    sqlx::query(
        "UPDATE orders
         SET status = $2, paid_amount = $3,
             state_version = state_version + 1, updated_at = now()
         WHERE id = $1",
    )
    .bind(order.id)
    .bind(to.as_db_value())
    .bind(cb_amount)
    .execute(&mut *db)
    .await
    .ok();
    // 会员累计消费 + 等级晋升: 累加 total_spent_micro; 若累计跨过 plus 阈值且当前 free, 升 plus
    sqlx::query(
        "UPDATE users
         SET total_spent_micro = total_spent_micro + $2
         WHERE id = (SELECT user_id FROM orders WHERE id = $1)",
    )
    .bind(order.id)
    .bind(cb_amount)
    .execute(&mut *db)
    .await
    .ok();
    // 会员等级自动晋升: 累计消费跨阈值 → free 升 plus (阈值 100 元 = 100000000 微单位)
    sqlx::query(
        "UPDATE users
         SET tier_code = 'plus',
             tier_expires_at = NOW() + interval '365 day'
         WHERE id = (SELECT user_id FROM orders WHERE id = $1)
           AND tier_code = 'free'
           AND total_spent_micro >= 100000000",
    )
    .bind(order.id)
    .execute(&mut *db)
    .await
    .ok();
    mark_txn(&mut db, gateway, "SETTLED", txn).await;
    if db.commit().await.is_err() {
        return ProcessOutcome::Failed;
    }

    state
        .events
        .publish_order_paid(order.id, &order.out_trade_no)
        .await;
    ProcessOutcome::Handled
}

// H2-01 (方案 B): FX 折算比对 — 订单币种 != 回调币种 且订单锁定了美元锚定汇率时,
// 把期望金额按 locked_exchange_rate (1 单位订单币种 = rate USD) 折算成回调币种再比对.
// 返回 Ok(true) 同币种直接比; Ok(false) 需按 rate 折算; Err 无锁汇 → 冻结 (杜绝无依据入账).
fn fx_comparable(
    order_currency: &str,
    cb_currency: &str,
    rate: Option<Decimal>,
) -> Result<bool, &'static str> {
    if order_currency.eq_ignore_ascii_case(cb_currency) {
        return Ok(true);
    }
    match rate {
        Some(r) if r > Decimal::ZERO => Ok(false),
        _ => Err("fx order missing locked exchange rate"),
    }
}

async fn mark_txn(
    db: &mut Transaction<'static, sqlx::Postgres>,
    gateway: GatewayType,
    status: &str,
    txn: &GatewayTransaction,
) {
    sqlx::query(
        "UPDATE payment_transactions
         SET status = $3 WHERE gateway_txn_id = $1 AND gateway_type = $2",
    )
    .bind(&txn.gateway_txn_id)
    .bind(gateway.as_db_value())
    .bind(status)
            .execute(&mut **db)
            .await
            .ok();
}

#[cfg(test)]
mod tests {
    use super::fx_comparable;
    use rust_decimal::Decimal;

    // H2-01 (方案 B) 折算判定单测
    #[test]
    fn fx_comparable_same_currency_direct() {
        // 同币种 (即使锁了汇率) 走直接比较
        assert_eq!(
            fx_comparable("USD", "usd", Some(Decimal::new(7, 0))),
            Ok(true)
        );
    }

    #[test]
    fn fx_comparable_cross_currency_with_rate_converts() {
        // CNY 订单 + USD 回调 + 有锁汇 → 需折算 (false)
        assert_eq!(
            fx_comparable("CNY", "USD", Some(Decimal::new(7, 1))),
            Ok(false)
        );
    }

    #[test]
    fn fx_comparable_cross_currency_without_rate_rejected() {
        // 跨币种但无锁汇 → Err (回调侧据此冻结)
        assert!(fx_comparable("CNY", "USD", None).is_err());
    }

    #[test]
    fn fx_comparable_zero_rate_rejected() {
        // 锁汇为 0 视为无效 (防 0 折算放大金额)
        assert!(fx_comparable("CNY", "USD", Some(Decimal::ZERO)).is_err());
    }
}
