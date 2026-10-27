use std::cmp::Ordering;
use std::collections::HashMap;

use alloy_primitives::{Address, U256};

use crate::error::QuoteError;
use crate::graph::Graph;
use crate::model::{Hop, QuoteRequest, Route};
use crate::source::QuoteSource;
use crate::util::u256_to_f64;

#[derive(Debug, Clone, Copy)]
pub struct RouterConfig {
    pub max_hops: usize,
    pub top_k: usize,
    pub lambda: f64,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            max_hops: 3,
            top_k: 4,
            lambda: 0.0,
        }
    }
}

pub fn edge_weight(rate: f64, reserve_in: U256, amount_in: U256, lambda: f64) -> f64 {
    if rate <= 0.0 || rate.is_infinite() || rate.is_nan() || reserve_in.is_zero() {
        return f64::INFINITY;
    }
    -rate.ln() + lambda * (u256_to_f64(amount_in) / u256_to_f64(reserve_in))
}

#[derive(Clone)]
struct Candidate {
    cost: f64,
    edges: Vec<usize>,
    visited: Vec<Address>,
}

pub struct Router {
    config: RouterConfig,
}

impl Default for Router {
    fn default() -> Self {
        Self::new(RouterConfig::default())
    }
}

impl Router {
    pub fn new(config: RouterConfig) -> Self {
        Self { config }
    }

    pub async fn route(
        &self,
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

        let max_hops = request.max_hops.unwrap_or(self.config.max_hops).max(1);

        // Coarse stage: cached mid-price + liquidity penalty give each edge a weight.
        let mut weights = vec![0.0f64; graph.edge_count()];
        for (idx, w) in weights.iter_mut().enumerate() {
            let edge = graph.edge(idx);
            let pool = graph.pool(edge.pool_idx);
            let rate = source.mid_price(pool, edge.i, edge.j).await?;
            let reserve = source.reserve_in(pool, edge.i).await?;
            *w = edge_weight(rate, reserve, request.amount_in, self.config.lambda);
        }

        // K-best DP over (hop, token) with a simple-path constraint.
        let mut prev: HashMap<Address, Vec<Candidate>> = HashMap::new();
        prev.entry(request.token_in).or_default().push(Candidate {
            cost: 0.0,
            edges: Vec::new(),
            visited: vec![request.token_in],
        });

        let mut candidates: Vec<Candidate> = Vec::new();
        for _ in 1..=max_hops {
            let mut curr: HashMap<Address, Vec<Candidate>> = HashMap::new();
            for (u, prev_candidates) in &prev {
                for cand in prev_candidates {
                    for &eidx in graph.out_edge_indices(*u) {
                        let edge = graph.edge(eidx);
                        let pool = graph.pool(edge.pool_idx);
                        let v = pool.coins[edge.j];
                        if cand.visited.contains(&v) {
                            continue;
                        }
                        let w = weights[eidx];
                        if w.is_infinite() || w.is_nan() {
                            continue;
                        }
                        let mut next = Candidate {
                            cost: cand.cost + w,
                            edges: cand.edges.clone(),
                            visited: cand.visited.clone(),
                        };
                        next.edges.push(eidx);
                        next.visited.push(v);
                        curr.entry(v).or_default().push(next);
                    }
                }
            }
            for list in curr.values_mut() {
                list.sort_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap_or(Ordering::Equal));
                list.truncate(self.config.top_k.max(1));
            }
            if let Some(list) = curr.get(&request.token_out) {
                candidates.extend(list.iter().cloned());
            }
            prev = curr;
        }

        if candidates.is_empty() {
            return Err(QuoteError::NoPath);
        }
        candidates.sort_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap_or(Ordering::Equal));
        candidates.truncate(self.config.top_k.max(1));

        // Refine stage: chain the exact amount_out along each candidate path.
        let mut best: Option<Route> = None;
        for cand in candidates {
            let mut amount_in = request.amount_in;
            let mut hops = Vec::with_capacity(cand.edges.len());
            let mut ok = true;
            for &eidx in &cand.edges {
                let edge = graph.edge(eidx);
                let pool = graph.pool(edge.pool_idx);
                match source.amount_out(pool, edge.i, edge.j, amount_in).await {
                    Ok(out) => {
                        hops.push(Hop {
                            pool: pool.clone(),
                            i: edge.i,
                            j: edge.j,
                            token_in: pool.coins[edge.i],
                            token_out: pool.coins[edge.j],
                            amount_in,
                            amount_out: out,
                        });
                        amount_in = out;
                    }
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            let route = Route {
                hops,
                amount_out: amount_in,
            };
            if best
                .as_ref()
                .is_none_or(|r| route.amount_out > r.amount_out)
            {
                best = Some(route);
            }
        }

        best.ok_or(QuoteError::NoPath)
    }
}

