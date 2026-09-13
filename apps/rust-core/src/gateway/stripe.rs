use http::header::HeaderMap;
use rust_decimal::Decimal;
use serde_json::Value;

use super::{constant_time_eq, hmac_sha256_hex, GatewayTransaction, GwError};

const MAX_AGE_SECS: i64 = 300;

// Stripe 事件 webhook 验签: v1 = HMAC-SHA256("{t}.{payload}", secret), 带 300s 时间戳防重放窗口
pub fn verify(
    headers: &HeaderMap,
    body: &[u8],
    secret: &str,
) -> Result<GatewayTransaction, GwError> {
    let sig = headers
        .get("Stripe-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(GwError::SignatureInvalid("missing Stripe-Signature".into()))?;

    let mut ts: Option<i64> = None;
    let mut v1: Option<String> = None;
    for part in sig.split(',') {
        if let Some((k, v)) = part.split_once('=') {
            match k {
                "t" => ts = v.parse().ok(),
                "v1" => v1 = Some(v.to_string()),
                _ => {}
            }
        }
    }
    let (ts, v1) = match (ts, v1) {
        (Some(t), Some(v)) => (t, v),
        _ => return Err(GwError::SignatureInvalid("signature format".into())),
    };
    let now = chrono::Utc::now().timestamp();
    if (now - ts).abs() > MAX_AGE_SECS {
        return Err(GwError::SignatureInvalid("timestamp drift".into()));
    }
    let payload = format!("{ts}.{}", String::from_utf8_lossy(body));
    let expected = hmac_sha256_hex(secret, payload.as_bytes());
    if !constant_time_eq(&v1, &expected) {
        return Err(GwError::SignatureInvalid("stripe v1".into()));
    }

    let json: Value =
        serde_json::from_slice(body).map_err(|e| GwError::PayloadParse(e.to_string()))?;
    // C-02: 事件类型白名单 — 仅成功支付事件入账, 退款/失败/取消一律拒收
    let event_type = json["type"].as_str().unwrap_or("");
    if !matches!(event_type, "payment_intent.succeeded" | "charge.succeeded") {
        return Err(GwError::EventRejected(event_type.to_string()));
    }
    let obj = &json["data"]["object"];
    let amount_cents: i64 = obj["amount"]
        .as_i64()
        .ok_or(GwError::AmountParse("cents".into()))?;
    // 分 -> 两位小数精确 Decimal, 不走浮点
    let amount = Decimal::new(amount_cents, 2).to_string();
    Ok(GatewayTransaction {
        // M2-01: 流水号缺失即拒收 — 空串会退化 uk_txn 去重键, 致合法回调被误判重放
        gateway_txn_id: obj["id"]
            .as_str()
            .ok_or(GwError::MissingField("data.object.id".into()))?
            .to_string(),
        out_trade_no: obj["metadata"]["out_trade_no"]
            .as_str()
            .ok_or(GwError::MissingField("out_trade_no".into()))?
            .to_string(),
        amount,
        currency: obj["currency"].as_str().unwrap_or("usd").to_string(),
        confirmations: None,
        crypto_network: None,
        raw: json,
    })
}
