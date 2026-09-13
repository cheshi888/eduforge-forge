//! 会员中心 + 内容交付 API: /user/* (用户侧, 需登录 uid) 与 /admin/memberships, /admin/deliveries.
//! 信任边界: 用户侧按 uid 归属校验 (同 internal C-08); admin 侧 Bearer fail-closed.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response, Json};
use axum::Router;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::api::admin::guard_admin;
use crate::domain::membership::Tier;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // 用户侧
        .route(
            "/user/profile",
            axum::routing::get(get_profile).post(upgrade_tier),
        )
        .route("/user/deliveries", axum::routing::get(list_deliveries))
        .route(
            "/user/deliveries/{id}/download",
            axum::routing::post(mark_downloaded),
        )
        .route("/user/tier", axum::routing::get(get_tiers))
        // admin 侧
        .route("/admin/memberships/tiers", axum::routing::get(list_tiers))
        .route("/admin/users/{uid}/tier", axum::routing::post(set_user_tier))
        .route(
            "/admin/deliveries/files",
            axum::routing::post(upload_file_meta).get(list_files),
        )
}

// ---------------------------------------------------------------------------
// 用户侧
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UidQuery {
    uid: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDto {
    uid: String,
    tier_code: String,
    tier_expires_at: Option<String>,
    total_spent_micro: i64,
    discount_pct: i32,
    max_downloads: i32,
    perk_labels: Vec<String>,
}

#[derive(Debug, FromRow)]
struct ProfileRow {
    uid: String,
    tier_code: String,
    tier_expires_at: Option<String>,
    total_spent_micro: Decimal,
    discount_pct: i32,
    max_downloads: i32,
    perk_json: serde_json::Value,
}

fn micros(d: &Decimal) -> i64 {
    let m = (d * Decimal::new(1_000_000, 0)).round_dp(0);
    m.to_string().parse::<i64>().unwrap_or(0)
}

fn perk_labels(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|it| it.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub async fn get_profile(
    State(state): State<AppState>,
    Query(q): Query<UidQuery>,
) -> Result<Response, Response> {
    let row: Option<ProfileRow> = sqlx::query_as(
        "SELECT u.uid, u.tier_code, to_char(u.tier_expires_at,'YYYY-MM-DD') AS tier_expires_at,
                u.total_spent_micro, t.discount_pct, t.max_downloads, t.perk_json
         FROM users u JOIN membership_tiers t ON t.code = u.tier_code
         WHERE u.uid = $1",
    )
    .bind(&q.uid)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let row = row.ok_or_else(|| (StatusCode::NOT_FOUND, "user not found").into_response())?;
    let dto = ProfileDto {
        uid: row.uid,
        tier_code: row.tier_code,
        tier_expires_at: row.tier_expires_at,
        total_spent_micro: micros(&row.total_spent_micro),
        discount_pct: row.discount_pct,
        max_downloads: row.max_downloads,
        perk_labels: perk_labels(&row.perk_json),
    };
    Ok(Json(dto).into_response())
}

#[derive(Debug, Deserialize)]
pub struct UpgradeReq {
    uid: String,
    tier_code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeResp {
    uid: String,
    tier_code: String,
}

// 升级会员: 仅允许向更高等级迁移 (can_upgrade_from), 续期/降级走 admin 通道
pub async fn upgrade_tier(
    State(state): State<AppState>,
    Json(req): Json<UpgradeReq>,
) -> Result<Response, Response> {
    let to = Tier::from_db_value(&req.tier_code)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid tier code").into_response())?;
    let cur: Option<Tier> = sqlx::query_as(
        "SELECT tier_code FROM users WHERE uid = $1",
    )
    .bind(&req.uid)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?
    .map(|r: (String,)| {
        Tier::from_db_value(&r.0)
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "bad stored tier").into_response())
    })
    .transpose()
    .map_err(|e| e)?;
    let Some(cur) = cur else {
        return Ok((StatusCode::NOT_FOUND, "user not found").into_response());
    };
    if !cur.can_upgrade_from(to) {
        return Ok((
            StatusCode::BAD_REQUEST,
            format!("cannot upgrade from {} to {}", cur.as_db_value(), to.as_db_value()),
        )
            .into_response());
    }
    sqlx::query(
        "UPDATE users SET tier_code = $2,
             tier_expires_at = COALESCE(tier_expires_at, now()) + interval '1 year',
             updated_at = now()
         WHERE uid = $1",
    )
    .bind(&req.uid)
    .bind(to.as_db_value())
    .execute(&state.pool)
    .await
    .map_err(db_err)?;
    Ok(Json(UpgradeResp {
        uid: req.uid,
        tier_code: to.as_db_value().to_string(),
    })
    .into_response())
}

#[derive(Debug, FromRow)]
struct TierRow {
    code: String,
    name: String,
    price_micro: Decimal,
    duration_days: i32,
    discount_pct: i32,
    max_downloads: i32,
    perk_json: serde_json::Value,
}

pub async fn get_tiers(State(state): State<AppState>) -> Result<Response, Response> {
    let rows: Vec<TierRow> = sqlx::query_as(
        "SELECT code, name, price_micro, duration_days, discount_pct, max_downloads, perk_json
         FROM membership_tiers ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "code": r.code,
                "name": r.name,
                "priceMicro": micros(&r.price_micro),
                "durationDays": r.duration_days,
                "discountPct": r.discount_pct,
                "maxDownloads": r.max_downloads,
                "perks": perk_labels(&r.perk_json),
            })
        })
        .collect();
    Ok(Json(out).into_response())
}

