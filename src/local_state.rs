//! Background-owned chain reads and immutable, network-free pricing contexts.
//! The first migration stage covers complete V2/ShibaSwap universes only.
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use alloy_primitives::{Address, U256};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    adapters::decode_reserves,
    error::QuoteError,
    graph::Graph,
    model::{Pool, PoolKind, QuoteRequest, Route},
    router::{PriceSnapshot, Router, RouterConfig},
    rpc::{BlockHeader, RpcClient},
    source::{QuoteSource, constant_product_out, ratio},
    split::{SplitOptimizer, SplitResult},
    state::{GET_RESERVES, multicall},
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
}

impl Default for LocalStateConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 2000,
            sync_timeout_ms: 30_000,
            batch_size: 200,
            max_age_secs: 60,
            max_lag_blocks: 3,
        }
    }
}

impl LocalStateConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=60_000).contains(&self.poll_interval_ms)
            || !(1..=3_600_000).contains(&self.sync_timeout_ms)
            || self.max_age_secs == 0
            || !(1..=400).contains(&self.batch_size)
        {
            return Err("local_state needs poll_interval_ms 1..60000, sync_timeout_ms 1..3600000, positive max_age_secs and batch_size 1..400".into());
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
}

#[derive(Debug, Serialize)]
pub struct QuoteState {
    pub pricing_mode: &'static str,
    pub block_hash: alloy_primitives::B256,
    pub parent_hash: alloy_primitives::B256,
    pub block_timestamp: u64,
    pub state_age_secs: u64,
    pub lag_blocks: u64,
}

struct FrozenV2 {
    pools: HashMap<Address, (Pool, [U256; 2])>,
}

impl FrozenV2 {
    fn reserves(&self, pool: &Pool) -> Result<[U256; 2], QuoteError> {
        let (identity, reserves) = self
            .pools
            .get(&pool.address)
            .ok_or(QuoteError::UnknownPool)?;
        if identity.kind != pool.kind || identity.coins != pool.coins {
            return Err(QuoteError::UnknownPool);
        }
        Ok(*reserves)
    }
}

#[async_trait]
impl QuoteSource for FrozenV2 {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount: U256,
    ) -> Result<U256, QuoteError> {
        let reserves = self.reserves(pool)?;
        if i > 1 || j > 1 || i == j {
            return Err(QuoteError::UnknownPool);
        }
        if reserves.iter().any(U256::is_zero) {
            return Err(QuoteError::InsufficientLiquidity);
        }
        constant_product_out(reserves[i], reserves[j], amount)
    }
    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        let reserves = self.reserves(pool)?;
        if i > 1 || j > 1 || i == j {
            return Err(QuoteError::UnknownPool);
        }
        Ok(ratio(reserves[j], reserves[i]))
    }
    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError> {
        self.reserves(pool)?
            .get(i)
            .copied()
            .ok_or(QuoteError::UnknownPool)
    }
}

/// No RPC/cache handles. The Router's coarse snapshot is derived from this
/// exact frozen source before the complete context is published atomically.
pub struct LocalSnapshot {
    block: BlockHeader,
    graph: Arc<Graph>,
    source: FrozenV2,
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
    fn age_secs(&self) -> u64 {
        now_secs().saturating_sub(self.block.timestamp).max(
            self.initial_age_secs
                .saturating_add(self.published_at.elapsed().as_secs()),
        )
    }
    pub async fn route(&self, request: &QuoteRequest) -> Result<Route, QuoteError> {
        self.router.route(&self.graph, &self.source, request).await
    }
    pub async fn split(
        &self,
        optimizer: &SplitOptimizer,
        request: &QuoteRequest,
        deadline: tokio::time::Instant,
    ) -> Result<SplitResult, QuoteError> {
        let mut result = optimizer
            .quote_until(&self.router, &self.graph, &self.source, request, deadline)
            .await?;
        // FrozenV2 and this Router contain no network-capable collaborators.
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
    fn invalidate(&mut self) {
        self.current = None;
        self.epoch += 1;
        self.last_error = Some(StateError::Reorg);
        self.reorgs += 1;
    }
}

pub struct SnapshotStore {
    graph: Arc<Graph>,
    router_config: RouterConfig,
    config: LocalStateConfig,
    state: RwLock<Published>,
    sync_lock: tokio::sync::Mutex<()>,
    unsupported_pools: Vec<UnsupportedPool>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl SnapshotStore {
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
                !matches!(p.kind, PoolKind::UniswapV2 | PoolKind::ShibaSwapV2) || p.coins.len() != 2
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
            sync_lock: tokio::sync::Mutex::new(()),
            unsupported_pools,
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
        }
    }

    fn invalidate(&self) {
        self.state.write().unwrap().invalidate();
    }
}

/// The only component holding an RPC client. Each new block is fully reloaded
/// in bounded Multicall batches in this first stage; there is no log-gap/cache
/// assumption. Reinitializing from the new canonical head also handles reorgs.
pub struct StateSynchronizer {
    rpc: RpcClient,
    store: Arc<SnapshotStore>,
}

impl StateSynchronizer {
    pub fn new(rpc: RpcClient, store: Arc<SnapshotStore>) -> Self {
        Self { rpc, store }
    }

    pub async fn run(self) {
        loop {
            if let Err(error) = self.sync_once().await {
                tracing::warn!(%error,"local state sync incomplete");
                if error == StateError::Unsupported {
                    return;
                } // fixed startup topology cannot gain support on retry
            }
            tokio::time::sleep(Duration::from_millis(self.store.config.poll_interval_ms)).await;
        }
    }

    pub async fn sync_once(&self) -> Result<(), StateError> {
        let _guard = self.store.sync_lock.lock().await;
        let before = self.rpc.request_count();
        let started = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_millis(self.store.config.sync_timeout_ms),
            self.load(),
        )
        .await
        .unwrap_or(Err(StateError::SyncFailed));
        let mut state = self.store.state.write().unwrap();
        state.sync_rpc_requests += self.rpc.request_count().saturating_sub(before);
        state.last_sync_ms = started.elapsed().as_millis();
        state.last_error = result.as_ref().err().copied();
        result
    }

    async fn load(&self) -> Result<(), StateError> {
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
        let old = {
            let mut state = self.store.state.write().unwrap();
            // A shorter head or a replacement at the published height already
            // proves a fork. Invalidate *all* leases under the same lock that
            // exposes that head, before any further network await.
            if state
                .current
                .as_ref()
                .is_some_and(|old| old.block.number >= head.number && old.block != head)
            {
                state.invalidate();
            }
            state.head = Some(head.clone());
            state.current.clone()
        };
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
        for chunk in self
            .store
            .graph
            .pools()
            .chunks(self.store.config.batch_size)
        {
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
                    .insert(pool.address, (pool.clone(), reserves))
                    .is_some()
                {
                    return Err(StateError::Incomplete);
                }
            }
        }
        let source = FrozenV2 { pools };
        let router = Router::new(self.store.router_config);
        router.set_snapshot(Arc::new(
            PriceSnapshot::build(&self.store.graph, &source).await,
        ));
        let canonical = self
            .rpc
            .block_header(Some(head.number))
            .await
            .map_err(|_| StateError::SyncFailed)?;
        if canonical != head {
            self.store.invalidate();
            return Err(StateError::Reorg);
        }
        let mut state = self.store.state.write().unwrap();
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
        state.current = Some(snapshot);
        state.versions += 1;
        Ok(())
    }
}
