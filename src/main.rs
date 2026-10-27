use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{Address, U256};

use fluxion::adapters::{CachingSource, Curve, Dispatcher, UniswapV2, UniswapV3};
use fluxion::api::{self, AppState, RateLimiter};
use fluxion::config::Config;
use fluxion::discovery;
use fluxion::graph::Graph;
use fluxion::model::{Pool, PoolKind};
use fluxion::router::{PriceSnapshot, Router, RouterConfig};
use fluxion::rpc::RpcClient;
use fluxion::source::MockV2;
use fluxion::state::StateCache;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    dotenvy::dotenv().ok();

    let config = Config::from_file("config.yaml").unwrap_or_default();
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

    // Build the initial coarse-stage price snapshot.
    tracing::info!(edges = graph.edge_count(), "building price snapshot");
    let snapshot = PriceSnapshot::build(&graph, cached.as_ref()).await;
    router.set_snapshot(Arc::new(snapshot));
    tracing::info!("snapshot ready");

    // Background: refresh V2 reserves and rebuild the price snapshot.
    {
        let bg_router = router.clone();
        let bg_graph = graph.clone();
        let bg_cached = cached.clone();
        let interval = Duration::from_secs(config.refresh_interval_secs.max(1));
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                if let Some(state) = &refresher
                    && let Err(e) = state.refresh_hot().await
                {
                    tracing::warn!(error = %e, "hot-pool refresh failed");
                }
                let snap = PriceSnapshot::build(&bg_graph, bg_cached.as_ref()).await;
                bg_router.set_snapshot(Arc::new(snap));
            }
        });
    }

    let state = Arc::new(AppState {
        graph,
        source: cached,
        router,
        limiter: RateLimiter::new(1000, Duration::from_secs(1)),
        rpc: Some(rpc),
    });

    let app = api::app(state);
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

async fn build_universe(
    config: &Config,
    rpc: RpcClient,
) -> (Graph, Arc<CachingSource>, Option<Arc<StateCache>>) {
    let ttl = Duration::from_secs(config.refresh_interval_secs.max(1));

    let Some(key) = std::env::var("GRAPH_API_KEY").ok() else {
        tracing::warn!("GRAPH_API_KEY not set — serving the mock universe");
        return mock_universe(ttl);
    };

    let mut pools: Vec<Pool> = Vec::new();
    if platform_enabled(config, "uniswap-v2") {
        match discovery::discover_v2(config, &key).await {
            Ok(p) => {
                tracing::info!(count = p.len(), "discovered v2 pools");
                pools.extend(p);
            }
            Err(e) => tracing::warn!(error = %e, "v2 discovery failed"),
        }
    }
    if platform_enabled(config, "uniswap-v3") {
        match discovery::discover_v3(config, &key).await {
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
    );
    let cached = Arc::new(CachingSource::new(Box::new(dispatcher), ttl));
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
