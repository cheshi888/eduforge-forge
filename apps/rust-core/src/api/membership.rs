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
use crate::api::auth::user_id_from_token;
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
            "/user/deliveries/:id/download",
            axum::routing::post(mark_downloaded),
        )
        .route("/files/:id", axum::routing::get(download_file))
        .route("/user/tier", axum::routing::get(get_tiers))
        // admin 侧
        .route(
            "/admin/memberships/tiers",
            axum::routing::get(list_tiers_admin),
        )
        .route("/admin/users/:uid/tier", axum::routing::post(set_user_tier))
        .route(
            "/admin/deliveries/files",
            axum::routing::post(upload_file_meta).get(list_files),
        )
}

// ---------------------------------------------------------------------------
// 用户侧
// ---------------------------------------------------------------------------

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
    discount_pct: i16,
    max_downloads: i16,
    perk_json: serde_json::Value,
}

// 字段 price_micro / total_spent_micro 在库内已是微单位 (1 元 = 1_000_000 micro),
// 直接取整返回即可。修复 B4-11: 原实现又乘 1_000_000, 导致会员套餐价显示放大 100 万倍
// (¥99 → ¥99,000,000)。这里的 Decimal 已是微单位数值 (如 99000000.000000), 无需再换算。
fn micros(d: &Decimal) -> i64 {
    d.round_dp(0).to_string().parse::<i64>().unwrap_or(0)
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
    headers: HeaderMap,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    let row: Option<ProfileRow> = sqlx::query_as(
        "SELECT u.uid, u.tier_code, to_char(u.tier_expires_at,'YYYY-MM-DD') AS tier_expires_at,
                u.total_spent_micro, t.discount_pct, t.max_downloads, t.perk_json
         FROM users u JOIN membership_tiers t ON t.code = u.tier_code
         WHERE u.id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let row = row.ok_or_else(|| (StatusCode::NOT_FOUND, "user not found").into_response())?;
    let dto = ProfileDto {
        uid: row.uid,
        tier_code: row.tier_code,
        tier_expires_at: row.tier_expires_at,
        total_spent_micro: micros(&row.total_spent_micro),
        discount_pct: row.discount_pct as i32,
        max_downloads: row.max_downloads as i32,
        perk_labels: perk_labels(&row.perk_json),
    };
    Ok(Json(dto).into_response())
}

#[derive(Debug, Deserialize)]
pub struct UpgradeReq {
    #[allow(dead_code)]
    tier_code: String,
}

// 升级会员: B4-09 (免费升级漏洞) 修复 — 会员升级必须走支付流程, 禁止直接免费调用.
// 正确路径: 购买会员商品下单支付 → webhook 结算时按 total_spent_micro 自动晋升 (webhook.rs),
// 或由 admin 通道 (set_user_tier) 人工变更. 本端点不再接受用户侧直接升级.
pub async fn upgrade_tier(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_req): Json<UpgradeReq>,
) -> Result<Response, Response> {
    // 保留鉴权 (确认身份), 但拒绝直接升级, 引导走支付流程.
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    let _ = user_id;
    Ok((
        StatusCode::FORBIDDEN,
        "membership upgrade requires payment; place an order for the tier instead",
    )
        .into_response())
}

#[derive(Debug, FromRow)]
struct TierRow {
    code: String,
    name: String,
    price_micro: Decimal,
    duration_days: i32,
    discount_pct: i16,
    max_downloads: i16,
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
    headers: HeaderMap,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    let rows: Vec<DeliveryRow> = sqlx::query_as(
        "SELECT d.id, d.delivery_type, d.file_name, d.file_url, d.card_key_id,
                d.download_count, p.sku AS product_sku, p.title AS product_title, p.category AS product_category, k.key_code AS card_key, to_char(d.created_at,'YYYY-MM-DD') AS created_at
         FROM product_deliveries d
         JOIN products p ON p.id = d.product_id
         LEFT JOIN card_keys k ON k.id = d.card_key_id
         WHERE d.user_id = $1
         ORDER BY d.id DESC",
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
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    let row: Option<(i16, i32, Option<i64>)> = sqlx::query_as(
        "SELECT u.max_downloads, COALESCE(d.download_count,0), d.id
          FROM (SELECT u.id, t.max_downloads FROM users u JOIN membership_tiers t
                ON t.code = u.tier_code WHERE u.id=$1) u
          LEFT JOIN product_deliveries d ON d.id=$2 AND d.user_id=u.id",
    )
    .bind(user_id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let Some((max_dl, count, _)) = row else {
        return Ok((StatusCode::NOT_FOUND, "delivery not found").into_response());
    };
    if count >= max_dl as i32 {
        return Ok((
            StatusCode::FORBIDDEN,
            format!("download limit reached for tier (max {max_dl})"),
        )
            .into_response());
    }
    let res = sqlx::query(
        "UPDATE product_deliveries SET download_count = download_count + 1,
             downloaded_at = now() WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
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

// admin 侧等级列表: 与用户侧数据一致, 但必须走 admin 鉴权 (越权枚举防护)
pub async fn list_tiers_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let out = tiers_payload(&state).await?;
    Ok(Json(out).into_response())
}

async fn tiers_payload(state: &AppState) -> Result<Vec<serde_json::Value>, Response> {
    let rows: Vec<TierRow> = sqlx::query_as(
        "SELECT code, name, price_micro, duration_days, discount_pct, max_downloads, perk_json
         FROM membership_tiers ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    Ok(rows
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
        .collect())
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
    storage_key: Option<String>,
    size_bytes: Option<i64>,
    mime_type: Option<String>,
    checksum_sha256: Option<String>,
    // 可选内联内容 (base64 或明文), 用于本地文件服务接线; 留空则只登记元数据
    content: Option<String>,
}

// 登记已上传文件: 若带 content, 写入本地 uploads 目录并登记 storage_key = local://<abs>;
// 否则仅落库元数据 (真实对象存储/MinIO 接线留接口)。
pub async fn upload_file_meta(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UploadFileMetaReq>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    // B4-06 (路径穿越): file_name 只取 basename, 拒绝含路径分隔符/../的输入, 防逃逸 uploads 目录写任意文件.
    let safe_name = sanitize_file_name(&req.file_name);
    if safe_name.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, "invalid file name").into_response());
    }
    let storage_key = if let Some(content) = req.content.as_ref() {
        let base = state
            .uploads_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().map(|p| p.join("uploads")).unwrap_or_default());
        let dir = base.join(req.product_id.to_string());
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("mkdir failed: {e}")).into_response())?;
        let dest = dir.join(&safe_name);
        // 优先按 base64 解码; 解码失败则按原文写入 (兼容明文上传)
        let raw = base64_decode(content.as_bytes())
            .unwrap_or_else(|| content.as_bytes().to_vec());
        tokio::fs::write(&dest, &raw)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("write failed: {e}")).into_response())?;
        format!("local://{}", dest.display())
    } else {
        req.storage_key.clone().unwrap_or_default()
    };
    let size = req.size_bytes.unwrap_or(0);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO uploaded_files (product_id, file_name, storage_key, size_bytes, mime_type, checksum_sha256)
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(req.product_id)
    .bind(&safe_name)
    .bind(&storage_key)
    .bind(size)
    .bind(&req.mime_type)
    .bind(&req.checksum_sha256)
    .fetch_one(&state.pool)
    .await
    .map_err(db_err)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id, "storageKey": storage_key })),
    )
    .into_response())
}

