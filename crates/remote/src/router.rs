use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use alloy_primitives::{Address, U256};
use futures::stream::{self, StreamExt};

use crate::error::QuoteError;
use crate::graph::Graph;
use crate::model::{Hop, Pool, PoolKind, QuoteRequest, Route};
use crate::source::QuoteSource;
use crate::util::u256_to_f64;

#[derive(Debug, Clone, Copy)]
pub struct RouterConfig {
    pub max_hops: usize,
    pub top_k: usize,
    pub lambda: f64,
    pub reserve_ratio_k: u128,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            max_hops: 3,
            top_k: 4,
            lambda: 0.0,
            reserve_ratio_k: 0,
        }
    }
}

pub fn edge_weight(rate: f64, reserve_in: U256, amount_in: U256, lambda: f64) -> f64 {
    if rate <= 0.0 || rate.is_infinite() || rate.is_nan() || reserve_in.is_zero() {
        return f64::INFINITY;
    }
    -rate.ln() + lambda * (u256_to_f64(amount_in) / u256_to_f64(reserve_in))
}

/// Approximate a swap's fee as a cost so multi-hop routes don't win on
/// mid-price alone (each extra hop pays another fee).
fn fee_penalty(kind: PoolKind) -> f64 {
    let frac = match kind {
        PoolKind::UniswapV2 | PoolKind::ShibaSwapV2 => 0.003,
        PoolKind::UniswapV3 { fee }
        | PoolKind::SushiSwapV3 { fee }
        | PoolKind::PancakeSwapV3 { fee } => fee as f64 / 1_000_000.0,
        PoolKind::Curve => 0.0004,
        PoolKind::Conversion { .. } => 0.0,
    };
    -(1.0 - frac).ln()
}

#[derive(Clone)]
struct Candidate {
    cost: f64,
    edges: Vec<usize>,
    visited: Vec<Address>,
    // Bounded diversity: original venues, conversion, ShibaSwap, or both.
    // A high spot rate in one family must not crowd every other family out
    // before exact quoting can account for slippage.
    family: u8,
}

fn retain_diverse_candidates(list: &mut Vec<Candidate>, top_k: usize) {
    list.sort_by(|a, b| {
        a.cost
            .total_cmp(&b.cost)
            .then_with(|| a.edges.cmp(&b.edges))
    });
    let mut kept = [0usize; 4];
    list.retain(|candidate| {
        let count = &mut kept[candidate.family as usize];
        if *count >= top_k.max(1) {
            false
        } else {
            *count += 1;
            true
        }
    });
}

/// Per-edge coarse data: mid-price and input reserve, indexed by edge id.
#[derive(Clone, Copy)]
pub struct EdgePrice {
    pub mid_price: f64,
    pub reserve_in: U256,
}

/// Coarse, unquoted candidate path. Split allocation probes these at multiple
/// input sizes so a path that cannot serve the full amount can still
/// participate with a smaller branch.
#[derive(Debug, Clone)]
pub struct RoutedPath {
    pub hops: Vec<(Pool, usize, usize)>,
    pub cost: f64,
}

/// A precomputed snapshot of all edge prices so path search is synchronous.
pub struct PriceSnapshot {
    per_edge: Vec<EdgePrice>,
}

impl PriceSnapshot {
    /// Read-only per-edge prices, indexed by edge id.
    pub fn edge_prices(&self) -> &[EdgePrice] {
        &self.per_edge
    }

    pub async fn build(graph: &Graph, source: &(dyn QuoteSource + Send + Sync)) -> Self {
        let pairs: Vec<(usize, EdgePrice)> = stream::iter(0..graph.edge_count())
            .map(|idx| async move {
                let edge = graph.edge(idx);
                let pool = graph.pool(edge.pool_idx);
                let mid_price = source
                    .mid_price(pool, edge.i, edge.j)
                    .await
                    .unwrap_or(f64::NAN);
                let reserve_in = source.reserve_in(pool, edge.i).await.unwrap_or(U256::ZERO);
                (
                    idx,
                    EdgePrice {
                        mid_price,
                        reserve_in,
                    },
                )
            })
            .buffer_unordered(16)
            .collect()
            .await;
        let mut per_edge = vec![
            EdgePrice {
                mid_price: f64::NAN,
                reserve_in: U256::ZERO,
            };
            graph.edge_count()
        ];
        for (idx, ep) in pairs {
            per_edge[idx] = ep;
        }
        Self { per_edge }
    }
}

