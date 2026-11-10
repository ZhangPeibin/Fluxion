//! `wallet` capability family: read-only account balances and asset discovery.

use alloy_primitives::{Address, B256};
use serde::Serialize;

use crate::capability::{Amount, CapabilityDescriptor, SCHEMA_VERSION, TokenRef};

pub const BALANCE_CAPABILITY: &str = "wallet.balance";
pub const ASSETS_CAPABILITY: &str = "wallet.assets";
pub const ALLOWANCE_CAPABILITY: &str = "wallet.allowance";
pub const COLLECTIBLES_CAPABILITY: &str = "wallet.collectibles";
pub const CONNECT_CAPABILITY: &str = "wallet.connect";
pub const STATUS_CAPABILITY: &str = "wallet.status";
pub const DISCONNECT_CAPABILITY: &str = "wallet.disconnect";
pub const AGENT_ID_CAPABILITY: &str = "wallet.agent-id";

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
pub struct WalletPeer {
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub icons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletConnectResponse {
    /// `pending` or `connected`.
    pub status: String,
    pub connection_id: String,
    pub uri: String,
    pub qr_ascii: String,
    pub expires_at: i64,
    pub accounts: Vec<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<u64>,
    #[serde(skip)]
    pub qr_png_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_png_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletStatusResponse {
    /// `disconnected`, `pending`, `connected`, `expired`, or `error`.
    pub status: String,
    pub accounts: Vec<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet: Option<WalletPeer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentIdQuery {
    /// `handle` or `address`.
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletAgentIdResponse {
    pub chain: String,
    pub chain_id: u64,
    pub query: AgentIdQuery,
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of_block: Option<u64>,
    pub coverage: String,
    pub sources: Vec<AssetSource>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletDisconnectResponse {
    pub disconnected: bool,
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

pub fn describe_connect() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: CONNECT_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}

pub fn describe_status() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: STATUS_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}

pub fn describe_disconnect() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: DISCONNECT_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}

pub fn describe_agent_id() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: AGENT_ID_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "hybrid",
        transports: vec!["cli", "mcp-stdio"],
    }
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
        let connect = describe_connect();
        assert_eq!(connect.name, "wallet.connect");
        assert_eq!(connect.execution, "local");
        let status = describe_status();
        assert_eq!(status.name, "wallet.status");
        assert_eq!(status.execution, "local");
        let disconnect = describe_disconnect();
        assert_eq!(disconnect.name, "wallet.disconnect");
        assert_eq!(disconnect.execution, "local");
        let agent_id = describe_agent_id();
        assert_eq!(agent_id.name, "wallet.agent-id");
        assert_eq!(agent_id.execution, "hybrid");
    }
}
