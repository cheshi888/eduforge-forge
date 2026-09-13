use std::env;

#[derive(Clone)]
pub struct Config {
    pub bind_addr: String,
    pub database_url: String,
    pub nats_url: String,
    pub hmac_secret: String,
    pub alipay_public_key_pem: String,
    pub stripe_webhook_secret: String,
    pub coinbase_webhook_secret: String,
    // 以下为可选网关密钥: 留空时对应 /webhooks/<gateway> 路由仍开放, 但验签必然失败 (安全默认)
    pub wechat_platform_cert_pem: String,
    pub wechat_api_v3_key: String,
    pub paypal_webhook_secret: String,
    // 卡收单槽位: card_acquirer 为 braintree|adyen 开关; 两组密钥各自可选
    pub braintree_merchant_id: String,
    pub braintree_webhook_secret: String,
    pub card_acquirer: String,
    pub adyen_hmac_key: String,
    pub adyen_merchant_account: String,
    // 管理后台鉴权 token: 缺省空串 = 所有 /admin/* 一律 401 fail-closed
    pub admin_token: String,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            bind_addr: env::var("FORGE_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            database_url: env::var("FORGE_DATABASE_URL")?,
            nats_url: env::var("FORGE_NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into()),
            hmac_secret: env::var("FORGE_HMAC_SECRET")?,
            alipay_public_key_pem: env::var("FORGE_ALIPAY_PUBLIC_KEY_PEM")?,
            stripe_webhook_secret: env::var("FORGE_STRIPE_WEBHOOK_SECRET")?,
            coinbase_webhook_secret: env::var("FORGE_COINBASE_WEBHOOK_SECRET")?,
            wechat_platform_cert_pem: env::var("FORGE_WECHAT_PLATFORM_CERT_PEM")
                .unwrap_or_default(),
            wechat_api_v3_key: env::var("FORGE_WECHAT_API_V3_KEY").unwrap_or_default(),
            paypal_webhook_secret: env::var("FORGE_PAYPAL_WEBHOOK_SECRET").unwrap_or_default(),
            braintree_merchant_id: env::var("FORGE_BRAINTREE_MERCHANT_ID").unwrap_or_default(),
            braintree_webhook_secret: env::var("FORGE_BRAINTREE_WEBHOOK_SECRET")
                .unwrap_or_default(),
            card_acquirer: env::var("FORGE_CARD_ACQUIRER").unwrap_or_else(|_| "braintree".into()),
            adyen_hmac_key: env::var("FORGE_ADYEN_HMAC_KEY").unwrap_or_default(),
            adyen_merchant_account: env::var("FORGE_ADYEN_MERCHANT_ACCOUNT").unwrap_or_default(),
            admin_token: env::var("FORGE_ADMIN_TOKEN").unwrap_or_default(),
        })
    }
}
