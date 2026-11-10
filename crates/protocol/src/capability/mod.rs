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
    #[serde(rename = "wallet.allowance")]
    WalletAllowance,
    #[serde(rename = "wallet.collectibles")]
    WalletCollectibles,
    #[serde(rename = "wallet.connect")]
    WalletConnect,
    #[serde(rename = "wallet.status")]
    WalletStatus,
    #[serde(rename = "wallet.disconnect")]
    WalletDisconnect,
    #[serde(rename = "wallet.agent-id")]
    WalletAgentId,
    #[serde(rename = "transaction.simulate")]
    TransactionSimulate,
}

impl CapabilityId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SwapQuote => "swap.quote",
            Self::WalletBalance => "wallet.balance",
            Self::WalletAssets => "wallet.assets",
            Self::WalletAllowance => "wallet.allowance",
            Self::WalletCollectibles => "wallet.collectibles",
            Self::WalletConnect => "wallet.connect",
            Self::WalletStatus => "wallet.status",
            Self::WalletDisconnect => "wallet.disconnect",
            Self::WalletAgentId => "wallet.agent-id",
            Self::TransactionSimulate => "transaction.simulate",
        }
    }

    pub const fn all() -> &'static [Self] {
        &[
            Self::SwapQuote,
            Self::WalletBalance,
            Self::WalletAssets,
            Self::WalletAllowance,
            Self::WalletCollectibles,
            Self::WalletConnect,
            Self::WalletStatus,
            Self::WalletDisconnect,
            Self::WalletAgentId,
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
                "wallet.allowance",
                "wallet.collectibles",
                "wallet.connect",
                "wallet.status",
                "wallet.disconnect",
                "wallet.agent-id",
                "transaction.simulate"
            ]
        );
        assert_eq!(CapabilityId::SwapQuote.as_str(), "swap.quote");
        assert_eq!(CapabilityId::WalletBalance.as_str(), "wallet.balance");
        assert_eq!(CapabilityId::WalletAssets.as_str(), "wallet.assets");
        assert_eq!(CapabilityId::WalletAllowance.as_str(), "wallet.allowance");
        assert_eq!(
            CapabilityId::WalletCollectibles.as_str(),
            "wallet.collectibles"
        );
        assert_eq!(CapabilityId::WalletConnect.as_str(), "wallet.connect");
        assert_eq!(CapabilityId::WalletStatus.as_str(), "wallet.status");
        assert_eq!(CapabilityId::WalletDisconnect.as_str(), "wallet.disconnect");
        assert_eq!(CapabilityId::WalletAgentId.as_str(), "wallet.agent-id");
        assert_eq!(
            CapabilityId::TransactionSimulate.as_str(),
            "transaction.simulate"
        );
    }
}