#[derive(Clone)]
pub struct Router {
    config: RouterConfig,
    snapshot: Arc<RwLock<Option<Arc<PriceSnapshot>>>>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new(RouterConfig::default())
    }
}

impl Router {
    pub fn new(config: RouterConfig) -> Self {
        Self {
            config,
            snapshot: Arc::new(RwLock::new(None)),
        }
    }

    /// Install a precomputed price snapshot (rebuilt periodically).
    pub fn set_snapshot(&self, snapshot: Arc<PriceSnapshot>) {
        *self.snapshot.write().unwrap() = Some(snapshot);
    }

    /// Cloned handle to the currently installed snapshot, if any. Read-only and
    /// side-effect free: never builds a fresh snapshot.
    pub fn snapshot(&self) -> Option<Arc<PriceSnapshot>> {
        self.snapshot.read().unwrap().clone()
    }

    /// Coarse candidate paths. No amount_out calls are made here; callers can
    /// probe several input sizes without losing paths that fail at the full
    /// requested amount.
    pub async fn candidate_paths(
        &self,
        graph: &Graph,
        source: &(dyn QuoteSource + Send + Sync),
        request: &QuoteRequest,
    ) -> Result<Vec<RoutedPath>, QuoteError> {
        let candidates = self.coarse_candidates(graph, source, request).await;
        Ok(candidates
            .into_iter()
            .map(|candidate| RoutedPath {
                hops: candidate
                    .edges
                    .into_iter()
                    .map(|idx| {
                        let edge = graph.edge(idx);
                        (graph.pool(edge.pool_idx).clone(), edge.i, edge.j)
                    })
                    .collect(),
                cost: candidate.cost,
            })
            .collect())
    }

