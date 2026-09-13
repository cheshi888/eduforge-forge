pub mod adyen;
pub mod braintree;

use http::header::HeaderMap;

use crate::config::Config;

use super::{GatewayTransaction, GwError};

// 独立卡收单槽位 (CardAcq): Braintree / Adyen 双实现, 由 FORGE_CARD_ACQUIRER 切换
// (braintree|adyen, 默认 braintree)。更换收单商 = 改环境变量 + 对应密钥,
// 状态机/幂等/金额比对三层零改动。
pub fn verify(
    cfg: &Config,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<GatewayTransaction, GwError> {
    match cfg.card_acquirer.as_str() {
        "adyen" => adyen::verify(cfg, headers, body),
        // 默认与未知取值一律走 Braintree: 保持安全兜底, 空槽位绝不放行
        _ => braintree::verify(
            headers,
            body,
            &cfg.braintree_merchant_id,
            &cfg.braintree_webhook_secret,
        ),
    }
}
