//! Bounded two-/multi-pool allocation. The caller supplies one immutable
//! pricing context (pin RPC state for meaningful baseline/split comparisons).
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use alloy_primitives::{Address, U256};
use async_trait::async_trait;
use futures::{StreamExt, stream};
use serde::{Deserialize, Serialize};

use crate::error::QuoteError;
use crate::graph::Graph;
use crate::model::{Hop, Pool, PoolKind, QuoteRequest, Route};
use crate::router::Router;
use crate::source::QuoteSource;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SplitConfig {
    pub coarse_segments: usize,
    pub refinement_segments: usize,
    pub refinement_rounds: usize,
    pub max_pools: usize,
    pub max_quote_calls: usize,
    pub concurrency: usize,
    pub timeout_ms: u64,
    pub max_branches: usize,
    pub allocation_segments: usize,
}

impl Default for SplitConfig {
    fn default() -> Self {
        Self {
            coarse_segments: 10,
            refinement_segments: 4,
            refinement_rounds: 2,
            max_pools: 4,
            max_quote_calls: 128,
            concurrency: 8,
            timeout_ms: 8000,
            max_branches: 2,
            allocation_segments: 20,
        }
    }
}

impl SplitConfig {
    pub fn multi_pool() -> Self {
        Self {
            max_branches: 4,
            max_quote_calls: 256,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    Baseline,
    Direct,
    Split,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    #[default]
    Completed,
    NoPairs,
    QuoteBudget,
    TimeBudget,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct SearchStats {
    pub baseline_ms: u128,
    pub search_ms: u128,
    pub baseline_quote_calls: usize,
    pub baseline_rpc_requests: Option<u64>,
    pub additional_rpc_requests: Option<u64>,
    pub additional_quote_calls: usize,
    pub completed_additional_quotes: usize,
    pub failed_additional_quotes: usize,
    pub cache_hits: usize,
    pub eligible_pools: usize,
    pub searched_pools: usize,
    pub pairs: usize,
    pub evaluated_plans: usize,
    pub valid_plans: usize,
    pub refinement_rounds: usize,
    pub stop_reason: StopReason,
    pub multi_pool_search: bool,
    pub multi_pool_completed: bool,
    pub allocation_plans_evaluated: usize,
}

#[derive(Debug, Clone)]
pub struct Allocation {
    pub amount_in: U256,
    pub amount_out: U256,
    pub hops: Vec<Hop>,
}

#[derive(Debug, Clone)]
pub struct SplitResult {
    pub baseline: Route,
    pub kind: PlanKind,
    pub amount_out: U256,
    pub allocations: Vec<Allocation>,
    pub stats: SearchStats,
}

type QuoteKey = (Address, usize, usize, U256);
type QuoteCache = Mutex<HashMap<QuoteKey, Result<U256, QuoteError>>>;

/// Request-local memoization includes baseline quotes; only completed outcomes
/// enter the cache. Extra search schedules each missing key once per stage.
struct ObservedSource<'a> {
    inner: &'a (dyn QuoteSource + Send + Sync),
    cache: QuoteCache,
    calls: AtomicUsize,
    completed: AtomicUsize,
    failed: AtomicUsize,
    hits: AtomicUsize,
}

impl ObservedSource<'_> {
    fn key(pool: &DirectPool, amount: U256) -> QuoteKey {
        (pool.pool.address, pool.i, pool.j, amount)
    }
    fn cached(&self, pool: &DirectPool, amount: U256) -> Option<Result<U256, QuoteError>> {
        if amount.is_zero() {
            Some(Ok(U256::ZERO))
        } else {
            self.cache
                .lock()
                .unwrap()
                .get(&Self::key(pool, amount))
                .cloned()
        }
    }
}

#[async_trait]
impl QuoteSource for ObservedSource<'_> {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount: U256,
    ) -> Result<U256, QuoteError> {
        let key = (pool.address, i, j, amount);
        if let Some(result) = self.cache.lock().unwrap().get(&key).cloned() {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return result;
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        let result = self.inner.amount_out(pool, i, j, amount).await;
        self.completed.fetch_add(1, Ordering::Relaxed);
        if result.is_err() {
            self.failed.fetch_add(1, Ordering::Relaxed);
        }
        self.cache.lock().unwrap().insert(key, result.clone());
        result
    }
    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        self.inner.mid_price(pool, i, j).await
    }
    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError> {
        self.inner.reserve_in(pool, i).await
    }
}

