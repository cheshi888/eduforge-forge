use std::collections::BTreeMap;

use rsa::pkcs1::DecodeRsaPublicKey;
use sha2::{Digest, Sha256};
use url::form_urlencoded;

use super::{base64_decode, GatewayTransaction, GwError};

// 支付宝异步通知验签: 字段字典序排序拼接 + SHA256WithRSA (PKCS#1 v1.5, 即 RSA2)
pub fn verify(body: &str, pubkey_pem: &str) -> Result<GatewayTransaction, GwError> {
    let pairs: BTreeMap<String, String> = form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    let sign = pairs
        .get("sign")
        .cloned()
        .ok_or_else(|| GwError::MissingField("sign".into()))?;
    let sign_type = pairs.get("sign_type").map(|s| s.as_str()).unwrap_or("RSA2");
    if sign_type != "RSA2" {
        return Err(GwError::SignatureInvalid(format!("sign_type={sign_type}")));
    }

    let mut msg_fields: Vec<(String, String)> = pairs
        .iter()
        .filter(|(k, _)| k.as_str() != "sign" && k.as_str() != "sign_type")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    msg_fields.sort_by(|a, b| a.0.cmp(&b.0));
    let msg = msg_fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    let pk = match rsa::RsaPublicKey::from_pkcs1_pem(pubkey_pem) {
        Ok(pk) => pk,
        // 支付宝官方下发的公钥 PEM 为 PKCS#8 ("BEGIN PUBLIC KEY"), 两者都接受
        Err(_) => {
            use rsa::pkcs8::DecodePublicKey;
            rsa::RsaPublicKey::from_public_key_pem(pubkey_pem)
                .map_err(|_| GwError::SignatureInvalid("alipay pem (pkcs1/pkcs8)".into()))?
        }
    };
    // 支付宝 sign 规范为 hex 编码; 先按 hex 解码, 失败再尝试 base64 兜底
    let sig_bytes = match hex::decode(&sign) {
        Ok(bytes) => bytes,
        Err(_) => {
            base64_decode(&sign).ok_or_else(|| GwError::SignatureInvalid("sign encoding".into()))?
        }
    };
    // 支付宝 RSA2 = SHA256WithRSA (PKCS#1 v1.5 填充)
    // verify 需要预哈希的 SHA-256 digest; sig 为原始签名字节
    let hashed = Sha256::digest(msg.as_bytes());
    pk.verify(
        rsa::pkcs1v15::Pkcs1v15Sign::new::<Sha256>(),
        &hashed,
        &sig_bytes,
    )
        .map_err(|_| GwError::SignatureInvalid("alipay rsa2".into()))?;

    // C2-02: notify_type 白名单 — 仅支付成功同步通知 (trade_status_sync) 可入账;
    // 退款 (trade_refund) / 改价等同 URL 通知一律拒收 (与 wechat/stripe 事件白名单语义对齐)
    match pairs.get("notify_type").map(|s| s.as_str()) {
        Some("trade_status_sync") => {}
        Some(other) => {
            return Err(GwError::EventRejected(format!(
                "alipay notify_type {other} not whitelisted"
            )))
        }
        None => {
            return Err(GwError::EventRejected(
                "alipay notify_type missing".into(),
            ))
        }
    }

    Ok(GatewayTransaction {
        gateway_txn_id: pairs
            .get("trade_no")
            .cloned()
            .ok_or_else(|| GwError::MissingField("trade_no".into()))?,
        out_trade_no: pairs
            .get("out_trade_no")
            .cloned()
            .ok_or_else(|| GwError::MissingField("out_trade_no".into()))?,
        amount: pairs
            .get("total_amount")
            .cloned()
            .ok_or_else(|| GwError::MissingField("total_amount".into()))?,
        currency: pairs
            .get("currency")
            .cloned()
            .unwrap_or_else(|| "CNY".into()),
        confirmations: None,
        crypto_network: None,
        raw: serde_json::json!({ "form": true, "keys": pairs.keys().cloned().collect::<Vec<_>>() }),
    })
}

#[cfg(test)]
mod tests {
    use super::verify;
    use base64::Engine;
    use rsa::pkcs1::EncodeRsaPublicKey;
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::pkcs8::EncodePublicKey;
    use rsa::{Pkcs1v15Sign, RsaPrivateKey};
    use sha2::{Digest, Sha256};

