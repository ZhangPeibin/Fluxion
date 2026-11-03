use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alloy_primitives::{Address, U256};
use axum::{Json, Router, routing::post};
use fluxion::{
    api::{self, AppState, RateLimiter},
    graph::Graph,
    local_state::{LocalStateConfig, SnapshotStore, StateSynchronizer},
    model::{Pool, PoolKind},
    router::{Router as QuoteRouter, RouterConfig},
    rpc::RpcClient,
    source::FixedRate,
    split::{SplitConfig, SplitOptimizer},
};
use serde_json::{Value, json};
use tower::ServiceExt;

fn graph() -> Graph {
    let mut graph = Graph::new();
    for id in 10..14 {
        graph.add_pool(Pool {
            kind: if id == 10 {
                PoolKind::ShibaSwapV2
            } else {
                PoolKind::UniswapV2
            },
            address: Address::with_last_byte(id),
            coins: vec![Address::with_last_byte(1), Address::with_last_byte(2)],
        });
    }
    graph
}

fn header() -> Value {
    json!({"number":"0x7b", "hash":format!("0x{:064x}",123),
        "parentHash":format!("0x{:064x}",122),
        "timestamp":format!("0x{:x}",SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs())})
}

// ABI fixture: (bool, bytes)[] with four getReserves results. It intentionally
// does not use the production encoder/decoder to generate its expected output.
fn reserve_results() -> String {
    reserve_fixture(100_000)
}

fn reserve_fixture(output_reserve: u64) -> String {
    let mut data = format!("{:064x}{:064x}", 32, 4);
    for idx in 0..4 {
        data += &format!("{:064x}", 4 * 32 + idx * 192);
    }
    for _ in 0..4 {
        data += &format!(
            "{:064x}{:064x}{:064x}{:064x}{:064x}{:064x}",
            1, 64, 96, 1000, output_reserve, 0
        );
    }
    format!("0x{data}")
}

struct Chain {
    headers: std::collections::BTreeMap<u64, Value>,
    output_reserve: u64,
    fail_calls: bool,
    incomplete_batch: bool,
    reorg_on_call: bool,
    call_delay_ms: u64,
    numbered_header_delay_ms: u64,
}

impl Chain {
    fn advance(&mut self, number: u64, hash: u64, parent: u64) {
        let mut block = header();
        block["number"] = json!(format!("0x{number:x}"));
        block["hash"] = json!(format!("0x{hash:064x}"));
        block["parentHash"] = json!(format!("0x{parent:064x}"));
        self.headers.insert(number, block);
    }

    fn reply(&mut self, request: Value) -> (u64, Value) {
        if request["method"] == "eth_getBlockByNumber" {
            let tag = request["params"][0].as_str().unwrap();
            let block = if tag == "latest" {
                self.headers.last_key_value().unwrap().1.clone()
            } else {
                self.headers
                    .get(&u64::from_str_radix(tag.trim_start_matches("0x"), 16).unwrap())
                    .cloned()
                    .unwrap_or(Value::Null)
            };
            return (
                if tag == "latest" {
                    0
                } else {
                    self.numbered_header_delay_ms
                },
                json!({"jsonrpc":"2.0","id":1,"result":block}),
            );
        }
        assert_eq!(request["method"], "eth_call");
        if self.fail_calls {
            return (
                0,
                json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"test batch failure"}}),
            );
        }
        let mut result = reserve_fixture(self.output_reserve);
        if self.incomplete_batch {
            // Mark the final tuple unsuccessful, after earlier valid pools.
            result.replace_range(
                2 + (2 + 4 + 3 * 6) * 64..2 + (2 + 4 + 3 * 6 + 1) * 64,
                &format!("{:064x}", 0),
            );
        }
        if self.reorg_on_call {
            self.reorg_on_call = false;
            self.headers.last_entry().unwrap().get_mut()["hash"] = json!(format!("0x{:064x}", 999));
        }
        (
            self.call_delay_ms,
            json!({"jsonrpc":"2.0","id":1,"result":result}),
        )
    }
}

