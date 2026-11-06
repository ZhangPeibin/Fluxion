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
use crate::router::{RoutedPath, Router};
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

pub use fluxion_protocol::swap::PlanKind;

impl SplitConfig {
    pub fn multi_pool() -> Self {
        Self {
            max_branches: 4,
            max_quote_calls: 1024,
            ..Self::default()
        }
    }
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
    pub multi_route_search: bool,
    pub multi_route_completed: bool,
    pub route_tree: bool,
    pub route_branches: usize,
    pub route_groups: usize,
    pub allocation_plans_evaluated: usize,
}

#[derive(Debug, Clone)]
pub struct Allocation {
    pub amount_in: U256,
    pub amount_out: U256,
    pub hops: Vec<Hop>,
    /// Prefix branches when multiple pools feed one intermediate node. Each
    /// source's `amount_out` is in the intermediate token.
    pub sources: Vec<Allocation>,
    /// Suffix branches when one intermediate node fans out to multiple
    /// downstream pools. Each child's `amount_in` is in the intermediate token.
    pub children: Vec<Allocation>,
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

#[derive(Clone)]
struct RouteBranch {
    hops: Vec<(Pool, usize, usize)>,
}

/// Probe sizes are full, half, and quarter input. They are exact path quotes,
/// not spot approximations, so a multi-hop path is never ranked by its extra
/// fee alone.
const PROBE_FULL: usize = 0;
const PROBE_HALF: usize = 1;
const PROBE_QUARTER: usize = 2;

#[derive(Clone)]
struct PathCandidate {
    hops: Vec<(Pool, usize, usize)>,
    pool_set: Vec<Address>,
    probe_outputs: [Option<U256>; 3],
}

fn scaled_probe(output: Option<U256>, factor: u64) -> Option<U256> {
    output.map(|out| out.checked_mul(U256::from(factor)).unwrap_or(U256::MAX))
}

/// Compare candidates at a common notional input so a path that looks weak at
/// the full size but scales well (or vice versa) is still retained.
fn probe_score(candidate: &PathCandidate) -> U256 {
    [
        scaled_probe(candidate.probe_outputs[PROBE_FULL], 1),
        scaled_probe(candidate.probe_outputs[PROBE_HALF], 2),
        scaled_probe(candidate.probe_outputs[PROBE_QUARTER], 4),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(U256::ZERO)
}

/// Probe a bounded, depth-diverse set of coarse paths at full, half, and
/// quarter input. A path that reverts at the full amount can still win a
/// smaller branch, so full-size failure never removes it from the split search.
async fn probe_path_candidates(
    paths: Vec<RoutedPath>,
    source: &ObservedSource<'_>,
    amount: U256,
    config: &SplitConfig,
    baseline_calls: usize,
    deadline: tokio::time::Instant,
) -> (Vec<PathCandidate>, StopReason) {
    let limit = config.max_branches.saturating_mul(8).clamp(8, 64);
    let signature = |path: &RoutedPath| {
        path.hops
            .iter()
            .map(|(pool, i, j)| (pool.address, *i, *j))
            .collect::<Vec<_>>()
    };
    let mut seen = HashSet::new();
    let mut paths = paths
        .into_iter()
        .filter(|path| {
            if path.hops.is_empty() {
                return false;
            }
            let unique_pools = path
                .hops
                .iter()
                .map(|(pool, _, _)| pool.address)
                .collect::<HashSet<_>>();
            // Reusing one pool inside a path would quote hops against unchanged
            // reserves and could also alias the same liquidity twice.
            unique_pools.len() == path.hops.len() && seen.insert(signature(path))
        })
        .collect::<Vec<_>>();
    paths.sort_by(|a, b| {
        a.cost
            .total_cmp(&b.cost)
            .then_with(|| a.hops.len().cmp(&b.hops.len()))
    });

    // Keep the cheapest path from every depth first, then fill by coarse cost.
    // Depth diversity prevents a cheap direct-looking path from crowding out
    // every multi-hop candidate before exact fractions are probed.
    let mut chosen = Vec::new();
    let mut chosen_paths = HashSet::new();
    let max_depth = paths.iter().map(|path| path.hops.len()).max().unwrap_or(0);
    for depth in 1..=max_depth {
        if let Some(path) = paths.iter().find(|path| path.hops.len() == depth)
            && chosen_paths.insert(signature(path))
        {
            chosen.push(path.clone());
        }
    }
    for path in paths {
        if chosen.len() >= limit {
            break;
        }
        if chosen_paths.insert(signature(&path)) {
            chosen.push(path);
        }
    }
    if chosen.is_empty() {
        return (Vec::new(), StopReason::Completed);
    }
    let branches = chosen
        .iter()
        .map(|path| RouteBranch {
            hops: path.hops.clone(),
        })
        .collect::<Vec<_>>();
    let full = amount;
    let half = amount / U256::from(2u8);
    let quarter = amount / U256::from(4u8);
    let mut jobs = Vec::new();
    let mut keys = HashSet::new();
    for idx in 0..branches.len() {
        for probe in [full, half, quarter] {
            if !probe.is_zero() && keys.insert((idx, probe)) {
                jobs.push((idx, probe));
            }
        }
    }
    let (results, stop) =
        fill_route_quotes(&branches, source, jobs, config, baseline_calls, deadline).await;
    let mut candidates = branches
        .into_iter()
        .enumerate()
        .map(|(idx, branch)| {
            let probe = |probe: U256| {
                results
                    .get(&(idx, probe))
                    .and_then(|result| result.as_ref().ok().copied())
            };
            PathCandidate {
                pool_set: branch
                    .hops
                    .iter()
                    .map(|(pool, _, _)| pool.address)
                    .collect(),
                hops: branch.hops,
                probe_outputs: [probe(full), probe(half), probe(quarter)],
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        probe_score(b)
            .cmp(&probe_score(a))
            .then_with(|| a.hops.len().cmp(&b.hops.len()))
    });
    (candidates, stop)
}

/// Independent quotes may only be summed when the paths do not share a pool.
/// Keep the best path and the best disjoint multi-hop path, then fill by probe
/// output. The DP may still assign zero to any branch; a worse branch cannot
/// make the returned split worse.
fn select_route_branches(candidates: Vec<PathCandidate>, limit: usize) -> Vec<RouteBranch> {
    let disjoint = |candidate: &PathCandidate, used: &HashSet<Address>| {
        candidate.pool_set.iter().all(|pool| !used.contains(pool))
    };
    let mut chosen = Vec::new();
    let mut used = HashSet::new();
    if let Some(first) = candidates.first() {
        used.extend(first.pool_set.iter().copied());
        chosen.push(first.clone());
    }
    if limit >= 2
        && let Some(multi_hop) = candidates
            .iter()
            .find(|candidate| candidate.hops.len() > 1 && disjoint(candidate, &used))
    {
        used.extend(multi_hop.pool_set.iter().copied());
        chosen.push(multi_hop.clone());
    }
    for candidate in &candidates {
        if chosen.len() >= limit {
            break;
        }
        if chosen
            .iter()
            .any(|selected| selected.hops == candidate.hops)
            || !disjoint(candidate, &used)
        {
            continue;
        }
        used.extend(candidate.pool_set.iter().copied());
        chosen.push(candidate.clone());
    }
    chosen
        .into_iter()
        .map(|candidate| RouteBranch {
            hops: candidate.hops,
        })
        .collect()
}

async fn route_out(
    branch: &RouteBranch,
    amount: U256,
    source: &ObservedSource<'_>,
) -> Result<U256, QuoteError> {
    let mut current = amount;
    for (pool, i, j) in &branch.hops {
        current = source.amount_out(pool, *i, *j, current).await?;
    }
    Ok(current)
}

async fn route_allocation(
    branch: &RouteBranch,
    amount_in: U256,
    source: &ObservedSource<'_>,
) -> Result<Allocation, QuoteError> {
    let mut current = amount_in;
    let mut hops = Vec::with_capacity(branch.hops.len());
    for (pool, i, j) in &branch.hops {
        let out = source.amount_out(pool, *i, *j, current).await?;
        hops.push(Hop {
            pool: pool.clone(),
            i: *i,
            j: *j,
            token_in: pool.coins[*i],
            token_out: pool.coins[*j],
            amount_in: current,
            amount_out: out,
        });
        current = out;
    }
    Ok(Allocation {
        amount_in,
        amount_out: current,
        hops,
        sources: Vec::new(),
        children: Vec::new(),
    })
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
        sources: Vec::new(),
        children: Vec::new(),
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

async fn fill_route_quotes(
    branches: &[RouteBranch],
    source: &ObservedSource<'_>,
    jobs: Vec<(usize, U256)>,
    config: &SplitConfig,
    baseline_calls: usize,
    deadline: tokio::time::Instant,
) -> (HashMap<(usize, U256), Result<U256, QuoteError>>, StopReason) {
    let results = Mutex::new(HashMap::new());
    if tokio::time::Instant::now() >= deadline {
        return (results.into_inner().unwrap(), StopReason::TimeBudget);
    }
    let remaining = config
        .max_quote_calls
        .saturating_sub(source.calls.load(Ordering::Relaxed) - baseline_calls);
    let truncated = jobs.len() > remaining;
    let mut pending = stream::iter(jobs.into_iter().take(remaining))
        .map(|(idx, amount): (usize, U256)| {
            let results = &results;
            async move {
                let out = route_out(&branches[idx], amount, source).await;
                results.lock().unwrap().insert((idx, amount), out);
            }
        })
        .buffer_unordered(config.concurrency);
    let stop = loop {
        match tokio::time::timeout_at(deadline, pending.next()).await {
            Ok(Some(())) => {}
            Ok(None) => break None,
            Err(_) => break Some(StopReason::TimeBudget),
        }
    };
    let stop = stop.unwrap_or(if truncated {
        StopReason::QuoteBudget
    } else {
        StopReason::Completed
    });
    drop(pending);
    (results.into_inner().unwrap(), stop)
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

/// Best discrete plan for one resource-allocation node. Each table is indexed
/// by input units; `None` marks an infeasible sample.
fn best_units(
    tables: &[Vec<Option<U256>>],
    segments: usize,
    branches: usize,
    deadline: tokio::time::Instant,
) -> Option<(U256, Vec<usize>)> {
    let plans = allocation_dp(tables, segments, branches, deadline).ok()?;
    let mut best: Option<(U256, Vec<usize>)> = None;
    for units in plans {
        let mut total = U256::ZERO;
        let mut complete = true;
        for (idx, used) in units.iter().copied().enumerate() {
            if used == 0 {
                continue;
            }
            let Some(value) = tables.get(idx).and_then(|table| table.get(used)) else {
                complete = false;
                break;
            };
            let Some(value) = value else {
                complete = false;
                break;
            };
            let Some(sum) = total.checked_add(*value) else {
                complete = false;
                break;
            };
            total = sum;
        }
        if complete && best.as_ref().is_none_or(|(old, _)| total > *old) {
            best = Some((total, units));
        }
    }
    best
}

type RouteQuotes = HashMap<(usize, U256), Result<U256, QuoteError>>;

/// Sum a plan only when every non-zero branch has a completed exact quote.
fn plan_total(inputs: &[U256], quotes: &RouteQuotes) -> Option<U256> {
    let mut total = U256::ZERO;
    let mut active = false;
    for (idx, input) in inputs.iter().copied().enumerate() {
        if input.is_zero() {
            continue;
        }
        active = true;
        let out = quotes.get(&(idx, input))?.as_ref().ok()?;
        total = total.checked_add(*out)?;
    }
    active.then_some(total)
}

async fn apply_route_plan(
    branches: &[RouteBranch],
    source: &ObservedSource<'_>,
    inputs: &[U256],
    quotes: &RouteQuotes,
    result: &mut SplitResult,
) -> Option<U256> {
    let total = plan_total(inputs, quotes)?;
    let mut allocations = Vec::new();
    for (idx, input) in inputs.iter().copied().enumerate() {
        if input.is_zero() {
            continue;
        }
        let allocation = route_allocation(&branches[idx], input, source).await.ok()?;
        allocations.push(allocation);
    }
    if allocations.is_empty() {
        return None;
    }
    result.stats.allocation_plans_evaluated += 1;
    result.stats.evaluated_plans += 1;
    result.stats.valid_plans += 1;
    if total > result.amount_out {
        result.kind = if allocations.len() > 1 {
            PlanKind::Split
        } else {
            PlanKind::Direct
        };
        result.amount_out = total;
        result.allocations = allocations;
    }
    Some(total)
}

/// Move a small slice between two branches. `cell_segments` controls the
/// starting cell size and `steps` controls sub-cell resolution.
fn refined_plans_with(inputs: &[U256], cell_segments: usize, steps: usize) -> Vec<Vec<U256>> {
    let mut plans = Vec::new();
    for a in 0..inputs.len() {
        for b in (a + 1)..inputs.len() {
            let Some(combined) = inputs[a].checked_add(inputs[b]) else {
                continue;
            };
            if combined.is_zero() {
                continue;
            }
            let cell = fraction(combined, 1, cell_segments.max(1));
            if cell.is_zero() {
                continue;
            }
            for step in 1..=steps.max(1) {
                let delta = fraction(cell, step, steps.max(1));
                if delta.is_zero() {
                    continue;
                }
                if inputs[a] >= delta {
                    let mut plan = inputs.to_vec();
                    plan[a] -= delta;
                    plan[b] += delta;
                    plans.push(plan);
                }
                if inputs[b] >= delta {
                    let mut plan = inputs.to_vec();
                    plan[a] += delta;
                    plan[b] -= delta;
                    plans.push(plan);
                }
            }
        }
    }
    plans.sort();
    plans.dedup();
    plans
}

fn refined_plans(inputs: &[U256], config: &SplitConfig) -> Vec<Vec<U256>> {
    refined_plans_with(
        inputs,
        config.allocation_segments,
        config.refinement_segments,
    )
}

/// Outer-plan variants with both one-cell moves and small moves relative to the
/// whole request. The latter can activate 1%–5% branches that a coarse DP grid
/// cannot represent directly.
fn adaptive_refined_plans(inputs: &[U256], amount: U256, config: &SplitConfig) -> Vec<Vec<U256>> {
    let mut plans = refined_plans(inputs, config);
    for a in 0..inputs.len() {
        for b in (a + 1)..inputs.len() {
            for denominator in [64usize, 32, 16, 8] {
                let delta = fraction(amount, 1, denominator);
                if delta.is_zero() {
                    continue;
                }
                if inputs[a] >= delta {
                    let mut plan = inputs.to_vec();
                    plan[a] -= delta;
                    plan[b] += delta;
                    plans.push(plan);
                }
                if inputs[b] >= delta {
                    let mut plan = inputs.to_vec();
                    plan[a] += delta;
                    plan[b] -= delta;
                    plans.push(plan);
                }
            }
        }
    }
    plans.sort();
    plans.dedup();
    plans
}

async fn refine_route_plan(
    branches: &[RouteBranch],
    source: &ObservedSource<'_>,
    start: (U256, Vec<U256>),
    config: &SplitConfig,
    baseline_calls: usize,
    deadline: tokio::time::Instant,
    result: &mut SplitResult,
) -> StopReason {
    let (mut output, mut inputs) = start;
    let mut stop = StopReason::Completed;
    for _ in 0..config.refinement_rounds {
        let plans = refined_plans(&inputs, config);
        if plans.is_empty() {
            break;
        }
        let mut jobs = Vec::new();
        let mut keys = HashSet::new();
        for plan in &plans {
            for (idx, input) in plan.iter().copied().enumerate() {
                if !input.is_zero() && keys.insert((idx, input)) {
                    jobs.push((idx, input));
                }
            }
        }
        let (quotes, quote_stop) =
            fill_route_quotes(branches, source, jobs, config, baseline_calls, deadline).await;
        if quote_stop != StopReason::Completed {
            stop = quote_stop;
        }
        let mut improved = false;
        for plan in plans {
            if let Some(total) = apply_route_plan(branches, source, &plan, &quotes, result).await
                && total > output
            {
                output = total;
                inputs = plan;
                improved = true;
            }
        }
        if !improved || stop != StopReason::Completed {
            break;
        }
    }
    stop
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

async fn multi_route(
    branches: &[RouteBranch],
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
    for (idx, _) in branches.iter().enumerate() {
        for k in 1..=segments {
            let input = fraction(amount, k, segments);
            if !input.is_zero() && keys.insert((idx, input)) {
                jobs.push((idx, input));
            }
        }
    }
    let (mut results, table_stop) =
        fill_route_quotes(branches, source, jobs, config, baseline_calls, deadline).await;
    let tables = branches
        .iter()
        .enumerate()
        .map(|(idx, _)| {
            (0..=segments)
                .map(|k| {
                    if k == 0 {
                        Some(U256::ZERO)
                    } else {
                        results
                            .get(&(idx, fraction(amount, k, segments)))
                            .and_then(|result| result.as_ref().ok().copied())
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let allocations = match allocation_dp(
        &tables,
        segments,
        config.max_branches.min(branches.len()),
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
            if input.is_zero() {
                continue;
            }
            if !results.contains_key(&(idx, input)) && keys.insert((idx, input)) {
                jobs.push((idx, input));
            }
        }
    }
    let (more, final_stop) =
        fill_route_quotes(branches, source, jobs, config, baseline_calls, deadline).await;
    results.extend(more);
    let mut best_dp: Option<(U256, Vec<U256>)> = None;
    for inputs in plans {
        if let Some(out) = apply_route_plan(branches, source, &inputs, &results, result).await
            && best_dp.as_ref().is_none_or(|(best, _)| out > *best)
        {
            best_dp = Some((out, inputs));
        }
    }
    let refine_stop = match best_dp {
        Some(start) => {
            refine_route_plan(
                branches,
                source,
                start,
                config,
                baseline_calls,
                deadline,
                result,
            )
            .await
        }
        None => StopReason::Completed,
    };
    if final_stop != StopReason::Completed {
        final_stop
    } else if refine_stop != StopReason::Completed {
        refine_stop
    } else {
        table_stop
    }
}

type EdgeSignature = (Address, usize, usize);
type SuffixSignature = Vec<EdgeSignature>;

#[derive(Clone)]
struct RouteGroup {
    /// Parallel first-hop edges into the same intermediate token. At most one
    /// is used for a given input amount.
    prefix_idx: Vec<usize>,
    /// Suffix alternatives after that intermediate token.
    suffix_idx: Vec<usize>,
}

#[derive(Clone)]
struct GroupResult {
    input: U256,
    output: U256,
    hops: Vec<Hop>,
    sources: Vec<Allocation>,
    children: Vec<Allocation>,
}

/// True when two leaves share a prefix or a suffix but are not identical.
/// That coupling needs the route-tree allocator; unrelated linear paths keep
/// the simpler branch DP.
fn needs_route_tree(paths: &[RoutedPath]) -> bool {
    let mut first_to_suffixes: HashMap<EdgeSignature, HashSet<SuffixSignature>> = HashMap::new();
    let mut suffix_to_firsts: HashMap<SuffixSignature, HashSet<EdgeSignature>> = HashMap::new();
    for path in paths {
        if path.hops.len() < 2 {
            continue;
        }
        let (pool, i, j) = &path.hops[0];
        let first = (pool.address, *i, *j);
        let suffix = path.hops[1..]
            .iter()
            .map(|(pool, i, j)| (pool.address, *i, *j))
            .collect::<Vec<_>>();
        if first_to_suffixes
            .entry(first)
            .or_default()
            .insert(suffix.clone())
            && first_to_suffixes
                .get(&first)
                .is_some_and(|seen| seen.len() >= 2)
        {
            return true;
        }
        if suffix_to_firsts.entry(suffix).or_default().insert(first)
            && suffix_to_firsts.values().any(|firsts| firsts.len() >= 2)
        {
            return true;
        }
    }
    false
}

/// Merge linear coarse paths into first-hop groups. Sharing a prefix inside a
/// group is allowed; different groups stay pool-disjoint.
fn build_route_groups(
    paths: Vec<RoutedPath>,
    config: &SplitConfig,
    graph: &Graph,
    token_out: Address,
) -> (Vec<RouteGroup>, Vec<RouteBranch>) {
    #[derive(Clone, Copy, Hash, PartialEq, Eq)]
    enum GroupKey {
        Direct(Address),
        Token(Address),
    }

    struct TempGroup {
        prefixes: Vec<(Pool, usize, usize)>,
        suffixes: Vec<Vec<(Pool, usize, usize)>>,
        seen_prefixes: HashSet<EdgeSignature>,
        seen_suffixes: HashSet<SuffixSignature>,
        cost: f64,
    }

    let mut seen_paths = HashSet::new();
    let mut entries = Vec::new();
    for path in paths {
        if path.hops.is_empty() {
            continue;
        }
        let unique_pools = path
            .hops
            .iter()
            .map(|(pool, _, _)| pool.address)
            .collect::<HashSet<_>>();
        if unique_pools.len() != path.hops.len() {
            continue;
        }
        let signature = path
            .hops
            .iter()
            .map(|(pool, i, j)| (pool.address, *i, *j))
            .collect::<Vec<_>>();
        if seen_paths.insert(signature) {
            entries.push(path);
        }
    }
    entries.sort_by(|a, b| a.cost.total_cmp(&b.cost));

    let suffix_limit = config.max_branches.clamp(4, 8);
    let mut grouped: HashMap<GroupKey, TempGroup> = HashMap::new();
    for path in entries {
        let prefix = path.hops[0].clone();
        let suffix = &path.hops[1..];
        let key = if suffix.is_empty() {
            GroupKey::Direct(prefix.0.address)
        } else {
            GroupKey::Token(prefix.0.coins[prefix.2])
        };
        let group = grouped.entry(key).or_insert_with(|| TempGroup {
            prefixes: Vec::new(),
            suffixes: Vec::new(),
            seen_prefixes: HashSet::new(),
            seen_suffixes: HashSet::new(),
            cost: path.cost,
        });
        group.cost = group.cost.min(path.cost);
        let prefix_signature = (prefix.0.address, prefix.1, prefix.2);
        if !group.seen_prefixes.contains(&prefix_signature) {
            group.seen_prefixes.insert(prefix_signature);
            group.prefixes.push(prefix);
        }
        if group.suffixes.len() >= suffix_limit {
            continue;
        }
        let signature = suffix
            .iter()
            .map(|(pool, i, j)| (pool.address, *i, *j))
            .collect::<Vec<_>>();
        if group.seen_suffixes.insert(signature) {
            group.suffixes.push(suffix.to_vec());
        }
    }

    // Router candidates can still miss direct suffix pools when their coarse
    // rank is poor. Add every direct edge from each intermediate token to the
    // target so exact suffix quotes, not mid-price, decide.
    for group in grouped.values_mut() {
        if group.suffixes.is_empty() {
            continue;
        }
        let Some(prefix) = group.prefixes.first() else {
            continue;
        };
        let intermediate = prefix.0.coins[prefix.2];
        for &edge_idx in graph.out_edge_indices(intermediate) {
            if group.suffixes.len() >= suffix_limit {
                break;
            }
            let edge = graph.edge(edge_idx);
            let pool = graph.pool(edge.pool_idx);
            if pool.coins[edge.j] != token_out {
                continue;
            }
            let suffix = vec![(pool.clone(), edge.i, edge.j)];
            let signature = vec![(pool.address, edge.i, edge.j)];
            if group.seen_suffixes.insert(signature) {
                group.suffixes.push(suffix);
            }
        }
    }

    let mut groups = grouped.into_values().collect::<Vec<_>>();
    groups.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    let group_limit = config.max_branches.saturating_mul(2).clamp(8, 32);
    let mut used = HashSet::new();
    let mut selected = Vec::new();
    for group in groups {
        let mut pools = HashSet::new();
        for prefix in &group.prefixes {
            pools.insert(prefix.0.address);
        }
        for suffix in &group.suffixes {
            pools.extend(suffix.iter().map(|(pool, _, _)| pool.address));
        }
        if !pools.is_disjoint(&used) {
            continue;
        }
        used.extend(pools);
        selected.push(group);
        if selected.len() >= group_limit {
            break;
        }
    }

    let mut branches = Vec::new();
    let mut result = Vec::new();
    for group in selected {
        let prefix_idx = group
            .prefixes
            .into_iter()
            .map(|prefix| {
                branches.push(RouteBranch { hops: vec![prefix] });
                branches.len() - 1
            })
            .collect::<Vec<_>>();
        let suffix_idx = group
            .suffixes
            .into_iter()
            .map(|suffix| {
                branches.push(RouteBranch { hops: suffix });
                branches.len() - 1
            })
            .collect::<Vec<_>>();
        result.push(RouteGroup {
            prefix_idx,
            suffix_idx,
        });
    }
    (result, branches)
}

async fn ensure_route_quotes(
    branches: &[RouteBranch],
    jobs: Vec<(usize, U256)>,
    quotes: &mut RouteQuotes,
    source: &ObservedSource<'_>,
    config: &SplitConfig,
    baseline_calls: usize,
    deadline: tokio::time::Instant,
) -> StopReason {
    let mut seen = HashSet::new();
    let jobs = jobs
        .into_iter()
        .filter(|(idx, amount)| {
            !amount.is_zero()
                && !quotes.contains_key(&(*idx, *amount))
                && seen.insert((*idx, *amount))
        })
        .collect::<Vec<_>>();
    if jobs.is_empty() {
        return StopReason::Completed;
    }
    let (more, stop) =
        fill_route_quotes(branches, source, jobs, config, baseline_calls, deadline).await;
    quotes.extend(more);
    stop
}

fn merge_stop(stop: &mut StopReason, next: StopReason) {
    if next != StopReason::Completed {
        *stop = next;
    }
}

/// Exact quote of one first-hop group: prefix output is split over all suffix
/// alternatives, with an integer remainder retried on the active suffixes.
#[allow(clippy::too_many_arguments)]
async fn apply_integer_remainder(
    branch_idx: &[usize],
    total_input: U256,
    inputs: &mut Vec<U256>,
    best_out: &mut U256,
    quotes: &mut RouteQuotes,
    branches: &[RouteBranch],
    source: &ObservedSource<'_>,
    config: &SplitConfig,
    baseline_calls: usize,
    deadline: tokio::time::Instant,
    stop: &mut StopReason,
) {
    let Some(sum) = inputs
        .iter()
        .try_fold(U256::ZERO, |sum, value| sum.checked_add(*value))
    else {
        return;
    };
    let Some(remainder) = total_input.checked_sub(sum) else {
        return;
    };
    if remainder.is_zero() {
        return;
    }
    let mut variants = Vec::new();
    for idx in 0..inputs.len() {
        if inputs[idx].is_zero() && inputs.iter().any(|value| !value.is_zero()) {
            continue;
        }
        let mut adjusted = inputs.clone();
        adjusted[idx] += remainder;
        variants.push(adjusted);
    }
    let jobs = variants
        .iter()
        .flat_map(|variant| variant.iter().copied().enumerate())
        .filter(|(idx, amount)| {
            !amount.is_zero() && !quotes.contains_key(&(branch_idx[*idx], *amount))
        })
        .map(|(idx, amount)| (branch_idx[idx], amount))
        .collect::<Vec<_>>();
    let quote_stop = ensure_route_quotes(
        branches,
        jobs,
        quotes,
        source,
        config,
        baseline_calls,
        deadline,
    )
    .await;
    merge_stop(stop, quote_stop);
    for variant in variants {
        let mut total = U256::ZERO;
        let mut complete = true;
        for (idx, amount) in variant.iter().copied().enumerate() {
            if amount.is_zero() {
                continue;
            }
            match quotes
                .get(&(branch_idx[idx], amount))
                .and_then(|result| result.as_ref().ok())
            {
                Some(out) => {
                    let Some(sum) = total.checked_add(*out) else {
                        complete = false;
                        break;
                    };
                    total = sum;
                }
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if complete && total >= *best_out {
            *best_out = total;
            *inputs = variant;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn quote_group(
    group: &RouteGroup,
    branches: &[RouteBranch],
    input: U256,
    inner_segments: usize,
    refine: bool,
    quotes: &mut RouteQuotes,
    source: &ObservedSource<'_>,
    config: &SplitConfig,
    baseline_calls: usize,
    deadline: tokio::time::Instant,
    stop: &mut StopReason,
) -> Option<GroupResult> {
    if input.is_zero() {
        return None;
    }

    let (prefix_out, prefix_allocations) = if group.prefix_idx.len() == 1 {
        let prefix_idx = group.prefix_idx[0];
        let quote_stop = ensure_route_quotes(
            branches,
            vec![(prefix_idx, input)],
            quotes,
            source,
            config,
            baseline_calls,
            deadline,
        )
        .await;
        merge_stop(stop, quote_stop);
        let out = quotes.get(&(prefix_idx, input))?.as_ref().ok().copied()?;
        let allocation = route_allocation(&branches[prefix_idx], input, source)
            .await
            .ok()?;
        (out, vec![allocation])
    } else {
        let prefix_segments = inner_segments;
        let jobs = group
            .prefix_idx
            .iter()
            .flat_map(|&prefix_idx| {
                (1..=prefix_segments)
                    .map(move |step| (prefix_idx, fraction(input, step, prefix_segments)))
            })
            .collect::<Vec<_>>();
        let quote_stop = ensure_route_quotes(
            branches,
            jobs,
            quotes,
            source,
            config,
            baseline_calls,
            deadline,
        )
        .await;
        merge_stop(stop, quote_stop);
        let tables = group
            .prefix_idx
            .iter()
            .map(|&prefix_idx| {
                (0..=prefix_segments)
                    .map(|step| {
                        if step == 0 {
                            Some(U256::ZERO)
                        } else {
                            let amount = fraction(input, step, prefix_segments);
                            if amount.is_zero() {
                                Some(U256::ZERO)
                            } else {
                                quotes
                                    .get(&(prefix_idx, amount))
                                    .and_then(|result| result.as_ref().ok().copied())
                            }
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        match best_units(
            &tables,
            prefix_segments,
            config.max_branches.min(tables.len()),
            deadline,
        ) {
            Some((mut best, units)) => {
                let mut inputs = units
                    .iter()
                    .map(|&used| fraction(input, used, prefix_segments))
                    .collect::<Vec<_>>();
                apply_integer_remainder(
                    &group.prefix_idx,
                    input,
                    &mut inputs,
                    &mut best,
                    quotes,
                    branches,
                    source,
                    config,
                    baseline_calls,
                    deadline,
                    stop,
                )
                .await;
                let mut allocations = Vec::new();
                let mut total = U256::ZERO;
                for (idx, amount) in inputs.into_iter().enumerate() {
                    if amount.is_zero() {
                        continue;
                    }
                    let allocation =
                        route_allocation(&branches[group.prefix_idx[idx]], amount, source)
                            .await
                            .ok()?;
                    total = total.checked_add(allocation.amount_out)?;
                    allocations.push(allocation);
                }
                (total, allocations)
            }
            None => {
                // A partial quote table can leave the prefix DP infeasible.
                // Fall back to the best single prefix for this input.
                let quote_stop = ensure_route_quotes(
                    branches,
                    group
                        .prefix_idx
                        .iter()
                        .map(|&prefix_idx| (prefix_idx, input))
                        .collect(),
                    quotes,
                    source,
                    config,
                    baseline_calls,
                    deadline,
                )
                .await;
                merge_stop(stop, quote_stop);
                let mut chosen: Option<(U256, Allocation)> = None;
                for &prefix_idx in &group.prefix_idx {
                    let Some(out) = quotes
                        .get(&(prefix_idx, input))
                        .and_then(|result| result.as_ref().ok().copied())
                    else {
                        continue;
                    };
                    if chosen.as_ref().is_none_or(|(best, _)| out > *best) {
                        let allocation = route_allocation(&branches[prefix_idx], input, source)
                            .await
                            .ok()?;
                        chosen = Some((out, allocation));
                    }
                }
                let (out, allocation) = chosen?;
                (out, vec![allocation])
            }
        }
    };

    if group.suffix_idx.is_empty() {
        return Some(if prefix_allocations.len() == 1 {
            GroupResult {
                input,
                output: prefix_out,
                hops: prefix_allocations.into_iter().next().unwrap().hops,
                sources: Vec::new(),
                children: Vec::new(),
            }
        } else {
            GroupResult {
                input,
                output: prefix_out,
                hops: Vec::new(),
                sources: prefix_allocations,
                children: Vec::new(),
            }
        });
    }

    let mut jobs = Vec::new();
    for &suffix_idx in &group.suffix_idx {
        for step in 1..=inner_segments {
            let amount = fraction(prefix_out, step, inner_segments);
            jobs.push((suffix_idx, amount));
        }
    }
    let quote_stop = ensure_route_quotes(
        branches,
        jobs,
        quotes,
        source,
        config,
        baseline_calls,
        deadline,
    )
    .await;
    merge_stop(stop, quote_stop);

    let tables = group
        .suffix_idx
        .iter()
        .map(|&suffix_idx| {
            (0..=inner_segments)
                .map(|step| {
                    if step == 0 {
                        Some(U256::ZERO)
                    } else {
                        let amount = fraction(prefix_out, step, inner_segments);
                        if amount.is_zero() {
                            Some(U256::ZERO)
                        } else {
                            quotes
                                .get(&(suffix_idx, amount))
                                .and_then(|result| result.as_ref().ok().copied())
                        }
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let (mut best_out, units) = best_units(
        &tables,
        inner_segments,
        config.max_branches.min(tables.len()),
        deadline,
    )?;
    let mut inputs = units
        .iter()
        .map(|&used| fraction(prefix_out, used, inner_segments))
        .collect::<Vec<_>>();
    apply_integer_remainder(
        &group.suffix_idx,
        prefix_out,
        &mut inputs,
        &mut best_out,
        quotes,
        branches,
        source,
        config,
        baseline_calls,
        deadline,
        stop,
    )
    .await;

    // Refine the suffix split within one DP cell. This is bounded and only
    // uses exact quotes; table construction skips it to keep RPC use low.
    if refine {
        for _ in 0..config.refinement_rounds {
            let variants = refined_plans_with(&inputs, inner_segments, config.refinement_segments);
            if variants.is_empty() || tokio::time::Instant::now() >= deadline {
                break;
            }
            let jobs = variants
                .iter()
                .take(32)
                .flat_map(|variant| variant.iter().copied().enumerate())
                .filter(|(idx, amount)| {
                    !amount.is_zero() && !quotes.contains_key(&(group.suffix_idx[*idx], *amount))
                })
                .map(|(idx, amount)| (group.suffix_idx[idx], amount))
                .collect::<Vec<_>>();
            let quote_stop = ensure_route_quotes(
                branches,
                jobs,
                quotes,
                source,
                config,
                baseline_calls,
                deadline,
            )
            .await;
            merge_stop(stop, quote_stop);
            let mut improved = false;
            for variant in variants.into_iter().take(32) {
                let mut total = U256::ZERO;
                let mut complete = true;
                for (idx, amount) in variant.iter().copied().enumerate() {
                    if amount.is_zero() {
                        continue;
                    }
                    match quotes
                        .get(&(group.suffix_idx[idx], amount))
                        .and_then(|result| result.as_ref().ok())
                    {
                        Some(out) => {
                            let Some(sum) = total.checked_add(*out) else {
                                complete = false;
                                break;
                            };
                            total = sum;
                        }
                        None => {
                            complete = false;
                            break;
                        }
                    }
                }
                if complete && total > best_out {
                    best_out = total;
                    inputs = variant;
                    improved = true;
                }
            }
            if !improved || *stop != StopReason::Completed {
                break;
            }
        }
    }

    let mut suffix_allocations = Vec::new();
    let mut total = U256::ZERO;
    for (idx, amount) in inputs.into_iter().enumerate() {
        if amount.is_zero() {
            continue;
        }
        let allocation = route_allocation(&branches[group.suffix_idx[idx]], amount, source)
            .await
            .ok()?;
        total = total.checked_add(allocation.amount_out)?;
        suffix_allocations.push(allocation);
    }

    let multi_source = prefix_allocations.len() > 1;
    let multi_suffix = suffix_allocations.len() > 1;
    if !multi_source && !multi_suffix {
        let prefix = prefix_allocations.into_iter().next()?;
        let suffix = suffix_allocations.into_iter().next()?;
        let mut hops = prefix.hops;
        hops.extend(suffix.hops);
        Some(GroupResult {
            input,
            output: total,
            hops,
            sources: Vec::new(),
            children: Vec::new(),
        })
    } else if !multi_source {
        Some(GroupResult {
            input,
            output: total,
            hops: prefix_allocations.into_iter().next()?.hops,
            sources: Vec::new(),
            children: suffix_allocations,
        })
    } else {
        Some(GroupResult {
            input,
            output: total,
            hops: Vec::new(),
            sources: prefix_allocations,
            children: suffix_allocations,
        })
    }
}

#[allow(clippy::too_many_arguments)]
async fn eval_group_plan(
    groups: &[RouteGroup],
    branches: &[RouteBranch],
    inputs: &[U256],
    inner_segments: usize,
    refine: bool,
    cache: &mut HashMap<(usize, U256, bool), Option<GroupResult>>,
    quotes: &mut RouteQuotes,
    source: &ObservedSource<'_>,
    config: &SplitConfig,
    baseline_calls: usize,
    deadline: tokio::time::Instant,
    stop: &mut StopReason,
) -> Option<(U256, Vec<GroupResult>)> {
    let mut total = U256::ZERO;
    let mut results = Vec::new();
    for (idx, input) in inputs.iter().copied().enumerate() {
        let group = groups.get(idx)?;
        if input.is_zero() {
            continue;
        }
        let result = match cache.get(&(idx, input, refine)).cloned() {
            Some(result) => result,
            None => {
                let result = quote_group(
                    group,
                    branches,
                    input,
                    inner_segments,
                    refine,
                    quotes,
                    source,
                    config,
                    baseline_calls,
                    deadline,
                    stop,
                )
                .await;
                cache.insert((idx, input, refine), result.clone());
                result
            }
        }?;
        total = total.checked_add(result.output)?;
        results.push(result);
    }
    Some((total, results))
}

fn apply_group_plan(results: Vec<GroupResult>, total: U256, result: &mut SplitResult) {
    if total <= result.amount_out {
        return;
    }
    let allocations = results
        .into_iter()
        .map(|group| Allocation {
            amount_in: group.input,
            amount_out: group.output,
            hops: group.hops,
            sources: group.sources,
            children: group.children,
        })
        .collect::<Vec<_>>();
    result.kind = if allocations.len() > 1
        || allocations
            .iter()
            .any(|allocation| !allocation.sources.is_empty() || !allocation.children.is_empty())
    {
        PlanKind::Split
    } else {
        PlanKind::Direct
    };
    result.amount_out = total;
    result.allocations = allocations;
}

/// Tree allocation: the outer DP splits input across first hops; each first-hop
/// group runs its own suffix DP. A shared prefix is therefore quoted once and
/// its output can fan out to multiple suffix pools.
#[allow(clippy::too_many_arguments)]
async fn multi_route_tree(
    paths: Vec<RoutedPath>,
    graph: &Graph,
    source: &ObservedSource<'_>,
    amount: U256,
    token_out: Address,
    config: &SplitConfig,
    baseline_calls: usize,
    deadline: tokio::time::Instant,
    result: &mut SplitResult,
) -> StopReason {
    let (groups, branches) = build_route_groups(paths, config, graph, token_out);
    if groups.is_empty() {
        return StopReason::Completed;
    }
    result.stats.route_tree = true;
    result.stats.route_branches = groups.len();
    result.stats.route_groups = groups.len();
    let segments = config.allocation_segments;
    let inner_segments = config.allocation_segments.clamp(2, 8);
    let mut quotes = RouteQuotes::new();
    let mut cache = HashMap::new();
    let mut stop = StopReason::Completed;

    let mut tables = Vec::new();
    for (idx, group) in groups.iter().enumerate() {
        let mut row = vec![None; segments + 1];
        row[0] = Some(U256::ZERO);
        for (step, cell) in row.iter_mut().enumerate().take(segments + 1).skip(1) {
            let input = fraction(amount, step, segments);
            if input.is_zero() {
                *cell = Some(U256::ZERO);
                continue;
            }
            if tokio::time::Instant::now() >= deadline {
                merge_stop(&mut stop, StopReason::TimeBudget);
                break;
            }
            let evaluated = quote_group(
                group,
                &branches,
                input,
                inner_segments,
                false,
                &mut quotes,
                source,
                config,
                baseline_calls,
                deadline,
                &mut stop,
            )
            .await;
            if let Some(evaluated) = &evaluated {
                *cell = Some(evaluated.output);
            }
            cache.insert((idx, input, false), evaluated);
        }
        tables.push(row);
        if stop == StopReason::TimeBudget {
            break;
        }
    }
    if tables.len() != groups.len() {
        // Keep already-computed groups; the outer DP can still use them.
        while tables.len() < groups.len() {
            tables.push(vec![None; segments + 1]);
        }
    }

    let Some((dp_total, units)) = best_units(
        &tables,
        segments,
        config.max_branches.min(groups.len()),
        deadline,
    ) else {
        return stop;
    };
    let mut inputs = units
        .iter()
        .map(|&used| fraction(amount, used, segments))
        .collect::<Vec<_>>();
    let Some(sum) = inputs
        .iter()
        .try_fold(U256::ZERO, |sum, value| sum.checked_add(*value))
    else {
        return stop;
    };
    let mut best_total = dp_total;
    let mut best_inputs = inputs.clone();
    if let Some(remainder) = amount.checked_sub(sum)
        && !remainder.is_zero()
    {
        let mut variants = Vec::new();
        for idx in 0..inputs.len() {
            if inputs[idx].is_zero() && inputs.iter().any(|value| !value.is_zero()) {
                continue;
            }
            let mut adjusted = inputs.clone();
            adjusted[idx] += remainder;
            variants.push(adjusted);
        }
        for variant in variants {
            result.stats.evaluated_plans += 1;
            if let Some((total, _)) = eval_group_plan(
                &groups,
                &branches,
                &variant,
                inner_segments,
                false,
                &mut cache,
                &mut quotes,
                source,
                config,
                baseline_calls,
                deadline,
                &mut stop,
            )
            .await
            {
                result.stats.valid_plans += 1;
                if total > best_total {
                    best_total = total;
                    best_inputs = variant;
                }
            }
        }
        inputs = best_inputs.clone();
    }

    // Apply the DP/remainder winner.
    if let Some((total, results)) = eval_group_plan(
        &groups,
        &branches,
        &inputs,
        inner_segments,
        false,
        &mut cache,
        &mut quotes,
        source,
        config,
        baseline_calls,
        deadline,
        &mut stop,
    )
    .await
    {
        result.stats.evaluated_plans += 1;
        result.stats.valid_plans += 1;
        result.stats.allocation_plans_evaluated += 1;
        best_total = best_total.max(total);
        apply_group_plan(results, total, result);
    }

    // Local coordinate refinement around the discrete outer allocation.
    for _ in 0..config.refinement_rounds {
        if stop != StopReason::Completed || tokio::time::Instant::now() >= deadline {
            break;
        }
        let variants = adaptive_refined_plans(&best_inputs, amount, config);
        let mut improved = false;
        for variant in variants.into_iter().take(96) {
            result.stats.evaluated_plans += 1;
            let Some((total, results)) = eval_group_plan(
                &groups,
                &branches,
                &variant,
                inner_segments,
                false,
                &mut cache,
                &mut quotes,
                source,
                config,
                baseline_calls,
                deadline,
                &mut stop,
            )
            .await
            else {
                continue;
            };
            result.stats.valid_plans += 1;
            if total > best_total {
                best_total = total;
                best_inputs = variant;
                result.stats.allocation_plans_evaluated += 1;
                apply_group_plan(results, total, result);
                improved = true;
            }
        }
        if !improved {
            break;
        }
    }

    // Suffix refinement is expensive, so run it only for the final outer plan.
    if stop == StopReason::Completed
        && tokio::time::Instant::now() < deadline
        && let Some((total, results)) = eval_group_plan(
            &groups,
            &branches,
            &best_inputs,
            inner_segments,
            true,
            &mut cache,
            &mut quotes,
            source,
            config,
            baseline_calls,
            deadline,
            &mut stop,
        )
        .await
    {
        result.stats.evaluated_plans += 1;
        result.stats.valid_plans += 1;
        result.stats.allocation_plans_evaluated += 1;
        apply_group_plan(results, total, result);
    }
    stop
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
        let route_paths = router.candidate_paths(graph, &source, request).await?;
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
                sources: Vec::new(),
                children: Vec::new(),
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
                    PoolKind::UniswapV2
                        | PoolKind::UniswapV3 { .. }
                        | PoolKind::SushiSwapV3 { .. }
                        | PoolKind::PancakeSwapV3 { .. }
                        | PoolKind::ShibaSwapV2
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
        if self.config.max_branches >= 2
            && route_paths.iter().any(|path| path.hops.len() > 1)
            && matches!(
                result.stats.stop_reason,
                StopReason::Completed | StopReason::NoPairs
            )
        {
            if needs_route_tree(&route_paths) {
                result.stats.multi_route_search = true;
                let stop = multi_route_tree(
                    route_paths,
                    graph,
                    &source,
                    request.amount_in,
                    request.token_out,
                    &self.config,
                    baseline_calls,
                    deadline,
                    &mut result,
                )
                .await;
                result.stats.multi_route_completed = stop == StopReason::Completed;
                if stop != StopReason::Completed {
                    result.stats.stop_reason = stop;
                }
            } else {
                let (candidates, _probe_stop) = probe_path_candidates(
                    route_paths,
                    &source,
                    request.amount_in,
                    &self.config,
                    baseline_calls,
                    deadline,
                )
                .await;
                let branches = select_route_branches(candidates, self.config.max_branches);
                if branches.len() >= 2 && branches.iter().any(|branch| branch.hops.len() > 1) {
                    result.stats.multi_route_search = true;
                    result.stats.route_branches = branches.len();
                    let stop = multi_route(
                        &branches,
                        &source,
                        request.amount_in,
                        &self.config,
                        baseline_calls,
                        deadline,
                        &mut result,
                    )
                    .await;
                    result.stats.multi_route_completed = stop == StopReason::Completed;
                    if stop != StopReason::Completed {
                        result.stats.stop_reason = stop;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PoolKind;

    fn pool(addr: u8, from: u8, to: u8) -> Pool {
        Pool {
            kind: PoolKind::UniswapV2,
            address: Address::with_last_byte(addr),
            coins: vec![Address::with_last_byte(from), Address::with_last_byte(to)],
        }
    }

    fn path(pools: &[(u8, u8, u8)], full: u64, half: u64, quarter: u64) -> PathCandidate {
        let hops = pools
            .iter()
            .map(|&(addr, from, to)| (pool(addr, from, to), 0, 1))
            .collect::<Vec<_>>();
        PathCandidate {
            pool_set: hops.iter().map(|(pool, _, _)| pool.address).collect(),
            hops,
            probe_outputs: [
                Some(U256::from(full)),
                Some(U256::from(half)),
                Some(U256::from(quarter)),
            ],
        }
    }

    #[test]
    fn dp_allocates_across_independent_branches() {
        let tables = vec![
            vec![
                Some(U256::ZERO),
                Some(U256::from(10)),
                Some(U256::from(19)),
                Some(U256::from(27)),
            ],
            vec![
                Some(U256::ZERO),
                Some(U256::from(9)),
                Some(U256::from(17)),
                Some(U256::from(24)),
            ],
        ];
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let plans = allocation_dp(&tables, 3, 2, deadline).unwrap();
        assert!(plans.iter().any(|units| units == &vec![2usize, 1usize]));
        assert!(!plans.is_empty());
    }

    #[test]
    fn refinement_moves_only_between_branches_and_conserves_input() {
        let inputs = vec![U256::from(400), U256::from(600)];
        let plans = refined_plans(&inputs, &SplitConfig::default());
        assert!(!plans.is_empty());
        for plan in plans {
            assert_eq!(plan.len(), 2);
            assert_eq!(
                plan.iter()
                    .try_fold(U256::ZERO, |sum, x| sum.checked_add(*x)),
                Some(U256::from(1000))
            );
        }
    }

    #[test]
    fn selection_excludes_shared_pools_and_keeps_multi_hop() {
        let mut candidates = vec![
            path(&[(10, 1, 2)], 950, 480, 240),
            path(&[(10, 1, 2)], 900, 460, 230),
            path(&[(12, 1, 3), (13, 3, 2)], 500, 260, 130),
            path(&[(11, 1, 2)], 100, 50, 25),
        ];
        candidates.sort_by_key(|candidate| std::cmp::Reverse(probe_score(candidate)));
        let branches = select_route_branches(candidates, 4);
        assert_eq!(branches.len(), 3);
        assert!(branches.iter().any(|branch| branch.hops.len() > 1));
        let single_pool_10 = branches
            .iter()
            .filter(|branch| {
                branch.hops.len() == 1 && branch.hops[0].0.address == Address::with_last_byte(10)
            })
            .count();
        assert_eq!(single_pool_10, 1);
    }

    #[test]
    fn probe_score_uses_normalized_fractional_outputs() {
        let candidate = PathCandidate {
            hops: vec![(pool(10, 1, 2), 0, 1)],
            pool_set: vec![Address::with_last_byte(10)],
            probe_outputs: [None, Some(U256::from(100)), Some(U256::from(60))],
        };
        assert_eq!(probe_score(&candidate), U256::from(240));
    }

    #[test]
    fn adaptive_refinement_preserves_amount_and_adds_small_moves() {
        let amount = U256::from(1_000_000);
        let inputs = vec![U256::from(950_000), U256::from(50_000)];
        let plans = adaptive_refined_plans(&inputs, amount, &SplitConfig::default());
        assert!(!plans.is_empty());
        assert!(plans.iter().all(|plan| {
            plan.iter()
                .try_fold(U256::ZERO, |sum, value| sum.checked_add(*value))
                == Some(amount)
        }));
        let small = fraction(amount, 1, 64);
        assert!(plans.iter().any(|plan| {
            plan[0].checked_sub(inputs[0]) == Some(small)
                || inputs[1].checked_sub(plan[1]) == Some(small)
        }));
    }
}
