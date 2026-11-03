//! Background-owned chain reads and immutable, network-free pricing contexts.
//! V2/ShibaSwap reserves, covered V3 ticks, and Lido v4 conversion state.
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use alloy_primitives::{Address, U256};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    adapters::decode_reserves,
    curve_local::{self, CurveState},
    error::QuoteError,
    graph::Graph,
    model::{ConversionKind, Pool, PoolKind, QuoteRequest, Route},
    router::{PriceSnapshot, Router, RouterConfig},
    rpc::{BlockHeader, RpcClient},
    source::{QuoteSource, constant_product_out, ratio},
    split::{SplitOptimizer, SplitResult},
    state::{GET_RESERVES, multicall},
    util::u256_to_f64,
    v3_local::V3State,
    wsteth_local::{SUPPORTED_VERSION, WstEthState},
};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PricingMode {
    #[default]
    Rpc,
    LocalSnapshot,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LocalStateConfig {
    pub poll_interval_ms: u64,
    pub sync_timeout_ms: u64,
    pub batch_size: usize,
    pub max_age_secs: u64,
    pub max_lag_blocks: u64,
    pub v3_bitmap_radius: i16,
    pub v3_full_range: bool,
    pub v3_max_loaded_ticks: usize,
    pub compute_concurrency: usize,
}

impl Default for LocalStateConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 2000,
            sync_timeout_ms: 30_000,
            batch_size: 200,
            max_age_secs: 60,
            max_lag_blocks: 3,
            v3_bitmap_radius: 2,
            v3_full_range: false,
            v3_max_loaded_ticks: 50_000,
            compute_concurrency: 4,
        }
    }
}

impl LocalStateConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=60_000).contains(&self.poll_interval_ms)
            || !(1..=3_600_000).contains(&self.sync_timeout_ms)
            || self.max_age_secs == 0
            || !(1..=400).contains(&self.batch_size)
            || !(1..=16).contains(&self.v3_bitmap_radius)
            || !(1..=1_000_000).contains(&self.v3_max_loaded_ticks)
            || !(1..=64).contains(&self.compute_concurrency)
        {
            return Err("local_state needs poll_interval_ms 1..60000, sync_timeout_ms 1..3600000, positive max_age_secs, batch_size 1..400, v3_bitmap_radius 1..16, v3_max_loaded_ticks 1..1000000 and compute_concurrency 1..64".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, thiserror::Error, PartialEq, Eq)]
