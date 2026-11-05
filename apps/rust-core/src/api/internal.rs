use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use axum::Router;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::warn;

use crate::api::auth::user_id_from_token;
use crate::gateway::GatewayType;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/orders/preview",
            axum::routing::get(preview_order),
        )
        .route("/internal/orders", axum::routing::post(create_order))
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

#[derive(Debug, FromRow)]
struct PreviewRow {
    out_trade_no: String,
    quantity: i32,
    expected_amount: Decimal,
    currency: String,
    locked_exchange_rate: Option<Decimal>,
    gateway_type: Option<String>,
}

// B-01: 订单预览 — 返回当前登录用户最新 PENDING 订单的真实数据.
// C-08 彻底闭合: 身份由 Bearer token 解析 (user_id_from_token), 不再信任客户端 ?uid=.
pub async fn preview_order(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OrderPreview>, (StatusCode, String)> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "unauthorized".to_string()))?;

    let row: Option<PreviewRow> = sqlx::query_as(
        "SELECT o.out_trade_no, o.quantity, o.expected_amount, o.currency,
                o.locked_exchange_rate, o.gateway_type
         FROM orders o
         WHERE o.user_id = $1 AND o.status = 'PENDING'
         ORDER BY o.id DESC
         LIMIT 1",
    )
    .bind(user_id)
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

// 购物车条目: 单个商品 SKU + 数量. 金额/币种一律以库内 products 为准, 不信任客户端传价.
#[derive(Debug, Deserialize, Clone)]
pub struct CartItem {
    sku: String,
    quantity: i32,
}

// 下单创建订单请求: 支持多商品购物车合并下单 (单 SKU 亦兼容).
// 身份由 Bearer token 解析, 客户端不再传 uid (防越权下单).
// 可选 coupon_code: 优惠券码, 在会员折扣基础上叠加优惠 (阶段3营销体系).
#[derive(Debug, Deserialize)]
pub struct CreateOrderReq {
    items: Vec<CartItem>,
    #[serde(default)]
    coupon_code: Option<String>,
}

// 下单响应: 与 OrderPreview 同构 (camelCase), 前端 checkout 页直接消费.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOrderResp {
    out_trade_no: String,
    quantity: i32,
    amount_micro: i64,
    currency: String,
    locked_rate_usd: Option<String>,
    // 命中会员折扣时返回折后信息, 供前端展示 (原价/折扣率/折后价), 金额均为 6 位小数微单位 int64
    original_amount_micro: Option<i64>,
    discount_pct: Option<i32>,
    // 优惠券信息 (阶段3): 命中时返回券码与优惠金额, 供前端展示
    coupon_code: Option<String>,
    coupon_discount_micro: Option<i64>,
}

