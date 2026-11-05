//! Shared discovery and pricing assembly for the server and read-only experiments.
//! Discovery fixes the pool inventory; pricing may then use a separately pinned RPC.
use std::sync::Arc;
use std::time::Duration;

use crate::adapters::{
    CachingSource, Curve, Dispatcher, SLOT0, UniswapV2, UniswapV3, decode_reserves,
    v3_price_from_slot0,
};
use crate::config::Config;
use crate::conversion::ProtocolConversions;
use crate::discovery::{self, DiscoveryError};
use crate::graph::Graph;
use crate::model::{Pool, PoolKind};
use crate::rpc::RpcClient;
use crate::state::{GET_RESERVES, StateCache, multicall};
use crate::util::u256_to_f64;
use alloy_primitives::{Address, U256};

pub fn platform_enabled(config: &Config, name: &str) -> bool {
    config.enabled_platforms.is_empty()
        || config
            .enabled_platforms
            .iter()
            .any(|p| p.eq_ignore_ascii_case(name))
}

/// Address-specific policy; matching coins or pool kind do not exclude another deployment.
pub fn retain_enabled_pools(config: &Config, pools: &mut Vec<Pool>) {
    pools.retain(|pool| {
        let keep = !config.excluded_pools.contains(&pool.address);
        if !keep {
            tracing::info!(pool=%pool.address, "excluded pool by explicit configuration");
        }
        keep
    });
}

/// Empty/failed real discovery is reported, never replaced with mock data here.
/// The server owns its offline fallback; experiments must reject empty universes.
pub async fn discover_pools(
    config: &Config,
    rpc: &RpcClient,
    key: Option<&str>,
) -> Result<Vec<Pool>, DiscoveryError> {
    let mut pools = Vec::new();
    if (platform_enabled(config, "uniswap-v2") || platform_enabled(config, "uniswap-v3"))
        && key.is_none_or(|key| key.trim().is_empty())
    {
        return Err(DiscoveryError::Request(
            "GRAPH_API_KEY is required for enabled Uniswap discovery".into(),
        ));
    }
    if platform_enabled(config, "uniswap-v2") {
        match discovery::discover_v2(config, key.unwrap()).await {
            Ok(p) => {
                tracing::info!(count = p.len(), "discovered v2 pools");
                pools.extend(p);
            }
            Err(error) => tracing::warn!(%error, "v2 discovery failed"),
        }
    }
    if platform_enabled(config, "uniswap-v3") {
        match discovery::discover_v3(config, key.unwrap()).await {
            Ok(p) => {
                tracing::info!(count = p.len(), "discovered v3 pools");
                pools.extend(p);
            }
            Err(error) => tracing::warn!(%error, "v3 discovery failed"),
        }
    }
    for fork in &config.v2_forks {
        let kind = if fork.shibaswap {
            PoolKind::ShibaSwapV2
        } else {
            PoolKind::UniswapV2
        };
        match discovery::discover_factory_v2(config, rpc, fork.factory, kind, &fork.name).await {
            Ok(p) => {
                tracing::info!(
                    platform = %fork.name,
                    count = p.len(),
                    "discovered V2 fork pools"
                );
                pools.extend(p);
            }
            Err(error) => tracing::warn!(%error, platform=%fork.name, "V2 fork discovery failed"),
        }
    }
    for subgraph in &config.v3_subgraphs {
        let Some(key) = key.filter(|key| !key.trim().is_empty()) else {
            tracing::warn!(
                platform = %subgraph.name,
                "GRAPH_API_KEY missing; skipping configured V3 subgraph"
            );
            continue;
        };
        match discovery::discover_v3_subgraph(config, key, &subgraph.subgraph_id).await {
            Ok(p) => {
                tracing::info!(
                    platform = %subgraph.name,
                    count = p.len(),
                    "discovered V3 subgraph pools"
                );
                pools.extend(p);
            }
            Err(error) => {
                tracing::warn!(%error, platform=%subgraph.name, "V3 subgraph discovery failed")
            }
        }
    }
    if platform_enabled(config, "curve") {
        match discovery::discover_curve(config, rpc).await {
            Ok(p) => {
                tracing::info!(count = p.len(), "discovered curve pools");
                pools.extend(p);
            }
            Err(error) => tracing::warn!(%error, "curve discovery failed"),
        }
    }
    if platform_enabled(config, "shibaswap-v2") {
        match discovery::discover_shibaswap_v2(config, rpc).await {
            Ok(p) => {
                tracing::info!(count = p.len(), "discovered ShibaSwap V2 pools");
                pools.extend(p);
            }
            Err(error) => tracing::warn!(%error, "ShibaSwap V2 discovery failed"),
        }
    }
    let conversions = ProtocolConversions::new(rpc.clone());
    for configured in config
        .protocol_conversions
        .iter()
        .filter(|conversion| conversion.enabled)
    {
        let pool = configured.pool();
        if !discovery::conversion_allowed(config, &pool) {
            continue;
        }
        match conversions.validate_pool(&pool).await {
            Ok(()) => {
                tracing::info!(contract=%pool.address, "enabled protocol conversion");
                pools.push(pool);
            }
            Err(error) => {
                tracing::warn!(%error, contract=%pool.address, "protocol conversion unavailable")
            }
        }
    }
    retain_enabled_pools(config, &mut pools);
    if pools.is_empty() {
        return Err(DiscoveryError::Request("no real pools discovered".into()));
    }
    Ok(pools)
}

