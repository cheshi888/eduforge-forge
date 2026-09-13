pub mod alipay;
pub mod cardacq;
pub mod coinbase;
pub mod paypal;
pub mod stripe;
pub mod wechat;

use axum::response::Response;
use base64::Engine;
use hmac::{Hmac, Mac};
use http::header::HeaderMap;
use rust_decimal::Decimal;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::str::FromStr;
use tracing::info;

use crate::config::Config;
use crate::domain::order::Blockchain;

pub const FORGE_HMAC_HEADER: &str = "x-forge-signature";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayType {
    Alipay,
    Stripe,
    Coinbase,
    WeChat,
    PayPal,
    CardAcq,
}

impl GatewayType {
    pub fn from_str_v(s: &str) -> Result<Self, GwError> {
        Ok(match s {
            "alipay" => Self::Alipay,
            "stripe" => Self::Stripe,
            "coinbase" => Self::Coinbase,
            "wechat" => Self::WeChat,
            "paypal" => Self::PayPal,
            "cardacq" => Self::CardAcq,
            _ => return Err(GwError::UnknownGateway(s.to_string())),
        })
    }

    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::Alipay => "alipay",
            Self::Stripe => "stripe",
            Self::Coinbase => "coinbase",
            Self::WeChat => "wechat",
            Self::PayPal => "paypal",
            Self::CardAcq => "cardacq",
        }
    }

    // 各网关要求不同的"成功"应答体, 错误应答会让网关无限重试
    pub fn ack(self, status: &str) -> Response {
        let body = match self {
            Self::Alipay => "success".to_string(),
            Self::Stripe => "ok".to_string(),
            Self::Coinbase => serde_json::json!({"code": "SUCCESS", "note": status}).to_string(),
            Self::WeChat => serde_json::json!({"code": "SUCCESS", "message": "成功"}).to_string(),
            Self::PayPal => "ok".to_string(),
            Self::CardAcq => "ok".to_string(),
        };
        Response::builder()
            .status(200)
            .header("Content-Type", "text/plain")
            .body(body.into())
            .unwrap()
    }

    // 与 ack 对称的 4xx 回包, 供网关侧需要返回拒绝语义的场景使用 (当前未接线, 属预留 API)
    #[allow(dead_code)]
    pub fn reject(self, message: &str) -> Response {
        Response::builder()
            .status(400)
            .header("Content-Type", "text/plain")
            .body(message.to_string().into())
            .unwrap()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GwError {
    #[error("unknown gateway: {0}")]
    UnknownGateway(String),
    #[error("ingress hmac mismatch")]
    IngressHmacMismatch,
    #[error("gateway signature invalid: {0}")]
    SignatureInvalid(String),
    #[error("payload parse error: {0}")]
    PayloadParse(String),
    #[error("missing field: {0}")]
    MissingField(String),
    #[error("amount parse error: {0}")]
    AmountParse(String),
    #[error("resource decrypt error: {0}")]
    Decrypt(String),
    #[error("event type rejected: {0}")]
    EventRejected(String),
}

#[derive(Debug, Clone)]
pub struct GatewayTransaction {
    pub gateway_txn_id: String,
    pub out_trade_no: String,
    // 金额以十进制字符串传输, 处理层用 Decimal::from_str 解析, 全程不经过 f64
    pub amount: String,
    pub currency: String,
    pub confirmations: Option<i32>,
    pub crypto_network: Option<Blockchain>,
    pub raw: Value,
}

pub fn hmac_sha256_hex(key: &str, data: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).unwrap();
    mac.update(data);
    hex::encode(mac.finalize().into_bytes())
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

/// 网关入口层 (Envoy/Nginx) 附加的自定义 HMAC 头, 防中间人伪造回调
pub fn verify_ingress_hmac(secret: &str, body: &[u8], header: Option<&str>) -> Result<(), GwError> {
    let provided = header.ok_or(GwError::IngressHmacMismatch)?;
    let expected = hmac_sha256_hex(secret, body);
    if !constant_time_eq(provided, &expected) {
        return Err(GwError::IngressHmacMismatch);
    }
    Ok(())
}

/// 网关原生验签 + 载荷解析: 各网关适配器在 gateway/<name>.rs, 新增网关 = 新增子模块 + 枚举变体
pub fn parse_and_verify(
    cfg: &Config,
    gateway: GatewayType,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<GatewayTransaction, GwError> {
    let text = std::str::from_utf8(body).map_err(|_| GwError::PayloadParse("utf8".into()))?;
    let txn = match gateway {
        GatewayType::Alipay => alipay::verify(text, &cfg.alipay_public_key_pem)?,
        GatewayType::Stripe => stripe::verify(headers, body, &cfg.stripe_webhook_secret)?,
        GatewayType::Coinbase => coinbase::verify(headers, body, &cfg.coinbase_webhook_secret)?,
        GatewayType::WeChat => wechat::verify(cfg, headers, body)?,
        GatewayType::PayPal => paypal::verify(headers, body, &cfg.paypal_webhook_secret)?,
        GatewayType::CardAcq => cardacq::verify(cfg, headers, body)?,
    };
    if Decimal::from_str(&txn.amount).is_err() {
        return Err(GwError::AmountParse(txn.amount.clone()));
    }
    info!(gw = %gateway.as_db_value(), txn = %txn.gateway_txn_id, "gateway txn verified");
    Ok(txn)
}

// H-10: 安全策略 runtime 断言 — 全 crate 代码路径禁用 f32/f64 (docs/architecture.md 金额节:
// 全程 Decimal/整数微单位)。此测试即 CI 门禁单元 (scripts/check-security-policy.sh 调用),
// 防止策略退化为"仅存文档"。注释行先剥离, 避免策略字样 (如"不经过 f64") 造成误报。
#[cfg(test)]
mod float_policy {
    const SOURCES: &[(&str, &str)] = &[
        ("gateway/mod.rs", include_str!("mod.rs")),
        ("gateway/alipay.rs", include_str!("alipay.rs")),
        ("gateway/stripe.rs", include_str!("stripe.rs")),
        ("gateway/coinbase.rs", include_str!("coinbase.rs")),
        ("gateway/paypal.rs", include_str!("paypal.rs")),
        ("gateway/wechat.rs", include_str!("wechat.rs")),
        ("gateway/cardacq.rs", include_str!("cardacq.rs")),
        (
            "gateway/cardacq/braintree.rs",
            include_str!("cardacq/braintree.rs"),
        ),
        ("gateway/cardacq/adyen.rs", include_str!("cardacq/adyen.rs")),
        ("api/mod.rs", include_str!("../api/mod.rs")),
        ("api/webhook.rs", include_str!("../api/webhook.rs")),
        ("api/internal.rs", include_str!("../api/internal.rs")),
        ("consumer.rs", include_str!("../consumer.rs")),
        ("events.rs", include_str!("../events.rs")),
        ("db.rs", include_str!("../db.rs")),
        ("config.rs", include_str!("../config.rs")),
        ("domain/mod.rs", include_str!("../domain/mod.rs")),
        ("domain/order.rs", include_str!("../domain/order.rs")),
        ("main.rs", include_str!("../main.rs")),
    ];

    fn strip_comment_lines(src: &str) -> String {
        src.lines()
            .filter(|l| {
                let t = l.trim();
                !t.starts_with("//") && !t.starts_with("/*") && !t.starts_with("*")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn no_floating_point_in_any_source() {
        // 令牌在运行时拼接, 避免检查器源码自身含连续字面量造成自命中
        let t64 = format!("f{}", 64);
        let t32 = format!("f{}", 32);
        for (path, src) in SOURCES {
            let code = strip_comment_lines(src);
            let hits: Vec<&str> = code
                .lines()
                .filter(|l| l.contains(&t64) || l.contains(&t32))
                .collect();
            assert!(
                hits.is_empty(),
                "floating point token in {path} (policy: amounts are Decimal/integer micro-units only): {hits:?}"
            );
        }
    }
}
