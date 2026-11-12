//! Local `swap.build` calldata construction from an existing quote.
//!
//! The first slice supports a single Uniswap V2 allocation only. Split routes,
//! V3, Curve and aggregators are rejected rather than approximated.

use alloy_primitives::{Address, U256, address};

use crate::capability::{Amount, CapabilityError, CapabilityErrorCode};
use fluxion_protocol::swap::{QuoteResponse, SwapBuildHop, SwapBuildResponse};

const UNISWAP_V2_ROUTER: Address = address!("7a250d5630B4cF539739dF2C5dAcb4c659F2488D");

pub struct BuildRequest {
    pub recipient: Address,
    pub slippage_bps: u16,
    pub deadline: u64,
}

pub fn build(
    quote: &QuoteResponse,
    request: BuildRequest,
) -> Result<SwapBuildResponse, CapabilityError> {
    if quote.chain != "ethereum" {
        return Err(unsupported("swap.build currently supports Ethereum only"));
    }
    if request.slippage_bps > 10_000 {
        return Err(CapabilityError::invalid_argument(
            "slippage_bps must be between 0 and 10000",
        ));
    }
    if quote.allocations.len() != 1 {
        return Err(unsupported(
            "swap.build currently requires exactly one allocation; split routes are not built",
        ));
    }
    let allocation = &quote.allocations[0];
    if !allocation.sources.is_empty() || !allocation.children.is_empty() {
        return Err(unsupported(
            "swap.build does not support nested/split quote allocations yet",
        ));
    }
    if allocation.hops.is_empty() {
        return Err(CapabilityError::new(
            CapabilityErrorCode::StateIncomplete,
            "quote allocation has no hops",
            false,
        ));
    }

    let mut path = Vec::with_capacity(allocation.hops.len() + 1);
    for (index, hop) in allocation.hops.iter().enumerate() {
        if hop.platform != "uniswap-v2" {
            return Err(unsupported(format!(
                "swap.build currently supports uniswap-v2 hops only; found {}",
                hop.platform
            )));
        }
        let token_in = hop.token_in.parse::<Address>().map_err(|_| {
            CapabilityError::new(
                CapabilityErrorCode::StateIncomplete,
                format!("invalid quote hop token_in at index {index}"),
                false,
            )
        })?;
        let token_out = hop.token_out.parse::<Address>().map_err(|_| {
            CapabilityError::new(
                CapabilityErrorCode::StateIncomplete,
                format!("invalid quote hop token_out at index {index}"),
                false,
            )
        })?;
        if index == 0 {
            path.push(token_in);
        } else if path.last() != Some(&token_in) {
            return Err(CapabilityError::new(
                CapabilityErrorCode::StateIncomplete,
                "quote hops are not contiguous",
                false,
            ));
        }
        if path.last() == Some(&token_out) {
            return Err(CapabilityError::new(
                CapabilityErrorCode::StateIncomplete,
                "quote hop repeats the same token",
                false,
            ));
        }
        path.push(token_out);
    }

    let amount_in = parse_raw("amount_in", &quote.amount_in.raw)?;
    let amount_out = parse_raw("amount_out", &quote.amount_out.raw)?;
    let min_out =
        amount_out.saturating_mul(U256::from(10_000 - request.slippage_bps)) / U256::from(10_000);
    let data = encode_swap_exact_tokens(
        amount_in,
        min_out,
        &path,
        request.recipient,
        request.deadline,
    );
    let hops = allocation
        .hops
        .iter()
        .map(|hop| {
            Ok(SwapBuildHop {
                platform: hop.platform.clone(),
                token_in: hop.token_in.parse().map_err(|_| {
                    CapabilityError::new(
                        CapabilityErrorCode::StateIncomplete,
                        "invalid quote hop token_in",
                        false,
                    )
                })?,
                token_out: hop.token_out.parse().map_err(|_| {
                    CapabilityError::new(
                        CapabilityErrorCode::StateIncomplete,
                        "invalid quote hop token_out",
                        false,
                    )
                })?,
                amount_in: hop.amount_in.clone(),
                amount_out: hop.amount_out.clone(),
            })
        })
        .collect::<Result<Vec<_>, CapabilityError>>()?;

    Ok(SwapBuildResponse {
        chain: quote.chain.clone(),
        chain_id: 1,
        quote_block: quote.provenance.quote_block,
        router: UNISWAP_V2_ROUTER,
        value: "0".to_string(),
        data,
        token_in: quote.token_in.address,
        token_out: quote.token_out.address,
        amount_in: quote.amount_in.clone(),
        amount_out_min: Amount::from_raw(min_out, quote.token_out.decimals),
        recipient: request.recipient,
        deadline: request.deadline,
        path,
        hops,
    })
}

fn parse_raw(field: &str, value: &str) -> Result<U256, CapabilityError> {
    value.parse().map_err(|_| {
        CapabilityError::new(
            CapabilityErrorCode::StateIncomplete,
            format!("quote {field} is not a raw U256"),
            false,
        )
    })
}

