//! Local `swap.build` calldata construction from an existing quote.
//!
//! Supports one allocation built either from all-V2 hops or all-V3 hops on the
//! same venue. Split routes, mixed V2/V3 routes, Curve and aggregators are
//! rejected rather than approximated.

use alloy_primitives::{Address, U256, address};

use crate::capability::{Amount, CapabilityError, CapabilityErrorCode};
use crate::chain::Chain;
use eollo_protocol::swap::{QuoteResponse, SwapBuildHop, SwapBuildResponse};

const UNISWAP_V2_ROUTER: Address = address!("7a250d5630B4cF539739dF2C5dAcb4c659F2488D");
const PANCAKE_V2_ROUTER: Address = address!("10ED43C718714eb63d5aA57B78B54704E256024E");
const UNISWAP_V3_ROUTER: Address = address!("E592427A0AEce92De3Edee1F18E0157C05861564");
const PANCAKE_V3_ROUTER: Address = address!("1b81D678ffb9C0263b24A97847620C99d213eB14");

pub struct BuildRequest {
    pub recipient: Address,
    pub slippage_bps: u16,
    pub deadline: u64,
}

pub fn build(
    quote: &QuoteResponse,
    request: BuildRequest,
) -> Result<SwapBuildResponse, CapabilityError> {
    let chain = Chain::parse(&quote.chain)?;
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

    let amount_in = parse_raw("amount_in", &quote.amount_in.raw)?;
    let amount_out = parse_raw("amount_out", &quote.amount_out.raw)?;
    let min_out =
        amount_out.saturating_mul(U256::from(10_000 - request.slippage_bps)) / U256::from(10_000);

    let all_v2 = allocation
        .hops
        .iter()
        .all(|hop| hop.platform == "uniswap-v2");
    let all_v3 = allocation
        .hops
        .iter()
        .all(|hop| hop.platform == "uniswap-v3" || hop.platform == "pancake-v3");

    let (router, data, path) = if all_v2 {
        let path = contiguous_path(allocation, |hop| hop.platform == "uniswap-v2")?;
        let router = match chain {
            Chain::Ethereum => UNISWAP_V2_ROUTER,
            Chain::Bsc => PANCAKE_V2_ROUTER,
        };
        let data = encode_swap_exact_tokens(
            amount_in,
            min_out,
            &path,
            request.recipient,
            request.deadline,
        );
        (router, data, path)
    } else if all_v3 {
        let venue = allocation.hops[0].platform.as_str();
        if allocation.hops.iter().any(|hop| hop.platform != venue) {
            return Err(unsupported("swap.build cannot mix V3 venues in one route"));
        }
        let (path, packed) = contiguous_v3_route(allocation)?;
        let router = match venue {
            "uniswap-v3" => UNISWAP_V3_ROUTER,
            "pancake-v3" => PANCAKE_V3_ROUTER,
            other => {
                return Err(unsupported(format!(
                    "swap.build has no V3 router for platform {other}"
                )));
            }
        };
        let data = encode_exact_input(
            &packed,
            request.recipient,
            request.deadline,
            amount_in,
            min_out,
        );
        (router, data, path)
    } else {
        return Err(unsupported(
            "swap.build supports all-V2 or all-V3 routes; mixed routes are not built",
        ));
    };

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
        chain_id: chain.chain_id(),
        quote_block: quote.provenance.quote_block,
        router,
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

fn contiguous_path(
    allocation: &eollo_protocol::swap::AllocationResponse,
    allowed: impl Fn(&eollo_protocol::swap::HopResponse) -> bool,
) -> Result<Vec<Address>, CapabilityError> {
    let mut path = Vec::with_capacity(allocation.hops.len() + 1);
    for (index, hop) in allocation.hops.iter().enumerate() {
        if !allowed(hop) {
            return Err(unsupported(format!(
                "swap.build cannot build hop platform {}",
                hop.platform
            )));
        }
        let token_in = parse_hop_token(&hop.token_in, index, "token_in")?;
        let token_out = parse_hop_token(&hop.token_out, index, "token_out")?;
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
    Ok(path)
}

fn contiguous_v3_route(
    allocation: &eollo_protocol::swap::AllocationResponse,
) -> Result<(Vec<Address>, Vec<u8>), CapabilityError> {
    let mut path = Vec::with_capacity(allocation.hops.len() + 1);
    let mut packed = Vec::with_capacity(allocation.hops.len() * 43 + 20);
    for (index, hop) in allocation.hops.iter().enumerate() {
        let fee = hop.fee.ok_or_else(|| {
            CapabilityError::new(
                CapabilityErrorCode::StateIncomplete,
                "V3 quote hop is missing its fee tier",
                false,
            )
        })?;
        if fee > 0xFF_FFFF {
            return Err(CapabilityError::new(
                CapabilityErrorCode::StateIncomplete,
                "V3 fee tier does not fit in 24 bits",
                false,
            ));
        }
        let token_in = parse_hop_token(&hop.token_in, index, "token_in")?;
        let token_out = parse_hop_token(&hop.token_out, index, "token_out")?;
        if index == 0 {
            path.push(token_in);
            packed.extend_from_slice(token_in.as_slice());
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
        let fee_bytes = fee.to_be_bytes();
        packed.extend_from_slice(&fee_bytes[1..]);
        packed.extend_from_slice(token_out.as_slice());
        path.push(token_out);
    }
    Ok((path, packed))
}

fn parse_hop_token(value: &str, index: usize, field: &str) -> Result<Address, CapabilityError> {
    value.parse::<Address>().map_err(|_| {
        CapabilityError::new(
            CapabilityErrorCode::StateIncomplete,
            format!("invalid quote hop {field} at index {index}"),
            false,
        )
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

/// Uniswap/PancakeSwap V3 `SwapRouter.exactInput`.
fn encode_exact_input(
    path: &[u8],
    recipient: Address,
    deadline: u64,
    amount_in: U256,
    amount_out_min: U256,
) -> String {
    let mut data = Vec::new();
    data.extend_from_slice(&[0xc0, 0x4b, 0x8d, 0x59]);
    // Offset to the tuple argument.
    data.extend_from_slice(&U256::from(32).to_be_bytes::<32>());
    // Offset to `bytes path` inside the tuple head.
    data.extend_from_slice(&U256::from(160).to_be_bytes::<32>());
    data.extend_from_slice(&address_word(recipient));
    data.extend_from_slice(&U256::from(deadline).to_be_bytes::<32>());
    data.extend_from_slice(&amount_in.to_be_bytes::<32>());
    data.extend_from_slice(&amount_out_min.to_be_bytes::<32>());
    data.extend_from_slice(&U256::from(path.len()).to_be_bytes::<32>());
    let mut padded = path.to_vec();
    while !padded.len().is_multiple_of(32) {
        padded.push(0);
    }
    data.extend_from_slice(&padded);
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
    use eollo_protocol::capability::{CapabilityMeta, Provenance, TokenRef};
    use eollo_protocol::swap::{AllocationResponse, HopResponse, PlanKind};

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
                    fee: None,
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

    fn request() -> BuildRequest {
        BuildRequest {
            recipient: Address::with_last_byte(3),
            slippage_bps: 50,
            deadline: 2_000_000_000,
        }
    }

    #[test]
    fn builds_pancake_v2_calldata_on_bsc() {
        let mut quote = quote();
        quote.chain = "bsc".to_string();
        let response = build(&quote, request()).unwrap();
        assert_eq!(response.router, PANCAKE_V2_ROUTER);
        assert_eq!(response.chain_id, 56);
        assert!(response.data.starts_with("0x38ed1739"));
    }

    #[test]
    fn builds_pancake_v3_exact_input_on_bsc() {
        let mut quote = quote();
        quote.chain = "bsc".to_string();
        let hop = &mut quote.allocations[0].hops[0];
        hop.platform = "pancake-v3".to_string();
        hop.fee = Some(2_500);
        let response = build(&quote, request()).unwrap();
        assert_eq!(response.router, PANCAKE_V3_ROUTER);
        assert_eq!(response.chain_id, 56);
        assert!(response.data.starts_with("0xc04b8d59"));
        assert_eq!(response.path.len(), 2);
        // Packed path: token_in (20) + fee u24 + token_out (20).
        assert!(response.data.contains("0009c4"));
    }

    #[test]
    fn rejects_mixed_v2_v3_route() {
        let mut quote = quote();
        quote.chain = "bsc".to_string();
        let mut second = quote.allocations[0].hops[0].clone();
        second.platform = "pancake-v3".to_string();
        second.fee = Some(2_500);
        second.token_in = second.token_out.clone();
        second.token_out = "0x0000000000000000000000000000000000000009".to_string();
        quote.allocations[0].hops.push(second);
        assert!(build(&quote, request()).is_err());
    }

    #[test]
    fn rejects_v3_route_without_fee() {
        let mut quote = quote();
        quote.chain = "bsc".to_string();
        quote.allocations[0].hops[0].platform = "pancake-v3".to_string();
        quote.allocations[0].hops[0].fee = None;
        assert!(build(&quote, request()).is_err());
    }
}