    // 测试专用密钥对 (非生产密钥, 仅用于回归测试)
    const PRIV_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDfmlCzZD4PAPs4\nIVit9IjqqmroQacFws4gKG0Wv0nt3q1QPipfvVmYtyELDPYFsAOECMOIf3LedATI\nJd5sUdQBFzp03LAzlxiPufenVM6eIBgkokt9mqvpRy5szZqLG5gdx+G+0Oavuc1f\nxpbyvT6UZ8wbESeUSLk/W1SHkLYzqwu7k+Js5UKt23K3sEMpZ5XzUYWcKv1bImBo\n/+bIb2FCqfvi2f5qnVK68zu6bmuxARlqVchGHW2RQPHMaxhsFJlvFL+T/bLqd3Us\niSsAActcFIHzIA3ZfKGbGMqTIpBvnVn/2bb5ULH5yMLDdPlSd/Rgvu0a6eefewjl\n37dhaNwpAgMBAAECggEAEBdD88ePcGH4Mh+y3iso8oYv/6XAgLl2PuSEhoIJLm0m\nA2PDMIo+Q9n4g+87Vx5wVP89HvoDnf7dseTeaY9xQQuBtpODueVc1vRMM5uhQMom\nIqDBQPkxTNjFU6lvaST5CzpCmbThHPKIf89ui7+oR2K5fUB8nNRSQb0Z9frTzx+U\nqxJw89G4OgnUGuwbC6rRUZ9EHa5g08S7u4MoyDJf7c4BO9M6+egyA/DjLRp/kCRy\neQAxKaF50QnYwY8k6z3pf+St9G6c9TVJW/VM/grKummIUJsKpVZxqajn1K4BnHAy\nUm1LSrnCSr9qbhetNexclr5nR0uK62L/AEObR8Xr/wKBgQD6hxroMFLjuH6+30bX\nkozbKzkBygxNcbMyvIRDz79KjRdUIOZ32ZB+ZPMFONQGt517rVeykeJvN+PqCxIR\nODXrFAlE/61z6QIdNiXA1eTyLWE4VsO32obq90GH1ocB53qOxmwbNnSnpEMs0llW\noYybBNeYOUGGdDAmlLA7LrZkdwKBgQDkfKbcrOEvW5bLPG6Pht3u1zoC/vBCDnW/\nFImEQ9mZHEt/18iLq68UpudISK37IH0/steN4jCxJrEh4PaJKI1kAoCQHSvUTFgn\nl/BpywX/1CdmsrbdFOlo3gSKeVgBgwRKq5f7NHDHiAvnhGmqp+6NDmiJwkQCvc/D\nC8WZDhwMXwKBgQDIIhnqloABDqehzzgGijof851swn2leEkRJOv8R/+mjKOwP0Ad\nSh5V+krxRv6KBzC1VjqSmlvrQRi/JOzf+KmKC82D40QrLl7T+yeZrsVgecQKf7WG\nL/pLxg4Bd+0OJl8Po49FBfGW4nAPZFKeFBwA0pNwNX0U7chxaaKHBMAL7QKBgAKI\n23UBOfJDitWwUu2mqlmW657S4ylobafz+16LPiOzq6a8Zw4dlnCdNV2Y6M0rXBhT\nzI5XXwDoE/jzPOjimUbtvC1dFXpiamvuBI234dYnrVQ3UU38iGZQaC59EBvcLzUA\n2EvCFioweSlVT7LD8CKCcWhtzkFVh7N7MvLytB7zAoGAao1RmkGWkG1EHsiAL5wC\ne/89LSdj67tEJiPpFTdBfKJ0aiSTu3Ve4jf0Yk22L64kQhkmZXZBzYvMecdReOyM\n6lYEPRiGwP66v8d59p4FYnfKc5XJunByC+UHLGGKly0sCxo4l3NFBesrikYq7c5j\nm7h3o1F3UqFkt2U1G+h8dqM=\n-----END PRIVATE KEY-----\n";

    fn pem_block(der_bytes: &[u8], label: &str) -> String {
        let b64 = base64::engine::general_purpose::STANDARD.encode(der_bytes);
        let mut body = String::new();
        for chunk in b64.as_bytes().chunks(64) {
            body.push_str(std::str::from_utf8(chunk).unwrap());
            body.push('\n');
        }
        format!("-----BEGIN {label}-----\n{body}-----END {label}-----\n")
    }