#[derive(Debug, FromRow)]
struct DeliveryRow {
    id: i64,
    delivery_type: String,
    file_name: Option<String>,
    file_url: Option<String>,
    card_key_id: Option<i64>,
    download_count: i32,
    product_sku: String,
    product_title: String,
    product_category: String,
    card_key: Option<String>,
    created_at: String,
}

pub async fn list_deliveries(
    State(state): State<AppState>,
    Query(q): Query<UidQuery>,
) -> Result<Response, Response> {
    let rows: Vec<DeliveryRow> = sqlx::query_as(
        "SELECT d.id, d.delivery_type, d.file_name, d.file_url, d.card_key_id,
                d.download_count, p.sku, p.title, p.category, k.key_code, to_char(d.created_at,'YYYY-MM-DD')
         FROM product_deliveries d
         JOIN products p ON p.id = d.product_id
         LEFT JOIN card_keys k ON k.id = d.card_key_id
         WHERE d.user_id = (SELECT id FROM users WHERE uid = $1)
         ORDER BY d.id DESC",
    )
    .bind(&q.uid)
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "deliveryType": r.delivery_type,
                "fileName": r.file_name,
                "fileUrl": r.file_url,
                "cardKeyId": r.card_key_id,
                "cardKey": r.card_key,
                "downloadCount": r.download_count,
                "productSku": r.product_sku,
                "productTitle": r.product_title,
                "productCategory": r.product_category,
                "createdAt": r.created_at,
            })
        })
        .collect();
    Ok(Json(out).into_response())
}

