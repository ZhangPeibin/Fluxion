//! Read-only position analysis contract for curated yield markets.

use alloy_primitives::{Address, B256};
use serde::{Deserialize, Serialize};

use crate::capability::{Amount, CapabilityDescriptor, SCHEMA_VERSION};
use crate::wallet::AssetSource;
use crate::yield_capability::{
    SafetyTier, YieldAction, YieldAsset, YieldInfo, YieldProtocol, YieldRiskFactor, YieldWithdrawal,
};

pub const CAPABILITY: &str = "position.analyze";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionCostBasis {
    pub opportunity_id: String,
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionAnalyzeRequest {
    pub owner: Address,
    #[serde(default = "default_chain")]
    pub chain: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opportunity_ids: Vec<String>,
    #[serde(default)]
    pub include_zero: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cost_basis: Vec<PositionCostBasis>,
}

fn default_chain() -> String {
    "ethereum".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedAmount {
    /// Signed raw units as a decimal integer string.
    pub raw: String,
    pub decimal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionEntry {
    pub opportunity_id: String,
    pub safety_tier: SafetyTier,
    pub protocol: YieldProtocol,
    pub action: YieldAction,
    pub receipt_asset: YieldAsset,
    pub receipt_balance: Amount,
    pub underlying_asset: YieldAsset,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_eth: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_basis_eth: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_eth: Option<SignedAmount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_percent: Option<f64>,
    pub apy: YieldInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_annual_yield_eth: Option<Amount>,
    pub withdrawal: YieldWithdrawal,
    pub risk_factors: Vec<YieldRiskFactor>,
    /// `high`, `medium`, or `low`.
    pub data_confidence: String,
    /// `active` or `empty`.
    pub status: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionTotals {
    pub position_count: usize,
    pub active_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_eth: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_annual_yield_eth: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_basis_eth: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_eth: Option<SignedAmount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionAnalyzeResponse {
    pub schema_version: u32,
    pub capability: String,
    pub request_id: String,
    pub chain: String,
    pub owner: Address,
    pub block_number: u64,
    pub block_hash: B256,
    pub as_of: u64,
    pub yield_data_as_of: u64,
    pub positions: Vec<PositionEntry>,
    pub totals: PositionTotals,
    pub coverage: String,
    pub sources: Vec<AssetSource>,
    pub disclaimer: String,
    pub warnings: Vec<String>,
}

pub fn describe() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "remote",
        transports: vec!["cli", "mcp-stdio", "http"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_ethereum() {
        let request: PositionAnalyzeRequest =
            serde_json::from_str(r#"{"owner":"0x0000000000000000000000000000000000000001"}"#)
                .expect("request");
        assert_eq!(request.chain, "ethereum");
        assert!(!request.include_zero);
        assert!(request.opportunity_ids.is_empty());
    }

    #[test]
    fn describes_remote_execution() {
        assert_eq!(describe().name, "position.analyze");
        assert_eq!(describe().execution, "remote");
    }
}