    async fn coarse_candidates(
        &self,
        graph: &Graph,
        source: &(dyn QuoteSource + Send + Sync),
        request: &QuoteRequest,
    ) -> Vec<Candidate> {
        let max_hops = request.max_hops.unwrap_or(self.config.max_hops).max(1);

        // Use the precomputed snapshot if installed; otherwise build one.
        let installed = { self.snapshot.read().unwrap().clone() };
        let snapshot = match installed {
            Some(s) => s,
            None => Arc::new(PriceSnapshot::build(graph, source).await),
        };

        let __t0 = std::time::Instant::now();
        // Coarse stage: read the snapshot (synchronous) to weight each edge.
        let penalty_on = self.config.lambda != 0.0;
        let ratio_k = U256::from(self.config.reserve_ratio_k);
        let need_reserve = penalty_on || ratio_k > U256::ZERO;
        let amount_in = request.amount_in;
        let amount_f = u256_to_f64(amount_in);
        let lambda = self.config.lambda;
        let mut weights = vec![0.0f64; graph.edge_count()];
        for (idx, w) in weights.iter_mut().enumerate() {
            let ep = &snapshot.per_edge[idx];
            if ep.mid_price <= 0.0 || !ep.mid_price.is_finite() {
                *w = f64::INFINITY;
                continue;
            }
            let pool = graph.pool(graph.edge(idx).pool_idx);
            let mut weight = -ep.mid_price.ln() + fee_penalty(pool.kind);
            if need_reserve {
                if ep.reserve_in.is_zero() {
                    *w = f64::INFINITY;
                    continue;
                }
                if ratio_k > U256::ZERO {
                    let threshold = amount_in.checked_mul(ratio_k).unwrap_or(U256::MAX);
                    if ep.reserve_in < threshold {
                        *w = f64::INFINITY;
                        continue;
                    }
                }
                if penalty_on {
                    weight += lambda * (amount_f / u256_to_f64(ep.reserve_in));
                }
            }
            *w = weight;
        }
        tracing::debug!(
            coarse_ms = __t0.elapsed().as_millis() as u64,
            edges = graph.edge_count(),
            "coarse done"
        );

        // K-best DP over (hop, token) with a simple-path constraint.
        let mut prev: HashMap<Address, Vec<Candidate>> = HashMap::new();
        prev.entry(request.token_in).or_default().push(Candidate {
            cost: 0.0,
            edges: Vec::new(),
            visited: vec![request.token_in],
            family: 0,
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
                            family: cand.family
                                | match pool.kind {
                                    PoolKind::Conversion { .. } => 1,
                                    PoolKind::ShibaSwapV2 => 2,
                                    _ => 0,
                                },
                        };
                        next.edges.push(eidx);
                        next.visited.push(v);
                        curr.entry(v).or_default().push(next);
                    }
                }
            }
            for list in curr.values_mut() {
                retain_diverse_candidates(list, self.config.top_k);
            }
            if let Some(list) = curr.get(&request.token_out) {
                candidates.extend(list.iter().cloned());
            }
            prev = curr;
        }

        // Keep candidates from every hop length instead of truncating the whole
        // pool to top-K. A cheap-looking longer path must not crowd out a
        // feasible shorter path before the exact refine stage (which picks by
        // real amount_out, not coarse cost).

        // Always consider direct routes: coarse mid-price can misrank thin or
        // mispriced pools and crowd the obvious best route out of the top-K.
        for &eidx in graph.out_edge_indices(request.token_in) {
            let edge = graph.edge(eidx);
            if graph.pool(edge.pool_idx).coins[edge.j] == request.token_out
                && (ratio_k == U256::ZERO
                    || snapshot.per_edge[eidx].reserve_in
                        >= amount_in.checked_mul(ratio_k).unwrap_or(U256::MAX))
                && !candidates
                    .iter()
                    .any(|c| c.edges.len() == 1 && c.edges[0] == eidx)
            {
                candidates.push(Candidate {
                    cost: 0.0,
                    edges: vec![eidx],
                    visited: vec![request.token_in, request.token_out],
                    family: 0,
                });
            }
        }
        candidates
    }

    /// Exact-quoted candidate routes, best output first. Callers that need
    /// multi-route allocation can reuse this list instead of re-running the
    /// coarse path search.
    pub async fn candidates(
        &self,
        graph: &Graph,
        source: &(dyn QuoteSource + Send + Sync),
        request: &QuoteRequest,
    ) -> Result<Vec<Route>, QuoteError> {
        let candidates = self.coarse_candidates(graph, source, request).await;
        let __t0 = std::time::Instant::now();
        // Refine candidates with bounded concurrency to keep RPC use predictable.
        let candidate_count = candidates.len();
        let refine = stream::iter(candidates).map(|cand: Candidate| async move {
            let mut amount_in = request.amount_in;
            let mut hops = Vec::with_capacity(cand.edges.len());
            for &eidx in &cand.edges {
                let edge = graph.edge(eidx);
                let pool = graph.pool(edge.pool_idx);
                let out = source.amount_out(pool, edge.i, edge.j, amount_in).await?;
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
            Ok::<Route, QuoteError>(Route {
                hops,
                amount_out: amount_in,
                price_impact_bps: 0,
            })
        });
        let mut routes = Vec::new();
        let mut refined = refine.buffer_unordered(16);
        while let Some(result) = refined.next().await {
            if let Ok(route) = result {
                routes.push(route);
            }
        }
        routes.sort_by_key(|route| std::cmp::Reverse(route.amount_out));
        tracing::debug!(
            refine_ms = __t0.elapsed().as_millis() as u64,
            candidates = candidate_count,
            routes = routes.len(),
            "refine done"
        );
        Ok(routes)
    }

    pub async fn route(
        &self,
        graph: &Graph,
        source: &(dyn QuoteSource + Send + Sync),
        request: &QuoteRequest,
    ) -> Result<Route, QuoteError> {
        if request.amount_in.is_zero() {
            return Err(QuoteError::ZeroAmount);
        }
        if request.token_in == request.token_out {
            if request
                .amount_out_min
                .is_some_and(|min| request.amount_in < min)
            {
                return Err(QuoteError::AmountOutBelowMin);
            }
            return Ok(Route {
                hops: Vec::new(),
                amount_out: request.amount_in,
                price_impact_bps: 0,
            });
        }

        let mut route = self
            .candidates(graph, source, request)
            .await?
            .into_iter()
            .next()
            .ok_or(QuoteError::NoPath)?;
        route.price_impact_bps = self.price_impact_bps(source, request, &route).await;
        if let Some(min) = request.amount_out_min
            && route.amount_out < min
        {
            return Err(QuoteError::AmountOutBelowMin);
        }
        Ok(route)
    }

    async fn price_impact_bps(
        &self,
        source: &(dyn QuoteSource + Send + Sync),
        request: &QuoteRequest,
        route: &Route,
    ) -> i64 {
        let mut mid_rate = 1.0f64;
        for hop in &route.hops {
            if let Ok(rate) = source.mid_price(&hop.pool, hop.i, hop.j).await
                && rate > 0.0
                && rate.is_finite()
            {
                mid_rate *= rate;
            }
        }
        if mid_rate <= 0.0 || !mid_rate.is_finite() {
            return 0;
        }
        let exec_rate = u256_to_f64(route.amount_out) / u256_to_f64(request.amount_in);
        let impact = 1.0 - exec_rate / mid_rate;
        (impact * 10_000.0).round() as i64
    }
}