// 标记下载 (受会员等级 max_downloads 约束, 防滥下)
pub async fn mark_downloaded(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<UidQuery>,
) -> Result<Response, Response> {
    let row: Option<(i32, i32, Option<i64>)> = sqlx::query_as(
        "SELECT u.max_downloads, COALESCE(d.download_count,0), d.id
         FROM (SELECT max_downloads FROM membership_tiers t JOIN users u ON u.tier_code=t.code WHERE u.uid=$1) u
         LEFT JOIN product_deliveries d ON d.id=$2 AND d.user_id=u.id",
    )
    .bind(&q.uid)
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let Some((max_dl, count, _)) = row else {
        return Ok((StatusCode::NOT_FOUND, "delivery not found").into_response());
    };
    if count >= max_dl {
        return Ok((
            StatusCode::FORBIDDEN,
            format!("download limit reached for tier (max {max_dl})"),
        )
            .into_response());
    }
    let res = sqlx::query(
        "UPDATE product_deliveries SET download_count = download_count + 1,
             downloaded_at = now() WHERE id = $1 AND user_id = (SELECT id FROM users WHERE uid = $2)",
    )
    .bind(id)
    .bind(&q.uid)
    .execute(&state.pool)
    .await
    .map_err(db_err)?;
    if res.rows_affected() == 0 {
        return Ok((StatusCode::NOT_FOUND, "delivery not found or not owned").into_response());
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ---------------------------------------------------------------------------
// admin 侧
// ---------------------------------------------------------------------------

pub async fn list_tiers(State(state): State<AppState>) -> Result<Response, Response> {
    let rows: Vec<TierRow> = sqlx::query_as(
        "SELECT code, name, price_micro, duration_days, discount_pct, max_downloads, perk_json
         FROM membership_tiers ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "code": r.code,
                "name": r.name,
                "priceMicro": micros(&r.price_micro),
                "durationDays": r.duration_days,
                "discountPct": r.discount_pct,
                "maxDownloads": r.max_downloads,
                "perks": perk_labels(&r.perk_json),
            })
        })
        .collect();
    Ok(Json(out).into_response())
}
#[derive(Debug, Deserialize)]
pub struct SetTierReq {
    uid: String,
    tier_code: String,
}

pub async fn set_user_tier(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SetTierReq>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let to = Tier::from_db_value(&req.tier_code)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid tier code").into_response())?;
    let res = sqlx::query(
        "UPDATE users SET tier_code = $2,
             tier_expires_at = COALESCE(tier_expires_at, now()) + interval '1 year',
             updated_at = now() WHERE uid = $1",
    )
    .bind(&req.uid)
    .bind(to.as_db_value())
    .execute(&state.pool)
    .await
    .map_err(db_err)?;
    if res.rows_affected() == 0 {
        return Ok((StatusCode::NOT_FOUND, "user not found").into_response());
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Debug, Deserialize)]
pub struct UploadFileMetaReq {
    product_id: i64,
    file_name: String,
    storage_key: String,
    size_bytes: Option<i64>,
    mime_type: Option<String>,
    checksum_sha256: Option<String>,
}

// 登记已上传文件 (真实上传走对象存储/MinIO, 这里落库元数据 + 绑定产品)
pub async fn upload_file_meta(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UploadFileMetaReq>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO uploaded_files (product_id, file_name, storage_key, size_bytes, mime_type, checksum_sha256)
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(req.product_id)
    .bind(&req.file_name)
    .bind(&req.storage_key)
    .bind(req.size_bytes)
    .bind(&req.mime_type)
    .bind(&req.checksum_sha256)
    .fetch_one(&state.pool)
    .await
    .map_err(db_err)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id })),
    )
    .into_response())
}

#[derive(Debug, FromRow)]
struct FileRow {
    id: i64,
    product_id: i64,
    file_name: String,
    storage_key: String,
    size_bytes: Option<i64>,
    active: bool,
}

pub async fn list_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<Option<FileQuery>>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let prod_opt = q.as_ref().and_then(|x| x.product_id);
    let mut sql = String::from(
        "SELECT id, product_id, file_name, storage_key, size_bytes, active FROM uploaded_files WHERE 1=1",
    );
    if prod_opt.is_some() {
        sql.push_str(" AND product_id = $1");
    }
    sql.push_str(" ORDER BY id DESC");
    let mut query = sqlx::query_as::<_, FileRow>(&sql);
    if let Some(prod) = prod_opt {
        query = query.bind(prod);
    }
    let rows: Vec<FileRow> = query.fetch_all(&state.pool).await.map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "productId": r.product_id,
                "fileName": r.file_name,
                "storageKey": r.storage_key,
                "sizeBytes": r.size_bytes,
                "active": r.active,
            })
        })
        .collect();
    Ok(Json(out).into_response())
}

#[derive(Debug, Deserialize, Default)]
pub struct FileQuery {
    product_id: Option<i64>,
}

fn db_err(e: sqlx::Error) -> Response {
    let status = match &e {
        sqlx::Error::Database(db) => {
            let code = db.code();
            if code.as_deref() == Some("23505") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, e.to_string()).into_response()
}