pub enum StateError {
    #[error("SnapshotNotReady")]
    NotReady,
    #[error("SnapshotStale")]
    Stale,
    #[error("SnapshotLagging")]
    Lagging,
    #[error("SnapshotReorg")]
    Reorg,
    #[error("SnapshotIncomplete")]
    Incomplete,
    #[error("UnsupportedLocalPool")]
    Unsupported,
    #[error("SnapshotSyncFailed")]
    SyncFailed,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnsupportedPool {
    pub address: Address,
    pub platform: &'static str,
}

#[derive(Serialize)]
pub struct StateStatus {
    pub pricing_mode: &'static str,
    pub ready: bool,
    pub unavailable_reason: Option<String>,
    pub block: Option<BlockHeader>,
    pub observed_head: Option<BlockHeader>,
    pub state_age_secs: Option<u64>,
    pub lag_blocks: Option<u64>,
    pub loaded_pools: usize,
    pub total_pools: usize,
    pub unsupported_pools: Vec<UnsupportedPool>,
    pub last_sync_error: Option<String>,
    pub sync_rpc_requests: u64,
    pub last_sync_ms: u128,
    pub published_versions: u64,
    pub reorgs: u64,
    pub v3_coverage: Vec<V3Coverage>,
    pub conversion_state: Vec<ConversionState>,
    pub curve_state: Vec<CurveStateInfo>,
}

#[derive(Debug, Serialize)]
pub struct V3Coverage {
    pub pool: Address,
    pub first_bitmap_word: i16,
    pub last_bitmap_word: i16,
    pub initialized_ticks: usize,
}

#[derive(Debug, Serialize)]
pub struct ConversionState {
    pub kind: &'static str,
    pub contract: Address,
    pub underlying: Address,
    pub protocol_version: u64,
    pub internal_ether: String,
    pub internal_shares: String,
    pub wrap_enabled: bool,
    pub unwrap_enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct CurveStateInfo {
    pub pool: Address,
    pub profile: &'static str,
    pub balance_source: &'static str,
    pub coins: Vec<Address>,
    pub balances: Vec<String>,
    pub amplification: String,
    pub a_precision: u64,
    pub fee_model: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<String>,
    pub fee_denominator: u64,
    pub has_liquidity: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crypto: Option<CryptoStateInfo>,
}

#[derive(Debug, Serialize)]
pub struct CryptoStateInfo {
    pub gamma: String,
    pub stored_d: String,
    pub price_scale: [String; 2],
    pub fee_gamma: String,
    pub mid_fee: String,
    pub out_fee: String,
    pub math_contract: Address,
    pub views_contract: Address,
}

#[derive(Debug, Serialize)]
pub struct QuoteState {
    pub pricing_mode: &'static str,
    pub block_hash: alloy_primitives::B256,
    pub parent_hash: alloy_primitives::B256,
    pub block_timestamp: u64,
    pub state_age_secs: u64,
    pub lag_blocks: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub v3_coverage: Vec<V3Coverage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conversion_state: Vec<ConversionState>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub curve_state: Vec<CurveStateInfo>,
}

enum PoolState {
    V2([U256; 2]),
    V3(V3State),
    WstEth(WstEthState),
    Curve(Box<CurveState>),
}

struct FrozenSource {
    pools: HashMap<Address, (Pool, PoolState)>,
}

impl FrozenSource {
    fn curve_states(&self) -> Vec<CurveStateInfo> {
        let mut entries = self
            .pools
            .values()
            .filter_map(|(pool, state)| {
                let PoolState::Curve(state) = state else {
                    return None;
                };
                Some(CurveStateInfo {
                    pool: pool.address,
                    profile: state.profile().name(),
                    balance_source: state.profile().balance_source(),
                    coins: pool.coins.clone(),
                    balances: state.balances().iter().map(ToString::to_string).collect(),
                    amplification: state.amplification().to_string(),
                    a_precision: state.profile().a_precision(),
                    fee_model: if state.crypto_parameters().is_some() {
                        "dynamic-post-trade"
                    } else {
                        "fixed"
                    },
                    fee: state.fixed_fee().map(|f| f.to_string()),
                    fee_denominator: 10_000_000_000,
                    has_liquidity: state.is_liquid(),
                    crypto: state.crypto_parameters().map(|p| CryptoStateInfo {
                        gamma: p.gamma.to_string(),
                        stored_d: p.stored_d.to_string(),
                        price_scale: p.price_scale.map(|x| x.to_string()),
                        fee_gamma: p.fee_gamma.to_string(),
                        mid_fee: p.mid_fee.to_string(),
                        out_fee: p.out_fee.to_string(),
                        math_contract: crate::tricrypto2_local::MATH.parse().unwrap(),
                        views_contract: crate::tricrypto2_local::VIEWS.parse().unwrap(),
                    }),
                })
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.pool);
        entries
    }
    fn conversions(&self) -> Vec<ConversionState> {
        let mut entries = self
            .pools
            .values()
            .filter_map(|(pool, state)| {
                let PoolState::WstEth(state) = state else {
                    return None;
                };
                let PoolKind::Conversion { wrap, unwrap, .. } = pool.kind else {
                    return None;
                };
                Some(ConversionState {
                    kind: "wsteth",
                    contract: pool.address,
                    underlying: pool.coins[0],
                    protocol_version: SUPPORTED_VERSION,
                    internal_ether: state.internal_ether().to_string(),
                    internal_shares: state.internal_shares().to_string(),
                    wrap_enabled: wrap,
                    unwrap_enabled: unwrap,
                })
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.contract);
        entries
    }
    fn state(&self, pool: &Pool) -> Result<&PoolState, QuoteError> {
        let (identity, state) = self
            .pools
            .get(&pool.address)
            .ok_or(QuoteError::UnknownPool)?;
        if identity.kind != pool.kind || identity.coins != pool.coins {
            return Err(QuoteError::UnknownPool);
        }
        Ok(state)
    }

    fn coverage(&self) -> Vec<V3Coverage> {
        let mut coverage = self
            .pools
            .iter()
            .filter_map(|(address, (_, state))| match state {
                PoolState::V3(state) => {
                    let (first_bitmap_word, last_bitmap_word) = state.covered_words();
                    Some(V3Coverage {
                        pool: *address,
                        first_bitmap_word,
                        last_bitmap_word,
                        initialized_ticks: state.initialized_tick_count(),
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        coverage.sort_by_key(|entry| entry.pool);
        coverage
    }
}

#[async_trait]
impl QuoteSource for FrozenSource {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount: U256,
    ) -> Result<U256, QuoteError> {
        if i >= pool.coins.len() || j >= pool.coins.len() || !pool.kind.allows(i, j) {
            return Err(QuoteError::UnknownPool);
        }
        match self.state(pool)? {
            PoolState::V2(reserves) => {
                if reserves.iter().any(U256::is_zero) {
                    return Err(QuoteError::InsufficientLiquidity);
                }
                constant_product_out(reserves[i], reserves[j], amount)
            }
            PoolState::V3(state) => Ok(state.quote(amount, i == 0)?.amount_out),
            PoolState::WstEth(state) => {
                let out = state.amount_out(amount, i == 0)?;
                if out.is_zero() {
                    Err(QuoteError::InsufficientLiquidity)
                } else {
                    Ok(out)
                }
            }
            PoolState::Curve(state) => state.amount_out(i, j, amount),
        }
    }
    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        if i >= pool.coins.len() || j >= pool.coins.len() || !pool.kind.allows(i, j) {
            return Err(QuoteError::UnknownPool);
        }
        Ok(match self.state(pool)? {
            PoolState::V2(reserves) => ratio(reserves[j], reserves[i]),
            PoolState::V3(state) => {
                let sqrt = u256_to_f64(state.sqrt_price()) / 79228162514264337593543950336.0;
                if i == 0 {
                    sqrt * sqrt
                } else {
                    1.0 / (sqrt * sqrt)
                }
            }
            PoolState::WstEth(state) => state.mid_price(i == 0),
            PoolState::Curve(state) => return state.mid_price(i, j),
        })
    }
    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError> {
        if i >= pool.coins.len() {
            return Err(QuoteError::UnknownPool);
        }
        Ok(match self.state(pool)? {
            PoolState::V2(reserves) => reserves[i],
            // Keep Curve's coarse reserve convention identical to the RPC
            // adapter. Actual pool balances remain available for graph display.
            PoolState::V3(_) | PoolState::WstEth(_) | PoolState::Curve(_) => U256::MAX,
        })
    }
}

/// The generic Router intentionally isolates ordinary failed candidates. State
/// coverage failures are different: even if another route succeeds, they must
/// invalidate the request rather than silently reduce the advertised universe.
struct GuardedSource<'a> {
    inner: &'a FrozenSource,
    incomplete: AtomicBool,
}
impl<'a> GuardedSource<'a> {
    fn new(inner: &'a FrozenSource) -> Self {
        Self {
            inner,
            incomplete: AtomicBool::new(false),
        }
    }
    fn finish<T>(&self, result: Result<T, QuoteError>) -> Result<T, QuoteError> {
        if self.incomplete.load(Ordering::Relaxed) {
            Err(QuoteError::IncompleteState)
        } else {
            result
        }
    }
}
#[async_trait]
impl QuoteSource for GuardedSource<'_> {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount: U256,
    ) -> Result<U256, QuoteError> {
        let result = self.inner.amount_out(pool, i, j, amount).await;
        if result == Err(QuoteError::IncompleteState) {
            tracing::debug!(pool=%pool.address, i, j, input=%amount, "local quote exceeds loaded state coverage");
            self.incomplete.store(true, Ordering::Relaxed);
        }
        result
    }
    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        self.inner.mid_price(pool, i, j).await
    }
    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError> {
        self.inner.reserve_in(pool, i).await
    }
}

/// No RPC/cache handles. The Router's coarse snapshot is derived from this
/// exact frozen source before the complete context is published atomically.
pub struct LocalSnapshot {
    block: BlockHeader,
    graph: Arc<Graph>,
    source: FrozenSource,
    router: Router,
    epoch: u64,
    published_at: Instant,
    initial_age_secs: u64,
}

impl LocalSnapshot {
    pub fn block(&self) -> &BlockHeader {
        &self.block
    }
    pub fn graph(&self) -> &Graph {
        &self.graph
    }
    /// Optional display reserve, separate from coarse-search heuristics.
    pub fn graph_reserve(&self, edge_index: usize) -> Option<U256> {
        if edge_index >= self.graph.edge_count() {
            return None;
        }
        let edge = self.graph.edge(edge_index);
        let pool = self.graph.pool(edge.pool_idx);
        match self.source.state(pool).ok()? {
            PoolState::Curve(state) => Some(state.balances()[edge.i]),
            _ => None,
        }
    }
    fn age_secs(&self) -> u64 {
        now_secs().saturating_sub(self.block.timestamp).max(
            self.initial_age_secs
                .saturating_add(self.published_at.elapsed().as_secs()),
        )
    }
    pub async fn route(&self, request: &QuoteRequest) -> Result<Route, QuoteError> {
        let source = GuardedSource::new(&self.source);
        source.finish(self.router.route(&self.graph, &source, request).await)
    }
    pub async fn split(
        &self,
        optimizer: &SplitOptimizer,
        request: &QuoteRequest,
        deadline: tokio::time::Instant,
    ) -> Result<SplitResult, QuoteError> {
        let source = GuardedSource::new(&self.source);
        let result = optimizer
            .quote_until(&self.router, &self.graph, &source, request, deadline)
            .await;
        let mut result = source.finish(result)?;
        // FrozenSource and this Router contain no network-capable collaborators.
        result.stats.baseline_rpc_requests = Some(0);
        result.stats.additional_rpc_requests = Some(0);
        Ok(result)
    }
    pub fn prices(&self) -> Arc<PriceSnapshot> {
        self.router.snapshot().expect("published coarse prices")
    }
}

#[derive(Default)]
struct Published {
    current: Option<Arc<LocalSnapshot>>,
    head: Option<BlockHeader>,
    last_error: Option<StateError>,
    sync_rpc_requests: u64,
    last_sync_ms: u128,
    versions: u64,
    reorgs: u64,
    epoch: u64,
}

impl Published {
    fn invalidate(&mut self) -> Option<Arc<LocalSnapshot>> {
        let retired = self.current.take();
        self.epoch += 1;
        self.last_error = Some(StateError::Reorg);
        self.reorgs += 1;
        retired
    }
}

pub struct SnapshotStore {
    graph: Arc<Graph>,
    router_config: RouterConfig,
    config: LocalStateConfig,
    state: RwLock<Published>,
    sync_lock: Arc<tokio::sync::Mutex<()>>,
    sync_admission: tokio::sync::Semaphore,
    retired: Mutex<Vec<Arc<LocalSnapshot>>>,
    unsupported_pools: Vec<UnsupportedPool>,
    #[cfg(test)]
    build_probe: std::sync::Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl SnapshotStore {
    pub(crate) fn compute_concurrency(&self) -> usize {
        self.config.compute_concurrency
    }
    pub fn new(
        graph: Graph,
        router_config: RouterConfig,
        config: LocalStateConfig,
    ) -> Result<Self, String> {
        config.validate()?;
        let unsupported_pools = graph
            .pools()
            .iter()
            .filter(|p| {
                if p.kind == PoolKind::Curve {
                    return !curve_local::supported(p);
                }
                let supported = matches!(
                    p.kind,
                    PoolKind::UniswapV2 | PoolKind::ShibaSwapV2 | PoolKind::UniswapV3 { .. }
                ) || matches!(
                    p.kind,
                    PoolKind::Conversion {
                        kind: ConversionKind::WstEth,
                        wrap: true,
                        ..
                    } | PoolKind::Conversion {
                        kind: ConversionKind::WstEth,
                        unwrap: true,
                        ..
                    }
                );
                !supported
                    || p.coins.len() != 2
                    || (p.kind.is_conversion()
                        && (p.coins[1] != p.address
                            || p.coins[0] == Address::ZERO
                            || p.coins[0] == p.coins[1]))
            })
            .map(|p| UnsupportedPool {
                address: p.address,
                platform: p.kind.name(),
            })
            .collect();
        Ok(Self {
            graph: Arc::new(graph),
            router_config,
            config,
            state: RwLock::new(Published::default()),
            sync_lock: Arc::new(tokio::sync::Mutex::new(())),
            sync_admission: tokio::sync::Semaphore::new(1),
            retired: Mutex::new(Vec::new()),
            unsupported_pools,
            #[cfg(test)]
            build_probe: std::sync::Mutex::new(None),
        })
    }

    fn check(&self, state: &Published, snapshot: &LocalSnapshot) -> Result<(), StateError> {
        if !Arc::ptr_eq(&self.graph, &snapshot.graph) {
            return Err(StateError::Incomplete);
        }
        if snapshot.epoch != state.epoch {
            return Err(StateError::Reorg);
        }
        if state.head.as_ref().is_some_and(|head| {
            head.number < snapshot.block.number
                || (head.number == snapshot.block.number && head.hash != snapshot.block.hash)
        }) {
            return Err(StateError::Reorg);
        }
        if snapshot.age_secs() > self.config.max_age_secs {
            return Err(StateError::Stale);
        }
        if state.head.as_ref().is_some_and(|h| {
            h.number.saturating_sub(snapshot.block.number) > self.config.max_lag_blocks
        }) {
            return Err(StateError::Lagging);
        }
        Ok(())
    }

    pub fn acquire(&self) -> Result<Arc<LocalSnapshot>, StateError> {
        if !self.unsupported_pools.is_empty() {
            return Err(StateError::Unsupported);
        }
        let state = self.state.read().unwrap();
        let snapshot = state
            .current
            .clone()
            .ok_or(state.last_error.unwrap_or(StateError::NotReady))?;
        self.check(&state, &snapshot)?;
        Ok(snapshot)
    }

    /// Recheck after computation: a snapshot invalidated by a detected reorg
    /// must not be returned just because it was acquired before detection.
    pub fn quote_state(&self, snapshot: &LocalSnapshot) -> Result<QuoteState, StateError> {
        let state = self.state.read().unwrap();
        self.check(&state, snapshot)?;
        Ok(QuoteState {
            pricing_mode: "local_snapshot",
            block_hash: snapshot.block.hash,
            parent_hash: snapshot.block.parent_hash,
            block_timestamp: snapshot.block.timestamp,
            state_age_secs: snapshot.age_secs(),
            lag_blocks: state
                .head
                .as_ref()
                .map_or(0, |h| h.number.saturating_sub(snapshot.block.number)),
            v3_coverage: snapshot.source.coverage(),
            conversion_state: snapshot.source.conversions(),
            curve_state: snapshot.source.curve_states(),
        })
    }

    pub fn status(&self) -> StateStatus {
        let unsupported_pools = self.unsupported_pools.clone();
        let state = self.state.read().unwrap();
        let reason = if !unsupported_pools.is_empty() {
            Some(StateError::Unsupported)
        } else {
            state.current.as_ref().map_or_else(
                || Some(state.last_error.unwrap_or(StateError::NotReady)),
                |s| self.check(&state, s).err(),
            )
        };
        StateStatus {
            pricing_mode: "local_snapshot",
            ready: reason.is_none(),
            unavailable_reason: reason.map(|e| e.to_string()),
            block: state.current.as_ref().map(|s| s.block.clone()),
            observed_head: state.head.clone(),
            state_age_secs: state.current.as_ref().map(|s| s.age_secs()),
            lag_blocks: state
                .current
                .as_ref()
                .zip(state.head.as_ref())
                .map(|(s, h)| h.number.saturating_sub(s.block.number)),
            loaded_pools: state.current.as_ref().map_or(0, |s| s.source.pools.len()),
            total_pools: self.graph.pool_count(),
            unsupported_pools,
            last_sync_error: state.last_error.map(|e| e.to_string()),
            sync_rpc_requests: state.sync_rpc_requests,
            last_sync_ms: state.last_sync_ms,
            published_versions: state.versions,
            reorgs: state.reorgs,
            v3_coverage: state
                .current
                .as_ref()
                .map_or_else(Vec::new, |s| s.source.coverage()),
            conversion_state: state
                .current
                .as_ref()
                .map_or_else(Vec::new, |s| s.source.conversions()),
            curve_state: state
                .current
                .as_ref()
                .map_or_else(Vec::new, |s| s.source.curve_states()),
        }
    }

    fn invalidate(&self) {
        let retired = self.state.write().unwrap().invalidate();
        self.retire(retired);
    }

    fn retire(&self, snapshot: Option<Arc<LocalSnapshot>>) {
        if let Some(snapshot) = snapshot {
            self.retired.lock().unwrap().push(snapshot);
        }
    }

    /// Only called on the bounded writer. Keep one ownership reference while
    /// readers exist, so their last drop cannot destroy large V3 maps on I/O.
    fn reclaim_retired(&self) {
        let retired = std::mem::take(&mut *self.retired.lock().unwrap());
        let mut retained = Vec::new();
        for snapshot in retired {
            match Arc::try_unwrap(snapshot) {
                Ok(snapshot) => drop(snapshot),
                Err(snapshot) => retained.push(snapshot),
            }
        }
        self.retired.lock().unwrap().extend(retained);
    }
}

/// The only component holding an RPC client. Each new block is fully reloaded
/// in bounded Multicall batches in this first stage; there is no log-gap/cache
/// assumption. Reinitializing from the new canonical head also handles reorgs.
#[derive(Clone)]
pub struct StateSynchronizer {
    rpc: RpcClient,
    store: Arc<SnapshotStore>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WritePhase {
    Running,
    Cancelled,
    Published,
}

struct WriteOperation {
    phase: Mutex<WritePhase>,
    cancel: tokio::sync::Notify,
    deadline: Instant,
}

impl WriteOperation {
    /// Linearizes cancellation with the short publication critical section.
    fn cancel(&self) -> bool {
        let mut phase = self.phase.lock().unwrap();
        if *phase == WritePhase::Published {
            return true;
        }
        *phase = WritePhase::Cancelled;
        self.cancel.notify_one();
        false
    }
}

struct CancelWriteOnDrop(Arc<WriteOperation>);
impl Drop for CancelWriteOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl StateSynchronizer {
    pub fn new(rpc: RpcClient, store: Arc<SnapshotStore>) -> Self {
        Self { rpc, store }
    }

