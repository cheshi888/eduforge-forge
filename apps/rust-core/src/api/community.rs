//! 社区 API: /catalog/* 评论/评分/问答 (公开读) 与 /user/* 收藏/通知 (用户侧).
//! 评论/评分仅已购用户可写 (校验 product_deliveries 归属), 防刷评.
//! 通知为站内信, 支付/发货/升级等履约事件由消费侧落库.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response, Json};
use axum::Router;
use serde::Deserialize;
use sqlx::FromRow;

use crate::api::auth::user_id_from_token;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // 公开读 (评论/问答)
        .route("/catalog/products/:sku/reviews", axum::routing::get(list_reviews))
        .route("/catalog/products/:sku/questions", axum::routing::get(list_questions))
        // 用户侧 (评论/问答/收藏/通知)
        .route("/user/reviews", axum::routing::post(create_review))
        .route("/user/questions", axum::routing::post(create_question))
        .route("/user/favorites", axum::routing::get(list_favorites).post(toggle_favorite))
        .route("/user/notifications", axum::routing::get(list_notifications))
        .route("/user/notifications/read", axum::routing::post(mark_all_read))
}

// ---------------------------------------------------------------------------
// 公开读
// ---------------------------------------------------------------------------

#[derive(Debug, FromRow)]
struct ReviewRow {
    id: i64,
    rating: i16,
    content: Option<String>,
    user_uid: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_reviews(
    State(state): State<AppState>,
    Path(sku): Path<String>,
) -> Result<Response, Response> {
    let rows: Vec<ReviewRow> = sqlx::query_as(
        "SELECT r.id, r.rating, r.content, u.uid AS user_uid, r.created_at
         FROM product_reviews r
         JOIN users u ON u.id = r.user_id
         JOIN products p ON p.id = r.product_id
         WHERE p.sku = $1
         ORDER BY r.created_at DESC LIMIT 100",
    )
    .bind(&sku)
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "rating": r.rating,
                "content": r.content,
                "userUid": r.user_uid,
                "createdAt": r.created_at.to_rfc3339(),
            })
        })
        .collect();
    Ok(Json(out).into_response())
}

#[derive(Debug, FromRow)]
struct QuestionRow {
    id: i64,
    question: String,
    answer: Option<String>,
    user_uid: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_questions(
    State(state): State<AppState>,
    Path(sku): Path<String>,
) -> Result<Response, Response> {
    let rows: Vec<QuestionRow> = sqlx::query_as(
        "SELECT q.id, q.question, q.answer, u.uid AS user_uid, q.created_at
         FROM product_questions q
         JOIN users u ON u.id = q.user_id
         JOIN products p ON p.id = q.product_id
         WHERE p.sku = $1
         ORDER BY q.created_at DESC LIMIT 100",
    )
    .bind(&sku)
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|q| {
            serde_json::json!({
                "id": q.id,
                "question": q.question,
                "answer": q.answer,
                "userUid": q.user_uid,
                "createdAt": q.created_at.to_rfc3339(),
            })
        })
        .collect();
    Ok(Json(out).into_response())
}

// ---------------------------------------------------------------------------
// 用户侧 (写)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReviewReq {
    sku: String,
    rating: i16,
    content: Option<String>,
}

// 发表评论: 仅已购用户可评 (校验该用户对该产品存在 product_deliveries 记录).
pub async fn create_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReviewReq>,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    if !(1..=5).contains(&req.rating) {
        return Ok((StatusCode::BAD_REQUEST, "rating must be 1..=5").into_response());
    }
    // B4-10 (DoS 防护): 评论内容长度上限, 防超长文本刷库.
    if let Some(content) = req.content.as_deref() {
        if content.chars().count() > 2000 {
            return Ok((StatusCode::BAD_REQUEST, "review content too long (max 2000)").into_response());
        }
    }
    // 已购校验: 该用户对该商品存在交付记录
    let owned: Option<(i64,)> = sqlx::query_as(
        "SELECT 1::bigint FROM product_deliveries d
         JOIN products p ON p.id = d.product_id
         WHERE d.user_id = $1 AND p.sku = $2 LIMIT 1",
    )
    .bind(user_id)
    .bind(&req.sku)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    if owned.is_none() {
        return Ok((StatusCode::FORBIDDEN, "only buyers can review").into_response());
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO product_reviews (product_id, user_id, rating, content)
         VALUES ((SELECT id FROM products WHERE sku = $1), $2, $3, $4) RETURNING id",
    )
    .bind(&req.sku)
    .bind(user_id)
    .bind(req.rating)
    .bind(&req.content)
    .fetch_one(&state.pool)
    .await
    .map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateQuestionReq {
    sku: String,
    question: String,
}

