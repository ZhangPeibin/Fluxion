//! Four.meme read-only capability contracts.

use alloy_primitives::Address;
use serde::Serialize;

use crate::capability::{CapabilityDescriptor, SCHEMA_VERSION};
use crate::wallet::AssetSource;

pub const AGENT_TOKENS_CAPABILITY: &str = "fourmeme.agent-tokens";

#[derive(Debug, Clone, Serialize)]
pub struct FourMemeToken {
    pub address: Address,
    pub name: String,
    pub symbol: String,
    pub template_id: String,
    /// `bonding`, `graduated`, or `unknown`.
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raised: Option<String>,
    pub agent_id_exclusive: bool,
    /// `true | false | null` for the queried wallet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet_eligible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eligibility_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FourMemeAgentTokensResponse {
    pub chain: String,
    pub mode: String,
    pub scanned: usize,
    pub lookback_blocks: u64,
    pub from_block: u64,
    pub to_block: u64,
    pub tokens: Vec<FourMemeToken>,
    pub coverage: String,
    pub sources: Vec<AssetSource>,
    pub warnings: Vec<String>,
}

pub fn describe() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: AGENT_TOKENS_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}
