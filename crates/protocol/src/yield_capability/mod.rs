//! Read-only yield discovery contract for Ethereum ETH opportunities.
//!
//! The capability ranks curated staking and lending markets by a safety tier
//! and a normalized APY. It never builds, signs or broadcasts transactions.

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

use crate::capability::{Amount, CapabilityDescriptor, SCHEMA_VERSION};

pub mod registry;

pub use registry::{Registry, RegistryEntry};

pub const CAPABILITY: &str = "yield.discover";
pub const BUILD_CAPABILITY: &str = "yield.build";
pub const HISTORY_CAPABILITY: &str = "yield.history";
pub const WITHDRAW_CAPABILITY: &str = "yield.withdraw";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RiskProfile {
    #[default]
    Conservative,
    Balanced,
    Aggressive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyTier {
    Core,
    Established,
    Experimental,
}

impl SafetyTier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Established => "established",
            Self::Experimental => "experimental",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YieldAction {
    Stake,
    Lend,
}

impl YieldAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stake => "stake",
            Self::Lend => "lend",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct YieldAsset {
    pub symbol: String,
    /// `native` for ETH, otherwise a 0x-prefixed ERC20 address.
    pub address: String,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldDiscoverRequest {
    #[serde(default = "default_chain")]
    pub chain: String,
    pub asset: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
    #[serde(default)]
    pub risk_profile: RiskProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_tvl_usd: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_lockup_days: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_reward_apy: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocols: Vec<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_chain() -> String {
    "ethereum".to_string()
}

fn default_limit() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldSource {
    pub name: String,
    pub url: String,
    pub fetched_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldProtocol {
    pub slug: String,
    pub name: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tvl_usd: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audits: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audit_links: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incident_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldWithdrawal {
    /// `instant`, `queue`, `secondary_market`, or `secondary_market_or_queue`.
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lockup_days: Option<u32>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldMarket {
    pub name: String,
    pub tvl_usd: u64,
    pub contract: Address,
    pub deposit_asset: YieldAsset,
    pub receipt_asset: YieldAsset,
    pub requires_wrap: bool,
    pub withdrawal: YieldWithdrawal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldInfo {
    pub apy_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_apy_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward_apy_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apy_mean_30d_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apy_pct_30d: Option<f64>,
    pub ranking_apy_percent: f64,
    /// `staking_apr` or `supply_apy`.
    pub yield_type: String,
    pub methodology: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldRiskFactor {
    /// Stable upper-case code, e.g. `SLASHING`, `DEPEG`, `SMART_CONTRACT`.
    pub code: String,
    /// `low`, `medium`, or `high`.
    pub level: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldEvidence {
    pub source: String,
    pub fact: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldOpportunity {
    pub id: String,
    pub safety_tier: SafetyTier,
    pub protocol: YieldProtocol,
    pub action: YieldAction,
    pub market: YieldMarket,
    #[serde(rename = "yield")]
    pub yield_info: YieldInfo,
    pub risk_factors: Vec<YieldRiskFactor>,
    pub evidence: Vec<YieldEvidence>,
    /// `high`, `medium`, or `low`.
    pub data_confidence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_annual_yield: Option<Amount>,
    pub sources: Vec<YieldSource>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldRecommendation {
    pub opportunity_id: String,
    pub reason: String,
    pub tradeoffs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldExclusion {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldDiscoverResponse {
    pub schema_version: u32,
    pub capability: String,
    pub request_id: String,
    pub chain: String,
    pub asset: YieldAsset,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<Amount>,
    pub risk_profile: RiskProfile,
    pub objective: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<YieldRecommendation>,
    pub opportunities: Vec<YieldOpportunity>,
    pub excluded: Vec<YieldExclusion>,
    pub as_of: u64,
    pub age_secs: u64,
    pub stale: bool,
    pub coverage: String,
    pub sources: Vec<YieldSource>,
    pub disclaimer: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldBuildRequest {
    pub opportunity_id: String,
    pub amount: String,
    pub recipient: Address,
    /// `ETH` (default) or `WETH`. WETH assumes the sender already holds WETH.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub funding_asset: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldBuildStep {
    /// `wrap`, `approve`, `stake`, `supply`, or `deposit`.
    pub kind: String,
    pub to: Address,
    pub data: String,
    /// Raw native value in wei.
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<Amount>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldApproval {
    pub token: YieldAsset,
    pub spender: Address,
    pub amount: Amount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldBuildResponse {
    pub schema_version: u32,
    pub capability: String,
    pub request_id: String,
    pub chain: String,
    pub opportunity_id: String,
    pub action: YieldAction,
    pub amount: Amount,
    pub recipient: Address,
    pub deposit_asset: YieldAsset,
    pub receipt_asset: YieldAsset,
    pub requires_wrap: bool,
    pub steps: Vec<YieldBuildStep>,
    pub approvals: Vec<YieldApproval>,
    pub warnings: Vec<String>,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldHistoryRequest {
    pub opportunity_id: String,
    #[serde(default = "default_history_days")]
    pub days: u32,
    #[serde(default = "default_history_points")]
    pub max_points: usize,
}

fn default_history_days() -> u32 {
    30
}

fn default_history_points() -> usize {
    500
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldHistoryPoint {
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apy_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_apy_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward_apy_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tvl_usd: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldHistorySummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apy_mean_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apy_min_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apy_max_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apy_latest_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apy_change_bps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_apy_mean_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward_apy_mean_percent: Option<f64>,
    pub points: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldHistoryResponse {
    pub schema_version: u32,
    pub capability: String,
    pub request_id: String,
    pub chain: String,
    pub opportunity_id: String,
    pub days: u32,
    pub series: Vec<YieldHistoryPoint>,
    pub summary: YieldHistorySummary,
    pub as_of: u64,
    pub coverage: String,
    pub sources: Vec<YieldSource>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldWithdrawRequest {
    pub opportunity_id: String,
    /// Human amount in the receipt asset, or `max` where the protocol supports it.
    pub amount: String,
    pub recipient: Address,
    /// Position owner/signer; defaults to `recipient`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldWithdrawResponse {
    pub schema_version: u32,
    pub capability: String,
    pub request_id: String,
    pub chain: String,
    pub opportunity_id: String,
    pub action: YieldAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<Amount>,
    /// `exact` or `max`.
    pub amount_kind: String,
    pub recipient: Address,
    pub owner: Address,
    pub receipt_asset: YieldAsset,
    pub underlying_asset: YieldAsset,
    pub steps: Vec<YieldBuildStep>,
    pub approvals: Vec<YieldApproval>,
    pub warnings: Vec<String>,
    pub disclaimer: String,
}

pub fn describe_build() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: BUILD_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}

pub fn describe_withdraw() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: WITHDRAW_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}

pub fn describe_history() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: HISTORY_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "remote",
        transports: vec!["cli", "mcp-stdio", "http"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_remote_execution() {
        let descriptor = describe();
        assert_eq!(descriptor.name, "yield.discover");
        assert_eq!(descriptor.execution, "remote");
        assert_eq!(descriptor.schema_version, 1);
    }

    #[test]
    fn request_defaults_to_ethereum_conservative() {
        let request: YieldDiscoverRequest =
            serde_json::from_str(r#"{"asset":"ETH"}"#).expect("request");
        assert_eq!(request.chain, "ethereum");
        assert_eq!(request.risk_profile, RiskProfile::Conservative);
        assert_eq!(request.limit, 10);
        assert!(request.amount.is_none());
    }

    #[test]
    fn history_request_defaults_to_30_days() {
        let request: YieldHistoryRequest =
            serde_json::from_str(r#"{"opportunity_id":"lido-steth"}"#).expect("request");
        assert_eq!(request.days, 30);
        assert_eq!(request.max_points, 500);
    }

    #[test]
    fn build_and_history_descriptors_have_fixed_execution() {
        assert_eq!(describe_build().name, "yield.build");
        assert_eq!(describe_build().execution, "local");
        assert_eq!(describe_history().name, "yield.history");
        assert_eq!(describe_history().execution, "remote");
    }

    #[test]
    fn serializes_yield_field_with_keyword_name() {
        let opportunity = YieldOpportunity {
            id: "x".to_string(),
            safety_tier: SafetyTier::Core,
            protocol: YieldProtocol {
                slug: "p".to_string(),
                name: "P".to_string(),
                category: "Lending".to_string(),
                tvl_usd: Some(1),
                audits: Some(2),
                audit_links: vec![],
                listed_at: Some(1),
                incident_count: Some(0),
            },
            action: YieldAction::Lend,
            market: YieldMarket {
                name: "M".to_string(),
                tvl_usd: 1,
                contract: Address::with_last_byte(1),
                deposit_asset: YieldAsset {
                    symbol: "ETH".to_string(),
                    address: "native".to_string(),
                    decimals: 18,
                },
                receipt_asset: YieldAsset {
                    symbol: "aETH".to_string(),
                    address: "0x0000000000000000000000000000000000000001".to_string(),
                    decimals: 18,
                },
                requires_wrap: true,
                withdrawal: YieldWithdrawal {
                    mode: "instant".to_string(),
                    lockup_days: Some(0),
                    notes: String::new(),
                },
            },
            yield_info: YieldInfo {
                apy_percent: 1.0,
                base_apy_percent: Some(1.0),
                reward_apy_percent: None,
                apy_mean_30d_percent: Some(1.0),
                apy_pct_30d: None,
                ranking_apy_percent: 1.0,
                yield_type: "supply_apy".to_string(),
                methodology: "test".to_string(),
            },
            risk_factors: vec![],
            evidence: vec![],
            data_confidence: "high".to_string(),
            estimated_annual_yield: None,
            sources: vec![],
            warnings: vec![],
        };
        let value = serde_json::to_value(&opportunity).expect("json");
        assert_eq!(value["yield"]["apy_percent"], 1.0);
        assert!(value.get("yield_info").is_none());
    }
}
