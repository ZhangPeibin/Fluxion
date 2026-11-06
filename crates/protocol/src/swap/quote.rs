//! `swap.quote` request/response contract.

use alloy_primitives::{Address, U256};
use serde::Serialize;

use crate::capability::{
    Amount, CapabilityError, CapabilityMeta, Provenance, SCHEMA_VERSION, TokenRef,
};
use crate::swap::PlanKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteRequest {
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub max_hops: Option<usize>,
    pub amount_out_min: Option<U256>,
    pub split: bool,
    pub split_max_branches: Option<usize>,
}

impl QuoteRequest {
    pub fn from_strings(
        token_in: &str,
        token_out: &str,
        amount_in: &str,
    ) -> Result<Self, CapabilityError> {
        Ok(Self {
            token_in: parse_address("token_in", token_in)?,
            token_out: parse_address("token_out", token_out)?,
            amount_in: parse_u256("amount_in", amount_in)?,
            max_hops: None,
            amount_out_min: None,
            split: false,
            split_max_branches: None,
        })
    }
}

fn parse_address(field: &str, value: &str) -> Result<Address, CapabilityError> {
    value
        .parse()
        .map_err(|_| CapabilityError::invalid_argument(format!("invalid {field}: {value}")))
}

fn parse_u256(field: &str, value: &str) -> Result<U256, CapabilityError> {
    value
        .parse()
        .map_err(|_| CapabilityError::invalid_argument(format!("invalid {field}: {value}")))
}

#[derive(Debug, Clone, Serialize)]
pub struct HopResponse {
    pub operation: String,
    pub platform: String,
    pub pool: String,
    pub i: usize,
    pub j: usize,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: Amount,
    pub amount_out: Amount,
}

#[derive(Debug, Clone, Serialize)]
pub struct AllocationResponse {
    pub amount_in: Amount,
    pub amount_out: Amount,
    pub hops: Vec<HopResponse>,
    /// Suffix branches when a shared prefix fans out into multiple pools.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<AllocationResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuoteResponse {
    #[serde(flatten)]
    pub meta: CapabilityMeta,
    pub chain: String,
    pub token_in: TokenRef,
    pub token_out: TokenRef,
    pub amount_in: Amount,
    pub amount_out: Amount,
    pub plan_kind: PlanKind,
    pub price_impact_bps: Option<i64>,
    pub allocations: Vec<AllocationResponse>,
    pub provenance: Provenance,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuoteErrorResponse {
    #[serde(flatten)]
    pub meta: CapabilityMeta,
    pub error: CapabilityError,
}

impl QuoteErrorResponse {
    pub fn new(request_id: impl Into<String>, error: CapabilityError) -> Self {
        Self {
            meta: CapabilityMeta {
                schema_version: SCHEMA_VERSION,
                capability: crate::swap::CAPABILITY.to_string(),
                request_id: request_id.into(),
            },
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_addresses_and_amounts() {
        let request = QuoteRequest::from_strings(
            "0x0000000000000000000000000000000000000001",
            "0x0000000000000000000000000000000000000002",
            "100000000000000000000",
        )
        .unwrap();
        assert_eq!(
            request.amount_in,
            U256::from(100_000_000_000_000_000_000u128)
        );
        assert!(QuoteRequest::from_strings("nope", "0x0", "1").is_err());
    }

    #[test]
    fn serializes_stable_swap_quote_shape() {
        let response = QuoteResponse {
            meta: CapabilityMeta {
                schema_version: SCHEMA_VERSION,
                capability: crate::swap::CAPABILITY.to_string(),
                request_id: "req-1".to_string(),
            },
            chain: "ethereum".to_string(),
            token_in: TokenRef::new(Some("LDO".into()), Address::with_last_byte(1), 18),
            token_out: TokenRef::new(Some("WETH".into()), Address::with_last_byte(2), 18),
            amount_in: Amount::from_raw(U256::from(10), 18),
            amount_out: Amount::from_raw(U256::from(5), 18),
            plan_kind: PlanKind::Baseline,
            price_impact_bps: Some(7),
            allocations: Vec::new(),
            provenance: Provenance::rpc(),
            latency_ms: 12,
        };
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["schema_version"], SCHEMA_VERSION);
        assert_eq!(value["capability"], "swap.quote");
        assert_eq!(value["request_id"], "req-1");
        assert_eq!(value["amount_in"]["raw"], "10");
        assert_eq!(value["amount_out"]["raw"], "5");
        assert_eq!(value["price_impact_bps"], 7);
    }

    #[test]
    fn serializes_structured_errors() {
        let value = serde_json::to_value(QuoteErrorResponse::new(
            "req-2",
            CapabilityError::invalid_argument("bad amount"),
        ))
        .unwrap();
        assert_eq!(value["error"]["code"], "INVALID_ARGUMENT");
        assert_eq!(value["error"]["retryable"], false);
    }
}
