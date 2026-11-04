use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{Address, U256};

use fluxion::adapters::CachingSource;
use fluxion::api::{self, AppState, RateLimiter};
use fluxion::config::Config;
use fluxion::graph::Graph;
use fluxion::local_state::{PricingMode, SnapshotStore, StateSynchronizer};
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

    // Opt-in migration: a complete local universe is required. Unsupported
    // venues are reported as unavailable, never silently removed from the graph.
    let local_store = if config.pricing_mode == PricingMode::LocalSnapshot {
        let store = Arc::new(
            SnapshotStore::new(
                graph.clone(),
                RouterConfig {
                    max_hops: config.max_hops,
                    top_k: config.top_k,
                    lambda: config.slippage_penalty_lambda,
                    reserve_ratio_k: config.reserve_ratio_k,
                },
                config.local_state.clone(),
            )
            .expect("validated local state configuration"),
        );
        if refresher.is_some() {
            let sync = StateSynchronizer::new(rpc.clone(), store.clone());
            tokio::spawn(sync.run());
        } else {
            tracing::warn!(
                "local_snapshot requires real chain data; no snapshot published for mock fallback"
            );
        }
        Some(store)
    } else {
        None
    };

    if local_store.is_none() && refresher.is_some() {
        warm_coarse_prices(&graph, cached.as_ref(), &rpc).await;
    }

    // Build the initial coarse-stage price snapshot. With a large pool set this
    // can take a while, so log the cost instead of failing silently.
    if local_store.is_none() {
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
    }

    // Background: refresh hot V2 reserves on the fast interval.
    if local_store.is_none() {
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
    if local_store.is_none() {
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

    // Add-only universe refresh: discovered pools are loaded into a candidate
    // graph and atomically published by the synchronizer, so existing quotes
    // keep using the old complete world until the swap.
    if let Some(store) = local_store.as_ref()
        && refresher.is_some()
        && config.universe_refresh_secs > 0
    {
        let store = store.clone();
        let config = config.clone();
        let rpc = rpc.clone();
        tokio::spawn(async move {
            let interval = Duration::from_secs(config.universe_refresh_secs.max(1));
            loop {
                tokio::time::sleep(interval).await;
                let key = std::env::var("GRAPH_API_KEY")
                    .ok()
                    .filter(|key| !key.trim().is_empty());
                match discover_pools(&config, &rpc, key.as_deref()).await {
                    Ok(pools) => {
                        let added = store.queue_new_pools(pools);
                        if added > 0 {
                            tracing::info!(added, "queued discovered pools for next snapshot");
                        }
                    }
                    Err(error) => tracing::warn!(%error, "universe refresh failed"),
                }
            }
        });
    }

    let info_mode = if refresher.is_some() {
        "mainnet"
    } else {
        "mock"
    };
    let info_config = config.clone();
    let info_store = local_store.clone();
    let info_graph = graph.clone();
    let state = Arc::new(AppState {
        graph,
        source: cached,
        router,
        limiter: RateLimiter::new(1000, Duration::from_secs(1)),
        rpc: Some(rpc),
        request_timeout: Duration::from_millis(config.request_timeout_ms.max(1)),
    });

    let optimizer =
        SplitOptimizer::new(config.split.clone()).expect("validated split configuration");
    let app = match local_store {
        Some(store) => api::app_with_local_state(state, optimizer, store),
        None => api::app_with_split(state, optimizer, refresher.is_some()),
    }
    .route(
        "/info",
        axum::routing::get(move || {
            let graph = info_store.as_ref().map_or_else(
                || std::sync::Arc::new(info_graph.clone()),
                |store| store.graph(),
            );
            async move { axum::Json(service_info(info_mode, &info_config, &graph)) }
        }),
    );
    let app = if std::env::var_os("FLUXION_DIAG").is_some() {
        tracing::info!(threads=?std::thread::available_parallelism(), "diag_runtime");
        tokio::spawn(async {
            loop {
                let at = std::time::Instant::now();
                tokio::time::sleep(Duration::from_millis(50)).await;
                let elapsed_ms = at.elapsed().as_millis();
                if elapsed_ms > 150 {
                    tracing::warn!(elapsed_ms, "diag_async_heartbeat_late");
                }
            }
        });
        std::thread::spawn(|| {
            loop {
                let at = std::time::Instant::now();
                std::thread::sleep(Duration::from_millis(50));
                let elapsed_ms = at.elapsed().as_millis();
                if elapsed_ms > 150 {
                    tracing::warn!(elapsed_ms, "diag_os_heartbeat_late");
                }
            }
        });
        app.layer(axum::middleware::from_fn(|request: axum::extract::Request, next: axum::middleware::Next| async move {
            let id = request.headers().get("x-diag-id").and_then(|s| s.to_str().ok()).unwrap_or("").to_owned();
            let received = std::time::Instant::now();
            if !id.is_empty() { tracing::info!(id=%id, "diag_http_enter"); }
            let response = next.run(request).await;
            if !id.is_empty() { tracing::info!(id=%id, elapsed_us=received.elapsed().as_micros(), "diag_http_response_ready"); }
            response
        }))
    } else {
        app
    };
    let addr: std::net::SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("invalid host/port in config");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::info!("listening on http://{addr}");
    println!("listening on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}

fn service_info(mode: &str, config: &Config, graph: &Graph) -> serde_json::Value {
    let mut platform_edges: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for idx in 0..graph.edge_count() {
        *platform_edges
            .entry(graph.pool(graph.edge(idx).pool_idx).kind.name())
            .or_default() += 1;
    }
    serde_json::json!({
        "mode": mode,
        "config": config.public_settings(),
        "pools": graph.pools().len(),
        "tokens": graph.tokens().len(),
        "edges": graph.edge_count(),
        "platform_edges": platform_edges,
    })
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
