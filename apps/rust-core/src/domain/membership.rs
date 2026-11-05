use thiserror::Error;

// 会员等级: 与 schema/003_membership.sql membership_tiers.code 对齐
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Free,
    Plus,
    Pro,
}

impl Tier {
    pub fn as_db_value(self) -> &'static str {
        match self {
            Tier::Free => "free",
            Tier::Plus => "plus",
            Tier::Pro => "pro",
        }
    }

    pub fn from_db_value(v: &str) -> Result<Self, MembershipError> {
        Ok(match v {
            "free" => Tier::Free,
            "plus" => Tier::Plus,
            "pro" => Tier::Pro,
            other => {
                return Err(MembershipError::UnknownTier(other.to_string()));
            }
        })
    }

    // 升级只允许向更高等级 (free<plus<pro), 降级/平级走续期不走此校验
    // (升级已改走支付流程, 此方法保留供单测与未来支付后晋升逻辑复用)
    #[allow(dead_code)]
    pub fn can_upgrade_from(self, other: Self) -> bool {
        self < other
    }

    #[allow(dead_code)]
    pub fn transition_to(&self, next: Self) -> Result<Self, MembershipError> {
        if !(*self < next) {
            return Err(MembershipError::IllegalUpgrade {
                from: self.as_db_value(),
                to: next.as_db_value(),
            });
        }
        Ok(next)
    }
}

#[derive(Debug, Error)]
pub enum MembershipError {
    #[error("unknown membership tier: {0}")]
    UnknownTier(String),
    #[error("cannot upgrade from tier {from} to {to}")]
    IllegalUpgrade { from: &'static str, to: &'static str },
}

#[cfg(test)]
mod tests {
    use super::Tier;

    #[test]
    fn upgrade_only_to_higher() {
        // 语义: 当前等级 cur, 能否升到目标 to —— 仅当 cur < to
        assert!(Tier::Free.can_upgrade_from(Tier::Plus));
        assert!(Tier::Free.can_upgrade_from(Tier::Pro));
        assert!(Tier::Plus.can_upgrade_from(Tier::Pro));
        assert!(!Tier::Plus.can_upgrade_from(Tier::Free));
        assert!(!Tier::Free.can_upgrade_from(Tier::Free));
        assert!(!Tier::Pro.can_upgrade_from(Tier::Plus));
    }

    #[test]
    fn transition_rejects_downgrade_and_flat() {
        assert!(Tier::Free.transition_to(Tier::Plus).is_ok());
        assert!(Tier::Free.transition_to(Tier::Pro).is_ok());
        assert!(Tier::Plus.transition_to(Tier::Pro).is_ok());
        assert!(Tier::Plus.transition_to(Tier::Free).is_err());
        assert!(Tier::Pro.transition_to(Tier::Free).is_err());
        assert!(Tier::Free.transition_to(Tier::Free).is_err());
    }

    #[test]
    fn db_value_roundtrip() {
        for t in [Tier::Free, Tier::Plus, Tier::Pro] {
            assert_eq!(Tier::from_db_value(t.as_db_value()).unwrap(), t);
        }
        assert!(Tier::from_db_value("gold").is_err());
    }
}
