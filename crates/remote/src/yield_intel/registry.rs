//! Curated yield universe.
//!
//! Safety tiers and withdrawal context live here, not in provider data. The
//! default registry is compiled into the binary so a deployment cannot
//! accidentally run without a reviewed universe.

use std::collections::HashSet;
use std::path::Path;

use alloy_primitives::Address;
use fluxion_protocol::yield_capability::{SafetyTier, YieldAction};
use serde::Deserialize;

pub const DEFAULT_REGISTRY: &str = include_str!("registry.yaml");

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryEntry {
    pub id: String,
    pub protocol_slug: String,
    pub protocol_name: String,
    pub category: String,
    pub action: YieldAction,
    pub safety_tier: SafetyTier,
    pub contract: Address,
    pub market_name: String,
    pub deposit_asset_symbol: String,
    pub deposit_asset_address: String,
    pub deposit_decimals: u8,
    pub receipt_asset_symbol: String,
    pub receipt_asset_address: Address,
    pub receipt_decimals: u8,
    #[serde(default)]
    pub requires_wrap: bool,
    pub defillama_project: String,
    pub defillama_symbol: String,
    #[serde(default)]
    pub defillama_pool_meta: Option<String>,
    #[serde(default)]
    pub defillama_pool: Option<String>,
    pub withdrawal: RegistryWithdrawal,
    #[serde(default)]
    pub risk_notes: Vec<RegistryRiskNote>,
    pub reviewed_at: String,
    pub reviewer: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryWithdrawal {
    pub mode: String,
    #[serde(default)]
    pub lockup_days: Option<u32>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryRiskNote {
    pub code: String,
    pub level: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub reviewed_at: String,
    pub reviewer: String,
    pub entries: Vec<RegistryEntry>,
}

impl Registry {
    pub fn load_default() -> Result<Self, String> {
        Self::from_yaml(DEFAULT_REGISTRY)
    }

    pub fn from_yaml(text: &str) -> Result<Self, String> {
        let registry: Self = serde_yaml::from_str(text)
            .map_err(|error| format!("invalid yield registry: {error}"))?;
        registry.validate()?;
        Ok(registry)
    }

    pub fn from_path(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read yield registry {}: {error}", path.display()))?;
        Self::from_yaml(&text)
    }

    pub fn entries(&self) -> &[RegistryEntry] {
        &self.entries
    }

    fn validate(&self) -> Result<(), String> {
        if self.version == 0 {
            return Err("yield registry version must be positive".to_string());
        }
        if self.entries.is_empty() {
            return Err("yield registry has no entries".to_string());
        }
        let mut ids = HashSet::new();
        let mut pools = HashSet::new();
        for entry in &self.entries {
            if entry.id.trim().is_empty() {
                return Err("yield registry entry has an empty id".to_string());
            }
            if !ids.insert(entry.id.as_str()) {
                return Err(format!("duplicate yield registry id: {}", entry.id));
            }
            if entry.contract == Address::ZERO || entry.receipt_asset_address == Address::ZERO {
                return Err(format!(
                    "yield registry entry {} has a zero address",
                    entry.id
                ));
            }
            if entry.deposit_decimals > 38 || entry.receipt_decimals > 38 {
                return Err(format!(
                    "yield registry entry {} has invalid decimals",
                    entry.id
                ));
            }
            if entry.defillama_project.trim().is_empty() || entry.defillama_symbol.trim().is_empty()
            {
                return Err(format!(
                    "yield registry entry {} needs a DefiLlama project and symbol",
                    entry.id
                ));
            }
            if let Some(pool) = &entry.defillama_pool
                && !pools.insert(pool.as_str())
            {
                return Err(format!("duplicate DefiLlama pool id: {pool}"));
            }
            if entry.withdrawal.mode.trim().is_empty() {
                return Err(format!(
                    "yield registry entry {} needs a withdrawal mode",
                    entry.id
                ));
            }
            for note in &entry.risk_notes {
                if !matches!(note.level.as_str(), "low" | "medium" | "high") {
                    return Err(format!(
                        "yield registry entry {} has invalid risk level: {}",
                        entry.id, note.level
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_loads() {
        let registry = Registry::load_default().expect("default registry");
        assert_eq!(registry.version, 1);
        assert!(registry.entries().len() >= 10);
        assert!(
            registry
                .entries()
                .iter()
                .any(|entry| entry.id == "lido-steth")
        );
    }

    #[test]
    fn rejects_duplicate_ids() {
        let text = DEFAULT_REGISTRY.replacen("id: rocket-pool-reth", "id: lido-steth", 1);
        let error = Registry::from_yaml(&text).unwrap_err();
        assert!(error.contains("duplicate"));
    }
}
