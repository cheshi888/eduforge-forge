//! 管理后台 API: /admin/* 统一 admin token 鉴权, 暴露商品/订单/卡密/风控/统计.
//! 信任边界: FORGE_ADMIN_TOKEN 必需, 缺省 fail-closed (整个 /admin 401).
//! 鉴权方式: 每个 handler 首行调 `guard_admin(&state, &headers)`, 校验 Bearer token.

use axum::extract::{Path, Query, State};
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response, Json};
use axum::Router;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::warn;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/products", axum::routing::get(list_products).post(create_product))
        .route(
            "/admin/products/{id}",
            axum::routing::get(get_product).put(update_product).delete(delete_product),
        )
        .route("/admin/products/{id}/stock", axum::routing::post(adjust_stock))
        .route("/admin/orders", axum::routing::get(list_orders))
        .route("/admin/orders/{out_trade_no}", axum::routing::get(get_order))
        .route(
            "/admin/orders/{out_trade_no}/ship",
            axum::routing::post(ship_order),
        )
        .route(
            "/admin/card-keys",
            axum::routing::get(list_card_keys).post(generate_card_keys),
        )
        .route(
            "/admin/card-keys/{id}",
            axum::routing::get(get_card_key).put(revoke_card_key),
        )
        .route("/admin/fraud", axum::routing::get(list_fraud_events))
        .route(
            "/admin/fraud/{id}",
            axum::routing::post(resolve_fraud_event),
        )
        .route("/admin/stats", axum::routing::get(get_stats))
}

// ---------------------------------------------------------------------------
// 鉴权
// ---------------------------------------------------------------------------