struct DirectPool {
    pool: Pool,
    i: usize,
    j: usize,
}

struct Pair {
    a: usize,
    b: usize,
    points: Vec<U256>,
    best: Option<(U256, U256)>, // input assigned to A, combined output
}

/// floor(total * numerator / denominator), without overflowing total*numerator.
fn fraction(total: U256, numerator: usize, denominator: usize) -> U256 {
    let den = U256::from(denominator);
    let num = U256::from(numerator);
    total / den * num + (total % den * num) / den
}

fn grid(left: U256, right: U256, segments: usize) -> Vec<U256> {
    let mut points = (0..=segments)
        .map(|i| left + fraction(right - left, i, segments))
        .collect::<Vec<_>>();
    points.dedup();
    points
}

fn allocation(pool: &DirectPool, amount_in: U256, amount_out: U256) -> Allocation {
    Allocation {
        amount_in,
        amount_out,
        hops: vec![Hop {
            pool: pool.pool.clone(),
            i: pool.i,
            j: pool.j,
            token_in: pool.pool.coins[pool.i],
            token_out: pool.pool.coins[pool.j],
            amount_in,
            amount_out,
        }],
    }
}

fn evaluate(
    pairs: &mut [Pair],
    pools: &[DirectPool],
    source: &ObservedSource<'_>,
    amount: U256,
    seen: &mut HashSet<(usize, usize, U256)>,
    result: &mut SplitResult,
) {
    for pair in pairs {
        for &x in &pair.points {
            let key = (pair.a, pair.b, x);
            if seen.contains(&key) {
                continue;
            }
            let (Some(a), Some(b)) = (
                source.cached(&pools[pair.a], x),
                source.cached(&pools[pair.b], amount - x),
            ) else {
                continue;
            };
            seen.insert(key);
            result.stats.evaluated_plans += 1;
            let (Ok(a), Ok(b)) = (a, b) else {
                continue;
            };
            let Some(out) = a.checked_add(b) else {
                continue;
            };
            result.stats.valid_plans += 1;
            if pair.best.is_none_or(|(_, old)| out > old) {
                pair.best = Some((x, out));
            }
            if out <= result.amount_out {
                continue;
            } // baseline wins ties
            let mut allocations = Vec::new();
            if !x.is_zero() {
                allocations.push(allocation(&pools[pair.a], x, a));
            }
            if x < amount {
                allocations.push(allocation(&pools[pair.b], amount - x, b));
            }
            result.kind = if allocations.len() == 2 {
                PlanKind::Split
            } else {
                PlanKind::Direct
            };
            result.amount_out = out;
            result.allocations = allocations;
        }
    }
}

async fn fill_quotes(
    pools: &[DirectPool],
    source: &ObservedSource<'_>,
    jobs: Vec<(usize, U256)>,
    config: &SplitConfig,
    baseline_calls: usize,
    deadline: tokio::time::Instant,
) -> StopReason {
    if tokio::time::Instant::now() >= deadline {
        return StopReason::TimeBudget;
    }
    let remaining = config
        .max_quote_calls
        .saturating_sub(source.calls.load(Ordering::Relaxed) - baseline_calls);
    let truncated = jobs.len() > remaining;
    let mut pending = stream::iter(jobs.into_iter().take(remaining))
        .map(|(idx, amount): (usize, U256)| async move {
            let pool = &pools[idx];
            let _ = source.amount_out(&pool.pool, pool.i, pool.j, amount).await;
        })
        .buffer_unordered(config.concurrency);
    loop {
        match tokio::time::timeout_at(deadline, pending.next()).await {
            Ok(Some(())) => {}
            Ok(None) => break,
            Err(_) => return StopReason::TimeBudget,
        }
    }
    if truncated {
        StopReason::QuoteBudget
    } else {
        StopReason::Completed
    }
}