pub fn pricing_source(config: &Config, rpc: RpcClient) -> (Arc<CachingSource>, Arc<StateCache>) {
    let ttl = Duration::from_secs(config.refresh_interval_secs.max(1));
    let state = Arc::new(StateCache::new(rpc.clone(), ttl));
    let source = Dispatcher::new(
        Box::new(UniswapV2::new(state.clone())),
        Some(Box::new(UniswapV3::new(rpc.clone()))),
        Some(Box::new(Curve::new(rpc.clone()))),
    )
    .with_conversions(Box::new(ProtocolConversions::new(rpc)));
    let price_ttl = Duration::from_secs(
        config
            .refresh_interval_secs
            .max(config.snapshot_refresh_secs)
            .max(1),
    );
    (
        Arc::new(CachingSource::new(Box::new(source), price_ttl)),
        state,
    )
}

/// Batched V2/V3 prefetch only seeds coarse caches, not the V2 hot set.
pub async fn warm_coarse_prices(graph: &Graph, cached: &CachingSource, rpc: &RpcClient) {
    let mut v2: Vec<Address> = Vec::new();
    let mut v3: Vec<Address> = Vec::new();
    for pool in graph.pools() {
        match pool.kind {
            PoolKind::UniswapV2 | PoolKind::ShibaSwapV2 => v2.push(pool.address),
            PoolKind::UniswapV3 { .. } => v3.push(pool.address),
            PoolKind::Curve | PoolKind::Conversion { .. } => {}
        }
    }
    let started = std::time::Instant::now();
    for chunk in v2.chunks(400) {
        let calls = chunk
            .iter()
            .map(|address| (*address, GET_RESERVES.to_string()))
            .collect::<Vec<_>>();
        match multicall(rpc, &calls).await {
            Ok(results) => {
                for (address, (ok, data)) in chunk.iter().zip(results) {
                    if !ok {
                        continue;
                    }
                    let Ok([r0, r1]) = decode_reserves(&data) else {
                        continue;
                    };
                    let f0 = u256_to_f64(r0);
                    let f1 = u256_to_f64(r1);
                    if f0 > 0.0 {
                        cached.seed_price(*address, 0, 1, f1 / f0);
                    }
                    if f1 > 0.0 {
                        cached.seed_price(*address, 1, 0, f0 / f1);
                    }
                    cached.seed_reserve(*address, 0, r0);
                    cached.seed_reserve(*address, 1, r1);
                }
            }
            Err(error) => tracing::warn!(%error, "v2 price prefetch failed"),
        }
    }
    for chunk in v3.chunks(400) {
        let calls = chunk
            .iter()
            .map(|address| (*address, SLOT0.to_string()))
            .collect::<Vec<_>>();
        match multicall(rpc, &calls).await {
            Ok(results) => {
                for (address, (ok, data)) in chunk.iter().zip(results) {
                    if !ok {
                        continue;
                    }
                    let Ok(price) = v3_price_from_slot0(&data, 0, 1) else {
                        continue;
                    };
                    if price > 0.0 {
                        cached.seed_price(*address, 0, 1, price);
                        cached.seed_price(*address, 1, 0, 1.0 / price);
                    }
                    cached.seed_reserve(*address, 0, U256::MAX);
                    cached.seed_reserve(*address, 1, U256::MAX);
                }
            }
            Err(error) => tracing::warn!(%error, "v3 price prefetch failed"),
        }
    }
    tracing::info!(
        v2_pools = v2.len(),
        v3_pools = v3.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "warmed coarse prices"
    );
}

#[cfg(test)]
mod exclusion_tests {
    use super::*;
    use crate::{
        curve_local::CurveProfile,
        curve_plain::PlainProfile,
        local_state::{LocalStateConfig, SnapshotStore},
        router::RouterConfig,
    };

    #[test]
    fn explicit_exclusion_removes_only_legacy_deployment_and_is_public() {
        let mut pools = vec![
            CurveProfile::Classic3.pool(),
            PlainProfile::TwoCrv.pool(),
            PlainProfile::FraxUsdc.pool(),
            CurveProfile::Tricrypto2.pool(),
        ];
        let mut legacy = CurveProfile::Tricrypto2.pool();
        legacy.address = "0x80466c64868e1ab14a1ddf27a676c3fcbe638fe5"
            .parse()
            .unwrap();
        pools.push(legacy.clone());
        let before = pools.iter().map(|p| p.address).collect::<Vec<_>>();
        retain_enabled_pools(&Config::default(), &mut pools);
        assert_eq!(pools.iter().map(|p| p.address).collect::<Vec<_>>(), before);
        let config: Config = serde_yaml::from_str(
            "excluded_pools: [\"0x80466c64868e1ab14a1ddf27a676c3fcbe638fe5\"]",
        )
        .unwrap();
        retain_enabled_pools(&config, &mut pools);
        assert_eq!(pools.len(), 4);
        assert!(!pools.iter().any(|p| p.address == legacy.address));
        assert!(
            pools
                .iter()
                .any(|p| p.address == CurveProfile::Tricrypto2.pool().address)
        );
        assert_eq!(
            config.public_settings()["excluded_pools"][0],
            serde_json::json!(legacy.address)
        );
        let mut graph = Graph::new();
        for pool in pools {
            graph.add_pool(pool);
        }
        let store = SnapshotStore::new(graph, RouterConfig::default(), LocalStateConfig::default())
            .unwrap();
        assert_eq!(store.status().total_pools, 4);
        assert!(store.status().unsupported_pools.is_empty());
        assert!(serde_yaml::from_str::<Config>("excluded_pools: [not-an-address]").is_err());
    }
}