pub async fn create_question(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateQuestionReq>,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    let q = req.question.trim();
    if q.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, "question is empty").into_response());
    }
    // B4-10 (DoS 防护): 提问长度上限.
    if q.chars().count() > 2000 {
        return Ok((StatusCode::BAD_REQUEST, "question too long (max 2000)").into_response());
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO product_questions (product_id, user_id, question)
         VALUES ((SELECT id FROM products WHERE sku = $1), $2, $3) RETURNING id",
    )
    .bind(&req.sku)
    .bind(user_id)
    .bind(q)
    .fetch_one(&state.pool)
    .await
    .map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response())
}

// ---------------------------------------------------------------------------
// 收藏
// ---------------------------------------------------------------------------

#[derive(Debug, FromRow)]
struct FavoriteRow {
    id: i64,
    sku: String,
    title: String,
    category: String,
    unit_price: rust_decimal::Decimal,
    currency: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_favorites(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    let rows: Vec<FavoriteRow> = sqlx::query_as(
        "SELECT f.id, p.sku, p.title, p.category, p.unit_price, p.currency, f.created_at
         FROM favorites f JOIN products p ON p.id = f.product_id
         WHERE f.user_id = $1 ORDER BY f.created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let micros = (r.unit_price * rust_decimal::Decimal::new(1_000_000, 0))
                .round_dp(0)
                .to_string()
                .parse::<i64>()
                .unwrap_or(0);
            serde_json::json!({
                "id": r.id,
                "sku": r.sku,
                "title": r.title,
                "category": r.category,
                "priceMicro": micros,
                "currency": r.currency.trim(),
                "createdAt": r.created_at.to_rfc3339(),
            })
        })
        .collect();
    Ok(Json(out).into_response())
}

#[derive(Debug, Deserialize)]
pub struct ToggleFavoriteReq {
    sku: String,
}

// 收藏/取消收藏 (toggle)
pub async fn toggle_favorite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ToggleFavoriteReq>,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    let exists: Option<(i64,)> = sqlx::query_as(
        "SELECT f.id FROM favorites f JOIN products p ON p.id = f.product_id
         WHERE f.user_id = $1 AND p.sku = $2 LIMIT 1",
    )
    .bind(user_id)
    .bind(&req.sku)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let favorited = if let Some((fid,)) = exists {
        sqlx::query("DELETE FROM favorites WHERE id = $1")
            .bind(fid)
            .execute(&state.pool)
            .await
            .map_err(db_err)?;
        false
    } else {
        sqlx::query(
            "INSERT INTO favorites (user_id, product_id)
             VALUES ($1, (SELECT id FROM products WHERE sku = $2))",
        )
        .bind(user_id)
        .bind(&req.sku)
        .execute(&state.pool)
        .await
        .map_err(db_err)?;
        true
    };
    Ok(Json(serde_json::json!({ "favorited": favorited })).into_response())
}

// ---------------------------------------------------------------------------
// 通知
// ---------------------------------------------------------------------------

#[derive(Debug, FromRow)]
struct NotificationRow {
    id: i64,
    kind: String,
    title: String,
    body: Option<String>,
    read: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    let rows: Vec<NotificationRow> = sqlx::query_as(
        "SELECT id, kind, title, body, read, created_at
         FROM notifications WHERE user_id = $1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "kind": n.kind,
                "title": n.title,
                "body": n.body,
                "read": n.read,
                "createdAt": n.created_at.to_rfc3339(),
            })
        })
        .collect();
    Ok(Json(out).into_response())
}

pub async fn mark_all_read(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    let user_id = user_id_from_token(&state, &headers)
        .await
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())?;
    sqlx::query("UPDATE notifications SET read = TRUE WHERE user_id = $1 AND read = FALSE")
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn db_err(e: sqlx::Error) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
}