#[derive(Clone)]
struct DpAllocation {
    out: U256,
    units: Vec<usize>,
}

/// Discrete resource allocation over sampled independent pool quote tables.
/// The DP optimizes the sampled additive model; integer residuals must still be
/// allocated and re-quoted before a plan can replace the retained baseline.
fn allocation_dp(
    tables: &[Vec<Option<U256>>],
    segments: usize,
    branches: usize,
    deadline: tokio::time::Instant,
) -> Result<Vec<Vec<usize>>, StopReason> {
    let width = branches + 1;
    let mut previous = vec![None::<DpAllocation>; (segments + 1) * width];
    previous[0] = Some(DpAllocation {
        out: U256::ZERO,
        units: vec![],
    });
    for table in tables {
        let mut next = vec![None::<DpAllocation>; (segments + 1) * width];
        for used in 0..=segments {
            if tokio::time::Instant::now() >= deadline {
                return Err(StopReason::TimeBudget);
            }
            for active in 0..=branches {
                let Some(old) = &previous[used * width + active] else {
                    continue;
                };
                for (units, value) in table.iter().enumerate().take(segments - used + 1) {
                    let count = active + usize::from(units > 0);
                    if count > branches {
                        continue;
                    }
                    let Some(value) = value else {
                        continue;
                    };
                    let Some(out) = old.out.checked_add(*value) else {
                        continue;
                    };
                    let target = &mut next[(used + units) * width + count];
                    if target.as_ref().is_none_or(|best| out > best.out) {
                        let mut allocation = old.units.clone();
                        allocation.push(units);
                        *target = Some(DpAllocation {
                            out,
                            units: allocation,
                        });
                    }
                }
            }
        }
        previous = next;
    }
    Ok((1..=branches)
        .filter_map(|b| previous[segments * width + b].take().map(|cell| cell.units))
        .collect())
}

