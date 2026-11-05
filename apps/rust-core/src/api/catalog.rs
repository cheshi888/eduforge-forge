//! 商品目录 API: /catalog/* 公开端点, 替代前端硬编码商品清单.
//! 只读、无需鉴权 (商品橱窗公开); 但仅暴露 active 商品, 不泄露库存/成本等敏感字段.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response, Json};
use axum::Router;
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::FromRow;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/catalog/products", axum::routing::get(list_products))
        .route("/catalog/products/:sku", axum::routing::get(get_product))
        .route("/catalog/categories", axum::routing::get(list_categories))
}

#[derive(Debug, Deserialize, Default)]
pub struct ProductQuery {
    category: Option<String>,
    search: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, FromRow)]
struct ProductRow {
    id: i64,
    sku: String,
    title: String,
    category: String,
    cover: Option<String>,
    description: Option<String>,
    unit_price: Decimal,
    currency: String,
    rating: Decimal,
    sold_count: i64,
    badge: Option<String>,
}

fn micros(d: &Decimal) -> i64 {
    let m = (d * Decimal::new(1_000_000, 0)).round_dp(0);
    m.to_string().parse::<i64>().unwrap_or(0)
}

fn product_to_json(r: &ProductRow) -> serde_json::Value {
    serde_json::json!({
        "id": r.id,
        "sku": r.sku,
        "name": r.title,
        "category": r.category,
        "cover": r.cover,
        "desc": r.description,
        "priceMicro": micros(&r.unit_price),
        "currency": r.currency.trim(),
        "rating": r.rating,
        "sold": r.sold_count,
        "badge": r.badge,
    })
}

// 商品列表: 支持分类筛选 / 关键词搜索 / 分页. 仅返回 active 商品.
pub async fn list_products(
    State(state): State<AppState>,
    Query(q): Query<ProductQuery>,
) -> Result<Response, Response> {
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let offset = q.offset.unwrap_or(0).max(0);

    let mut sql = String::from(
        "SELECT id, sku, title, category, cover, description, unit_price, currency,
                rating, sold_count, badge
         FROM products WHERE active = TRUE",
    );
    let mut binds: Vec<serde_json::Value> = Vec::new();
    if let Some(cat) = &q.category {
        sql.push_str(&format!(" AND category = ${}", binds.len() + 1));
        binds.push(serde_json::json!(cat));
    }
    if let Some(s) = &q.search {
        let ph = binds.len() + 1;
        sql.push_str(&format!(
            " AND (title ILIKE ${ph} OR description ILIKE ${ph})"
        ));
        binds.push(serde_json::json!(format!("%{}%", s)));
    }
    sql.push_str(" ORDER BY sold_count DESC, id ASC");
    sql.push_str(&format!(" LIMIT ${} OFFSET ${}", binds.len() + 1, binds.len() + 2));

    let mut query = sqlx::query_as::<_, ProductRow>(&sql);
    for b in &binds {
        query = query.bind(b.as_str().unwrap_or_default().to_string());
    }
    query = query.bind(limit).bind(offset);
    let rows: Vec<ProductRow> = query.fetch_all(&state.pool).await.map_err(db_err)?;
    let out: Vec<serde_json::Value> = rows.iter().map(product_to_json).collect();
    Ok(Json(out).into_response())
}

pub async fn get_product(
    State(state): State<AppState>,
    Path(sku): Path<String>,
) -> Result<Response, Response> {
    let r: Option<ProductRow> = sqlx::query_as(
        "SELECT id, sku, title, category, cover, description, unit_price, currency,
                rating, sold_count, badge
         FROM products WHERE sku = $1 AND active = TRUE",
    )
    .bind(&sku)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let Some(r) = r else {
        return Ok((StatusCode::NOT_FOUND, "product not found").into_response());
    };
    Ok(Json(product_to_json(&r)).into_response())
}

// 分类清单 (固定四类, 供前端筛选器渲染)
pub async fn list_categories() -> Response {
    Json(serde_json::json!([
        { "code": "source", "label": "源码" },
        { "code": "tutorial", "label": "教程" },
        { "code": "card", "label": "卡密" },
        { "code": "resource", "label": "资源包" }
    ]))
    .into_response()
}

fn db_err(e: sqlx::Error) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
}
