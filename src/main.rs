use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{Address, U256};

use fluxion::adapters::{
    CachingSource, Curve, Dispatcher, SLOT0, UniswapV2, UniswapV3, decode_reserves,
    v3_price_from_slot0,
};
use fluxion::api::{self, AppState, RateLimiter};
use fluxion::config::Config;
use fluxion::conversion::ProtocolConversions;
use fluxion::discovery;
use fluxion::graph::Graph;
use fluxion::model::{Pool, PoolKind};
use fluxion::router::{PriceSnapshot, Router, RouterConfig};
use fluxion::rpc::RpcClient;
use fluxion::source::MockV2;
use fluxion::state::{GET_RESERVES, StateCache, multicall};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    dotenvy::dotenv().ok();

    let mut args = std::env::args().skip(1);
    let config_path = match args.next().as_deref() {
        None => "config.yaml".to_string(),
        Some("--config") => args.next().expect("--config needs a YAML path"),
        Some("--help" | "-h") => {
            println!("Usage: cargo run -- [--config PATH]");
            return;
        }
        Some(other) => panic!("unknown argument: {other}"),
    };
    assert!(args.next().is_none(), "unexpected arguments");
    let config = Config::load(&config_path).expect("invalid configuration");
    tracing::info!(
        max_hops = config.max_hops,
        max_pools = config.max_pools,
        "loaded config"
    );

    let router = Router::new(RouterConfig {
        max_hops: config.max_hops,
        top_k: config.top_k,
        lambda: config.slippage_penalty_lambda,
        reserve_ratio_k: config.reserve_ratio_k,
    });

    let rpc = RpcClient::new(config.rpc_url.clone());
    let (graph, cached, refresher) = build_universe(&config, rpc.clone()).await;

    if refresher.is_some() {
        warm_coarse_prices(&graph, cached.as_ref(), &rpc).await;
    }

    // Build the initial coarse-stage price snapshot. With a large pool set this
    // can take a while, so log the cost instead of failing silently.
    tracing::info!(
        edges = graph.edge_count(),
        tokens = graph.tokens().len(),
        "building price snapshot"
    );
    let build_started = std::time::Instant::now();
    let snapshot = PriceSnapshot::build(&graph, cached.as_ref()).await;
    tracing::info!(
        edges = graph.edge_count(),
        elapsed_ms = build_started.elapsed().as_millis() as u64,
        "snapshot ready"
    );
    router.set_snapshot(Arc::new(snapshot));

    // Background: refresh hot V2 reserves on the fast interval.
    {
        let interval = Duration::from_secs(config.refresh_interval_secs.max(1));
        let refresher = refresher.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                if let Some(state) = &refresher
                    && let Err(e) = state.refresh_hot().await
                {
                    tracing::warn!(error = %e, "hot-pool refresh failed");
                }
            }
        });
    }

    // Background: rebuild the full price snapshot on the slower interval. The
    // hot refresh above keeps V2 reserves warm in between rebuilds.
    {
        let bg_router = router.clone();
        let bg_graph = graph.clone();
        let bg_cached = cached.clone();
        let bg_refresher = refresher.clone();
        let bg_rpc = rpc.clone();
        let interval = Duration::from_secs(config.snapshot_refresh_secs.max(1));
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                if bg_refresher.is_some() {
                    warm_coarse_prices(&bg_graph, bg_cached.as_ref(), &bg_rpc).await;
                }
                let started = std::time::Instant::now();
                let snap = PriceSnapshot::build(&bg_graph, bg_cached.as_ref()).await;
                bg_router.set_snapshot(Arc::new(snap));
                tracing::info!(
                    edges = bg_graph.edge_count(),
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "snapshot rebuilt"
                );
            }
        });
    }

    let mut platform_edges: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for idx in 0..graph.edge_count() {
        *platform_edges
            .entry(graph.pool(graph.edge(idx).pool_idx).kind.name())
            .or_default() += 1;
    }
    let info = serde_json::json!({
        "mode": if refresher.is_some() { "mainnet" } else { "mock" },
        "config": config.public_settings(),
        "pools": graph.pools().len(),
        "tokens": graph.tokens().len(),
        "edges": graph.edge_count(),
        "platform_edges": platform_edges,
    });
    let state = Arc::new(AppState {
        graph,
        source: cached,
        router,
        limiter: RateLimiter::new(1000, Duration::from_secs(1)),
        rpc: Some(rpc),
        request_timeout: Duration::from_millis(config.request_timeout_ms.max(1)),
    });

    let app = api::app(state).route(
        "/info",
        axum::routing::get(move || async move { axum::Json(info) }),
    );
    let addr: std::net::SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("invalid host/port in config");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::info!("listening on http://{addr}");
    println!("listening on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}

fn platform_enabled(config: &Config, name: &str) -> bool {
    config.enabled_platforms.is_empty()
        || config
            .enabled_platforms
            .iter()
            .any(|p| p.eq_ignore_ascii_case(name))
}

fn u256_f64(value: U256) -> f64 {
    value.to_string().parse().unwrap_or(f64::INFINITY)
}

/// Batch-fetch V2 reserves and V3 `slot0` through Multicall3 and seed the
/// coarse price/reserve caches. This keeps a 20k-pool snapshot build to a
/// handful of RPC round-trips instead of tens of thousands of `eth_call`s.
async fn warm_coarse_prices(graph: &Graph, cached: &CachingSource, rpc: &RpcClient) {
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
        let calls: Vec<(Address, String)> = chunk
            .iter()
            .map(|address| (*address, GET_RESERVES.to_string()))
            .collect();
        match multicall(rpc, &calls).await {
            Ok(results) => {
                for (address, (ok, data)) in chunk.iter().zip(results) {
                    if !ok {
                        continue;
                    }
                    let Ok([r0, r1]) = decode_reserves(&data) else {
                        continue;
                    };
                    let f0 = u256_f64(r0);
                    let f1 = u256_f64(r1);
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
            Err(error) => tracing::warn!(error = %error, "v2 price prefetch failed"),
        }
    }

    for chunk in v3.chunks(400) {
        let calls: Vec<(Address, String)> = chunk
            .iter()
            .map(|address| (*address, SLOT0.to_string()))
            .collect();
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
            Err(error) => tracing::warn!(error = %error, "v3 price prefetch failed"),
        }
    }

    tracing::info!(
        v2_pools = v2.len(),
        v3_pools = v3.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "warmed coarse prices"
    );
}

async fn build_universe(
    config: &Config,
    rpc: RpcClient,
) -> (Graph, Arc<CachingSource>, Option<Arc<StateCache>>) {
    let ttl = Duration::from_secs(config.refresh_interval_secs.max(1));

    let key = std::env::var("GRAPH_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty());
    let graph_platform_enabled =
        platform_enabled(config, "uniswap-v2") || platform_enabled(config, "uniswap-v3");
    if key.is_none() && graph_platform_enabled {
        tracing::warn!("GRAPH_API_KEY not set — serving the mock universe");
        return mock_universe(ttl);
    }

    let mut pools: Vec<Pool> = Vec::new();
    if platform_enabled(config, "uniswap-v2") {
        match discovery::discover_v2(config, key.as_deref().unwrap()).await {
            Ok(p) => {
                tracing::info!(count = p.len(), "discovered v2 pools");
                pools.extend(p);
            }
            Err(e) => tracing::warn!(error = %e, "v2 discovery failed"),
        }
    }
    if platform_enabled(config, "uniswap-v3") {
        match discovery::discover_v3(config, key.as_deref().unwrap()).await {
            Ok(p) => {
                tracing::info!(count = p.len(), "discovered v3 pools");
                pools.extend(p);
            }
            Err(e) => tracing::warn!(error = %e, "v3 discovery failed"),
        }
    }
    if platform_enabled(config, "curve") {
        match discovery::discover_curve(config, &rpc).await {
            Ok(p) => {
                tracing::info!(count = p.len(), "discovered curve pools");
                pools.extend(p);
            }
            Err(e) => tracing::warn!(error = %e, "curve discovery failed"),
        }
    }

    if platform_enabled(config, "shibaswap-v2") {
        match discovery::discover_shibaswap_v2(config, &rpc).await {
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
                tracing::info!(contract = %pool.address, "enabled protocol conversion");
                pools.push(pool);
            }
            Err(error) => {
                tracing::warn!(%error, contract = %pool.address, "protocol conversion unavailable")
            }
        }
    }

    if pools.is_empty() {
        tracing::warn!("no pools discovered — serving the mock universe");
        return mock_universe(ttl);
    }

    let mut graph = Graph::new();
    for pool in &pools {
        graph.add_pool(pool.clone());
    }

    let state = Arc::new(StateCache::new(rpc.clone(), ttl));
    let dispatcher = Dispatcher::new(
        Box::new(UniswapV2::new(state.clone())),
        Some(Box::new(UniswapV3::new(rpc.clone()))),
        Some(Box::new(Curve::new(rpc))),
    )
    .with_conversions(Box::new(conversions));
    // The coarse-price cache must outlive a full prefetch + snapshot build;
    // otherwise early seeds expire before the build finishes and every edge
    // falls back to a per-edge RPC call.
    let price_ttl = Duration::from_secs(
        config
            .refresh_interval_secs
            .max(config.snapshot_refresh_secs)
            .max(1),
    );
    let cached = Arc::new(CachingSource::new(Box::new(dispatcher), price_ttl));
    (graph, cached, Some(state))
}

fn mock_universe(ttl: Duration) -> (Graph, Arc<CachingSource>, Option<Arc<StateCache>>) {
    let eth = Address::with_last_byte(1);
    let usdc = Address::with_last_byte(2);
    let wbtc = Address::with_last_byte(3);

    let mut graph = Graph::new();
    let a = Address::with_last_byte(10);
    let b = Address::with_last_byte(11);
    let c = Address::with_last_byte(12);
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: a,
        coins: vec![eth, usdc],
    });
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: b,
        coins: vec![eth, usdc],
    });
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: c,
        coins: vec![usdc, wbtc],
    });

    let mut reserves = HashMap::new();
    reserves.insert(a, [U256::from(1000u64), U256::from(3_000_000u64)]);
    reserves.insert(b, [U256::from(500u64), U256::from(2_000_000u64)]);
    reserves.insert(c, [U256::from(1_000_000u64), U256::from(40u64)]);
    let cached = Arc::new(CachingSource::new(Box::new(MockV2::new(reserves)), ttl));
    (graph, cached, None)
}
