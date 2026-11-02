use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{Address, U256};

use fluxion::adapters::CachingSource;
use fluxion::api::{self, AppState, RateLimiter};
use fluxion::config::Config;
use fluxion::graph::Graph;
use fluxion::model::{Pool, PoolKind};
use fluxion::router::{PriceSnapshot, Router, RouterConfig};
use fluxion::rpc::RpcClient;
use fluxion::source::MockV2;
use fluxion::split::SplitOptimizer;
use fluxion::state::StateCache;
use fluxion::universe::{discover_pools, platform_enabled, pricing_source, warm_coarse_prices};

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

    let app = api::app_with_split(
        state,
        SplitOptimizer::new(config.split.clone()).expect("validated split configuration"),
        refresher.is_some(),
    )
    .route(
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

    let pools = match discover_pools(config, &rpc, key.as_deref()).await {
        Ok(pools) => pools,
        Err(error) => {
            tracing::warn!(%error, "no pools discovered — serving the mock universe");
            return mock_universe(ttl);
        }
    };

    let mut graph = Graph::new();
    for pool in &pools {
        graph.add_pool(pool.clone());
    }

    let (cached, state) = pricing_source(config, rpc);
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