async fn multi_pool(
    pools: &[DirectPool],
    source: &ObservedSource<'_>,
    amount: U256,
    config: &SplitConfig,
    baseline_calls: usize,
    deadline: tokio::time::Instant,
    result: &mut SplitResult,
) -> StopReason {
    let segments = config.allocation_segments;
    let mut jobs = Vec::new();
    let mut keys = HashSet::new();
    for (idx, pool) in pools.iter().enumerate() {
        for k in 1..=segments {
            let input = fraction(amount, k, segments);
            if source.cached(pool, input).is_none() && keys.insert(ObservedSource::key(pool, input))
            {
                jobs.push((idx, input));
            }
        }
    }
    let table_stop = fill_quotes(pools, source, jobs, config, baseline_calls, deadline).await;
    let tables = pools
        .iter()
        .map(|pool| {
            (0..=segments)
                .map(|k| {
                    source
                        .cached(pool, fraction(amount, k, segments))
                        .and_then(Result::ok)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let allocations = match allocation_dp(
        &tables,
        segments,
        config.max_branches.min(pools.len()),
        deadline,
    ) {
        Ok(plans) => plans,
        Err(stop) => return stop,
    };
    let mut plans = HashSet::new();
    for units in allocations {
        let inputs = units
            .into_iter()
            .map(|k| fraction(amount, k, segments))
            .collect::<Vec<_>>();
        let Some(sum) = inputs
            .iter()
            .try_fold(U256::ZERO, |sum, x| sum.checked_add(*x))
        else {
            continue;
        };
        let Some(remainder) = amount.checked_sub(sum) else {
            continue;
        };
        if remainder.is_zero() {
            plans.insert(inputs);
            continue;
        }
        // Try each already active branch for the rounding remainder. A failed
        // adjustment never leaves the remainder unspent in an accepted plan.
        for idx in 0..inputs.len() {
            if inputs[idx].is_zero() && inputs.iter().any(|x| !x.is_zero()) {
                continue;
            }
            let mut adjusted = inputs.clone();
            adjusted[idx] += remainder;
            plans.insert(adjusted);
        }
    }
    let mut plans = plans.into_iter().collect::<Vec<_>>();
    plans.sort();
    let mut jobs = Vec::new();
    let mut keys = HashSet::new();
    for inputs in &plans {
        for (idx, input) in inputs.iter().copied().enumerate() {
            if source.cached(&pools[idx], input).is_none()
                && keys.insert(ObservedSource::key(&pools[idx], input))
            {
                jobs.push((idx, input));
            }
        }
    }
    let final_stop = fill_quotes(pools, source, jobs, config, baseline_calls, deadline).await;
    for inputs in plans {
        let mut allocations = Vec::new();
        let mut total = Some(U256::ZERO);
        let mut complete = true;
        for (idx, input) in inputs.into_iter().enumerate() {
            if input.is_zero() {
                continue;
            }
            let Some(Ok(out)) = source.cached(&pools[idx], input) else {
                complete = false;
                break;
            };
            total = total.and_then(|sum| sum.checked_add(out));
            allocations.push(allocation(&pools[idx], input, out));
        }
        if !complete {
            continue;
        }
        result.stats.allocation_plans_evaluated += 1;
        result.stats.evaluated_plans += 1;
        let Some(out) = total else {
            continue;
        };
        result.stats.valid_plans += 1;
        if out > result.amount_out {
            result.kind = if allocations.len() > 1 {
                PlanKind::Split
            } else {
                PlanKind::Direct
            };
            result.amount_out = out;
            result.allocations = allocations;
        }
    }
    if final_stop != StopReason::Completed {
        final_stop
    } else {
        table_stop
    }
}

#[derive(Clone)]
pub struct SplitOptimizer {
    config: SplitConfig,
}

impl SplitOptimizer {
    pub fn config(&self) -> &SplitConfig {
        &self.config
    }
    pub fn new(config: SplitConfig) -> Result<Self, String> {
        if !(2..=1024).contains(&config.coarse_segments)
            || !(2..=1024).contains(&config.refinement_segments)
            || config.refinement_rounds > 8
            || !(2..=32).contains(&config.max_pools)
            || !(1..=64).contains(&config.concurrency)
            || config.timeout_ms > 3_600_000
            || !(2..=8).contains(&config.max_branches)
            || !(2..=128).contains(&config.allocation_segments)
        {
            return Err("segments must be 2..1024, allocation segments 2..128, branches 2..8, rounds <=8, pools 2..32, concurrency 1..64, timeout <=1 hour".into());
        }
        Ok(Self { config })
    }

    pub async fn quote(
        &self,
        router: &Router,
        graph: &Graph,
        source: &(dyn QuoteSource + Send + Sync),
        request: &QuoteRequest,
    ) -> Result<SplitResult, QuoteError> {
        self.quote_measured(router, graph, source, request, None)
            .await
    }

    /// Optional monotonic counter distinguishes real RPC attempts from adapter
    /// calls (V2 calculations/cache hits may make no network requests).
    pub async fn quote_measured(
        &self,
        router: &Router,
        graph: &Graph,
        source: &(dyn QuoteSource + Send + Sync),
        request: &QuoteRequest,
        rpc_requests: Option<&(dyn Fn() -> u64 + Send + Sync)>,
    ) -> Result<SplitResult, QuoteError> {
        self.quote_inner(router, graph, source, request, rpc_requests, None)
            .await
    }

    /// The service reserves a small response margin inside its request timeout.
    /// Additional search ends here, retaining a completed baseline/plan.
    pub async fn quote_until(
        &self,
        router: &Router,
        graph: &Graph,
        source: &(dyn QuoteSource + Send + Sync),
        request: &QuoteRequest,
        deadline: tokio::time::Instant,
    ) -> Result<SplitResult, QuoteError> {
        self.quote_inner(router, graph, source, request, None, Some(deadline))
            .await
    }

    async fn quote_inner(
        &self,
        router: &Router,
        graph: &Graph,
        source: &(dyn QuoteSource + Send + Sync),
        request: &QuoteRequest,
        rpc_requests: Option<&(dyn Fn() -> u64 + Send + Sync)>,
        hard_deadline: Option<tokio::time::Instant>,
    ) -> Result<SplitResult, QuoteError> {
        let rpc_before = rpc_requests.map(|counter| counter());
        let source = ObservedSource {
            inner: source,
            cache: Mutex::new(HashMap::new()),
            calls: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
            hits: AtomicUsize::new(0),
        };
        let started = Instant::now();
        let baseline = router.route(graph, &source, request).await?;
        let rpc_after_baseline = rpc_requests.map(|counter| counter());
        let baseline_calls = source.calls.load(Ordering::Relaxed);
        let baseline_completed = source.completed.load(Ordering::Relaxed);
        let baseline_failed = source.failed.load(Ordering::Relaxed);
        let mut result = SplitResult {
            kind: PlanKind::Baseline,
            amount_out: baseline.amount_out,
            allocations: vec![Allocation {
                amount_in: request.amount_in,
                amount_out: baseline.amount_out,
                hops: baseline.hops.clone(),
            }],
            baseline,
            stats: SearchStats {
                baseline_ms: started.elapsed().as_millis(),
                baseline_quote_calls: baseline_calls,
                ..Default::default()
            },
        };
        let search_started = Instant::now();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(self.config.timeout_ms);
        let deadline = hard_deadline.map_or(deadline, |hard| hard.min(deadline));
        let mut addresses = HashSet::new();
        let mut pools = graph
            .out_edge_indices(request.token_in)
            .iter()
            .filter_map(|&idx| {
                let edge = graph.edge(idx);
                let pool = graph.pool(edge.pool_idx);
                if !matches!(
                    pool.kind,
                    PoolKind::UniswapV2 | PoolKind::UniswapV3 { .. } | PoolKind::ShibaSwapV2
                ) || pool.coins[edge.j] != request.token_out
                    || !addresses.insert(pool.address)
                {
                    return None;
                }
                Some(DirectPool {
                    pool: pool.clone(),
                    i: edge.i,
                    j: edge.j,
                })
            })
            .collect::<Vec<_>>();
        result.stats.eligible_pools = pools.len();
        // Rank by already evaluated full-input quotes, not token symbols or spot
        // rates. The pool cap is explicit in the report; this is not exhaustive.
        pools.sort_by(|a, b| {
            let score = |pool: &DirectPool| {
                source
                    .cached(pool, request.amount_in)
                    .and_then(Result::ok)
                    .unwrap_or(U256::ZERO)
            };
            score(b)
                .cmp(&score(a))
                .then_with(|| a.pool.address.cmp(&b.pool.address))
        });
        pools.truncate(self.config.max_pools);
        result.stats.searched_pools = pools.len();
        let points = grid(U256::ZERO, request.amount_in, self.config.coarse_segments);
        let mut pairs = Vec::new();
        for a in 0..pools.len() {
            for b in a + 1..pools.len() {
                pairs.push(Pair {
                    a,
                    b,
                    points: points.clone(),
                    best: None,
                });
            }
        }
        result.stats.pairs = pairs.len();
        let mut seen = HashSet::new();
        if pairs.is_empty() {
            result.stats.stop_reason = StopReason::NoPairs;
        } else if self.config.max_quote_calls == 0 {
            result.stats.stop_reason = StopReason::QuoteBudget;
        } else if self.config.timeout_ms == 0 {
            result.stats.stop_reason = StopReason::TimeBudget;
        } else {
            for round in 0..=self.config.refinement_rounds {
                if tokio::time::Instant::now() >= deadline {
                    result.stats.stop_reason = StopReason::TimeBudget;
                    break;
                }
                if round > 0 {
                    for pair in &mut pairs {
                        let Some((x, _)) = pair.best else {
                            pair.points.clear();
                            continue;
                        };
                        let idx = pair
                            .points
                            .binary_search(&x)
                            .expect("best point remains in bracket");
                        let left = pair.points[idx.saturating_sub(1)];
                        let right = pair.points[(idx + 1).min(pair.points.len() - 1)];
                        pair.points = grid(left, right, self.config.refinement_segments);
                        pair.points.push(x);
                        pair.points.sort();
                        pair.points.dedup();
                    }
                    result.stats.refinement_rounds = round;
                }
                let mut keys = HashSet::new();
                let mut jobs = Vec::new();
                for pair in &pairs {
                    for &x in &pair.points {
                        for (idx, amount) in [(pair.a, x), (pair.b, request.amount_in - x)] {
                            if source.cached(&pools[idx], amount).is_none()
                                && keys.insert(ObservedSource::key(&pools[idx], amount))
                            {
                                jobs.push((idx, amount));
                            }
                        }
                    }
                }
                let used = source.calls.load(Ordering::Relaxed) - baseline_calls;
                let remaining = self.config.max_quote_calls.saturating_sub(used);
                let truncated = jobs.len() > remaining;
                let source_ref = &source;
                let pools_ref = &pools;
                let mut pending = stream::iter(jobs.into_iter().take(remaining))
                    .map(move |(idx, amount): (usize, U256)| async move {
                        let pool = &pools_ref[idx];
                        let _ = source_ref
                            .amount_out(&pool.pool, pool.i, pool.j, amount)
                            .await;
                    })
                    .buffer_unordered(self.config.concurrency);
                let mut expired = false;
                loop {
                    match tokio::time::timeout_at(deadline, pending.next()).await {
                        Ok(Some(())) => {}
                        Ok(None) => break,
                        Err(_) => {
                            expired = true;
                            break;
                        }
                    }
                }
                drop(pending); // cancel in-flight calls; completed cache entries survive
                evaluate(
                    &mut pairs,
                    &pools,
                    &source,
                    request.amount_in,
                    &mut seen,
                    &mut result,
                );
                if expired {
                    result.stats.stop_reason = StopReason::TimeBudget;
                    break;
                }
                if truncated {
                    result.stats.stop_reason = StopReason::QuoteBudget;
                    break;
                }
            }
        }
        if self.config.max_branches > 2
            && pools.len() > 2
            && result.stats.stop_reason == StopReason::Completed
        {
            result.stats.multi_pool_search = true;
            let stop = multi_pool(
                &pools,
                &source,
                request.amount_in,
                &self.config,
                baseline_calls,
                deadline,
                &mut result,
            )
            .await;
            result.stats.multi_pool_completed = stop == StopReason::Completed;
            result.stats.stop_reason = stop;
        }
        result.stats.search_ms = search_started.elapsed().as_millis();
        result.stats.additional_quote_calls = source.calls.load(Ordering::Relaxed) - baseline_calls;
        result.stats.completed_additional_quotes =
            source.completed.load(Ordering::Relaxed) - baseline_completed;
        result.stats.failed_additional_quotes =
            source.failed.load(Ordering::Relaxed) - baseline_failed;
        result.stats.cache_hits = source.hits.load(Ordering::Relaxed);
        result.stats.baseline_rpc_requests = rpc_before
            .zip(rpc_after_baseline)
            .map(|(a, b)| b.saturating_sub(a));
        result.stats.additional_rpc_requests = rpc_after_baseline
            .zip(rpc_requests.map(|counter| counter()))
            .map(|(a, b)| b.saturating_sub(a));
        Ok(result)
    }
}
