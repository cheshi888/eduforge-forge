use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::Router;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::warn;

use crate::gateway::GatewayType;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/orders/preview",
            axum::routing::get(preview_order),
        )
        .route("/internal/checkout", axum::routing::post(checkout))
}

// 前端 OrderSummary 契约 (camelCase, 金额 6 位小数微单位 int64)
// M2-05: locked_rate_usd 以十进制字符串下发 (Decimal::to_string), 前端全程字符串/BigInt 运算, 消除浮点
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderPreview {
    out_trade_no: String,
    quantity: i32,
    amount_micro: i64,
    currency: String,
    locked_rate_usd: Option<String>,
    gateway_payload: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UidQuery {
    uid: String,
}

#[derive(Debug, FromRow)]
struct PreviewRow {
    out_trade_no: String,
    quantity: i32,
    expected_amount: Decimal,
    currency: String,
    locked_exchange_rate: Option<Decimal>,
    gateway_type: Option<String>,
}

// B-01: 订单预览 — 返回该用户名下最新 PENDING 订单的真实数据
// (旧版该路由缺失, 前端恒 404 静默回退 mock 3000.00 CNY, 单号库内不存在)
// 信任边界: 仅集群内部调用; C-08 已闭合 — 前端 uid 改为服务端解析(不再读客户端 x-forge-user 头),
// checkout 侧按订单属主 uid 做归属校验
pub async fn preview_order(
    State(state): State<AppState>,
    Query(q): Query<UidQuery>,
) -> Result<Json<OrderPreview>, (StatusCode, String)> {
    let row: Option<PreviewRow> = sqlx::query_as(
        "SELECT o.out_trade_no, o.quantity, o.expected_amount, o.currency,
                o.locked_exchange_rate, o.gateway_type
         FROM orders o
         JOIN users u ON u.id = o.user_id
         WHERE u.uid = $1 AND o.status = 'PENDING'
         ORDER BY o.id DESC
         LIMIT 1",
    )
    .bind(&q.uid)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = row.ok_or((StatusCode::NOT_FOUND, "no pending order".to_string()))?;

    // NUMERIC(20,6) 微单位换算: ×10^6 后必为整数, 纯 Decimal 运算不经过 f64
    let micros = (row.expected_amount * Decimal::new(1_000_000, 0))
        .round_dp(0)
        .normalize();
    let amount_micro = micros
        .to_string()
        .parse::<i64>()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "amount overflow".into()))?;

    Ok(Json(OrderPreview {
        out_trade_no: row.out_trade_no,
        quantity: row.quantity,
        amount_micro,
        currency: row.currency,
        // M2-05: Decimal -> 十进制字符串, 前端全程字符串/BigInt 运算消除浮点
        locked_rate_usd: row.locked_exchange_rate.map(|d| d.to_string()),
        gateway_payload: row.gateway_type,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CheckoutReq {
    uid: String,
    out_trade_no: String,
    payment_method: String,
    // M2-07: 可选乐观并发版本号 — 提供时要求订单版本未前进才结算, 堵住双击/并发双锁窗口
    state_version: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct CheckoutResp {
    out_trade_no: String,
    gateway_payload: String,
}

// B-01: 结算锁定 — 校验支付通道合法并将 gateway_type 锁定到订单 (乐观并发 +1)。
// C-08: 归属校验 — 仅允许订单属主 uid 结算; 单号被伪造/越权时子查询无匹配 → 0 行 → 404。
// 网关支付链接/表单的真实生成需接入网关客户端 SDK, 本骨架返回锁定结果,
// gateway_payload 仅携带通道与单号, 前端凭 out_trade_no 继续发起网关页面。
pub async fn checkout(
    State(state): State<AppState>,
    Json(req): Json<CheckoutReq>,
) -> Result<Json<CheckoutResp>, (StatusCode, String)> {
    let gateway = GatewayType::from_str_v(&req.payment_method)
        .map_err(|_| (StatusCode::BAD_REQUEST, "unknown payment method".into()))?;

    // M2-07: state_version 作为可选 CAS 条件; 缺省行为不变 (向后兼容)
    let sql = match req.state_version {
        Some(_) => "UPDATE orders
          SET gateway_type = $2, state_version = state_version + 1, updated_at = now()
          WHERE out_trade_no = $1
            AND status = 'PENDING'
            AND user_id = (SELECT id FROM users WHERE uid = $3)
            AND state_version = $4",
        None => "UPDATE orders
          SET gateway_type = $2, state_version = state_version + 1, updated_at = now()
          WHERE out_trade_no = $1
            AND status = 'PENDING'
            AND user_id = (SELECT id FROM users WHERE uid = $3)",
    };
    let mut q = sqlx::query(sql)
        .bind(&req.out_trade_no)
        .bind(gateway.as_db_value())
        .bind(&req.uid);
    if let Some(v) = req.state_version {
        q = q.bind(v);
    }
    let res = q.execute(&state.pool).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if res.rows_affected() == 0 {
        warn!(
            out_trade_no = %req.out_trade_no,
            uid = %req.uid,
            "checkout: order not found, not PENDING, or not owned by uid"
        );
        return Err((
            StatusCode::NOT_FOUND,
            "order not found or not pending".into(),
        ));
    }

    let payload = serde_json::json!({
        "payment_method": gateway.as_db_value(),
        "out_trade_no": req.out_trade_no,
    })
    .to_string();

    Ok(Json(CheckoutResp {
        out_trade_no: req.out_trade_no,
        gateway_payload: payload,
    }))
}
