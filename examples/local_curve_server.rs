//! Explicit verified-deployment HTTP demo; does not filter production discovery.
use fluxion::{
    api::{self, AppState, RateLimiter},
    config::Config,
    curve_local::CurveProfile,
    curve_plain::PlainProfile,
    discovery::token_allowed,
    graph::Graph,
    local_state::{PricingMode, SnapshotStore, StateSynchronizer},
    router::{Router, RouterConfig},
    rpc::RpcClient,
    split::SplitOptimizer,
    universe::{platform_enabled, pricing_source},
};
use std::{sync::Arc, time::Duration};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let mut path = "config.local-curve.example.yaml".to_string();
    let mut all_stable = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--config" => path = args.next().expect("missing config"),
            "--all-stable" => all_stable = true,
            _ => panic!("use --config PATH [--all-stable]"),
        }
    }
    let config = Config::from_file(&path).expect("valid configuration");
    assert_eq!(
        config.pricing_mode,
        PricingMode::LocalSnapshot,
        "this example requires local_snapshot mode"
    );
    let rpc = RpcClient::new(config.rpc_url.clone());
    assert!(
        platform_enabled(&config, "curve"),
        "the demo requires enabled curve pricing"
    );
    let mut graph = Graph::new();
    let mut profiles = vec![CurveProfile::Classic3];
    if all_stable {
        profiles.extend([
            CurveProfile::Plain(PlainProfile::TwoCrv),
            CurveProfile::Plain(PlainProfile::FraxUsdc),
        ]);
    }
    for profile in profiles {
        let pool = profile.pool();
        assert!(
            pool.coins.iter().all(|c| token_allowed(&config, *c)),
            "all selected pool coins must be allowed"
        );
        graph.add_pool(pool);
    }
    let router_config = RouterConfig {
        max_hops: config.max_hops,
        top_k: config.top_k,
        lambda: config.slippage_penalty_lambda,
        reserve_ratio_k: config.reserve_ratio_k,
    };
    let store = Arc::new(
        SnapshotStore::new(graph.clone(), router_config, config.local_state.clone()).unwrap(),
    );
    let (source, _) = pricing_source(&config, rpc.clone());
    let info = serde_json::json!({"mode":"mainnet","profile":if all_stable {"explicit-verified-stable-only"} else {"explicit-classic-3pool-only"},"discovery":"not run: fixed verified deployment demo", "config":config.public_settings(),"pools":graph.pool_count(),"tokens":graph.tokens().len(),"edges":graph.edge_count(),"platform_edges":{"curve":graph.edge_count()}});
    let state = Arc::new(AppState {
        graph,
        source,
        router: Router::new(router_config),
        limiter: RateLimiter::new(1000, Duration::from_secs(1)),
        rpc: Some(rpc.clone()),
        request_timeout: Duration::from_millis(config.request_timeout_ms.max(1)),
    });
    let app = api::app_with_local_state(
        state,
        SplitOptimizer::new(config.split.clone()).unwrap(),
        store.clone(),
    )
    .route(
        "/info",
        axum::routing::get(move || async move { axum::Json(info) }),
    );
    tokio::spawn(StateSynchronizer::new(rpc, store).run());
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", config.host, config.port))
        .await
        .unwrap();
    println!(
        "explicit verified Curve demo on {}",
        listener.local_addr().unwrap()
    );
    axum::serve(listener, app).await.unwrap();
}