// B4-06 (路径穿越防护): 仅保留文件名 basename, 剔除路径分隔符、盘符、父目录标记.
// 返回值若为空表示输入非法 (如 ".." 或纯分隔符), 调用方应拒绝.
fn sanitize_file_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // 取最后一段 basename (兼容 '/' 与 '\\' 两种分隔符)
    let base = trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(trimmed);
    // 剔除控制字符与空字节, 拒绝 ".." 及盘符形式
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return String::new();
    }
    // 拒绝 Windows 盘符 (C:) 或剩余分隔符
    if cleaned.contains(':') || cleaned.contains('/') || cleaned.contains('\\') {
        return String::new();
    }
    cleaned
}

// 纯本地 base64 解码 (避免引入外部 crate); 不支持时返 None (调用方 fallback 原文)
fn base64_decode(input: &[u8]) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(input).ok()?.trim();
    let lookup = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(s.len());
    let mut bits: u32 = 0;
    let mut n = 0usize;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let v = lookup(c)?;
        bits = (bits << 6) | v as u32;
        n += 1;
        if n == 4 {
            out.push(((bits >> 16) & 0xff) as u8);
            out.push(((bits >> 8) & 0xff) as u8);
            out.push((bits & 0xff) as u8);
            bits = 0;
            n = 0;
        }
    }
    if n == 2 {
        out.push((bits << 4 & 0xff) as u8);
    } else if n == 3 {
        out.push((bits << 2 & 0xff) as u8);
        out.push((bits << 8 & 0xff) as u8);
    }
    Some(out)
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
    Query(q): Query<FileQuery>,
) -> Result<Response, Response> {
    guard_admin(&state, &headers)?;
    let prod_opt = q.product_id;
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

// 真实文件下载: 校验交付物归属 (非属主/无单 404 不泄露枚举), 读 storage_key 指向的本地文件,
// 流式返回. 额度约束走 /user/deliveries/{id}/download (mark_downloaded), 此端点只管取文件.
pub async fn download_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    let row: Option<(i64, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT d.user_id, d.file_url, uf.storage_key, COALESCE(uf.file_name, d.file_name, 'delivery')
         FROM product_deliveries d LEFT JOIN uploaded_files uf ON uf.id = (
             SELECT id FROM uploaded_files WHERE product_id = d.product_id AND active ORDER BY id DESC LIMIT 1
         ) WHERE d.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let Some((owner, file_url, storage_key, _fn)) = row else {
        return Ok((StatusCode::NOT_FOUND, "delivery not found").into_response());
    };
    if owner != user_id {
        return Ok((StatusCode::NOT_FOUND, "delivery not found").into_response());
    }
    let key = file_url.or(storage_key).unwrap_or_default();
    if key.is_empty() {
        return Ok((StatusCode::NOT_FOUND, "no file bound to this delivery").into_response());
    }
    let disp_name = key.rsplit('/').next().unwrap_or("file");
    let path_str = key.strip_prefix("local://").unwrap_or(key.as_str());
    let data = tokio::fs::read(path_str).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("read file failed: {e}")).into_response()
    })?;
    let resp = axum::http::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Disposition", format!("attachment; filename=\"{}\"", disp_name))
        .header("Content-Length", data.len())
        .body(axum::body::Body::from(data))
        .expect("build file response");
    Ok(resp.into_response())
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
