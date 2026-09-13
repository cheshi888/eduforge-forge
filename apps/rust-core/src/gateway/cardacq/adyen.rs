use base64::Engine;
use hmac::{Hmac, Mac};
use http::header::HeaderMap;
use serde_json::Value;
use sha2::Sha256;

use crate::config::Config;

use super::super::{constant_time_eq, GatewayTransaction, GwError};

const MAX_AGE_SECS: i64 = 300;

// Adyen 收单商实现: 官方 HMAC-SHA256 通知验签。
// key = base64(FORGE_ADYEN_HMAC_KEY), 明文参与 HMAC 计算。
// 规范串 (官方固定字段序):
//   pspReference:originalReference:merchantAccountCode:value:currency:eventCode:success
// value 为最小货币单位 (分), success 为 "true"/"false";
// 签名 (hex, 大小写不敏感) 位于 additionalData.hmacSignature。
// 防重放与兄弟槽位一致: 入口层附加 transaction-time 头 + 300s 窗口。
pub fn verify(
    cfg: &Config,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<GatewayTransaction, GwError> {
    let key_b64 = cfg.adyen_hmac_key.as_str();
    if key_b64.is_empty() {
        return Err(GwError::SignatureInvalid(
            "adyen hmac key not configured".into(),
        ));
    }
    let key = base64::engine::general_purpose::STANDARD
        .decode(key_b64)
        .map_err(|e| GwError::SignatureInvalid(format!("adyen hmac key not base64: {e}")))?;

    let tx_time = headers
        .get("transaction-time")
        .and_then(|v| v.to_str().ok())
        .ok_or(GwError::MissingField("transaction-time".into()))?
        .to_string();
    let parsed_at = chrono::DateTime::parse_from_rfc3339(&tx_time)
        .map_err(|_| GwError::SignatureInvalid("adyen transaction-time".into()))?;
    let now = chrono::Utc::now();
    let elapsed = (now - parsed_at.with_timezone(&chrono::Utc))
        .num_seconds()
        .abs();
    if elapsed > MAX_AGE_SECS {
        return Err(GwError::SignatureInvalid("adyen timestamp drift".into()));
    }

    let json: Value =
        serde_json::from_slice(body).map_err(|e| GwError::PayloadParse(e.to_string()))?;

    let psp_ref = json["pspReference"]
        .as_str()
        .ok_or(GwError::MissingField("pspReference".into()))?
        .to_string();
    let original_ref = json["originalReference"].as_str().unwrap_or("").to_string();
    let merchant_acct = json["merchantAccountCode"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let value_cents = json["amount"]["value"]
        .as_i64()
        .ok_or(GwError::MissingField("amount.value(cents)".into()))?;
    let currency = json["amount"]["currency"]
        .as_str()
        .unwrap_or("USD")
        .to_string();
    let event_code = json["eventCode"].as_str().unwrap_or("").to_string();
    let success = json["success"].as_bool().unwrap_or(false).to_string();

    // 可选商户号核对: 配置留空即跳过, 与 Braintree 槽位语义一致
    if !cfg.adyen_merchant_account.is_empty()
        && !merchant_acct.eq_ignore_ascii_case(&cfg.adyen_merchant_account)
    {
        return Err(GwError::SignatureInvalid("adyen merchant mismatch".into()));
    }

    let provided = json["additionalData"]["hmacSignature"]
        .as_str()
        .ok_or(GwError::MissingField("additionalData.hmacSignature".into()))?;

    let canonical = format!(
        "{psp_ref}:{original_ref}:{merchant_acct}:{value_cents}:{currency}:{event_code}:{success}"
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(&key)
        .map_err(|e| GwError::SignatureInvalid(e.to_string()))?;
    mac.update(canonical.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());
    if !constant_time_eq(&provided.to_ascii_lowercase(), &expected) {
        return Err(GwError::SignatureInvalid("adyen hmac signature".into()));
    }

    // C-02: 事件类型白名单 — 仅授权/请款成功事件入账, 退款/取消/失败一律拒收
    if !matches!(event_code.as_str(), "AUTHORISATION" | "CAPTURE") {
        return Err(GwError::EventRejected(format!(
            "adyen eventCode={event_code}"
        )));
    }
    if !json["success"].as_bool().unwrap_or(false) {
        return Err(GwError::EventRejected(format!(
            "adyen {event_code} success=false"
        )));
    }

    // value 是最小货币单位 (分), 纯整数运算换算为两位小数字符串, 全程不经过 f64
    let abs = value_cents.unsigned_abs();
    let amount = format!(
        "{}{}.{:02}",
        if value_cents < 0 { "-" } else { "" },
        abs / 100,
        abs % 100
    );

    Ok(GatewayTransaction {
        gateway_txn_id: psp_ref,
        out_trade_no: json["reference"]
            .as_str()
            .ok_or(GwError::MissingField("adyen reference".into()))?
            .to_string(),
        amount,
        currency,
        confirmations: None,
        crypto_network: None,
        raw: json,
    })
}
