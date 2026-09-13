use http::header::HeaderMap;
use serde_json::Value;

use super::{constant_time_eq, hmac_sha256_hex, GatewayTransaction, GwError};
use crate::domain::order::Blockchain;

// Coinbase Commerce webhook: x-cc-webhook-signature = HMAC-SHA256(body, secret)
pub fn verify(
    headers: &HeaderMap,
    body: &[u8],
    secret: &str,
) -> Result<GatewayTransaction, GwError> {
    let sig = headers
        .get("x-cc-webhook-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(GwError::SignatureInvalid(
            "missing coinbase signature".into(),
        ))?;
    let expected = hmac_sha256_hex(secret, body);
    if !constant_time_eq(sig, &expected) {
        return Err(GwError::SignatureInvalid("coinbase hmac".into()));
    }

    let json: Value =
        serde_json::from_slice(body).map_err(|e| GwError::PayloadParse(e.to_string()))?;
    // C-02: 事件类型白名单 — 仅 completed 状态入账, 失败/取消/新单/确认中一律拒收
    let payment = &json["data"];
    let status = payment["status"].as_str().unwrap_or("");
    if status != "completed" {
        return Err(GwError::EventRejected(format!("coinbase status={status}")));
    }
    let network = payment["network"].as_str().unwrap_or("");
    // C2-03: 未知链网络拒收 — 旧版未知值映射 None 会旁路 webhook 确认数校验,
    // 攻击者可声明任意 network + confirmations=0 直接 0 确认入账
    let crypto_network = match network {
        "TRC20" | "TRON" => Some(Blockchain::Trc20),
        "ERC20" | "ETHEREUM" => Some(Blockchain::Erc20),
        "BTC" | "BITCOIN" => Some(Blockchain::Btc),
        other => {
            return Err(GwError::EventRejected(format!(
                "coinbase network={other} unsupported"
            )))
        }
    };
    Ok(GatewayTransaction {
        // M2-01: 网关流水号缺失即拒收 — 空串会退化 uk_txn(gateway_txn_id,gateway_type) 去重键
        gateway_txn_id: payment["id"]
            .as_str()
            .ok_or(GwError::MissingField("data.id".into()))?
            .to_string(),
        out_trade_no: payment["metadata"]["out_trade_no"]
            .as_str()
            .ok_or(GwError::MissingField("out_trade_no".into()))?
            .to_string(),
        amount: payment["payment_amount"]["value"]
            .as_str()
            .ok_or(GwError::MissingField("payment_amount.value".into()))?
            .to_string(),
        currency: payment["payment_amount"]["currency"]
            .as_str()
            .unwrap_or("USDC")
            .to_string(),
        confirmations: payment["network_confirmations"].as_i64().map(|v| v as i32),
        crypto_network,
        raw: json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::header::{HeaderMap, HeaderValue};

    fn signed(body: &str, secret: &str) -> (HeaderMap, Vec<u8>) {
        let mut h = HeaderMap::new();
        h.insert(
            "x-cc-webhook-signature",
            HeaderValue::from_str(&hmac_sha256_hex(secret, body.as_bytes())).unwrap(),
        );
        (h, body.as_bytes().to_vec())
    }

    const BODY: &str = r#"{"data":{"id":"cb-1","status":"completed","network":"TRC20","network_confirmations":1,"payment_amount":{"value":"100.00","currency":"USDC"},"metadata":{"out_trade_no":"FT-01"}}}"#;

    // C2-03 回归: 未知链网络拒收, 不再映射 None 旁路确认数校验
    #[test]
    fn unknown_network_rejected() {
        let body = r#"{"data":{"id":"cb-1","status":"completed","network":"SOLANA","network_confirmations":0,"payment_amount":{"value":"100.00","currency":"USDC"},"metadata":{"out_trade_no":"FT-01"}}}"#;
        let (h, b) = signed(body, "s");
        let err = verify(&h, &b, "s").unwrap_err();
        assert!(matches!(err, GwError::EventRejected(_)));
    }

    // M2-01 回归: 流水号缺失拒收
    #[test]
    fn missing_txn_id_rejected() {
        let body = r#"{"data":{"status":"completed","network":"TRC20","payment_amount":{"value":"100.00","currency":"USDC"},"metadata":{"out_trade_no":"FT-01"}}}"#;
        let (h, b) = signed(body, "s");
        let err = verify(&h, &b, "s").unwrap_err();
        assert!(matches!(err, GwError::MissingField(_)));
    }

    // 正常路径回归: TRC-20 + 确认数声明
    #[test]
    fn known_network_accepted() {
        let (h, b) = signed(BODY, "s");
        let txn = verify(&h, &b, "s").unwrap();
        assert_eq!(txn.gateway_txn_id, "cb-1");
        assert_eq!(txn.confirmations, Some(1));
        assert_eq!(txn.crypto_network, Some(Blockchain::Trc20));
    }
}
