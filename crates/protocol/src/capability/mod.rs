//! Capability-first surface shared by CLI and HTTP adapters.
//!
//! Capability identifiers describe user actions (`swap.quote`), not protocols
//! or transport. Capability-specific code lives in its own top-level module
//! (`crate::swap`), while this module holds the stable contract.

pub mod error;
pub mod types;

pub use error::{CapabilityError, CapabilityErrorCode};
pub use types::{Amount, CapabilityMeta, Provenance, TokenRef};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityId {
    #[serde(rename = "swap.quote")]
    SwapQuote,
    #[serde(rename = "wallet.balance")]
    WalletBalance,
    #[serde(rename = "wallet.assets")]
    WalletAssets,
    #[serde(rename = "transaction.simulate")]
    TransactionSimulate,
}

impl CapabilityId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SwapQuote => "swap.quote",
            Self::WalletBalance => "wallet.balance",
            Self::WalletAssets => "wallet.assets",
            Self::TransactionSimulate => "transaction.simulate",
        }
    }

    pub const fn all() -> &'static [Self] {
        &[
            Self::SwapQuote,
            Self::WalletBalance,
            Self::WalletAssets,
            Self::TransactionSimulate,
        ]
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityDescriptor {
    pub name: &'static str,
    pub schema_version: u32,
    pub execution: &'static str,
    pub transports: Vec<&'static str>,
}

pub fn names() -> impl Iterator<Item = &'static str> {
    CapabilityId::all()
        .iter()
        .copied()
        .map(CapabilityId::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_stable_capability_names() {
        assert_eq!(
            names().collect::<Vec<_>>(),
            vec![
                "swap.quote",
                "wallet.balance",
                "wallet.assets",
                "transaction.simulate"
            ]
        );
        assert_eq!(CapabilityId::SwapQuote.as_str(), "swap.quote");
        assert_eq!(CapabilityId::WalletBalance.as_str(), "wallet.balance");
        assert_eq!(CapabilityId::WalletAssets.as_str(), "wallet.assets");
        assert_eq!(
            CapabilityId::TransactionSimulate.as_str(),
            "transaction.simulate"
        );
    }
}