fn encode_swap_exact_tokens(
    amount_in: U256,
    amount_out_min: U256,
    path: &[Address],
    recipient: Address,
    deadline: u64,
) -> String {
    let mut data = Vec::new();
    data.extend_from_slice(&[0x38, 0xed, 0x17, 0x39]);
    data.extend_from_slice(&amount_in.to_be_bytes::<32>());
    data.extend_from_slice(&amount_out_min.to_be_bytes::<32>());
    data.extend_from_slice(&U256::from(160).to_be_bytes::<32>());
    data.extend_from_slice(&address_word(recipient));
    data.extend_from_slice(&U256::from(deadline).to_be_bytes::<32>());
    data.extend_from_slice(&U256::from(path.len()).to_be_bytes::<32>());
    for address in path {
        data.extend_from_slice(&address_word(*address));
    }
    format!("0x{}", data_encoding::HEXLOWER.encode(&data))
}

fn address_word(address: Address) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(address.as_slice());
    word
}

fn unsupported(message: impl Into<String>) -> CapabilityError {
    CapabilityError::new(CapabilityErrorCode::Unsupported, message, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluxion_protocol::capability::{CapabilityMeta, Provenance, TokenRef};
    use fluxion_protocol::swap::{AllocationResponse, HopResponse, PlanKind};

    fn quote() -> QuoteResponse {
        QuoteResponse {
            meta: CapabilityMeta {
                schema_version: 1,
                capability: "swap.quote".to_string(),
                request_id: "test".to_string(),
            },
            chain: "ethereum".to_string(),
            token_in: TokenRef::new(Some("USDC".into()), Address::with_last_byte(1), 6),
            token_out: TokenRef::new(Some("WETH".into()), Address::with_last_byte(2), 18),
            amount_in: Amount::from_raw(U256::from(1_000_000u64), 6),
            amount_out: Amount::from_raw(U256::from(1_000_000_000_000_000u64), 18),
            plan_kind: PlanKind::Direct,
            price_impact_bps: Some(1),
            allocations: vec![AllocationResponse {
                amount_in: Amount::from_raw(U256::from(1_000_000u64), 6),
                amount_out: Amount::from_raw(U256::from(1_000_000_000_000_000u64), 18),
                hops: vec![HopResponse {
                    operation: "swap".to_string(),
                    platform: "uniswap-v2".to_string(),
                    pool: "0x0000000000000000000000000000000000000010".to_string(),
                    i: 0,
                    j: 1,
                    token_in: "0x0000000000000000000000000000000000000001".to_string(),
                    token_out: "0x0000000000000000000000000000000000000002".to_string(),
                    amount_in: Amount::from_raw(U256::from(1_000_000u64), 6),
                    amount_out: Amount::from_raw(U256::from(1_000_000_000_000_000u64), 18),
                }],
                sources: Vec::new(),
                children: Vec::new(),
            }],
            provenance: Provenance {
                quote_block: Some(100),
                ..Provenance::rpc()
            },
            latency_ms: 1,
        }
    }

    #[test]
    fn builds_uniswap_v2_calldata() {
        let response = build(
            &quote(),
            BuildRequest {
                recipient: Address::with_last_byte(3),
                slippage_bps: 50,
                deadline: 2_000_000_000,
            },
        )
        .unwrap();
        assert_eq!(response.router, UNISWAP_V2_ROUTER);
        assert_eq!(response.chain_id, 1);
        assert_eq!(response.quote_block, Some(100));
        assert_eq!(response.path.len(), 2);
        assert!(response.data.starts_with("0x38ed1739"));
        assert_eq!(response.amount_out_min.raw, "995000000000000");
        assert_eq!(response.deadline, 2_000_000_000);
        assert_eq!(response.hops.len(), 1);
    }

    #[test]
    fn rejects_split_and_unsupported_hops() {
        let mut split = quote();
        split.allocations.push(split.allocations[0].clone());
        assert!(
            build(
                &split,
                BuildRequest {
                    recipient: Address::with_last_byte(3),
                    slippage_bps: 50,
                    deadline: 2_000_000_000,
                }
            )
            .is_err()
        );

        let mut v3 = quote();
        v3.allocations[0].hops[0].platform = "uniswap-v3".to_string();
        assert!(
            build(
                &v3,
                BuildRequest {
                    recipient: Address::with_last_byte(3),
                    slippage_bps: 50,
                    deadline: 2_000_000_000,
                }
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_disconnected_hops() {
        let mut quote = quote();
        let mut second = quote.allocations[0].hops[0].clone();
        second.token_in = "0x0000000000000000000000000000000000000009".to_string();
        quote.allocations[0].hops.push(second);
        assert!(
            build(
                &quote,
                BuildRequest {
                    recipient: Address::with_last_byte(3),
                    slippage_bps: 50,
                    deadline: 2_000_000_000,
                }
            )
            .is_err()
        );
    }
}