// B-01 (下单): 创建订单并锁定 expected_amount. 支持多商品购物车合并下单.
// 会员折扣 (schema 003: membership_tiers.discount_pct, plus=10% / pro=20% / free=0%)
// 在此刻应用: 折后价 = Σ(unit_price * quantity) * (100 - discount_pct) / 100, 纯 Decimal 运算禁浮点.
// 折后价写入 expected_amount, 后续 webhook 回调金额比对 / 会员累计消费 / 库存扣减均以折后价为准, 全链路自洽.
// 安全边界: 金额只信任库内 products.unit_price 与 users.tier_code, 客户端不传价 (防改价越权);
// 身份由 Bearer token 解析 (user_id_from_token), 客户端不传 uid (防越权下单).
pub async fn create_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateOrderReq>,
) -> Result<Json<CreateOrderResp>, (StatusCode, String)> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "unauthorized".to_string()))?;

    if req.items.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "cart is empty".into()));
    }
    if req.items.iter().any(|i| i.quantity <= 0) {
        return Err((StatusCode::BAD_REQUEST, "quantity must be positive".into()));
    }
    // B4-07 (下单溢出防护): 单 SKU 数量与购物车 SKU 数上限, 防 i32 溢出与天价订单/垃圾订单 DoS.
    const MAX_ITEMS: usize = 50;
    const MAX_QTY_PER_SKU: i32 = 999;
    if req.items.len() > MAX_ITEMS {
        return Err((StatusCode::BAD_REQUEST, "too many line items".into()));
    }
    if req.items.iter().any(|i| i.quantity > MAX_QTY_PER_SKU) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("quantity exceeds max {MAX_QTY_PER_SKU} per sku"),
        ));
    }

    // 1) 批量读商品原价 + 币种 (库内权威值), 校验所有 SKU 存在且币种一致
    let mut currency: Option<String> = None;
    let mut subtotal = Decimal::ZERO;
    let mut total_qty = 0i32;
    for item in &req.items {
        let prod: Option<(Decimal, String)> = sqlx::query_as(
            "SELECT unit_price, currency FROM products WHERE sku = $1",
        )
        .bind(&item.sku)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let Some((unit_price, cur)) = prod else {
            return Err((StatusCode::NOT_FOUND, format!("product not found: {}", item.sku)));
        };
        // 购物车币种必须一致 (跨币种下单需拆分, 当前平台单币种 CNY 为主)
        if let Some(ref existing) = currency {
            if *existing != cur {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("mixed currency not supported ({} vs {})", existing, cur),
                ));
            }
        } else {
            currency = Some(cur);
        }
        subtotal += unit_price * Decimal::from(item.quantity);
        total_qty += item.quantity;
    }
    let currency = currency.unwrap_or_else(|| "CNY".to_string());

    // 2) 读用户会员折扣率 (join membership_tiers, 缺省 0)
    let discount_pct: i16 = sqlx::query_scalar(
        "SELECT t.discount_pct
         FROM users u JOIN membership_tiers t ON t.code = u.tier_code
         WHERE u.id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .unwrap_or(0);

    // 3) 计算折后价: expected = subtotal * (100 - pct) / 100  (Decimal, 零浮点)
    let factor = Decimal::from(100i64 - discount_pct as i64) / Decimal::from(100i64);
    let mut expected_amount = (subtotal * factor).round_dp(6);

    if expected_amount <= Decimal::ZERO {
        return Err((StatusCode::BAD_REQUEST, "computed amount must be positive".into()));
    }

    // 3.5) 优惠券应用 (阶段3): 在会员折扣后的折后价上叠加优惠.
    // 校验: 券存在/active/未过期/用户已领且 UNUSED/门槛满足; 满足则核销计算并标记 LOCKED.
    let mut coupon_code_out: Option<String> = None;
    let mut coupon_discount_micro: Option<i64> = None;
    let mut coupon_ucid: Option<i64> = None;
    if let Some(code) = req.coupon_code.as_deref() {
        let code = code.trim().to_uppercase();
        let coupon: Option<(i64, String, Decimal, Decimal, Option<chrono::DateTime<chrono::Utc>>, i64)> =
            sqlx::query_as(
                "SELECT c.id, c.coupon_type, c.discount_value, c.threshold_micro, c.valid_until, uc.id
                 FROM coupons c
                 JOIN user_coupons uc ON uc.coupon_id = c.id AND uc.user_id = $2 AND uc.status = 'UNUSED'
                 WHERE c.code = $1 AND c.active = TRUE
                 ORDER BY uc.id ASC LIMIT 1",
            )
            .bind(&code)
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if let Some((_cid, ctype, cvalue, cthreshold, cuntil, ucid)) = coupon {
            // 过期校验
            let expired = cuntil.map(|t| t < chrono::Utc::now()).unwrap_or(false);
            if expired {
                return Err((StatusCode::GONE, "coupon expired".into()));
            }
            // 门槛校验: 折后价 >= threshold
            if expected_amount < cthreshold {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("order amount below coupon threshold"),
                ));
            }
            // 核销计算 (纯 Decimal)
            let (final_amt, discount_amt) =
                crate::api::promotion::apply_coupon(expected_amount, &ctype, cvalue);
            // 记录券占用 (先 LOCKED, 订单落库后回填 order_id; 结算成功转 USED, 失败回滚 UNUSED)
            let _ = sqlx::query(
                "UPDATE user_coupons SET status = 'LOCKED' WHERE id = $1 AND status = 'UNUSED'",
            )
            .bind(ucid)
            .execute(&state.pool)
            .await;
            // 记住待绑定订单的券记录 id, 稍后回填 order_id
            coupon_ucid = Some(ucid);
            coupon_code_out = Some(code.clone());
            coupon_discount_micro = Some(micros_to_i64(&discount_amt));
            expected_amount = final_amt;
        } else {
            return Err((StatusCode::NOT_FOUND, "coupon not found or not claimable".into()));
        }
    }

    // 4) 生成唯一商户单号 (时间戳 + 随机 hex)
    let out_trade_no = gen_out_trade_no();

    // 5) 落库订单 (expected_amount 即折后价, quantity 为合计数量).
    // 购物车多 SKU 明细落 order_items 表 (阶段2新增), 订单主表 quantity 存合计.
    let res = sqlx::query(
        "INSERT INTO orders (out_trade_no, user_id, product_id, quantity, expected_amount, currency, status)
         VALUES ($1, $2, (SELECT id FROM products WHERE sku = $3), $4, $5, $6, 'PENDING')",
    )
    .bind(&out_trade_no)
    .bind(user_id)
    .bind(&req.items[0].sku)
    .bind(total_qty)
    .bind(expected_amount)
    .bind(&currency)
    .execute(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if res.rows_affected() == 0 {
        return Err((StatusCode::BAD_REQUEST, "user or product not found".into()));
    }

    // 5.1) 购物车明细落库 (阶段2: order_items 表)
    let order_id: i64 = sqlx::query_scalar(
        "SELECT id FROM orders WHERE out_trade_no = $1",
    )
    .bind(&out_trade_no)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    for item in &req.items {
        let _ = sqlx::query(
            "INSERT INTO order_items (order_id, product_id, quantity, unit_price)
             VALUES ($1, (SELECT id FROM products WHERE sku = $2), $3,
                     (SELECT unit_price FROM products WHERE sku = $2))",
        )
        .bind(order_id)
        .bind(&item.sku)
        .bind(item.quantity)
        .execute(&state.pool)
        .await;
    }

    // 5.2) 回填优惠券绑定的订单 id (下单即占用)
    if let Some(ucid) = coupon_ucid {
        let _ = sqlx::query(
            "UPDATE user_coupons SET order_id = $2 WHERE id = $1 AND status = 'LOCKED'",
        )
        .bind(ucid)
        .bind(order_id)
        .execute(&state.pool)
        .await;
    }

    // 6) 组装响应: 金额换算为微单位 int64 (纯 Decimal 运算)
    let amount_micro = micros_to_i64(&expected_amount);
    let original_micro = micros_to_i64(&subtotal);
    let (original_amount_micro, discount_pct_out) = if discount_pct > 0 {
        (Some(original_micro), Some(discount_pct as i32))
    } else {
        (None, None)
    };

    Ok(Json(CreateOrderResp {
        out_trade_no,
        quantity: total_qty,
        amount_micro,
        currency,
        locked_rate_usd: None,
        original_amount_micro,
        discount_pct: discount_pct_out,
        coupon_code: coupon_code_out,
        coupon_discount_micro,
    }))
}

