use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::Aes256Gcm;
use http::header::HeaderMap;
use rsa::pkcs8::DecodePublicKey;
use rust_decimal::Decimal;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{base64_decode, GatewayTransaction, GwError};
use crate::config::Config;

const MAX_AGE_SECS: i64 = 300;

// 微信支付 APIv3 平台证书验签:
// 签名消息 = "{timestamp}\n{nonce}\n{body}\n", 平台证书 (PKCS#8) RSA-SHA256, 300s 时间戳窗口。
pub fn verify(
    cfg: &Config,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<GatewayTransaction, GwError> {
    let ts = header_str(headers, "Wechatpay-Timestamp")?;
    let nonce = header_str(headers, "Wechatpay-Nonce")?;
    let sig_b64 = header_str(headers, "Wechatpay-Signature")?;

    let ts_val: i64 = ts
        .parse()
        .map_err(|_| GwError::SignatureInvalid("wechat ts".into()))?;
    let now = chrono::Utc::now().timestamp();
    if (now - ts_val).abs() > MAX_AGE_SECS {
        return Err(GwError::SignatureInvalid("wechat timestamp drift".into()));
    }

    let pk = match rsa::RsaPublicKey::from_public_key_pem(&cfg.wechat_platform_cert_pem) {
        Ok(pk) => pk,
        Err(e) => return Err(GwError::SignatureInvalid(format!("wechat cert: {e}"))),
    };
    let sig = base64_decode(sig_b64)
        .ok_or_else(|| GwError::SignatureInvalid("wechat sig encoding".into()))?;
    // APIv3 签名消息格式: "{timestamp}\n{nonce}\n{body}\n"
    let msg = format!("{ts}\n{nonce}\n{}\n", String::from_utf8_lossy(body));
    // verify 需要预哈希的 SHA-256 digest; sig 为原始签名字节
    let hashed = Sha256::digest(msg.as_bytes());
    pk.verify(rsa::pkcs1v15::Pkcs1v15Sign::new::<Sha256>(), &hashed, &sig)
        .map_err(|_| GwError::SignatureInvalid("wechat platform cert".into()))?;

    let json: Value =
        serde_json::from_slice(body).map_err(|e| GwError::PayloadParse(e.to_string()))?;
    // L-01: 事件类型白名单 — 仅支付成功通知 (transaction.success) 可入账;
    // 退款 (refund.*) 及其他类型一律拒收, 与 stripe/adyen/coinbase/paypal 语义对齐
    match json.get("event_type").and_then(Value::as_str) {
        Some("transaction.success") => {}
        Some(other) => {
            return Err(GwError::EventRejected(format!(
                "wechat event_type {other} not whitelisted"
            )))
        }
        None => return Err(GwError::EventRejected("wechat event_type missing".into())),
    }
    let resource = &json["resource"];
    // 平台通知 resource 可能被 APIv3 key AES-256-GCM 加密
    let object = match &resource["object"] {
        Value::Object(_) => resource["object"].clone(),
        _ => {
            let cipher_text = base64_decode(
                resource["ciphertext"]
                    .as_str()
                    .ok_or(GwError::MissingField("ciphertext".into()))?,
            )
            .ok_or_else(|| GwError::Decrypt("ciphertext base64".into()))?;
            let aead = Aes256Gcm::new_from_slice(cfg.wechat_api_v3_key.as_bytes())
                .map_err(|_| GwError::Decrypt("wechat api v3 key must be 32 bytes".into()))?;
            let nonce_str = resource["nonce"]
                .as_str()
                .ok_or(GwError::MissingField("resource.nonce".into()))?;
            if nonce_str.len() != 12 {
                return Err(GwError::Decrypt("resource.nonce must be 12 bytes".into()));
            }
            let mut gcm_nonce = [0u8; 12];
            gcm_nonce.copy_from_slice(nonce_str.as_bytes());
            let aad = resource["associated_data"]
                .as_str()
                .unwrap_or("")
                .as_bytes();
            // GCM 将 AAD 前置到密文缓冲区
            let mut buf = aad.to_vec();
            buf.extend_from_slice(&cipher_text);
            let nonce = generic_array::GenericArray::from_slice(&gcm_nonce);
            let plain = aead
                .decrypt(nonce, &buf[..])
                .map_err(|_| GwError::Decrypt("resource aes-gcm".into()))?;
            serde_json::from_slice(&plain).map_err(|e| GwError::Decrypt(e.to_string()))?
        }
    };

    // amount.total 单位为分 (int), 精确 Decimal 两位小数
    let total_fen = object["amount"]["total"]
        .as_i64()
        .ok_or(GwError::AmountParse("wechat total".into()))?;
    Ok(GatewayTransaction {
        // M2-01: 流水号缺失即拒收 — 空串会退化 uk_txn 去重键
        gateway_txn_id: object["transaction_id"]
            .as_str()
            .ok_or(GwError::MissingField("transaction_id".into()))?
            .to_string(),
        out_trade_no: object["out_trade_no"]
            .as_str()
            .ok_or(GwError::MissingField("out_trade_no".into()))?
            .to_string(),
        amount: Decimal::new(total_fen, 2).to_string(),
        currency: object["amount"]["currency"]
            .as_str()
            .unwrap_or("CNY")
            .to_string(),
        confirmations: None,
        crypto_network: None,
        raw: json,
    })
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, GwError> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or(GwError::MissingField(name.to_string()))
}