    pub async fn run(self) {
        loop {
            if let Err(error) = self.sync_once().await {
                tracing::warn!(%error,"local state sync incomplete");
                if error == StateError::Unsupported && !self.store.unsupported_pools.is_empty() {
                    return;
                } // Only statically unsupported topology is permanent. A protocol
                // version mismatch is block-dependent and can recover on reorg.
            }
            tokio::time::sleep(Duration::from_millis(self.store.config.poll_interval_ms)).await;
        }
    }

    pub async fn sync_once(&self) -> Result<(), StateError> {
        // At most one active caller may either run or wait for the previous
        // cancelled worker. The worker retains its owned lock until it exits.
        let _admission = self
            .store
            .sync_admission
            .try_acquire()
            .map_err(|_| StateError::SyncFailed)?;
        let started = Instant::now();
        let timeout = Duration::from_millis(self.store.config.sync_timeout_ms);
        let deadline = started + timeout;
        let guard = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            self.store.sync_lock.clone().lock_owned(),
        )
        .await
        .map_err(|_| StateError::SyncFailed)?;
        let before = self.rpc.request_count();
        let operation = Arc::new(WriteOperation {
            phase: Mutex::new(WritePhase::Running),
            cancel: tokio::sync::Notify::new(),
            deadline,
        });
        let _cancel = CancelWriteOnDrop(operation.clone());
        let sync = self.clone();
        let runtime = tokio::runtime::Handle::current();
        let active = operation.clone();
        let worker = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            sync.store.reclaim_retired();
            let result = runtime.block_on(async {
                tokio::select! {
                    biased;
                    _ = active.cancel.notified() => Err(StateError::SyncFailed),
                    result = sync.load(&active) => result,
                }
            });
            sync.store.reclaim_retired();
            {
                let mut state = sync.store.state.write().unwrap();
                state.sync_rpc_requests += sync.rpc.request_count().saturating_sub(before);
                state.last_sync_ms = started.elapsed().as_millis();
                state.last_error = result.as_ref().err().copied();
            }
            result
        });
        match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), worker).await {
            Ok(Ok(result)) => result,
            _ if operation.cancel() => Ok(()), // published before its deadline; cleanup may remain
            _ => {
                // The worker records final metrics after it exits. A deadline
                // must never block the I/O caller behind a published-state lock.
                if let Ok(mut state) = self.store.state.try_write() {
                    state.last_error = Some(StateError::SyncFailed);
                }
                Err(StateError::SyncFailed)
            }
        }
    }

