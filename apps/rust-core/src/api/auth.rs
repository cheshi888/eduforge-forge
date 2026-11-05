//! 用户认证 API: /auth/register /auth/login /auth/logout /auth/me
//! 安全设计:
//!   - 密码 Argon2id 哈希 (PHC 字符串), 绝不落库明文, 登录恒定时间比较.
//!   - 会话用 256-bit 随机 opaque token, DB 只存 SHA-256 哈希 (拖库不泄 token).
//!   - 登录失败不区分「用户不存在 / 密码错误」, 统一 401 防用户枚举.
//!   - 信任边界: 认证后由 Authorization: Bearer <token> 识别身份, 取代写死的 u-demo.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response, Json};
use axum::Router;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use tracing::warn;

use crate::AppState;

// 会话有效期: 30 天
const SESSION_TTL_DAYS: i64 = 30;
const BEARER_PREFIX: &str = "Bearer ";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/register", axum::routing::post(register))
        .route("/auth/login", axum::routing::post(login))
        .route("/auth/logout", axum::routing::post(logout))
        .route("/auth/me", axum::routing::get(me))
}

// ---------------------------------------------------------------------------
// 请求/响应 DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterReq {
    email: String,
    password: String,
    #[serde(default = "default_lang")]
    preferred_language: String,
}

fn default_lang() -> String {
    "zh".to_string()
}

#[derive(Debug, Deserialize)]
pub struct LoginReq {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResp {
    token: String,
    uid: String,
    email: String,
    preferred_language: String,
    tier_code: String,
}

#[derive(Debug, FromRow)]
struct UserAuthRow {
    id: i64,
    uid: String,
    email: String,
    password_hash: Option<String>,
    preferred_language: String,
    tier_code: String,
}

// ---------------------------------------------------------------------------
// 工具函数
// ---------------------------------------------------------------------------

fn hash_password(password: &str) -> Result<String, StatusCode> {
    let salt = SaltString::generate(&mut rand::thread_rng());
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn verify_password(password: &str, phc_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn new_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

// 从 Authorization 头解析 token, 经哈希查会话返回 user_id (无效/过期返回 None)
// 供全站用户侧端点统一鉴权复用 (internal / membership / catalog)
pub async fn user_id_from_token(state: &AppState, headers: &HeaderMap) -> Option<i64> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix(BEARER_PREFIX)?;
    let token_hash = sha256_hex(token.as_bytes());
    sqlx::query_scalar::<_, i64>(
        "SELECT user_id FROM auth_sessions
         WHERE token_hash = $1 AND expires_at > now()",
    )
    .bind(token_hash)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
}

fn invalid_credentials() -> Response {
    (StatusCode::UNAUTHORIZED, "invalid email or password").into_response()
}

fn db_err(e: sqlx::Error) -> Response {
    let status = match &e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, e.to_string()).into_response()
}

// ---------------------------------------------------------------------------
// handlers
// ---------------------------------------------------------------------------

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<Response, Response> {
    let email = req.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Ok((StatusCode::BAD_REQUEST, "invalid email").into_response());
    }
    if req.password.len() < 8 {
        return Ok((StatusCode::BAD_REQUEST, "password too short (min 8)").into_response());
    }
    let lang = if ["zh", "en", "ja", "ko"].contains(&req.preferred_language.as_str()) {
        req.preferred_language.clone()
    } else {
        "zh".to_string()
    };

    let pw_hash = hash_password(&req.password).map_err(|s| s.into_response())?;
    let uid = format!("u-{}", new_token()[..16].to_string());

    let row: UserAuthRow = sqlx::query_as(
        "INSERT INTO users (uid, email, preferred_language, password_hash, tier_code)
         VALUES ($1, $2, $3, $4, 'free')
         RETURNING id, uid, email, password_hash, preferred_language, tier_code",
    )
    .bind(&uid)
    .bind(&email)
    .bind(&lang)
    .bind(&pw_hash)
    .fetch_one(&state.pool)
    .await
    .map_err(db_err)?;

    // 注册即登录: 直接签发会话
    let token = new_token();
    let token_hash = sha256_hex(token.as_bytes());
    sqlx::query(
        "INSERT INTO auth_sessions (user_id, token_hash, expires_at)
         VALUES ($1, $2, now() + ($3 || ' days')::interval)",
    )
    .bind(row.id)
    .bind(&token_hash)
    .bind(SESSION_TTL_DAYS)
    .execute(&state.pool)
    .await
    .map_err(db_err)?;

    Ok(Json(AuthResp {
        token,
        uid: row.uid,
        email: row.email,
        preferred_language: row.preferred_language,
        tier_code: row.tier_code,
    })
    .into_response())
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Response, Response> {
    let email = req.email.trim().to_lowercase();
    let row: Option<UserAuthRow> = sqlx::query_as(
        "SELECT id, uid, email, password_hash, preferred_language, tier_code
         FROM users WHERE email = $1",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;

    let Some(row) = row else {
        return Ok(invalid_credentials());
    };
    // 未设置密码的遗留用户 (如 e2e seed) 不可走密码登录
    let Some(pw_hash) = row.password_hash.as_deref() else {
        return Ok(invalid_credentials());
    };
    if !verify_password(&req.password, pw_hash) {
        return Ok(invalid_credentials());
    }

    let token = new_token();
    let token_hash = sha256_hex(token.as_bytes());
    sqlx::query(
        "INSERT INTO auth_sessions (user_id, token_hash, expires_at)
         VALUES ($1, $2, now() + ($3 || ' days')::interval)",
    )
    .bind(row.id)
    .bind(&token_hash)
    .bind(SESSION_TTL_DAYS)
    .execute(&state.pool)
    .await
    .map_err(db_err)?;

    Ok(Json(AuthResp {
        token,
        uid: row.uid,
        email: row.email,
        preferred_language: row.preferred_language,
        tier_code: row.tier_code,
    })
    .into_response())
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let Some(token) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix(BEARER_PREFIX))
    else {
        return StatusCode::NO_CONTENT.into_response();
    };
    let token_hash = sha256_hex(token.as_bytes());
    sqlx::query("DELETE FROM auth_sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(&state.pool)
        .await
        .ok();
    StatusCode::NO_CONTENT.into_response()
}

pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let Some(user_id) = user_id_from_token(&state, &headers).await else {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    };
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT uid, email, preferred_language, tier_code FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let Some((uid, email, preferred_language, tier_code)) = row else {
        warn!(user_id, "session user row missing");
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    };
    Json(serde_json::json!({
        "uid": uid,
        "email": email,
        "preferredLanguage": preferred_language,
        "tierCode": tier_code,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_roundtrip() {
        let h = hash_password("correct-horse-battery").unwrap();
        assert!(h.starts_with("$argon2"));
        assert!(verify_password("correct-horse-battery", &h));
        assert!(!verify_password("wrong-password", &h));
    }

    #[test]
    fn token_hashes_are_unique_and_stable() {
        let t = new_token();
        assert_eq!(t.len(), 64); // 32 bytes hex
        assert_eq!(sha256_hex(t.as_bytes()), sha256_hex(t.as_bytes()));
        assert_ne!(new_token(), new_token());
    }
}
