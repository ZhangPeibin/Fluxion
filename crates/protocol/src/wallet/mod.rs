//! `wallet` capability family: read-only account balances and asset discovery.

use alloy_primitives::{Address, B256};
use serde::Serialize;

use crate::capability::{Amount, CapabilityDescriptor, SCHEMA_VERSION, TokenRef};

pub const BALANCE_CAPABILITY: &str = "wallet.balance";
pub const ASSETS_CAPABILITY: &str = "wallet.assets";
pub const ALLOWANCE_CAPABILITY: &str = "wallet.allowance";
pub const COLLECTIBLES_CAPABILITY: &str = "wallet.collectibles";

#[derive(Debug, Clone, Serialize)]
pub struct WalletTokenBalance {
    pub token: TokenRef,
    pub amount: Amount,
    pub known: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetSource {
    pub name: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletAllowance {
    pub token: TokenRef,
    pub spender: Address,
    pub amount: Amount,
    pub known: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletAllowancesResponse {
    pub owner: Address,
    pub spender: Address,
    pub block_number: u64,
    pub block_hash: B256,
    pub allowances: Vec<WalletAllowance>,
    pub coverage: String,
    pub sources: Vec<AssetSource>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectibleItem {
    pub token_id: String,
    pub balance: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletCollectible {
    pub collection: Address,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// `erc721` or `erc1155` when known.
    pub standard: String,
    pub enumerable: bool,
    pub item_count: usize,
    pub items: Vec<CollectibleItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletCollectiblesResponse {
    pub owner: Address,
    pub block_number: u64,
    pub block_hash: B256,
    pub collections: Vec<WalletCollectible>,
    pub coverage: String,
    pub sources: Vec<AssetSource>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletBalancesResponse {
    pub address: Address,
    pub block_number: u64,
    pub block_hash: B256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native: Option<WalletTokenBalance>,
    pub tokens: Vec<WalletTokenBalance>,
    /// Unknown nonzero tokens hidden unless `include_unknown` was requested.
    pub unknown_count: usize,
    pub coverage: String,
    pub sources: Vec<AssetSource>,
    pub warnings: Vec<String>,
}

pub fn describe_balance() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: BALANCE_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}

pub fn describe_assets() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: ASSETS_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "hybrid",
        transports: vec!["cli", "mcp-stdio"],
    }
}

pub fn describe_allowance() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: ALLOWANCE_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}

pub fn describe_collectibles() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: COLLECTIBLES_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "hybrid",
        transports: vec!["cli", "mcp-stdio"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_wallet_capabilities() {
        let balance = describe_balance();
        assert_eq!(balance.name, "wallet.balance");
        assert_eq!(balance.execution, "local");
        let assets = describe_assets();
        assert_eq!(assets.name, "wallet.assets");
        assert_eq!(assets.execution, "hybrid");
        let allowance = describe_allowance();
        assert_eq!(allowance.name, "wallet.allowance");
        assert_eq!(allowance.execution, "local");
        let collectibles = describe_collectibles();
        assert_eq!(collectibles.name, "wallet.collectibles");
        assert_eq!(collectibles.execution, "hybrid");
    }
}
