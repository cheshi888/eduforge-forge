use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Paid,
    PendingBalance,
    Shipped,
    FraudSuspended,
    Refunded,
    Cancelled,
}

impl OrderStatus {
    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Paid => "PAID",
            Self::PendingBalance => "PENDING_BALANCE",
            Self::Shipped => "SHIPPED",
            Self::FraudSuspended => "FRAUD_SUSPENDED",
            Self::Refunded => "REFUNDED",
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn from_db_value(v: &str) -> Result<Self, OrderError> {
        Ok(match v {
            "PENDING" => Self::Pending,
            "PAID" => Self::Paid,
            "PENDING_BALANCE" => Self::PendingBalance,
            "SHIPPED" => Self::Shipped,
            "FRAUD_SUSPENDED" => Self::FraudSuspended,
            "REFUNDED" => Self::Refunded,
            "CANCELLED" => Self::Cancelled,
            other => {
                return Err(OrderError::UnknownStatus(other.to_string()));
            }
        })
    }

    // 状态机: 唯一的合法迁移表, 任何绕过此函数的写库操作都是架构违规
    pub fn can_transition_to(&self, next: Self) -> bool {
        use OrderStatus::*;
        matches!(
            (self, next),
            (Pending, Paid | FraudSuspended | Cancelled)
                | (PendingBalance, Paid | FraudSuspended)
                | (Paid, Shipped | Refunded | FraudSuspended)
                | (Shipped, Refunded)
                | (FraudSuspended, Refunded | Cancelled)
        )
    }

    pub fn transition_to(&self, next: Self) -> Result<Self, OrderError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(OrderError::IllegalTransition {
                from: self.as_db_value(),
                to: next.as_db_value(),
            })
        }
    }
}

#[derive(Debug, Error)]
pub enum OrderError {
    #[error("illegal order state transition: {from} -> {to}")]
    IllegalTransition {
        from: &'static str,
        to: &'static str,
    },
    #[error("unknown order status: {0}")]
    UnknownStatus(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blockchain {
    Btc,
    Trc20,
    Erc20,
}

impl Blockchain {
    pub fn required_confirmations(&self) -> i32 {
        match self {
            Self::Btc => 6,
            Self::Trc20 | Self::Erc20 => 1,
        }
    }

    // 与 gateway/coinbase.rs 的取值约定对齐 (大小写不敏感)
    pub fn from_db_value(v: &str) -> Option<Self> {
        match v.to_ascii_uppercase().as_str() {
            "BTC" | "BITCOIN" => Some(Self::Btc),
            "TRC20" | "TRON" => Some(Self::Trc20),
            "ERC20" | "ETHEREUM" => Some(Self::Erc20),
            _ => None,
        }
    }
}
