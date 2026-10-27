use alloy_primitives::U256;

use crate::error::QuoteError;
use crate::graph::Graph;
use crate::model::{Hop, QuoteRequest, Route};
use crate::source::QuoteSource;

pub async fn route(
    graph: &Graph,
    source: &dyn QuoteSource,
    request: &QuoteRequest,
) -> Result<Route, QuoteError> {
    if request.token_in == request.token_out {
        return Ok(Route {
            hops: Vec::new(),
            amount_out: request.amount_in,
        });
    }
    if request.amount_in.is_zero() {
        return Err(QuoteError::ZeroAmount);
    }

    let mut best: Option<(usize, U256)> = None;
    for &edge_idx in graph.out_edge_indices(request.token_in) {
        let edge = graph.edge(edge_idx);
        let pool = graph.pool(edge.pool_idx);
        if pool.coins[edge.j] != request.token_out {
            continue;
        }
        let amount_out = source
            .amount_out(pool, edge.i, edge.j, request.amount_in)
            .await?;
        match best {
            Some((_, b)) if b >= amount_out => {}
            _ => best = Some((edge_idx, amount_out)),
        }
    }

    let (edge_idx, amount_out) = best.ok_or(QuoteError::NoPath)?;
    let edge = graph.edge(edge_idx);
    let pool = graph.pool(edge.pool_idx).clone();
    let hop = Hop {
        pool,
        i: edge.i,
        j: edge.j,
        token_in: request.token_in,
        token_out: request.token_out,
        amount_in: request.amount_in,
        amount_out,
    };
    Ok(Route {
        hops: vec![hop],
        amount_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Pool, PoolKind};
    use crate::source::{FixedRate, MockV2};
    use alloy_primitives::Address;
    use std::collections::HashMap;

    const ETH: u8 = 1;
    const USDC: u8 = 2;

    fn eth() -> Address {
        Address::with_last_byte(ETH)
    }
    fn usdc() -> Address {
        Address::with_last_byte(USDC)
    }

    fn graph_with_two_parallel_pools() -> (Graph, Address, Address) {
        let mut g = Graph::new();
        let a = Address::with_last_byte(10);
        let b = Address::with_last_byte(11);
        g.add_pool(Pool {
            kind: PoolKind::UniswapV2,
            address: a,
            coins: vec![eth(), usdc()],
        });
        g.add_pool(Pool {
            kind: PoolKind::UniswapV2,
            address: b,
            coins: vec![eth(), usdc()],
        });
        (g, a, b)
    }

    #[tokio::test]
    async fn picks_best_of_parallel_edges() {
        let (g, a, b) = graph_with_two_parallel_pools();
        let mut reserves = HashMap::new();
        // pool a: 1000 ETH / 5M USDC (better rate ~5000 USDC/ETH)
        reserves.insert(a, [U256::from(1000u64), U256::from(5_000_000u64)]);
        // pool b: 500 ETH / 2M USDC (worse rate ~4000 USDC/ETH)
        reserves.insert(b, [U256::from(500u64), U256::from(2_000_000u64)]);
        let source = MockV2::new(reserves);

        let req = QuoteRequest {
            token_in: eth(),
            token_out: usdc(),
            amount_in: U256::from(1u64),
            max_hops: Some(3),
            amount_out_min: None,
        };
        let route = route(&g, &source, &req).await.unwrap();
        assert_eq!(route.hops.len(), 1);
        assert_eq!(route.hops[0].pool.address, a);
        // better pool must win
        assert!(route.amount_out > U256::from(4_500u64));
    }

    #[tokio::test]
    async fn returns_no_path_when_pair_missing() {
        let (g, _, _) = graph_with_two_parallel_pools();
        let src = FixedRate::new(U256::from(1u64));
        let req = QuoteRequest {
            token_in: eth(),
            token_out: Address::with_last_byte(99),
            amount_in: U256::from(1u64),
            max_hops: None,
            amount_out_min: None,
        };
        assert_eq!(route(&g, &src, &req).await, Err(QuoteError::NoPath));
    }

    #[tokio::test]
    async fn same_token_echoes_input() {
        let (g, _, _) = graph_with_two_parallel_pools();
        let src = FixedRate::new(U256::from(1u64));
        let req = QuoteRequest {
            token_in: eth(),
            token_out: eth(),
            amount_in: U256::from(7u64),
            max_hops: None,
            amount_out_min: None,
        };
        let route = route(&g, &src, &req).await.unwrap();
        assert!(route.hops.is_empty());
        assert_eq!(route.amount_out, U256::from(7u64));
    }

    #[tokio::test]
    async fn zero_amount_is_an_error() {
        let (g, _, _) = graph_with_two_parallel_pools();
        let src = FixedRate::new(U256::from(1u64));
        let req = QuoteRequest {
            token_in: eth(),
            token_out: usdc(),
            amount_in: U256::ZERO,
            max_hops: None,
            amount_out_min: None,
        };
        assert_eq!(route(&g, &src, &req).await, Err(QuoteError::ZeroAmount));
    }
}