    async fn load(&self, operation: &WriteOperation) -> Result<(), StateError> {
        if !self.store.unsupported_pools.is_empty() {
            return Err(StateError::Unsupported);
        }
        if self.store.graph.pools().is_empty() {
            return Err(StateError::Incomplete);
        }
        let head = self
            .rpc
            .block_header(None)
            .await
            .map_err(|_| StateError::SyncFailed)?;
        let (old, retired) = {
            let mut state = self.store.state.write().unwrap();
            // A shorter head or a replacement at the published height already
            // proves a fork. Invalidate *all* leases under the same lock that
            // exposes that head, before any further network await.
            let retired = if state
                .current
                .as_ref()
                .is_some_and(|old| old.block.number >= head.number && old.block != head)
            {
                state.invalidate()
            } else {
                None
            };
            state.head = Some(head.clone());
            (state.current.clone(), retired)
        };
        self.store.retire(retired);
        if let Some(old) = old {
            if old.block == head {
                return Ok(());
            }
            let canonical = self
                .rpc
                .block_header(Some(old.block.number))
                .await
                .map_err(|_| StateError::SyncFailed)?;
            if canonical != old.block {
                self.store.invalidate();
            } else if old.block.number.checked_add(1) == Some(head.number)
                && head.parent_hash != old.block.hash
            {
                self.store.invalidate();
                return Err(StateError::Reorg);
            }
        }
        let rpc = self.rpc.clone().at_block_hash(head.hash);
        let mut pools = HashMap::new();
        let v2 = self
            .store
            .graph
            .pools()
            .iter()
            .filter(|p| matches!(p.kind, PoolKind::UniswapV2 | PoolKind::ShibaSwapV2))
            .collect::<Vec<_>>();
        for chunk in v2.chunks(self.store.config.batch_size) {
            let calls = chunk
                .iter()
                .map(|p| (p.address, GET_RESERVES.to_string()))
                .collect::<Vec<_>>();
            let results = multicall(&rpc, &calls)
                .await
                .map_err(|_| StateError::SyncFailed)?;
            for (pool, (ok, data)) in chunk.iter().zip(results) {
                if !ok || data.len() != 96 {
                    return Err(StateError::Incomplete);
                }
                let reserves = decode_reserves(&data).map_err(|_| StateError::Incomplete)?;
                if reserves.iter().any(|n| n.bit_len() > 112) {
                    return Err(StateError::Incomplete);
                }
                if pools
                    .insert(pool.address, ((*pool).clone(), PoolState::V2(reserves)))
                    .is_some()
                {
                    return Err(StateError::Incomplete);
                }
            }
        }
        let v3 = self
            .store
            .graph
            .pools()
            .iter()
            .filter(|p| matches!(p.kind, PoolKind::UniswapV3 { .. }))
            .collect::<Vec<_>>();
        let mut v3_states = crate::v3_sync::load(
            &v3,
            &rpc,
            self.store.config.batch_size,
            self.store.config.v3_bitmap_radius,
            self.store.config.v3_max_loaded_ticks,
            self.store.config.v3_full_range,
        )
        .await?;
        for pool in v3 {
            let state = v3_states
                .remove(&pool.address)
                .ok_or(StateError::Incomplete)?;
            if pools
                .insert(pool.address, (pool.clone(), PoolState::V3(state)))
                .is_some()
            {
                return Err(StateError::Incomplete);
            }
        }
        let conversions = self
            .store
            .graph
            .pools()
            .iter()
            .filter(|p| p.kind.is_conversion())
            .collect::<Vec<_>>();
        let mut conversion_states =
            crate::wsteth_sync::load(&conversions, &rpc, self.store.config.batch_size).await?;
        for pool in conversions {
            let state = conversion_states
                .remove(&pool.address)
                .ok_or(StateError::Incomplete)?;
            if pools
                .insert(pool.address, (pool.clone(), PoolState::WstEth(state)))
                .is_some()
            {
                return Err(StateError::Incomplete);
            }
        }
        let curves = self
            .store
            .graph
            .pools()
            .iter()
            .filter(|p| p.kind == PoolKind::Curve)
            .collect::<Vec<_>>();
        let mut curve_states =
            crate::curve_sync::load(&curves, &rpc, self.store.config.batch_size).await?;
        for pool in curves {
            let state = curve_states
                .remove(&pool.address)
                .ok_or(StateError::Incomplete)?;
            if pools
                .insert(
                    pool.address,
                    (pool.clone(), PoolState::Curve(Box::new(state))),
                )
                .is_some()
            {
                return Err(StateError::Incomplete);
            }
        }
        #[cfg(test)]
        let probe = self.store.build_probe.lock().unwrap().clone();
        #[cfg(test)]
        if let Some(probe) = probe {
            probe();
        }
        let phase = Instant::now();
        tracing::debug!(phase = "coarse_build", "diag_sync_begin");
        let source = FrozenSource { pools };
        let router = Router::new(self.store.router_config);
        router.set_snapshot(Arc::new(
            PriceSnapshot::build(&self.store.graph, &source).await,
        ));
        tracing::debug!(
            phase = "coarse_build",
            elapsed_ms = phase.elapsed().as_millis(),
            "diag_sync_end"
        );
        let canonical = self
            .rpc
            .block_header(Some(head.number))
            .await
            .map_err(|_| StateError::SyncFailed)?;
        if canonical != head {
            self.store.invalidate();
            return Err(StateError::Reorg);
        }
        let phase = Instant::now();
        let mut state = self.store.state.write().unwrap();
        let mut write_phase = operation.phase.lock().unwrap();
        if *write_phase == WritePhase::Cancelled || Instant::now() >= operation.deadline {
            return Err(StateError::SyncFailed);
        }
        tracing::debug!(
            phase = "publish_lock_wait",
            elapsed_us = phase.elapsed().as_micros(),
            "diag_sync_end"
        );
        let phase = Instant::now();
        let initial_age_secs = now_secs().saturating_sub(head.timestamp);
        let snapshot = Arc::new(LocalSnapshot {
            block: head,
            graph: self.store.graph.clone(),
            source,
            router,
            epoch: state.epoch,
            published_at: Instant::now(),
            initial_age_secs,
        });
        let retired = state.current.replace(snapshot);
        state.versions += 1;
        *write_phase = WritePhase::Published;
        drop(state);
        drop(write_phase);
        self.store.retire(retired);
        tracing::debug!(
            phase = "publish_swap_and_drop",
            elapsed_ms = phase.elapsed().as_millis(),
            "diag_sync_end"
        );
        Ok(())
    }
}