async fn chain_rpc() -> (
    RpcClient,
    Arc<std::sync::Mutex<Chain>>,
    tokio::task::JoinHandle<()>,
) {
    let chain = Arc::new(std::sync::Mutex::new(Chain {
        headers: std::collections::BTreeMap::from([(123, header())]),
        output_reserve: 100_000,
        fail_calls: false,
        incomplete_batch: false,
        reorg_on_call: false,
        call_delay_ms: 0,
        numbered_header_delay_ms: 0,
    }));
    let control = chain.clone();
    let app = Router::new().route(
        "/",
        post(move |Json(request): Json<Value>| {
            let chain = control.clone();
            async move {
                let (delay, result) = chain.lock().unwrap().reply(request);
                tokio::time::sleep(Duration::from_millis(delay)).await;
                Json(result)
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = RpcClient::new(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (rpc, chain, server)
}

#[tokio::test]
async fn observing_a_same_height_fork_invalidates_older_leases_before_more_rpc() {
    let (rpc, chain, server) = chain_rpc().await;
    let store = Arc::new(
        SnapshotStore::new(
            graph(),
            RouterConfig::default(),
            LocalStateConfig::default(),
        )
        .unwrap(),
    );
    let sync = Arc::new(StateSynchronizer::new(rpc, store.clone()));
    sync.sync_once().await.unwrap();
    let older = store.acquire().unwrap();
    chain.lock().unwrap().advance(124, 124, 123);
    sync.sync_once().await.unwrap();
    {
        let mut chain = chain.lock().unwrap();
        chain.advance(123, 998, 997);
        chain.advance(124, 999, 998);
        chain.numbered_header_delay_ms = 500;
        chain.call_delay_ms = 500;
    }
    let pending = tokio::spawn(async move { sync.sync_once().await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while store
            .status()
            .observed_head
            .as_ref()
            .unwrap()
            .hash
            .to_string()
            != format!("0x{:064x}", 999)
        {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        store.quote_state(&older).is_err(),
        "a known fork must invalidate all previous leases before waiting for more RPC"
    );
    pending.await.unwrap().unwrap();
    assert!(store.status().ready);
    server.abort();
}

#[tokio::test]
async fn a_fork_change_during_loading_is_never_published() {
    let (rpc, chain, server) = chain_rpc().await;
    chain.lock().unwrap().reorg_on_call = true;
    let store = Arc::new(
        SnapshotStore::new(
            graph(),
            RouterConfig::default(),
            LocalStateConfig::default(),
        )
        .unwrap(),
    );
    let sync = StateSynchronizer::new(rpc, store.clone());
    assert_eq!(
        sync.sync_once().await.unwrap_err(),
        fluxion::local_state::StateError::Reorg
    );
    assert!(!store.status().ready);
    assert_eq!(store.status().published_versions, 0);
    sync.sync_once().await.unwrap();
    assert_eq!(
        store.acquire().unwrap().block().hash.to_string(),
        format!("0x{:064x}", 999)
    );
    server.abort();
}

#[tokio::test]
async fn sync_deadline_cancels_an_incomplete_load_and_can_retry() {
    let (rpc, chain, server) = chain_rpc().await;
    chain.lock().unwrap().call_delay_ms = 500;
    let store = Arc::new(
        SnapshotStore::new(
            graph(),
            RouterConfig::default(),
            LocalStateConfig {
                sync_timeout_ms: 100,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let sync = StateSynchronizer::new(rpc, store.clone());
    assert_eq!(
        sync.sync_once().await.unwrap_err(),
        fluxion::local_state::StateError::SyncFailed
    );
    assert!(!store.status().ready);
    assert_eq!(store.status().published_versions, 0);
    chain.lock().unwrap().call_delay_ms = 0;
    sync.sync_once().await.unwrap();
    assert!(store.status().ready);
    server.abort();
}

fn quote_request() -> fluxion::model::QuoteRequest {
    fluxion::model::QuoteRequest {
        token_in: Address::with_last_byte(1),
        token_out: Address::with_last_byte(2),
        amount_in: U256::from(1001),
        max_hops: None,
        amount_out_min: None,
    }
}

#[tokio::test]
async fn readers_keep_one_version_but_old_leases_are_rejected_after_deep_reorg() {
    let (rpc, chain, server) = chain_rpc().await;
    let store = Arc::new(
        SnapshotStore::new(
            graph(),
            RouterConfig::default(),
            LocalStateConfig::default(),
        )
        .unwrap(),
    );
    let sync = StateSynchronizer::new(rpc, store.clone());
    sync.sync_once().await.unwrap();
    let old = store.acquire().unwrap();
    let original = old.route(&quote_request()).await.unwrap();
    {
        let mut chain = chain.lock().unwrap();
        chain.advance(124, 124, 123);
        chain.output_reserve = 200_000;
    }
    sync.sync_once().await.unwrap();
    assert_eq!(
        old.route(&quote_request()).await.unwrap().amount_out,
        original.amount_out
    );
    let latest = store.acquire().unwrap();
    assert_eq!(
        latest.route(&quote_request()).await.unwrap().amount_out,
        U256::from(99_899)
    );
    assert!(store.quote_state(&old).is_ok());
    {
        let mut chain = chain.lock().unwrap();
        chain.advance(123, 997, 996);
        chain.advance(124, 998, 997);
        chain.advance(125, 999, 998);
    }
    sync.sync_once().await.unwrap();
    assert!(store.quote_state(&latest).is_err());
    assert!(
        store.quote_state(&old).is_err(),
        "leases older than the replaced current version must also be invalidated"
    );
    assert_eq!(store.status().reorgs, 1);
    assert!(store.status().ready);
    server.abort();
}

#[tokio::test]
async fn failed_or_incomplete_updates_keep_the_complete_version_and_enforce_lag() {
    let (rpc, chain, server) = chain_rpc().await;
    let config = LocalStateConfig {
        max_lag_blocks: 1,
        ..Default::default()
    };
    let store = Arc::new(SnapshotStore::new(graph(), RouterConfig::default(), config).unwrap());
    let sync = StateSynchronizer::new(rpc, store.clone());
    sync.sync_once().await.unwrap();
    {
        let mut c = chain.lock().unwrap();
        c.advance(124, 124, 123);
        c.output_reserve = 200_000;
        c.incomplete_batch = true;
    }
    assert_eq!(
        sync.sync_once().await.unwrap_err(),
        fluxion::local_state::StateError::Incomplete
    );
    assert_eq!(
        store
            .acquire()
            .unwrap()
            .route(&quote_request())
            .await
            .unwrap()
            .amount_out,
        U256::from(49_949)
    );
    assert_eq!(store.status().published_versions, 1);
    assert_eq!(store.status().lag_blocks, Some(1));
    {
        let mut c = chain.lock().unwrap();
        c.advance(127, 127, 126);
        c.incomplete_batch = false;
        c.fail_calls = true;
    }
    assert!(sync.sync_once().await.is_err());
    assert!(matches!(
        store.acquire(),
        Err(fluxion::local_state::StateError::Lagging)
    ));
    assert!(!store.status().ready);
    {
        chain.lock().unwrap().fail_calls = false;
    }
    sync.sync_once().await.unwrap(); // full rebootstrap after a gap, no missed-log assumption
    assert_eq!(store.acquire().unwrap().block().number, 127);
    assert_eq!(store.status().loaded_pools, 4);
    assert!(store.status().last_sync_error.is_none());
    server.abort();
}

async fn request(app: axum::Router, body: Value) -> (u16, Value) {
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/quote")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

fn local_app(graph: Graph, rpc: RpcClient, store: Arc<SnapshotStore>) -> axum::Router {
    api::app_with_local_state(
        Arc::new(AppState {
            graph,
            source: Arc::new(FixedRate::new(U256::from(1))),
            router: QuoteRouter::default(),
            limiter: RateLimiter::new(100, Duration::from_secs(1)),
            rpc: Some(rpc),
            request_timeout: Duration::from_secs(2),
        }),
        SplitOptimizer::new(SplitConfig::multi_pool()).unwrap(),
        store,
    )
}

async fn get(app: axum::Router, path: &str) -> (u16, Value) {
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(path)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn http_distinguishes_cold_unsupported_and_stale_state_without_rpc_fallback() {
    let rpc = RpcClient::new("http://127.0.0.1:1".into());
    let store = Arc::new(
        SnapshotStore::new(
            graph(),
            RouterConfig::default(),
            LocalStateConfig::default(),
        )
        .unwrap(),
    );
    let app = local_app(graph(), rpc.clone(), store);
    let body = json!({"token_in":Address::with_last_byte(1),"token_out":Address::with_last_byte(2),"amount_in":"1001"});
    assert_eq!(
        request(app.clone(), body.clone()).await,
        (
            503,
            json!({"error":"SnapshotNotReady","pricing_mode":"local_snapshot"})
        )
    );
    assert_eq!(get(app.clone(), "/ready").await.0, 503);
    assert_eq!(get(app, "/health").await.0, 200);
    let mut unsupported = graph();
    unsupported.add_pool(Pool {
        kind: PoolKind::Curve,
        address: Address::with_last_byte(50),
        coins: vec![Address::with_last_byte(1), Address::with_last_byte(2)],
    });
    let store = Arc::new(
        SnapshotStore::new(
            unsupported.clone(),
            RouterConfig::default(),
            LocalStateConfig::default(),
        )
        .unwrap(),
    );
    let app = local_app(unsupported, rpc.clone(), store);
    assert_eq!(
        request(app.clone(), body.clone()).await.1["error"],
        "UnsupportedLocalPool"
    );
    let (_, status) = get(app, "/state").await;
    assert_eq!(status["unsupported_pools"][0]["platform"], "curve");
    assert_eq!(rpc.request_count(), 0);

    let (rpc, chain, server) = chain_rpc().await;
    chain.lock().unwrap().headers.get_mut(&123).unwrap()["timestamp"] = json!("0x1");
    let store = Arc::new(
        SnapshotStore::new(
            graph(),
            RouterConfig::default(),
            LocalStateConfig::default(),
        )
        .unwrap(),
    );
    let sync = StateSynchronizer::new(rpc.clone(), store.clone());
    sync.sync_once().await.unwrap();
    let calls = rpc.request_count();
    let app = local_app(graph(), rpc.clone(), store);
    assert_eq!(request(app.clone(), body).await.1["error"], "SnapshotStale");
    let (_, status) = get(app.clone(), "/ready").await;
    assert_eq!(status["unavailable_reason"], "SnapshotStale");
    assert_eq!(get(app, "/graph").await.0, 503);
    assert_eq!(
        rpc.request_count(),
        calls,
        "readiness must inspect published state, not probe RPC"
    );
    server.abort();
}

#[tokio::test]
async fn synced_snapshot_quotes_single_and_multiple_amounts_without_rpc() {
    let block = header();
    let rpc_app = Router::new().route(
        "/",
        post(move |Json(request): Json<Value>| {
            let block = block.clone();
            async move {
                let result = match request["method"].as_str().unwrap() {
                    "eth_getBlockByNumber" => block,
                    "eth_call" => {
                        assert_eq!(
                            request["params"][1],
                            json!({"blockHash":block["hash"],"requireCanonical":true})
                        );
                        json!(reserve_results())
                    }
                    method => panic!("unexpected RPC {method}"),
                };
                Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = RpcClient::new(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, rpc_app).await.unwrap() });
    let graph = graph();
    let store = Arc::new(
        SnapshotStore::new(
            graph.clone(),
            RouterConfig::default(),
            LocalStateConfig::default(),
        )
        .unwrap(),
    );
    StateSynchronizer::new(rpc.clone(), store.clone())
        .sync_once()
        .await
        .unwrap();
    server.abort();
    let count = rpc.request_count();
    let app = api::app_with_local_state(
        Arc::new(AppState {
            graph,
            source: Arc::new(FixedRate::new(U256::from(1))),
            router: QuoteRouter::default(),
            limiter: RateLimiter::new(100, Duration::from_secs(1)),
            rpc: Some(rpc.clone()),
            request_timeout: Duration::from_secs(2),
        }),
        SplitOptimizer::new(SplitConfig::multi_pool()).unwrap(),
        store,
    );
    let base = json!({"token_in":Address::with_last_byte(1),"token_out":Address::with_last_byte(2),"amount_in":"1001"});
    let (status, single) = request(app.clone(), base.clone()).await;
    assert_eq!(status, 200);
    assert_eq!(single["amount_out"], "49949"); // independently worked V2 example
    assert_eq!(single["quote_block"], 123);
    assert_eq!(single["state"]["pricing_mode"], "local_snapshot");
    let mut split = base;
    split["split"] = json!(true);
    split["amount_out_min"] = json!("70000");
    let (status, multi) = request(app.clone(), split.clone()).await;
    assert_eq!(status, 200);
    assert!(
        U256::from_str_radix(multi["amount_out"].as_str().unwrap(), 10).unwrap()
            > U256::from(70_000)
    );
    assert_eq!(multi["allocations"].as_array().unwrap().len(), 4);
    assert_eq!(multi["search"]["baseline_rpc_requests"], 0);
    assert_eq!(multi["search"]["additional_rpc_requests"], 0);
    split["split_max_branches"] = json!(2);
    assert_eq!(request(app.clone(), split.clone()).await.0, 422);
    split.as_object_mut().unwrap().remove("split_max_branches");
    split.as_object_mut().unwrap().remove("amount_out_min");
    split["amount_in"] = json!("19");
    assert_eq!(request(app, split).await.0, 200);
    assert_eq!(
        rpc.request_count(),
        count,
        "the RPC server is offline; search must be pure"
    );
}
