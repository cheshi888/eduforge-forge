//! 优惠券营销 API: /promotion/* 与 /user/coupons/*
//! 优惠券在结算链路中真实应用: 下单时叠加在会员折扣之后的折后价上.
//! 金额全程 Decimal 整数运算禁浮点, 与金额契约对齐.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response, Json};
use axum::Router;
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::FromRow;

use crate::api::admin::guard_admin;
use crate::api::auth::user_id_from_token;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // 用户侧: 领券 / 我的券 / 可用券
        .route("/user/coupons", axum::routing::get(list_my_coupons).post(claim_coupon))
        // 公开: 可领券列表 (橱窗展示)
        .route("/catalog/coupons", axum::routing::get(list_available_coupons))
        // admin 侧: 券模板管理
        .route("/admin/coupons", axum::routing::get(list_coupons).post(create_coupon))
}

#[derive(Debug, FromRow)]
pub struct CouponRow {
    id: i64,
    code: String,
    title: String,
    coupon_type: String,
    discount_value: Decimal,
    threshold_micro: Decimal,
    max_uses_total: i32,
    max_uses_per_user: i32,
    valid_until: Option<chrono::DateTime<chrono::Utc>>,
    active: bool,
}

fn micros(d: &Decimal) -> i64 {
    let m = (d * Decimal::new(1_000_000, 0)).round_dp(0);
    m.to_string().parse::<i64>().unwrap_or(0)
}

fn coupon_to_json(r: &CouponRow) -> serde_json::Value {
    serde_json::json!({
        "id": r.id,
        "code": r.code,
        "title": r.title,
        "couponType": r.coupon_type,
        "discountValueMicro": micros(&r.discount_value),
        "discountPct": if r.coupon_type == "percent" { Some(r.discount_value) } else { None },
        "thresholdMicro": micros(&r.threshold_micro),
        "maxUsesTotal": r.max_uses_total,
        "maxUsesPerUser": r.max_uses_per_user,
        "validUntil": r.valid_until.map(|t| t.to_rfc3339()),
        "active": r.active,
    })
}

// 公开可领券列表 (仅 active 且未过期的券模板)
pub async fn list_available_coupons(
    State(state): State<AppState>,
) -> Result<Response, Response> {
    let rows: Vec<CouponRow> = sqlx::query_as(
        "SELECT id, code, title, coupon_type, discount_value, threshold_micro,
                max_uses_total, max_uses_per_user, valid_until, active
         FROM coupons
         WHERE active = TRUE AND (valid_until IS NULL OR valid_until > now())
         ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows.iter().map(coupon_to_json).collect();
    Ok(Json(out).into_response())
}

// 我的券 (含状态)
#[derive(Debug, FromRow)]
struct MyCouponRow {
    id: i64,
    code: String,
    title: String,
    coupon_type: String,
    discount_value: Decimal,
    threshold_micro: Decimal,
    valid_until: Option<chrono::DateTime<chrono::Utc>>,
    status: String,
}

pub async fn list_my_coupons(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    let rows: Vec<MyCouponRow> = sqlx::query_as(
        "SELECT uc.id, c.code, c.title, c.coupon_type, c.discount_value, c.threshold_micro,
                c.valid_until, uc.status
         FROM user_coupons uc JOIN coupons c ON c.id = uc.coupon_id
         WHERE uc.user_id = $1
         ORDER BY uc.id DESC",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "code": r.code,
                "title": r.title,
                "couponType": r.coupon_type,
                "discountValueMicro": micros(&r.discount_value),
                "discountPct": if r.coupon_type == "percent" { Some(r.discount_value) } else { None },
                "thresholdMicro": micros(&r.threshold_micro),
                "validUntil": r.valid_until.map(|t| t.to_rfc3339()),
                "status": r.status,
            })
        })
        .collect();
    Ok(Json(out).into_response())
}

#[derive(Debug, Deserialize)]
pub struct ClaimCouponReq {
    code: String,
}