#[cfg(test)]
mod writer_tests {
    use super::*;
    use crate::{
        api::{self, AppState, RateLimiter},
        source::FixedRate,
        split::SplitConfig,
    };
    use alloy_primitives::Address;
    use axum::{Json, Router as HttpRouter, routing::post};
    use serde_json::{Value, json};
    use std::sync::atomic::AtomicU64;
    use tower::ServiceExt;

    async fn fixture(
        timeout_ms: u64,
    ) -> (
        Arc<SnapshotStore>,
        Arc<StateSynchronizer>,
        Arc<AtomicU64>,
        tokio::task::JoinHandle<()>,
    ) {
        let head = Arc::new(AtomicU64::new(123));
        let timestamp = now_secs();
        let control = head.clone();
        let mock = HttpRouter::new().route("/", post(move |Json(request): Json<Value>| {
            let head = control.clone();
            async move {
                let result = if request["method"] == "eth_getBlockByNumber" {
                    let tag = request["params"][0].as_str().unwrap();
                    let n = if tag == "latest" { head.load(Ordering::SeqCst) } else { u64::from_str_radix(&tag[2..], 16).unwrap() };
                    json!({"number":format!("0x{n:x}"),"hash":format!("0x{n:064x}"),"parentHash":format!("0x{:064x}",n-1),"timestamp":format!("0x{timestamp:x}")})
                } else {
                    assert_eq!(request["method"], "eth_call");
                    assert_eq!(request["params"][1]["requireCanonical"], true);
                    let reserve = if head.load(Ordering::SeqCst) == 123 { 100_000 } else { 200_000 };
                    json!(format!("0x{:064x}{:064x}{:064x}{:064x}{:064x}{:064x}{:064x}{reserve:064x}{:064x}",32,1,32,1,64,96,1000,0))
                };
                Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let rpc = RpcClient::new(format!("http://{}", listener.local_addr().unwrap()));
        let server = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
        let mut graph = Graph::new();
        graph.add_pool(Pool {
            kind: PoolKind::UniswapV2,
            address: Address::with_last_byte(10),
            coins: vec![Address::with_last_byte(1), Address::with_last_byte(2)],
        });
        let store = Arc::new(
            SnapshotStore::new(
                graph,
                RouterConfig::default(),
                LocalStateConfig {
                    sync_timeout_ms: timeout_ms,
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        let sync = Arc::new(StateSynchronizer::new(rpc, store.clone()));
        sync.sync_once().await.unwrap();
        (store, sync, head, server)
    }

    fn app(store: Arc<SnapshotStore>) -> HttpRouter {
        api::app_with_local_state(
            Arc::new(AppState {
                graph: store.graph.as_ref().clone(),
                source: Arc::new(FixedRate::new(U256::from(1))),
                router: Router::default(),
                rpc: None,
                limiter: RateLimiter::new(1000, Duration::from_secs(1)),
                request_timeout: Duration::from_secs(3),
            }),
            SplitOptimizer::new(SplitConfig::multi_pool()).unwrap(),
            store,
        )
    }

    async fn quote(app: HttpRouter) -> Value {
        let response = app.oneshot(axum::http::Request::builder().method("POST").uri("/quote")
            .header("content-type","application/json").body(axum::body::Body::from(json!({"token_in":Address::with_last_byte(1),"token_out":Address::with_last_byte(2),"amount_in":"1001","split":true}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), 200);
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap(),
        )
        .unwrap()
    }

    struct BuildGate {
        entered: Arc<tokio::sync::Notify>,
        release: Arc<AtomicBool>,
        watchdog_fired: Arc<AtomicBool>,
    }
    impl BuildGate {
        fn install(store: &SnapshotStore) -> Self {
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(AtomicBool::new(false));
            let watchdog_fired = Arc::new(AtomicBool::new(false));
            let (signal, gate) = (entered.clone(), release.clone());
            *store.build_probe.lock().unwrap() = Some(Arc::new(move || {
                signal.notify_one();
                while !gate.load(Ordering::SeqCst) {
                    std::hint::spin_loop();
                }
            }));
            let (gate, fired) = (release.clone(), watchdog_fired.clone());
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(1));
                if !gate.load(Ordering::SeqCst) {
                    fired.store(true, Ordering::SeqCst);
                    gate.store(true, Ordering::SeqCst);
                }
            });
            Self {
                entered,
                release,
                watchdog_fired,
            }
        }
    }
    impl Drop for BuildGate {
        fn drop(&mut self) {
            self.release.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn deadline_can_cancel_while_publication_waits_for_a_reader_lock() {
        let (store, sync, head, server) = fixture(80).await;
        let released = Arc::new(AtomicBool::new(false));
        let flag = released.clone();
        let weak = Arc::downgrade(&store);
        *store.build_probe.lock().unwrap() = Some(Arc::new(move || {
            let store = weak.upgrade().unwrap();
            let flag = flag.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let read = store.state.read().unwrap();
                tx.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(400));
                drop(read);
                flag.store(true, Ordering::SeqCst);
            });
            rx.recv().unwrap();
        }));
        head.store(124, Ordering::SeqCst);
        assert_eq!(sync.sync_once().await, Err(StateError::SyncFailed));
        assert!(
            !released.load(Ordering::SeqCst),
            "cancellation must not wait on the published-state lock"
        );
        *store.build_probe.lock().unwrap() = None;
        tokio::time::sleep(Duration::from_millis(450)).await;
        assert_eq!(store.status().published_versions, 1);
        head.store(125, Ordering::SeqCst);
        sync.sync_once().await.unwrap();
        assert_eq!(store.status().published_versions, 2);
        server.abort();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn aborting_sync_cancels_publication_without_stopping_the_running_worker_early() {
        let (store, sync, head, server) = fixture(3000).await;
        let gate = BuildGate::install(&store);
        head.store(124, Ordering::SeqCst);
        let first = sync.clone();
        let pending = tokio::spawn(async move { first.sync_once().await });
        gate.entered.notified().await;
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        assert_eq!(store.acquire().unwrap().block().number, 123);
        *store.build_probe.lock().unwrap() = None;
        head.store(125, Ordering::SeqCst);
        gate.release.store(true, Ordering::SeqCst);
        sync.sync_once().await.unwrap();
        assert_eq!(store.acquire().unwrap().block().number, 125);
        assert_eq!(store.status().published_versions, 2);
        server.abort();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn old_payload_is_retained_until_readers_finish_then_reclaimed_by_writer() {
        let (store, sync, head, server) = fixture(3000).await;
        let old = store.acquire().unwrap();
        let weak = Arc::downgrade(&old);
        head.store(124, Ordering::SeqCst);
        sync.sync_once().await.unwrap();
        assert_eq!(old.block().number, 123);
        assert_eq!(store.acquire().unwrap().block().number, 124);
        drop(old);
        assert!(
            weak.upgrade().is_some(),
            "dropping a reader must not reclaim its old payload on the read side"
        );
        sync.sync_once().await.unwrap();
        assert!(
            weak.upgrade().is_none(),
            "writer should reclaim retired payloads once readers have left"
        );
        assert_eq!(store.status().published_versions, 2);
        server.abort();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_running_writer_keeps_its_slot_and_only_one_retry_can_wait() {
        let (store, sync, head, server) = fixture(100).await;
        let gate = BuildGate::install(&store);
        head.store(124, Ordering::SeqCst);
        let first = sync.clone();
        let pending = tokio::spawn(async move { first.sync_once().await });
        gate.entered.notified().await;
        assert_eq!(pending.await.unwrap(), Err(StateError::SyncFailed));
        *store.build_probe.lock().unwrap() = None;
        let next = sync.clone();
        let waiting = tokio::spawn(async move { next.sync_once().await });
        tokio::task::yield_now().await;
        assert_eq!(
            sync.sync_once().await,
            Err(StateError::SyncFailed),
            "excess retry must not join an unbounded writer queue"
        );
        assert!(!gate.watchdog_fired.load(Ordering::SeqCst));
        assert_eq!(store.status().published_versions, 1);
        assert_eq!(quote(app(store.clone())).await["quote_block"], 123);
        gate.release.store(true, Ordering::SeqCst);
        waiting.await.unwrap().unwrap();
        assert_eq!(store.acquire().unwrap().block().number, 124);
        server.abort();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn timed_out_cpu_build_cannot_publish_its_late_result() {
        let (store, sync, head, server) = fixture(80).await;
        let gate = BuildGate::install(&store);
        head.store(124, Ordering::SeqCst);
        let first = sync.clone();
        let pending = tokio::spawn(async move { first.sync_once().await });
        gate.entered.notified().await;
        assert_eq!(pending.await.unwrap(), Err(StateError::SyncFailed));
        assert_eq!(store.acquire().unwrap().block().number, 123);
        *store.build_probe.lock().unwrap() = None;
        head.store(125, Ordering::SeqCst);
        gate.release.store(true, Ordering::SeqCst);
        sync.sync_once().await.unwrap();
        assert_eq!(store.acquire().unwrap().block().number, 125);
        assert_eq!(
            store.status().published_versions,
            2,
            "the timed-out version must never be published"
        );
        server.abort();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn readers_and_health_progress_while_a_new_snapshot_is_still_building() {
        let (store, sync, head, server) = fixture(3000).await;
        let http = app(store.clone());
        let old = quote(http.clone()).await;
        let gate = BuildGate::install(&store);
        head.store(124, Ordering::SeqCst);
        let pending = tokio::spawn(async move { sync.sync_once().await });
        gate.entered.notified().await;
        let health = http
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), 200);
        let during = quote(http.clone()).await;
        assert!(
            !gate.watchdog_fired.load(Ordering::SeqCst),
            "HTTP must progress before the held CPU build is released"
        );
        assert_eq!(during["quote_block"], 123);
        assert_eq!(during["amount_out"], old["amount_out"]);
        gate.release.store(true, Ordering::SeqCst);
        pending.await.unwrap().unwrap();
        assert_eq!(quote(http).await["quote_block"], 124);
        server.abort();
    }
}