// 纯 Decimal -> i64 微单位换算, 禁浮点
fn micros_to_i64(d: &Decimal) -> i64 {
    (d * Decimal::new(1_000_000, 0))
        .round_dp(0)
        .to_string()
        .parse::<i64>()
        .unwrap_or(0)
}

// 商户单号: FORGE-<unix_secs><6 位随机 hex>
fn gen_out_trade_no() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 纯 std 随机 (无 rand crate): 取纳秒低位 + 进程无关抖动
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("FORGE-{secs}{nanos:06x}")
}

#[derive(Debug, Deserialize)]
pub struct CheckoutReq {
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

// B-01: 结算锁定 — 校验支付通道合法并将 gateway_type 锁定到订单 (乐观并发 +1).
// C-08: 归属校验 — 身份由 Bearer token 解析, 仅允许订单属主结算; 越权时子查询无匹配 → 0 行 → 404.
pub async fn checkout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CheckoutReq>,
) -> Result<Json<CheckoutResp>, (StatusCode, String)> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "unauthorized".to_string()))?;

    let gateway = GatewayType::from_str_v(&req.payment_method)
        .map_err(|_| (StatusCode::BAD_REQUEST, "unknown payment method".into()))?;

    // M2-07: state_version 作为可选 CAS 条件; 缺省行为不变 (向后兼容)
    let sql = match req.state_version {
        Some(_) => "UPDATE orders
          SET gateway_type = $2, state_version = state_version + 1, updated_at = now()
          WHERE out_trade_no = $1
            AND status = 'PENDING'
            AND user_id = $3
            AND state_version = $4",
        None => "UPDATE orders
          SET gateway_type = $2, state_version = state_version + 1, updated_at = now()
          WHERE out_trade_no = $1
            AND status = 'PENDING'
            AND user_id = $3",
    };
    let mut q = sqlx::query(sql)
        .bind(&req.out_trade_no)
        .bind(gateway.as_db_value())
        .bind(user_id);
    if let Some(v) = req.state_version {
        q = q.bind(v);
    }
    let res = q.execute(&state.pool).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if res.rows_affected() == 0 {
        warn!(
            out_trade_no = %req.out_trade_no,
            user_id,
            "checkout: order not found, not PENDING, or not owned by user"
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

#[cfg(test)]
mod tests {
    use super::*;

    // 会员折扣计算核心: 折后价 = unit * qty * (100 - pct) / 100, 纯 Decimal 零浮点.
    // 抽成纯函数便于单测 (不依赖 DB), 与 create_order 内联逻辑保持一致.
    fn apply_discount(unit: Decimal, qty: i32, pct: i16) -> Decimal {
        let subtotal = unit * Decimal::from(qty);
        let factor = Decimal::from(100i64 - pct as i64) / Decimal::from(100i64);
        (subtotal * factor).round_dp(6)
    }

    #[test]
    fn discount_zero_keeps_full_price() {
        // free 会员 0% 折扣: 299 * 1 = 299.000000
        assert_eq!(
            apply_discount(Decimal::new(299, 0), 1, 0),
            Decimal::new(299_000_000, 6)
        );
    }

    #[test]
    fn discount_plus_ten_percent() {
        // plus 10%: 299 * 0.9 = 269.100000
        assert_eq!(
            apply_discount(Decimal::new(299, 0), 1, 10),
            Decimal::new(269_100_000, 6)
        );
    }

    #[test]
    fn discount_pro_twenty_percent_multi_qty() {
        // pro 20%: 120 * 3 * 0.8 = 288.000000
        assert_eq!(
            apply_discount(Decimal::new(120, 0), 3, 20),
            Decimal::new(288_000_000, 6)
        );
    }

    #[test]
    fn discount_rounding_to_six_places() {
        // 199.99 * 0.9 = 179.991000 (精确, 无浮点误差)
        assert_eq!(
            apply_discount(Decimal::new(199_990_000, 6), 1, 10),
            Decimal::new(179_991_000, 6)
        );
    }

    #[test]
    fn micros_to_i64_roundtrip() {
        // 269.1 元 -> 269100000 微单位
        assert_eq!(micros_to_i64(&Decimal::new(269_100_000, 6)), 269_100_000);
        // 0.01 元 -> 10000
        assert_eq!(micros_to_i64(&Decimal::new(10_000, 6)), 10_000);
    }

    #[test]
    fn gen_out_trade_no_unique_shape() {
        let a = gen_out_trade_no();
        let b = gen_out_trade_no();
        assert!(a.starts_with("FORGE-"));
        assert_ne!(a, b);
    }
}