pub fn guard_admin(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let expected = &state.cfg.admin_token;
    if expected.is_empty() {
        warn!("admin API accessed but FORGE_ADMIN_TOKEN not configured; rejecting");
        return Err(
            (StatusCode::UNAUTHORIZED, "admin token not configured on server").into_response(),
        );
    }
    let got = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !constant_time_eq(got, expected) {
        return Err((StatusCode::UNAUTHORIZED, "invalid admin token").into_response());
    }
    Ok(())
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn micros_from_decimal(d: &Decimal) -> i64 {
    let m = (d * Decimal::new(1_000_000, 0)).round_dp(0);
    m.to_string()
        .parse::<i64>()
        .unwrap_or(0)
}

fn db_err(e: sqlx::Error) -> Response {
    let status = match &e {
        sqlx::Error::Database(db) => {
            let code = db.code();
            let sqlstate = code.as_deref();
            if sqlstate == Some("23505") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, e.to_string()).into_response()
}

/// 业务层"未找到"判断: 按主键或业务键查询空结果 (sqlx 0.8 RowNotFound 为单位 variant)
#[allow(dead_code)]
fn is_not_found(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::RowNotFound)
}

#[derive(Debug, Deserialize, Default)]
pub struct ProductQuery {
    category: Option<String>,
    active: Option<bool>,
}

// ---------------------------------------------------------------------------
// 序列化契约 (camelCase, 金额 6 位微单位 int64, 汇率字符串)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProductDto {
    id: i64,
    sku: String,
    title: String,
    category: String,
    cover: Option<String>,
    description: Option<String>,
    unit_price: Decimal,
    currency: String,
    stock: i64,
    active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl ProductDto {
    fn to_api(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "sku": self.sku,
            "title": self.title,
            "category": self.category,
            "cover": self.cover,
            "description": self.description,
            "unitPriceMicro": micros_from_decimal(&self.unit_price),
            "currency": self.currency,
            "stock": self.stock,
            "active": self.active,
            "createdAt": self.created_at.to_rfc3339(),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductUpsertReq {
    sku: String,
    title: String,
    category: String,
    cover: Option<String>,
    description: Option<String>,
    unit_price_micro: i64,
    currency: String,
    stock: i64,
    active: bool,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct OrderDto {
    id: i64,
    out_trade_no: String,
    user_uid: String,
    product_sku: String,
    product_title: String,
    quantity: i32,
    expected_amount: Decimal,
    paid_amount: Option<Decimal>,
    currency: String,
    status: String,
    gateway_type: Option<String>,
    crypto_network: Option<String>,
    blockchain_confirmations: i32,
    locked_exchange_rate: Option<Decimal>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl OrderDto {
    fn to_api(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "outTradeNo": self.out_trade_no,
            "userUid": self.user_uid,
            "productSku": self.product_sku,
            "productTitle": self.product_title,
            "quantity": self.quantity,
            "expectedAmountMicro": micros_from_decimal(&self.expected_amount),
            "paidAmountMicro": self.paid_amount.map(|d| micros_from_decimal(&d)),
            "currency": self.currency,
            "status": self.status,
            "gatewayType": self.gateway_type,
            "cryptoNetwork": self.crypto_network,
            "blockchainConfirmations": self.blockchain_confirmations,
            "lockedExchangeRate": self.locked_exchange_rate.map(|d| d.to_string()),
            "createdAt": self.created_at.to_rfc3339(),
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct OrderListQuery {
    status: Option<String>,
    gateway_type: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CardKeyDto {
    id: i64,
    product_sku: String,
    key_code: String,
    status: String,
    order_out_trade_no: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl CardKeyDto {
    fn to_api(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "productSku": self.product_sku,
            "keyCode": self.key_code,
            "status": self.status,
            "orderOutTradeNo": self.order_out_trade_no,
            "createdAt": self.created_at.to_rfc3339(),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateKeysReq {
    product_id: i64,
    count: i32,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct FraudEventDto {
    id: i64,
    out_trade_no: String,
    order_id: Option<i64>,
    reason: String,
    received_amount: Option<Decimal>,
    expected_amount: Option<Decimal>,
    currency: Option<String>,
    gateway_type: Option<String>,
    resolved: bool,
    resolved_by: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl FraudEventDto {
    fn to_api(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "outTradeNo": self.out_trade_no,
            "orderId": self.order_id,
            "reason": self.reason,
            "receivedAmountMicro": self.received_amount.map(|d| micros_from_decimal(&d)),
            "expectedAmountMicro": self.expected_amount.map(|d| micros_from_decimal(&d)),
            "currency": self.currency,
            "gatewayType": self.gateway_type,
            "resolved": self.resolved,
            "resolvedBy": self.resolved_by,
            "createdAt": self.created_at.to_rfc3339(),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustStockReq {
    delta: i64,
}

// ---------------------------------------------------------------------------
// 商品管理
// ---------------------------------------------------------------------------

pub async fn list_products(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<Option<ProductQuery>>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let cat_opt = q.as_ref().and_then(|x| x.category.clone());
    let act_opt = q.and_then(|x| x.active);
    let mut sql = String::from(
        "SELECT id, sku, title, category, cover, description, unit_price, currency,
                stock, active, created_at FROM products WHERE 1=1",
    );
    let idx_cat = 1;
    let idx_act = if cat_opt.is_some() { 2 } else { 1 };
    if cat_opt.is_some() {
        sql.push_str(&format!(" AND category = ${}", idx_cat));
    }
    if act_opt.is_some() {
        sql.push_str(&format!(" AND active = ${}", idx_act));
    }
    sql.push_str(" ORDER BY id DESC");
    let mut query = sqlx::query_as::<_, ProductDto>(&sql);
    if let Some(cat) = &cat_opt {
        query = query.bind(cat.clone());
    }
    if let Some(active) = act_opt {
        query = query.bind(active);
    }
    let rows: Vec<ProductDto> = query.fetch_all(&state.pool).await.map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows.iter().map(|r| r.to_api()).collect();
    Ok(Json(out).into_response())
}

pub async fn create_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ProductUpsertReq>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    if !matches!(req.category.as_str(), "source" | "tutorial" | "card" | "resource") {
        return Ok((StatusCode::BAD_REQUEST, "invalid category").into_response());
    }
    let price = Decimal::new(req.unit_price_micro, 6);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO products (sku, title, category, cover, description, unit_price, currency, stock, active)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
    )
    .bind(&req.sku)
    .bind(&req.title)
    .bind(&req.category)
    .bind(&req.cover)
    .bind(&req.description)
    .bind(price)
    .bind(&req.currency)
    .bind(req.stock)
    .bind(req.active)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if is_not_found(&e) {
            db_err(e)
        } else if matches!(&e, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505")) {
            (StatusCode::CONFLICT, "duplicate sku").into_response()
        } else {
            db_err(e)
        }
    })?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id })),
    )
        .into_response())
}

pub async fn get_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let r: Option<ProductDto> = sqlx::query_as(
        "SELECT id, sku, title, category, cover, description, unit_price, currency,
                stock, active, created_at
         FROM products WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let Some(r) = r else {
        return Ok((StatusCode::NOT_FOUND, "product not found").into_response());
    };
    Ok(Json(r.to_api()).into_response())
}

pub async fn update_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(req): Json<ProductUpsertReq>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let price = Decimal::new(req.unit_price_micro, 6);
    let res = sqlx::query(
        "UPDATE products SET sku=$2, title=$3, category=$4, cover=$5, description=$6,
                unit_price=$7, currency=$8, stock=$9, active=$10
         WHERE id=$1",
    )
    .bind(id)
    .bind(&req.sku)
    .bind(&req.title)
    .bind(&req.category)
    .bind(&req.cover)
    .bind(&req.description)
    .bind(price)
    .bind(&req.currency)
    .bind(req.stock)
    .bind(req.active)
    .execute(&state.pool)
    .await
    .map_err(db_err)?;
    if res.rows_affected() == 0 {
        return Ok((StatusCode::NOT_FOUND, "product not found").into_response());
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn delete_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let res = sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(db_err)?;
    if res.rows_affected() == 0 {
        return Ok((StatusCode::NOT_FOUND, "product not found").into_response());
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn adjust_stock(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(req): Json<AdjustStockReq>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let res = sqlx::query(
        "UPDATE products SET stock = stock + $2 WHERE id = $1 AND stock + $2 >= 0",
    )
    .bind(id)
    .bind(req.delta)
    .execute(&state.pool)
    .await
    .map_err(db_err)?;
    if res.rows_affected() == 0 {
        return Ok((
            StatusCode::BAD_REQUEST,
            "product missing or stock would go negative",
        )
            .into_response());
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ---------------------------------------------------------------------------
// 订单管理
// ---------------------------------------------------------------------------

pub async fn list_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<OrderListQuery>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);

    // 动态 where: 占位符按出现顺序编号, 末尾追加 LIMIT/OFFSET 两个占位符
    let mut placeholders: Vec<String> = Vec::new();
    if q.status.is_some() {
        placeholders.push(format!("o.status = ${}", placeholders.len() + 1));
    }
    if q.gateway_type.is_some() {
        placeholders.push(format!("o.gateway_type = ${}", placeholders.len() + 1));
    }
    let where_sql = if placeholders.is_empty() {
        "1=1".to_string()
    } else {
        placeholders.join(" AND ")
    };
    let limit_ph = placeholders.len() + 1;
    let offset_ph = placeholders.len() + 2;
    let sql = format!(
        "SELECT o.id, o.out_trade_no, u.uid, p.sku, p.title, o.quantity,
                o.expected_amount, o.paid_amount, o.currency, o.status,
                o.gateway_type, o.crypto_network, o.blockchain_confirmations,
                o.locked_exchange_rate, o.created_at
         FROM orders o
         JOIN users u ON u.id = o.user_id
         JOIN products p ON p.id = o.product_id
         WHERE {where_sql}
         ORDER BY o.id DESC LIMIT ${limit_ph} OFFSET ${offset_ph}"
    );

    let mut query = sqlx::query_as::<_, OrderDto>(&sql);
    if let Some(status) = &q.status {
        query = query.bind(status.clone());
    }
    if let Some(gw) = &q.gateway_type {
        query = query.bind(gw.clone());
    }
    query = query.bind(limit).bind(offset);

    let rows: Vec<OrderDto> = query.fetch_all(&state.pool).await.map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows.iter().map(|r| r.to_api()).collect();
    Ok(Json(out).into_response())
}

pub async fn get_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(out_trade_no): Path<String>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let r: Option<OrderDto> = sqlx::query_as(
        "SELECT o.id, o.out_trade_no, u.uid, p.sku, p.title, o.quantity,
                o.expected_amount, o.paid_amount, o.currency, o.status,
                o.gateway_type, o.crypto_network, o.blockchain_confirmations,
                o.locked_exchange_rate, o.created_at
         FROM orders o
         JOIN users u ON u.id = o.user_id
         JOIN products p ON p.id = o.product_id
         WHERE o.out_trade_no = $1",
    )
    .bind(&out_trade_no)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let Some(r) = r else {
        return Ok((StatusCode::NOT_FOUND, "order not found").into_response());
    };

    #[derive(FromRow, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct TxnRow {
        gateway_txn_id: String,
        gateway_type: String,
        amount: Decimal,
        currency: String,
        status: String,
        confirmations: Option<i32>,
        received_at: chrono::DateTime<chrono::Utc>,
    }
    let txns: Vec<TxnRow> = sqlx::query_as(
        "SELECT gateway_txn_id, gateway_type, amount, currency, status, confirmations, received_at
         FROM payment_transactions WHERE out_trade_no = $1 ORDER BY id",
    )
    .bind(&out_trade_no)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    #[derive(FromRow, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct KeyRow {
        id: i64,
        key_code: String,
        status: String,
        used_at: Option<chrono::DateTime<chrono::Utc>>,
    }
    let keys: Vec<KeyRow> = sqlx::query_as(
        "SELECT id, key_code, status, used_at FROM card_keys WHERE order_id = $1 ORDER BY id",
    )
    .bind(r.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let payload = serde_json::json!({
        "order": r.to_api(),
        "transactions": txns.iter().map(|t| serde_json::json!({
            "gatewayTxnId": t.gateway_txn_id,
            "gatewayType": t.gateway_type,
            "amountMicro": micros_from_decimal(&t.amount),
            "currency": t.currency,
            "status": t.status,
            "confirmations": t.confirmations,
            "receivedAt": t.received_at.to_rfc3339(),
        })).collect::<Vec<_>>(),
        "cardKeys": keys.iter().map(|k| serde_json::json!({
            "id": k.id,
            "keyCode": k.key_code,
            "status": k.status,
            "usedAt": k.used_at.map(|t| t.to_rfc3339()),
        })).collect::<Vec<_>>(),
    });
    Ok(Json(payload).into_response())
}

pub async fn ship_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(out_trade_no): Path<String>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let res = sqlx::query(
        "UPDATE orders SET status = 'SHIPPED', state_version = state_version + 1, updated_at = now()
         WHERE out_trade_no = $1 AND status = 'PAID'",
    )
    .bind(&out_trade_no)
    .execute(&state.pool)
    .await
    .map_err(db_err)?;
    if res.rows_affected() == 0 {
        return Ok((StatusCode::CONFLICT, "order not in PAID state").into_response());
    }
    Ok(Json(serde_json::json!({ "shipped": out_trade_no })).into_response())
}

// ---------------------------------------------------------------------------
// 卡密库
// ---------------------------------------------------------------------------

pub async fn list_card_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let rows: Vec<CardKeyDto> = sqlx::query_as(
        "SELECT k.id, p.sku, k.key_code, k.status, o.out_trade_no, k.created_at
         FROM card_keys k
         JOIN products p ON p.id = k.product_id
         LEFT JOIN orders o ON o.id = k.order_id
         ORDER BY k.id DESC LIMIT 200",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows.iter().map(|r| r.to_api()).collect();
    Ok(Json(out).into_response())
}

pub async fn generate_card_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<GenerateKeysReq>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    if !(1..=500).contains(&req.count) {
        return Ok((StatusCode::BAD_REQUEST, "count must be 1..=500").into_response());
    }
    let prod: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM products WHERE id = $1 AND category = 'card'",
    )
    .bind(req.product_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let Some(prod) = prod else {
        return Ok((StatusCode::NOT_FOUND, "card product not found").into_response());
    };
    let prod_id = prod.0;

    use sha2::{Digest, Sha256};
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut entropy = [0u8; 32];
    {
        use std::io::Read;
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut entropy))
            .unwrap_or_else(|_| {
                // 兜底: 不可用 /dev/urandom 时用时间种子派生 (非加密强度, 仅演示)
                for (i, b) in entropy.iter_mut().enumerate() {
                    *b = ((nonce >> ((i % 8) * 8)) & 0xff) as u8 ^ (i as u8).wrapping_mul(7);
                }
            });
    }
    let mut inserted: Vec<i64> = Vec::new();
    for i in 0..req.count {
        let mut raw = entropy;
        let nonce_u64 = nonce as u64;
        raw[..8].copy_from_slice(nonce_u64.wrapping_add(i as u64).to_be_bytes().as_slice());
        let key_code = hex::encode(&raw);
        let hash = hex::encode(Sha256::digest(key_code.as_bytes()));
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO card_keys (product_id, key_code, key_hash)
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(prod_id)
        .bind(&key_code)
        .bind(&hash)
        .fetch_one(&state.pool)
        .await
        .map_err(db_err)?;
        inserted.push(id);
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "productId": prod_id,
            "count": inserted.len(),
            "ids": inserted
        })),
    )
        .into_response())
}

