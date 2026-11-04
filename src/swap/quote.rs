//! `swap.quote` request/response contract and conversion from the pricing engine.

use alloy_primitives::{Address, U256};
use serde::Serialize;

use crate::capability::{
    Amount, CapabilityError, CapabilityMeta, Provenance, SCHEMA_VERSION, TokenRef,
};
use crate::model::QuoteRequest as EngineRequest;
use crate::split::{PlanKind, SplitResult};

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

    pub fn to_engine(&self) -> EngineRequest {
        EngineRequest {
            token_in: self.token_in,
            token_out: self.token_out,
            amount_in: self.amount_in,
            max_hops: self.max_hops,
            amount_out_min: self.amount_out_min,
        }
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

#[derive(Debug, Clone)]
pub struct QuoteContext {
    pub chain: String,
    pub token_in: TokenRef,
    pub token_out: TokenRef,
    pub amount_in: U256,
    pub request_id: String,
    pub latency_ms: u64,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize)]
pub struct HopResponse {
    pub operation: &'static str,
    pub platform: &'static str,
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

impl QuoteResponse {
    pub fn from_split_result(result: SplitResult, context: QuoteContext) -> Self {
        let token_in_decimals = context.token_in.decimals;
        let token_out_decimals = context.token_out.decimals;
        let allocations = result
            .allocations
            .iter()
            .map(|allocation| AllocationResponse {
                amount_in: Amount::from_raw(allocation.amount_in, token_in_decimals),
                amount_out: Amount::from_raw(allocation.amount_out, token_out_decimals),
                hops: allocation
                    .hops
                    .iter()
                    .map(|hop| HopResponse {
                        operation: hop.pool.kind.operation(hop.i),
                        platform: hop.pool.kind.name(),
                        pool: hop.pool.address.to_string(),
                        i: hop.i,
                        j: hop.j,
                        token_in: hop.token_in.to_string(),
                        token_out: hop.token_out.to_string(),
                        amount_in: Amount::from_raw(hop.amount_in, token_in_decimals),
                        amount_out: Amount::from_raw(hop.amount_out, token_out_decimals),
                    })
                    .collect(),
            })
            .collect();
        let price_impact_bps =
            (result.kind == PlanKind::Baseline).then_some(result.baseline.price_impact_bps);
        Self {
            meta: CapabilityMeta {
                schema_version: SCHEMA_VERSION,
                capability: super::CAPABILITY.to_string(),
                request_id: context.request_id,
            },
            chain: context.chain,
            token_in: context.token_in,
            token_out: context.token_out,
            amount_in: Amount::from_raw(context.amount_in, token_in_decimals),
            amount_out: Amount::from_raw(result.amount_out, token_out_decimals),
            plan_kind: result.kind,
            price_impact_bps,
            allocations,
            provenance: context.provenance,
            latency_ms: context.latency_ms,
        }
    }
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
                capability: super::CAPABILITY.to_string(),
                request_id: request_id.into(),
            },
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Hop, Route};
    use crate::split::{Allocation, SearchStats};

    fn token_in() -> TokenRef {
        TokenRef::new(Some("LDO".to_string()), Address::with_last_byte(1), 18)
    }

    fn token_out() -> TokenRef {
        TokenRef::new(Some("WETH".to_string()), Address::with_last_byte(2), 18)
    }

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
        assert_eq!(request.to_engine().amount_in, request.amount_in);
        assert!(QuoteRequest::from_strings("nope", "0x0", "1").is_err());
    }

    #[test]
    fn serializes_stable_swap_quote_shape() {
        let result = SplitResult {
            baseline: Route {
                hops: Vec::<Hop>::new(),
                amount_out: U256::from(5),
                price_impact_bps: 7,
            },
            kind: PlanKind::Baseline,
            amount_out: U256::from(5),
            allocations: vec![Allocation {
                amount_in: U256::from(10),
                amount_out: U256::from(5),
                hops: Vec::new(),
            }],
            stats: SearchStats::default(),
        };
        let response = QuoteResponse::from_split_result(
            result,
            QuoteContext {
                chain: "ethereum".to_string(),
                token_in: token_in(),
                token_out: token_out(),
                amount_in: U256::from(10),
                request_id: "req-1".to_string(),
                latency_ms: 12,
                provenance: Provenance::rpc(),
            },
        );
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