    // 回归测试: 按支付宝官方规范签名 (参数 ASCII 升序, 排除 sign/sign_type) 的回调必须被接受
    #[test]
    fn officially_signed_callback_is_accepted() {
        let priv_k = RsaPrivateKey::from_pkcs8_pem(PRIV_PEM).unwrap();
        // 按支付宝官方规范: 字段 ASCII 升序 (total_amount 在 trade_no 之前)
        let base = "notify_type=trade_status_sync&out_trade_no=FT-TEST-001&total_amount=100.00&trade_no=202609102200140000001";
        let sig = priv_k
            .sign(
                Pkcs1v15Sign::new::<Sha256>(),
                &Sha256::digest(base.as_bytes()),
            )
            .unwrap();
        let body = format!("{base}&sign_type=RSA2&sign={}", hex::encode(&sig));

        let pubk = priv_k.to_public_key();
        // PKCS#1 格式
        let pkcs1_doc = pubk.to_pkcs1_der().unwrap();
        let pkcs1_pem = pem_block(pkcs1_doc.as_bytes(), "RSA PUBLIC KEY");
        let txn = verify(&body, &pkcs1_pem).unwrap();
        assert_eq!(txn.out_trade_no, "FT-TEST-001");
        assert_eq!(txn.amount, "100.00");
        assert_eq!(txn.gateway_txn_id, "202609102200140000001");

        // PKCS#8 格式 (支付宝官方公钥下发格式), 必须同样可验证
        let pkcs8_doc = pubk.to_public_key_der().unwrap();
        let pkcs8_pem = pem_block(pkcs8_doc.as_bytes(), "PUBLIC KEY");
        verify(&body, &pkcs8_pem).unwrap();
    }

    // C2-02 回归: 退款通知即使签名合法也必须拒收 (同 URL 通知场景)
    #[test]
    fn refund_notify_type_rejected() {
        use crate::gateway::GwError;
        let priv_k = RsaPrivateKey::from_pkcs8_pem(PRIV_PEM).unwrap();
        let base = "notify_type=trade_refund&out_trade_no=FT-TEST-001&total_amount=100.00&trade_no=202609110000000000002";
        let sig = priv_k
            .sign(
                Pkcs1v15Sign::new::<Sha256>(),
                &Sha256::digest(base.as_bytes()),
            )
            .unwrap();
        let body = format!("{base}&sign_type=RSA2&sign={}", hex::encode(&sig));
        let pkcs8_pem = pem_block(
            priv_k
                .to_public_key()
                .to_public_key_der()
                .unwrap()
                .as_bytes(),
            "PUBLIC KEY",
        );
        let err = verify(&body, &pkcs8_pem).unwrap_err();
        assert!(matches!(err, GwError::EventRejected(_)));
    }

    // C2-02 回归: 缺失 notify_type 同样拒收 (严格语义)
    #[test]
    fn missing_notify_type_rejected() {
        use crate::gateway::GwError;
        let priv_k = RsaPrivateKey::from_pkcs8_pem(PRIV_PEM).unwrap();
        let base = "out_trade_no=FT-TEST-001&total_amount=100.00&trade_no=202609110000000000003";
        let sig = priv_k
            .sign(
                Pkcs1v15Sign::new::<Sha256>(),
                &Sha256::digest(base.as_bytes()),
            )
            .unwrap();
        let body = format!("{base}&sign_type=RSA2&sign={}", hex::encode(&sig));
        let pkcs8_pem = pem_block(
            priv_k
                .to_public_key()
                .to_public_key_der()
                .unwrap()
                .as_bytes(),
            "PUBLIC KEY",
        );
        let err = verify(&body, &pkcs8_pem).unwrap_err();
        assert!(matches!(err, GwError::EventRejected(_)));
    }

    // 诊断: 两种 PEM 解析出的公钥必须一致
    #[test]
    fn pem_formats_yield_same_key() {
        use rsa::traits::PublicKeyParts;
        let priv_k = RsaPrivateKey::from_pkcs8_pem(PRIV_PEM).unwrap();
        let pubk = priv_k.to_public_key();
        use rsa::pkcs1::DecodeRsaPublicKey;
        let pk1 =
            rsa::RsaPublicKey::from_pkcs1_der(pubk.to_pkcs1_der().unwrap().as_bytes()).unwrap();
        use rsa::pkcs8::DecodePublicKey;
        let pk8 =
            rsa::RsaPublicKey::from_public_key_der(pubk.to_public_key_der().unwrap().as_bytes())
                .unwrap();
        assert_eq!(pk1.n(), pk8.n());
        assert_eq!(pk1.e(), pk8.e());
    }
}
