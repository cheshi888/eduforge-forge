use http::header::HeaderMap;
use serde_json::Value;

use super::super::{constant_time_eq, hmac_sha256_hex, sha256_hex, GatewayTransaction, GwError};

const MAX_AGE_SECS: i64 = 300;

// C-04 修复: 官方 Braintree Webhook 验签 —
// BT-Signature = hex(HMAC-SHA256(webhook_secret, hex(SHA256(raw_body))))。
// 旧版无密钥 sha256("{tx_time}.{amount}") 可被攻击者自行计算 (CRITICAL), 已移除。
// 语义: 空密钥绝不验签 (fail-closed, 与 Adyen 槽位一致);
// 保留 300s transaction-time 窗口作叠加防重放层 + 可选商户号核对。
pub fn verify(
    headers: &HeaderMap,
    body: &[u8],
    merchant_id: &str,
    webhook_secret: &str,
) -> Result<GatewayTransaction, GwError> {
    if webhook_secret.is_empty() {
        return Err(GwError::SignatureInvalid(
            "braintree webhook secret not configured".into(),
        ));
    }
    let tx_time = headers
        .get("transaction-time")
        .and_then(|v| v.to_str().ok())
        .ok_or(GwError::MissingField("transaction-time".into()))?
        .to_string();
    let sig = headers
        .get("BT-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(GwError::MissingField("BT-Signature".into()))?;
    let txn_id = headers
        .get("BT-Webhook-Id")
        .and_then(|v| v.to_str().ok())
        .ok_or(GwError::MissingField("BT-Webhook-Id".into()))?;

    let parsed_at = chrono::DateTime::parse_from_rfc3339(&tx_time)
        .map_err(|_| GwError::SignatureInvalid("braintree transaction-time".into()))?;
    let now = chrono::Utc::now();
    let elapsed = (now - parsed_at.with_timezone(&chrono::Utc))
        .num_seconds()
        .abs();
    if elapsed > MAX_AGE_SECS {
        return Err(GwError::SignatureInvalid(
            "braintree timestamp drift".into(),
        ));
    }

    let json: Value =
        serde_json::from_slice(body).map_err(|e| GwError::PayloadParse(e.to_string()))?;
    // C2-01: 事件类型白名单 — 仅扣款成功/结算类事件可入账;
    // transaction.voided / transaction.refunded / dispute.* 一律拒收 (与 adyen 槽位语义对齐)
    match json.get("kind").and_then(Value::as_str) {
        Some("transaction.settled")
        | Some("transaction.settling")
        | Some("transaction.submitted_for_settling") => {}
        Some(other) => {
            return Err(GwError::EventRejected(format!(
                "braintree kind {other} not whitelisted"
            )))
        }
        None => {
            return Err(GwError::EventRejected(
                "braintree kind missing".into(),
            ))
        }
    }
    let obj = &json["braintree_object"];
    let amount = obj["amount"]["transactionamount"]["value"]
        .as_str()
        .ok_or(GwError::MissingField(
            "amount.transactionamount.value".into(),
        ))?
        .to_string();

    // 官方验签: BT-Signature = hex(HMAC-SHA256(webhook_secret, hex(SHA256(raw_body))))
    let digest = sha256_hex(body);
    let expected = hmac_sha256_hex(webhook_secret, digest.as_bytes());
    if !constant_time_eq(&sig.to_ascii_lowercase(), &expected) {
        return Err(GwError::SignatureInvalid("braintree signature".into()));
    }
    if !merchant_id.is_empty() {
        let acct = obj["merchantaccount"].as_str().unwrap_or("");
        if !acct.eq_ignore_ascii_case(merchant_id) {
            return Err(GwError::SignatureInvalid(
                "braintree merchant mismatch".into(),
            ));
        }
    }

    Ok(GatewayTransaction {
        gateway_txn_id: txn_id.to_string(),
        out_trade_no: obj["reference"]
            .as_str()
            .ok_or(GwError::MissingField("braintree reference".into()))?
            .to_string(),
        amount,
        currency: obj["amount"]["transactionamount"]["currencyiso"]
            .as_str()
            .unwrap_or("USD")
            .to_string(),
        confirmations: None,
        crypto_network: None,
        raw: json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::header::HeaderMap;

    fn mk(kind: &str) -> (HeaderMap, Vec<u8>) {
        let body = format!(
            r#"{{"kind":"{kind}","braintree_object":{{"reference":"FT-01","amount":{{"transactionamount":{{"value":"10.00","currencyiso":"USD"}}}},"merchantaccount":"m-1"}}}}"#
        );
        let secret = "bt-secret";
        let digest = sha256_hex(body.as_bytes());
        let sig = hmac_sha256_hex(secret, digest.as_bytes());
        let ts = chrono::Utc::now().to_rfc3339();
        let mut h = HeaderMap::new();
        h.insert("transaction-time", ts.parse().unwrap());
        h.insert("BT-Webhook-Id", "bh-001".parse().unwrap());
        h.insert("BT-Signature", sig.parse().unwrap());
        (h, body.into_bytes())
    }

    // C2-01 回归: voided/refunded 类事件拒收
    #[test]
    fn voided_kind_rejected() {
        let (h, b) = mk("transaction.voided");
        let err = verify(&h, &b, "", "bt-secret").unwrap_err();
        assert!(matches!(err, GwError::EventRejected(_)));
    }

    // C2-01 回归: 缺失 kind 拒收
    #[test]
    fn missing_kind_rejected() {
        let (h, b) = mk("");
        let err = verify(&h, &b, "", "bt-secret").unwrap_err();
        assert!(matches!(err, GwError::EventRejected(_)));
    }

    // 正常路径回归: settled 事件入账
    #[test]
    fn settled_kind_accepted() {
        let (h, b) = mk("transaction.settled");
        let txn = verify(&h, &b, "m-1", "bt-secret").unwrap();
        assert_eq!(txn.out_trade_no, "FT-01");
        assert_eq!(txn.amount, "10.00");
    }
}