pub async fn get_card_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let r: Option<CardKeyDto> = sqlx::query_as(
        "SELECT k.id, p.sku, k.key_code, k.status, o.out_trade_no, k.created_at
         FROM card_keys k
         JOIN products p ON p.id = k.product_id
         LEFT JOIN orders o ON o.id = k.order_id
         WHERE k.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let Some(r) = r else {
        return Ok((StatusCode::NOT_FOUND, "card key not found").into_response());
    };
    Ok(Json(r.to_api()).into_response())
}

pub async fn revoke_card_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let res = sqlx::query(
        "UPDATE card_keys SET status = 'REVERSED' WHERE id = $1 AND status = 'USED'",
    )
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(db_err)?;
    if res.rows_affected() == 0 {
        return Ok((StatusCode::BAD_REQUEST, "only USED keys can be revoked").into_response());
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ---------------------------------------------------------------------------
// 风控事件
// ---------------------------------------------------------------------------

pub async fn list_fraud_events(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let rows: Vec<FraudEventDto> = sqlx::query_as(
        "SELECT id, out_trade_no, order_id, reason, received_amount, expected_amount,
                currency, gateway_type, resolved, resolved_by, created_at
         FROM fraud_events ORDER BY id DESC LIMIT 200",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows.iter().map(|r| r.to_api()).collect();
    Ok(Json(out).into_response())
}

pub async fn resolve_fraud_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let res = sqlx::query(
        "UPDATE fraud_events SET resolved = TRUE, resolved_by = 'admin', resolved_at = now()
         WHERE id = $1 AND resolved = FALSE",
    )
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(db_err)?;
    if res.rows_affected() == 0 {
        return Ok((
            StatusCode::NOT_FOUND,
            "fraud event not found or already resolved",
        )
            .into_response());
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ---------------------------------------------------------------------------
// 统计
// ---------------------------------------------------------------------------

pub async fn get_stats(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;

    #[derive(FromRow)]
    struct Today {
        orders: i64,
        revenue: Decimal,
    }
    let today: Today = sqlx::query_as(
        "SELECT count(*) AS orders, COALESCE(sum(paid_amount), 0) AS revenue
         FROM orders
         WHERE status = 'PAID' AND created_at >= date_trunc('day', now())",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(db_err)?;

    let pending_ship: i64 = sqlx::query_scalar("SELECT count(*) FROM orders WHERE status = 'PAID'")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

    let available_keys: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM card_keys WHERE status = 'AVAILABLE'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let unresolved_fraud: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fraud_events WHERE resolved = FALSE",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    #[derive(FromRow)]
    struct StatusCount {
        status: String,
        n: i64,
    }
    let orders_by_status: Vec<StatusCount> = sqlx::query_as(
        "SELECT status, count(*) AS n FROM orders GROUP BY status ORDER BY n DESC",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    #[derive(FromRow)]
    struct GwRev {
        gateway: String,
        rev: Decimal,
    }
    let revenue_by_gateway: Vec<GwRev> = sqlx::query_as(
        "SELECT COALESCE(gateway_type, 'unknown') AS gateway,
                COALESCE(sum(paid_amount), 0) AS rev
         FROM orders WHERE status = 'PAID'
         GROUP BY 1 ORDER BY rev DESC",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let dto = serde_json::json!({
        "todayOrders": today.orders,
        "todayRevenueMicro": micros_from_decimal(&today.revenue),
        "pendingShip": pending_ship,
        "availableCardKeys": available_keys,
        "unresolvedFraud": unresolved_fraud,
        "ordersByStatus": orders_by_status.iter().map(|r| serde_json::json!({
            "status": r.status, "count": r.n
        })).collect::<Vec<_>>(),
        "revenueByGateway": revenue_by_gateway.iter().map(|r| serde_json::json!({
            "gateway": r.gateway, "revenueMicro": micros_from_decimal(&r.rev)
        })).collect::<Vec<_>>(),
    });
    Ok(Json(dto).into_response())
}

// 编译期占位: 无需额外依赖