pub async fn route(
    graph: &Graph,
    source: &dyn QuoteSource,
    request: &QuoteRequest,
) -> Result<Route, QuoteError> {
    Router::default().route(graph, source, request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Pool, PoolKind};
    use crate::source::{FixedRate, MockPool, MockV2};
    use alloy_primitives::Address;
    use std::collections::HashMap;

    const ETH: u8 = 1;
    const USDC: u8 = 2;
    const DAI: u8 = 3;

    fn eth() -> Address {
        Address::with_last_byte(ETH)
    }
    fn usdc() -> Address {
        Address::with_last_byte(USDC)
    }
    fn dai() -> Address {
        Address::with_last_byte(DAI)
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

    #[tokio::test]
    async fn multi_hop_beats_direct() {
        let mut g = Graph::new();
        let direct = Address::with_last_byte(30);
        let hop_a = Address::with_last_byte(31);
        let hop_b = Address::with_last_byte(32);
        // direct ETH->USDC at a poor rate (~1)
        g.add_pool(Pool {
            kind: PoolKind::UniswapV2,
            address: direct,
            coins: vec![eth(), usdc()],
        });
        // ETH->DAI at ~5, then DAI->USDC at ~1
        g.add_pool(Pool {
            kind: PoolKind::UniswapV2,
            address: hop_a,
            coins: vec![eth(), dai()],
        });
        g.add_pool(Pool {
            kind: PoolKind::UniswapV2,
            address: hop_b,
            coins: vec![dai(), usdc()],
        });

        let mut reserves = HashMap::new();
        reserves.insert(direct, [U256::from(1000u64), U256::from(1000u64)]);
        reserves.insert(hop_a, [U256::from(1000u64), U256::from(5_000u64)]);
        reserves.insert(hop_b, [U256::from(5_000u64), U256::from(5_000u64)]);
        let source = MockV2::new(reserves);

        let req = QuoteRequest {
            token_in: eth(),
            token_out: usdc(),
            amount_in: U256::from(100u64),
            max_hops: Some(3),
            amount_out_min: None,
        };
        let r = route(&g, &source, &req).await.unwrap();
        assert_eq!(r.hops.len(), 2);
        assert!(r.amount_out > U256::from(300u64));
    }

    #[test]
    fn liquidity_penalty_favors_deep_pool() {
        let rate = 1.0;
        let amount = U256::from(10u64);
        let lambda = 1.0;
        let deep = edge_weight(rate, U256::from(1000u64), amount, lambda);
        let shallow = edge_weight(rate, U256::from(10u64), amount, lambda);
        assert!(deep < shallow);
    }

    #[tokio::test]
    async fn multi_coin_pool_routes_two_coins() {
        let mut g = Graph::new();
        let p = Address::with_last_byte(40);
        let usdt = Address::with_last_byte(4);
        g.add_pool(Pool {
            kind: PoolKind::Curve,
            address: p,
            coins: vec![usdc(), dai(), usdt],
        });
        let mut reserves = HashMap::new();
        reserves.insert(
            p,
            vec![
                U256::from(1000u64),
                U256::from(1000u64),
                U256::from(1000u64),
            ],
        );
        let source = MockPool::new(reserves);

        let req = QuoteRequest {
            token_in: usdc(),
            token_out: dai(),
            amount_in: U256::from(100u64),
            max_hops: Some(3),
            amount_out_min: None,
        };
        let r = route(&g, &source, &req).await.unwrap();
        assert_eq!(r.hops.len(), 1);
        assert!(r.amount_out > U256::ZERO);
    }
}
