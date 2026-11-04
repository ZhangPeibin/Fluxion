//! Thin client for a warm local Fluxion runtime.
//!
//! The public provider surface is the CLI/MCP process; this client talks to the
//! internal HTTP endpoint so repeated quotes reuse the runtime's warm state.

use std::time::Instant;

use alloy_primitives::{Address, B256, U256};
use serde::Deserialize;
use serde_json::json;

use crate::capability::{
    Amount, CapabilityError, CapabilityErrorCode, CapabilityMeta, Provenance, SCHEMA_VERSION,
    TokenRef,
};
use crate::split::PlanKind;
use crate::swap::quote::{AllocationResponse, HopResponse, QuoteRequest, QuoteResponse};
use crate::token;

pub struct RuntimeClient {
    base_url: String,
    http: reqwest::Client,
    api_key: Option<String>,
}

impl RuntimeClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("HTTP client"),
            api_key: None,
        }
    }

    pub fn with_api_key(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        let mut client = Self::new(base_url);
        client.api_key = api_key.filter(|key| !key.trim().is_empty());
        client
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn ready(&self) -> bool {
        self.http
            .get(format!("{}/ready", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    pub async fn state(&self) -> Option<serde_json::Value> {
        let response = self
            .http
            .get(format!("{}/state", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .ok()?;
        response.json().await.ok()
    }

    pub async fn swap_quote(
        &self,
        request: &QuoteRequest,
        token_in: TokenRef,
        token_out: TokenRef,
        chain: &str,
        provider: &str,
    ) -> Result<QuoteResponse, CapabilityError> {
        let body = json!({
            "token_in": format!("{:#x}", request.token_in),
            "token_out": format!("{:#x}", request.token_out),
            "amount_in": request.amount_in.to_string(),
            "max_hops": request.max_hops,
            "amount_out_min": request.amount_out_min.map(|amount| amount.to_string()),
            "split": request.split,
            "split_max_branches": request.split_max_branches,
        });
        let started = Instant::now();
        let mut http_request = self
            .http
            .post(format!("{}/quote", self.base_url))
            .json(&body);
        if let Some(key) = &self.api_key {
            http_request = http_request.header("authorization", format!("Bearer {key}"));
        }
        let response = http_request.send().await.map_err(|error| {
            CapabilityError::new(
                CapabilityErrorCode::RpcError,
                format!("local runtime unavailable: {error}"),
                true,
            )
        })?;
        let status = response.status();
        let text = response.text().await.map_err(|error| {
            CapabilityError::new(
                CapabilityErrorCode::RpcError,
                format!("invalid runtime response: {error}"),
                true,
            )
        })?;
        if !status.is_success() {
            return Err(http_error(status.as_u16(), &text));
        }
        let dto: HttpQuoteResponse = serde_json::from_str(&text)
            .map_err(|error| CapabilityError::internal(format!("invalid runtime JSON: {error}")))?;
        let latency_ms = started.elapsed().as_millis() as u64;
        build_response(
            dto, request, token_in, token_out, chain, provider, latency_ms,
        )
    }
}

#[derive(Debug, Deserialize)]
struct HttpQuoteResponse {
    amount_out: String,
    price_impact_bps: Option<i64>,
    #[serde(default)]
    path: Vec<HttpHop>,
    plan_kind: Option<PlanKind>,
    allocations: Option<Vec<HttpAllocation>>,
    quote_block: Option<u64>,
    state: Option<HttpState>,
}

#[derive(Debug, Deserialize)]
struct HttpAllocation {
    amount_in: String,
    amount_out: String,
    path: Vec<HttpHop>,
}

#[derive(Debug, Deserialize)]
struct HttpHop {
    operation: String,
    platform: String,
    pool: String,
    i: usize,
    j: usize,
    token_in: String,
    token_out: String,
    amount_in: String,
    amount_out: String,
}

#[derive(Debug, Deserialize)]
struct HttpState {
    pricing_mode: String,
    block_hash: B256,
    state_age_secs: u64,
    lag_blocks: u64,
}

fn parse_u256(field: &str, value: &str) -> Result<U256, CapabilityError> {
    value
        .parse()
        .map_err(|_| CapabilityError::internal(format!("invalid {field} from runtime: {value}")))
}

fn decimals_for(address: &str, fallback: u8) -> u8 {
    address
        .parse::<Address>()
        .ok()
        .and_then(token::decimals_for)
        .unwrap_or(fallback)
}

fn map_hop(hop: HttpHop, token_in_decimals: u8, token_out_decimals: u8) -> HopResponse {
    let in_decimals = decimals_for(&hop.token_in, token_in_decimals);
    let out_decimals = decimals_for(&hop.token_out, token_out_decimals);
    HopResponse {
        operation: hop.operation,
        platform: hop.platform,
        pool: hop.pool,
        i: hop.i,
        j: hop.j,
        token_in: hop.token_in,
        token_out: hop.token_out,
        amount_in: Amount::from_raw(
            parse_u256("hop amount_in", &hop.amount_in).unwrap_or_default(),
            in_decimals,
        ),
        amount_out: Amount::from_raw(
            parse_u256("hop amount_out", &hop.amount_out).unwrap_or_default(),
            out_decimals,
        ),
    }
}

fn build_response(
    dto: HttpQuoteResponse,
    request: &QuoteRequest,
    token_in: TokenRef,
    token_out: TokenRef,
    chain: &str,
    provider: &str,
    latency_ms: u64,
) -> Result<QuoteResponse, CapabilityError> {
    let amount_out = parse_u256("amount_out", &dto.amount_out)?;
    let token_in_decimals = token_in.decimals;
    let token_out_decimals = token_out.decimals;
    let mut allocations = Vec::new();
    if let Some(entries) = dto.allocations {
        for entry in entries {
            allocations.push(AllocationResponse {
                amount_in: Amount::from_raw(
                    parse_u256("allocation amount_in", &entry.amount_in)?,
                    token_in_decimals,
                ),
                amount_out: Amount::from_raw(
                    parse_u256("allocation amount_out", &entry.amount_out)?,
                    token_out_decimals,
                ),
                hops: entry
                    .path
                    .into_iter()
                    .map(|hop| map_hop(hop, token_in_decimals, token_out_decimals))
                    .collect(),
            });
        }
    } else {
        allocations.push(AllocationResponse {
            amount_in: Amount::from_raw(request.amount_in, token_in_decimals),
            amount_out: Amount::from_raw(amount_out, token_out_decimals),
            hops: dto
                .path
                .into_iter()
                .map(|hop| map_hop(hop, token_in_decimals, token_out_decimals))
                .collect(),
        });
    }
    let provenance = match dto.state {
        Some(state) => Provenance {
            provider: provider.to_string(),
            pricing_mode: state.pricing_mode,
            quote_block: dto.quote_block,
            block_hash: Some(state.block_hash),
            state_age_secs: Some(state.state_age_secs),
            lag_blocks: Some(state.lag_blocks),
        },
        None => Provenance {
            provider: provider.to_string(),
            pricing_mode: "rpc".to_string(),
            quote_block: dto.quote_block,
            block_hash: None,
            state_age_secs: None,
            lag_blocks: None,
        },
    };
    Ok(QuoteResponse {
        meta: CapabilityMeta {
            schema_version: SCHEMA_VERSION,
            capability: "swap.quote".to_string(),
            request_id: request_id(),
        },
        chain: chain.to_string(),
        token_in,
        token_out,
        amount_in: Amount::from_raw(request.amount_in, token_in_decimals),
        amount_out: Amount::from_raw(amount_out, token_out_decimals),
        plan_kind: dto.plan_kind.unwrap_or(PlanKind::Baseline),
        price_impact_bps: dto.price_impact_bps,
        allocations,
        provenance,
        latency_ms,
    })
}

fn http_error(status: u16, body: &str) -> CapabilityError {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| {
                    error
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| error.get("message")?.as_str().map(str::to_string))
                })
                .or_else(|| value.get("message")?.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| body.chars().take(200).collect());
    let (code, retryable) = match status {
        400 | 422 => (CapabilityErrorCode::InvalidArgument, false),
        408 | 504 => (CapabilityErrorCode::Timeout, true),
        503 => (CapabilityErrorCode::StaleState, true),
        _ => (CapabilityErrorCode::RpcError, true),
    };
    CapabilityError::new(code, message, retryable)
}

pub fn request_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("fluxion-{}-{nanos}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_dto() -> HttpQuoteResponse {
        HttpQuoteResponse {
            amount_out: "500".to_string(),
            price_impact_bps: Some(3),
            path: vec![],
            plan_kind: Some(PlanKind::Baseline),
            allocations: None,
            quote_block: Some(100),
            state: Some(HttpState {
                pricing_mode: "local_snapshot".to_string(),
                block_hash: B256::ZERO,
                state_age_secs: 1,
                lag_blocks: 0,
            }),
        }
    }

    #[test]
    fn maps_runtime_response_to_capability_shape() {
        let request = QuoteRequest {
            token_in: Address::with_last_byte(1),
            token_out: Address::with_last_byte(2),
            amount_in: U256::from(10),
            max_hops: None,
            amount_out_min: None,
            split: true,
            split_max_branches: None,
        };
        let response = build_response(
            response_dto(),
            &request,
            TokenRef::new(Some("LDO".into()), Address::with_last_byte(1), 18),
            TokenRef::new(Some("WETH".into()), Address::with_last_byte(2), 18),
            "ethereum",
            "local-cli",
            12,
        )
        .unwrap();
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["capability"], "swap.quote");
        assert_eq!(value["provenance"]["pricing_mode"], "local_snapshot");
        assert_eq!(value["provenance"]["quote_block"], 100);
        assert_eq!(value["amount_in"]["decimal"], "0.00000000000000001");
    }

    #[test]
    fn maps_http_status_to_stable_codes() {
        assert_eq!(
            http_error(400, r#"{"error":"bad"}"#).code,
            CapabilityErrorCode::InvalidArgument
        );
        assert_eq!(
            http_error(503, r#"{"error":"stale"}"#).code,
            CapabilityErrorCode::StaleState
        );
        assert!(http_error(500, "boom").retryable);
    }
}