pub async fn route(
    graph: &Graph,
    source: &(dyn QuoteSource + Send + Sync),
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
    async fn price_snapshot_builds_per_edge() {
        let (g, a, b) = graph_with_two_parallel_pools();
        let mut reserves = HashMap::new();
        reserves.insert(a, [U256::from(1000u64), U256::from(3000u64)]);
        reserves.insert(b, [U256::from(1000u64), U256::from(4000u64)]);
        let source = MockV2::new(reserves);
        let snap = PriceSnapshot::build(&g, &source).await;
        assert_eq!(snap.per_edge.len(), g.edge_count());
        assert!(
            snap.per_edge
                .iter()
                .all(|e| e.mid_price.is_finite() && e.mid_price > 0.0)
        );
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
    async fn same_token_still_enforces_zero_and_minimum_checks() {
        let g = Graph::new();
        let src = FixedRate::new(U256::from(1));
        let mut req = QuoteRequest {
            token_in: eth(),
            token_out: eth(),
            amount_in: U256::ZERO,
            max_hops: None,
            amount_out_min: None,
        };
        assert_eq!(route(&g, &src, &req).await, Err(QuoteError::ZeroAmount));
        req.amount_in = U256::from(7);
        req.amount_out_min = Some(U256::from(8));
        assert_eq!(
            route(&g, &src, &req).await,
            Err(QuoteError::AmountOutBelowMin)
        );
    }

    #[tokio::test]
    async fn direct_fallback_works_without_a_coarse_price_but_honors_reserve_filter() {
        let (g, a, b) = graph_with_two_parallel_pools();
        let source = MockV2::new(HashMap::from([
            (a, [U256::from(100), U256::from(1000)]),
            (b, [U256::from(100), U256::from(2000)]),
        ]));
        let req = QuoteRequest {
            token_in: eth(),
            token_out: usdc(),
            amount_in: U256::from(20),
            max_hops: None,
            amount_out_min: None,
        };
        let mut config = RouterConfig::default();
        let router = Router::new(config);
        router.set_snapshot(Arc::new(PriceSnapshot {
            per_edge: vec![
                EdgePrice {
                    mid_price: f64::NAN,
                    reserve_in: U256::from(100)
                };
                g.edge_count()
            ],
        }));
        assert!(router.route(&g, &source, &req).await.is_ok());
        config.reserve_ratio_k = 10;
        assert_eq!(
            Router::new(config).route(&g, &source, &req).await,
            Err(QuoteError::NoPath)
        );
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

    #[tokio::test]
    async fn candidates_keep_multi_hop_path_even_with_top_k_one() {
        let mut g = Graph::new();
        let direct = Address::with_last_byte(30);
        let hop_a = Address::with_last_byte(31);
        let hop_b = Address::with_last_byte(32);
        g.add_pool(Pool {
            kind: PoolKind::UniswapV2,
            address: direct,
            coins: vec![eth(), usdc()],
        });
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
        let source = MockV2::new(HashMap::from([
            (direct, [U256::from(1000u64), U256::from(5000u64)]),
            (hop_a, [U256::from(1000u64), U256::from(5000u64)]),
            (hop_b, [U256::from(5000u64), U256::from(5000u64)]),
        ]));
        let req = QuoteRequest {
            token_in: eth(),
            token_out: usdc(),
            amount_in: U256::from(100u64),
            max_hops: Some(3),
            amount_out_min: None,
        };
        let router = Router::new(RouterConfig {
            top_k: 1,
            ..RouterConfig::default()
        });
        let routes = router.candidates(&g, &source, &req).await.unwrap();
        assert!(routes.iter().any(|route| route.hops.len() == 1));
        assert!(routes.iter().any(|route| route.hops.len() == 2));
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
    async fn amount_out_min_rejects_low_output() {
        let (g, a, b) = graph_with_two_parallel_pools();
        let mut reserves = HashMap::new();
        reserves.insert(a, [U256::from(1000u64), U256::from(5_000_000u64)]);
        reserves.insert(b, [U256::from(500u64), U256::from(2_000_000u64)]);
        let source = MockV2::new(reserves);
        let req = QuoteRequest {
            token_in: eth(),
            token_out: usdc(),
            amount_in: U256::from(100u64),
            max_hops: Some(3),
            amount_out_min: Some(U256::from(1_000_000u64)),
        };
        assert_eq!(
            route(&g, &source, &req).await,
            Err(QuoteError::AmountOutBelowMin)
        );
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
