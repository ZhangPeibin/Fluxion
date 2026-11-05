//! Shared, capability-independent value types.

use alloy_primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub address: Address,
    pub decimals: u8,
}

impl TokenRef {
    pub fn new(symbol: Option<String>, address: Address, decimals: u8) -> Self {
        Self {
            symbol,
            address,
            decimals,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Amount {
    pub raw: String,
    pub decimal: String,
}

impl Amount {
    pub fn from_raw(raw: U256, decimals: u8) -> Self {
        Self {
            raw: raw.to_string(),
            decimal: format_units(raw, decimals),
        }
    }
}

fn format_units(value: U256, decimals: u8) -> String {
    if decimals == 0 {
        return value.to_string();
    }
    let text = value.to_string();
    let decimals = usize::from(decimals);
    let (integer, fraction) = if text.len() <= decimals {
        (
            "0".to_string(),
            format!("{}{}", "0".repeat(decimals - text.len()), text),
        )
    } else {
        let (integer, fraction) = text.split_at(text.len() - decimals);
        (integer.to_string(), fraction.to_string())
    };
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        integer
    } else {
        format!("{integer}.{fraction}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub execution: String,
    pub pricing_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_block: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_hash: Option<B256>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_age_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lag_blocks: Option<u64>,
}

impl Provenance {
    pub fn rpc() -> Self {
        Self {
            execution: "remote".to_string(),
            pricing_mode: "rpc".to_string(),
            quote_block: None,
            block_hash: None,
            state_age_secs: None,
            lag_blocks: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityMeta {
    pub schema_version: u32,
    pub capability: String,
    pub request_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_raw_units_without_loss() {
        assert_eq!(
            Amount::from_raw(U256::from(100_000_000_000_000_000_000u128), 18).decimal,
            "100"
        );
        assert_eq!(
            Amount::from_raw(U256::from(52_130_000_000_000_000u128), 18).decimal,
            "0.05213"
        );
        assert_eq!(Amount::from_raw(U256::from(1_500_000u64), 6).decimal, "1.5");
        assert_eq!(Amount::from_raw(U256::ZERO, 18).decimal, "0");
    }
}
