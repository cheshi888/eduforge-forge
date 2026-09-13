use http::header::HeaderMap;
use serde_json::Value;

use super::{constant_time_eq, hmac_sha256_hex, GatewayTransaction, GwError};

const MAX_AGE_SECS: i64 = 180;

// PayPal Webhook 验签: x-payload-hash = HMAC-SHA256("{x-timestamp}.{raw_body}", webhook_secret),
// x-timestamp 需在 180s 窗口内防重放。仅 PAYMENT.CAPTURE.COMPLETED 入账, 其余事件(退款/失败)拒收。
pub fn verify(
    headers: &HeaderMap,
    body: &[u8],
    secret: &str,
) -> Result<GatewayTransaction, GwError> {
    if secret.is_empty() {
        return Err(GwError::SignatureInvalid(
            "paypal webhook secret not configured".into(),
        ));
    }
    let ts = headers
        .get("x-timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or(GwError::MissingField("x-timestamp".into()))?
        .parse::<i64>()
        .map_err(|_| GwError::SignatureInvalid("paypal x-timestamp".into()))?;
    let hash = headers
        .get("x-payload-hash")
        .and_then(|v| v.to_str().ok())
        .ok_or(GwError::MissingField("x-payload-hash".into()))?;

    let now = chrono::Utc::now().timestamp();
    if (now - ts).abs() > MAX_AGE_SECS {
        return Err(GwError::SignatureInvalid("paypal timestamp drift".into()));
    }
    let msg = format!("{}.{}", ts, String::from_utf8_lossy(body));
    let expected = hmac_sha256_hex(secret, msg.as_bytes());
    if !constant_time_eq(hash.to_ascii_lowercase().as_str(), &expected) {
        return Err(GwError::SignatureInvalid("paypal payload hash".into()));
    }

    let json: Value =
        serde_json::from_slice(body).map_err(|e| GwError::PayloadParse(e.to_string()))?;
    // C-05 修复: 事件类型白名单 — 仅捕获成功事件入账, 退款/失败/授权事件一律拒收
    let event_type = json["event_type"]
        .as_str()
        .ok_or(GwError::MissingField("event_type".into()))?;
    if event_type != "PAYMENT.CAPTURE.COMPLETED" {
        return Err(GwError::EventRejected(event_type.to_string()));
    }
    let res = &json["resource"];
    Ok(GatewayTransaction {
        // M2-01: 捕获流水号缺失即拒收 — 空串会退化 uk_txn 去重键
        gateway_txn_id: res["id"]
            .as_str()
            .ok_or(GwError::MissingField("resource.id".into()))?
            .to_string(),
        out_trade_no: res["invoice_id"]
            .as_str()
            .ok_or(GwError::MissingField("invoice_id".into()))?
            .to_string(),
        amount: res["amount"]["value"]
            .as_str()
            .ok_or(GwError::MissingField("amount.value".into()))?
            .to_string(),
        currency: res["amount"]["currency_code"]
            .as_str()
            .unwrap_or("USD")
            .to_string(),
        confirmations: None,
        crypto_network: None,
        raw: json,
    })
}
