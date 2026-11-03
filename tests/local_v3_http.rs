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
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;

fn word(n: U256) -> Vec<u8> {
    n.to_be_bytes::<32>().to_vec()
}
fn value(n: u128) -> Vec<u8> {
    word(U256::from(n))
}

fn aggregate_return(results: Vec<Vec<u8>>) -> String {
    let mut data = value(32);
    data.extend(value(results.len() as u128));
    let mut offset = results.len() * 32;
    for result in &results {
        data.extend(value(offset as u128));
        offset += 96 + result.len().div_ceil(32) * 32;
    }
    for mut result in results {
        data.extend(value(1));
        data.extend(value(64));
        data.extend(value(result.len() as u128));
        result.resize(result.len().div_ceil(32) * 32, 0);
        data.extend(result);
    }
    format!(
        "0x{}",
        data.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}

fn answer_calls(request: &Value, wrong_fee: bool) -> String {
    let hex = request["params"][0]["data"]
        .as_str()
        .unwrap()
        .trim_start_matches("0x");
    let data = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let number = |start: usize| U256::from_be_slice(&data[start..start + 32]).to::<usize>();
    let count = number(36);
    let mut results = Vec::new();
    for idx in 0..count {
        let tuple = 68 + number(68 + idx * 32);
        let start = tuple + number(tuple + 64) + 32;
        let selector = &data[start..start + 4];
        let result = match selector {
            [0x09, 0x02, 0xf1, 0xac] => [value(1000), value(100_000), value(0)].concat(),
            [0x38, 0x50, 0xc7, 0xbd] => [
                word(U256::from(1) << 96),
                value(0),
                value(0),
                value(0),
                value(0),
                value(0),
                value(1),
            ]
            .concat(),
            [0x1a, 0x68, 0x65, 0x02] => value(1_000_000_000_000_000_000),
            [0xd0, 0xc9, 0x3a, 0x7c] => value(60),
            [0xdd, 0xca, 0x3f, 0x43] => value(if wrong_fee { 500 } else { 3000 }),
            [0x0d, 0xfe, 0x16, 0x81] => value(1),
            [0xd2, 0x12, 0x20, 0xa7] => value(2),
            [0xc4, 0x5a, 0x01, 0x55] => {
                word(U256::from_str_radix("1f98431c8ad98523631ae4a59f267346ea31f984", 16).unwrap())
            }
            [0x53, 0x39, 0xc2, 0x96] => value(0),
            selector => panic!("unexpected selector {selector:?}"),
        };
        results.push(result);
    }
    aggregate_return(results)
}

async fn setup(wrong_fee: bool) -> (Arc<SnapshotStore>, RpcClient, tokio::task::JoinHandle<()>) {
    let block = json!({"number":"0x7b","hash":format!("0x{:064x}",123),"parentHash":format!("0x{:064x}",122),
        "timestamp":format!("0x{:x}",SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs())});
    let rpc_app = Router::new().route(
        "/",
        post(move |Json(request): Json<Value>| {
            let block = block.clone();
            async move {
                let hash = format!("0x{:064x}", 123);
                let result = if request["method"] == "eth_getBlockByNumber" {
                    block
                } else {
                    assert_eq!(request["method"], "eth_call");
                    assert_eq!(
                        request["params"][1],
                        json!({"blockHash":hash,"requireCanonical":true})
                    );
                    json!(answer_calls(&request, wrong_fee))
                };
                Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = RpcClient::new(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, rpc_app).await.unwrap() });
    let mut graph = Graph::new();
    for (id, kind) in [
        (10, PoolKind::UniswapV2),
        (11, PoolKind::UniswapV3 { fee: 3000 }),
    ] {
        graph.add_pool(Pool {
            kind,
            address: Address::with_last_byte(id),
            coins: vec![Address::with_last_byte(1), Address::with_last_byte(2)],
        });
    }
    (
        Arc::new(
            SnapshotStore::new(
                graph,
                RouterConfig::default(),
                LocalStateConfig {
                    v3_bitmap_radius: 1,
                    compute_concurrency: 1,
                    ..Default::default()
                },
            )
            .unwrap(),
        ),
        rpc,
        server,
    )
}

#[tokio::test]
async fn mixed_v2_v3_http_is_offline_and_missing_coverage_cannot_hide_behind_a_baseline() {
    let (store, rpc, server) = setup(false).await;
    StateSynchronizer::new(rpc.clone(), store.clone())
        .sync_once()
        .await
        .unwrap();
    assert_eq!(store.status().v3_coverage.len(), 1);
    server.abort();
    let calls = rpc.request_count();
    let snapshot = store.acquire().unwrap();
    let app = api::app_with_local_state(
        Arc::new(AppState {
            graph: snapshot.graph().clone(),
            source: Arc::new(FixedRate::new(U256::from(1))),
            router: QuoteRouter::default(),
            limiter: RateLimiter::new(100, Duration::from_secs(1)),
            rpc: Some(rpc.clone()),
            request_timeout: Duration::from_secs(2),
        }),
        SplitOptimizer::new(SplitConfig::multi_pool()).unwrap(),
        store,
    );
    for (amount, status) in [
        ("20000", 200),
        ("1000000000000000000000000", 503),
        ("20000", 200),
    ] {
        let body = json!({"token_in":Address::with_last_byte(1),"token_out":Address::with_last_byte(2),"amount_in":amount,"split":true});
        let response = app
            .clone()
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
        assert_eq!(response.status().as_u16(), status);
        let result: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap(),
        )
        .unwrap();
        if status == 200 {
            assert_eq!(result["allocations"].as_array().unwrap().len(), 2);
            assert!(
                result["allocations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a["path"][0]["platform"] == "uniswap-v3")
            );
            assert_eq!(result["search"]["additional_rpc_requests"], 0);
        } else {
            assert_eq!(result["error"], "SnapshotIncomplete");
        }
    }
    assert_eq!(rpc.request_count(), calls);
}

#[tokio::test]
async fn mismatched_v3_identity_prevents_partial_snapshot_publication() {
    let (store, rpc, server) = setup(true).await;
    assert!(
        StateSynchronizer::new(rpc, store.clone())
            .sync_once()
            .await
            .is_err()
    );
    assert!(!store.status().ready);
    assert_eq!(store.status().loaded_pools, 0);
    server.abort();
}

#[test]
fn local_cpu_queue_is_bounded_and_http_timeout_does_not_block_readiness() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (store,rpc,server)=setup(false).await;
        StateSynchronizer::new(rpc.clone(),store.clone()).sync_once().await.unwrap();
        let snapshot=store.acquire().unwrap();
        let app=api::app_with_local_state(Arc::new(AppState {
            graph:snapshot.graph().clone(),source:Arc::new(FixedRate::new(U256::from(1))),router:QuoteRouter::default(),
            limiter:RateLimiter::new(100,Duration::from_secs(1)),rpc:Some(rpc),request_timeout:Duration::from_millis(50),
        }),SplitOptimizer::new(SplitConfig::multi_pool()).unwrap(),store);
        // Occupy the runtime's sole CPU worker. The first quote must queue,
        // readiness must remain responsive, and another quote must not queue.
        let (release,wait)=std::sync::mpsc::channel();
        let (entered,started)=tokio::sync::oneshot::channel();
        let blocker=tokio::task::spawn_blocking(move || {entered.send(()).unwrap();wait.recv().unwrap();});
        started.await.unwrap();
        let request=||axum::http::Request::builder().method("POST").uri("/quote").header("content-type","application/json")
            .body(axum::body::Body::from(json!({"token_in":Address::with_last_byte(1),"token_out":Address::with_last_byte(2),"amount_in":"20000","split":true}).to_string())).unwrap();
        let first=app.clone().oneshot(request());
        futures::pin_mut!(first);
        assert!(futures::poll!(first.as_mut()).is_pending());
        let second=app.clone().oneshot(request()).await.unwrap();
        assert_eq!(second.status(),429);
        let readiness=app.oneshot(axum::http::Request::builder().uri("/ready").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(readiness.status(),200);
        assert_eq!(first.await.unwrap().status(),504);
        release.send(()).unwrap();
        blocker.await.unwrap();
        server.abort();
    });
}