// 领券: 校验券存在/active/未过期/未超发/未超每人限次, 写入 user_coupons.
// B4-08 (并发领券竞态): 用事务 + SELECT FOR UPDATE 锁 coupon 行, 串行化同一券的并发领取,
// 堵住 count 检查与 INSERT 之间的 TOCTOU 窗口, 防同一用户并发超领 (max_uses_per_user 失效).
pub async fn claim_coupon(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ClaimCouponReq>,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;

    let code = req.code.trim().to_uppercase();
    let mut tx = state.pool.begin().await.map_err(db_err)?;

    // 锁定券模板行, 并发领券在此串行化 (同券的后续请求阻塞等待)
    let coupon: Option<CouponRow> = sqlx::query_as(
        "SELECT id, code, title, coupon_type, discount_value, threshold_micro,
                max_uses_total, max_uses_per_user, valid_until, active
         FROM coupons WHERE code = $1 AND active = TRUE FOR UPDATE",
    )
    .bind(&code)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?;
    let Some(coupon) = coupon else {
        let _ = tx.rollback().await;
        return Ok((StatusCode::NOT_FOUND, "coupon not found or inactive").into_response());
    };
    if let Some(until) = coupon.valid_until {
        if until < chrono::Utc::now() {
            let _ = tx.rollback().await;
            return Ok((StatusCode::GONE, "coupon expired").into_response());
        }
    }

    // 已领取数量校验 (事务内, 与上面的行锁保证原子性)
    let claimed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_coupons WHERE user_id = $1 AND coupon_id = $2",
    )
    .bind(user_id)
    .bind(coupon.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_err)?;
    if claimed >= coupon.max_uses_per_user as i64 {
        let _ = tx.rollback().await;
        return Ok((StatusCode::CONFLICT, "coupon already claimed max times").into_response());
    }

    // 全平台发放总量校验 (若配置了 max_uses_total > 0)
    if coupon.max_uses_total > 0 {
        let total_claimed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM user_coupons WHERE coupon_id = $1",
        )
        .bind(coupon.id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;
        if total_claimed >= coupon.max_uses_total as i64 {
            let _ = tx.rollback().await;
            return Ok((StatusCode::CONFLICT, "coupon sold out").into_response());
        }
    }

    sqlx::query(
        "INSERT INTO user_coupons (user_id, coupon_id, status)
         VALUES ($1, $2, 'UNUSED')",
    )
    .bind(user_id)
    .bind(coupon.id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    tx.commit().await.map_err(db_err)?;
    Ok(Json(serde_json::json!({ "claimed": coupon.code })).into_response())
}

// admin 侧: 券列表 + 创建券模板
pub async fn list_coupons(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let rows: Vec<CouponRow> = sqlx::query_as(
        "SELECT id, code, title, coupon_type, discount_value, threshold_micro,
                max_uses_total, max_uses_per_user, valid_until, active
         FROM coupons ORDER BY id DESC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows.iter().map(coupon_to_json).collect();
    Ok(Json(out).into_response())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCouponReq {
    code: String,
    title: String,
    coupon_type: String,
    discount_value_micro: i64,
    threshold_micro: i64,
    max_uses_total: i32,
    max_uses_per_user: i32,
    valid_until: Option<String>,
}

pub async fn create_coupon(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateCouponReq>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    if !matches!(req.coupon_type.as_str(), "fixed" | "percent") {
        return Ok((StatusCode::BAD_REQUEST, "invalid coupon type").into_response());
    }
    let discount = Decimal::new(req.discount_value_micro, 6);
    let threshold = Decimal::new(req.threshold_micro, 6);
    let valid_until = req
        .valid_until
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&chrono::Utc));
    sqlx::query(
        "INSERT INTO coupons (code, title, coupon_type, discount_value, threshold_micro,
                              max_uses_total, max_uses_per_user, valid_until)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&req.code.to_uppercase())
    .bind(&req.title)
    .bind(&req.coupon_type)
    .bind(discount)
    .bind(threshold)
    .bind(req.max_uses_total)
    .bind(req.max_uses_per_user)
    .bind(valid_until)
    .execute(&state.pool)
    .await
    .map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "code": req.code.to_uppercase() }))).into_response())
}

// 优惠券核销计算 (纯函数, 供下单链路复用 + 单测):
// 输入折后价 (会员折扣后) 与券, 返回 (最终价, 优惠金额).
// fixed: 减免 discount_value; percent: 折后价 * (100 - pct) / 100. 均受门槛约束 (调用方先校验门槛).
pub fn apply_coupon(
    discounted: Decimal,
    coupon_type: &str,
    discount_value: Decimal,
) -> (Decimal, Decimal) {
    let final_amount = match coupon_type {
        "fixed" => discounted - discount_value,
        "percent" => {
            let factor = Decimal::from(100i64) - discount_value;
            (discounted * factor / Decimal::from(100i64)).round_dp(6)
        }
        _ => discounted,
    };
    let final_amount = if final_amount < Decimal::ZERO {
        Decimal::ZERO
    } else {
        final_amount
    };
    (final_amount, discounted - final_amount)
}

fn db_err(e: sqlx::Error) -> Response {
    let status = match &e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, e.to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_coupon_reduces_amount() {
        // 折后 269.1 元 - 20 元 = 249.1 元
        let (final_amt, discount) =
            apply_coupon(Decimal::new(269_100_000, 6), "fixed", Decimal::new(20, 0));
        assert_eq!(final_amt, Decimal::new(249_100_000, 6));
        assert_eq!(discount, Decimal::new(20_000_000, 6));
    }

    #[test]
    fn percent_coupon_ten_percent() {
        // 折后 299 元 * 0.9 = 269.1 元
        let (final_amt, discount) =
            apply_coupon(Decimal::new(299_000_000, 6), "percent", Decimal::new(10, 0));
        assert_eq!(final_amt, Decimal::new(269_100_000, 6));
        assert_eq!(discount, Decimal::new(29_900_000, 6));
    }

    #[test]
    fn coupon_never_goes_negative() {
        // 折后 10 元 - 20 元 → 0 (不出现负数)
        let (final_amt, _) =
            apply_coupon(Decimal::new(10_000_000, 6), "fixed", Decimal::new(20, 0));
        assert_eq!(final_amt, Decimal::ZERO);
    }
}
