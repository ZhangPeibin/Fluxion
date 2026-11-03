use alloy_primitives::{Address, U256, keccak256};
use axum::{Json, Router, routing::post};
use fluxion::{
    api::{self, AppState, RateLimiter},
    graph::Graph,
    local_state::{LocalStateConfig, SnapshotStore, StateError, StateSynchronizer},
    model::{ConversionKind, Pool, PoolKind, QuoteRequest},
    router::{Router as QuoteRouter, RouterConfig},
    rpc::RpcClient,
    source::FixedRate,
    split::{SplitConfig, SplitOptimizer},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;

fn selector(signature: &str) -> [u8; 4] {
    keccak256(signature.as_bytes())[..4].try_into().unwrap()
}
fn word(n: u128) -> Vec<u8> {
    U256::from(n).to_be_bytes::<32>().to_vec()
}

#[derive(Clone)]
struct Block {
    header: Value,
    ether: u128,
    version: u128,
    bad_identity: bool,
    bad_view: bool,
    missing_state: bool,
}

impl Block {
    fn new(number: u64, ether: u128) -> Self {
        Self {
            header: json!({"number":format!("0x{number:x}"),"hash":format!("0x{number:064x}"),"parentHash":format!("0x{:064x}",number-1),
            "timestamp":format!("0x{:x}",SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs())}),
            ether,
            version: 4,
            bad_identity: false,
            bad_view: false,
            missing_state: false,
        }
    }

    fn call(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        let sel: &[u8] = &bytes[..4];
        if sel == selector("getReserves()") {
            return Some([word(1000), word(100_000), word(0)].concat());
        }
        let value = if sel == selector("stETH()") {
            if self.bad_identity { 99 } else { 1 }
        } else if sel == selector("getContractVersion()") {
            self.version
        } else if sel == selector("getTotalPooledEther()") {
            self.ether + (2 * self.ether / 7)
        } else if sel == selector("getTotalShares()") {
            9
        } else if sel == selector("getExternalEther()") {
            if self.missing_state {
                return None;
            }
            2 * self.ether / 7
        } else if sel == selector("getExternalShares()") {
            2
        } else {
            let amount = U256::from_be_slice(&bytes[4..36]).to::<u128>();
            let out = if sel == selector("getWstETHByStETH(uint256)") {
                amount * 7 / self.ether
            } else if sel == selector("getStETHByWstETH(uint256)") {
                amount * self.ether / 7
            } else {
                panic!("unexpected selector {sel:?}")
            };
            out + u128::from(self.bad_view)
        };
        Some(word(value))
    }
}

// Independent small ABI fixture for arbitrary Multicall3 reads.
fn reply_calls(request: &Value, block: &Block) -> Value {
    let hex = request["params"][0]["data"]
        .as_str()
        .unwrap()
        .trim_start_matches("0x");
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let integer = |at: usize| U256::from_be_slice(&bytes[at..at + 32]).to::<usize>();
    let count = integer(36);
    let results = (0..count)
        .map(|i| {
            let tuple = 68 + integer(68 + i * 32);
            let data = tuple + integer(tuple + 64);
            block.call(&bytes[data + 32..data + 32 + integer(data)])
        })
        .collect::<Vec<_>>();
    let mut encoded = word(32);
    encoded.extend(word(count as u128));
    let mut offset = count * 32;
    for result in &results {
        encoded.extend(word(offset as u128));
        offset += 96
            + result
                .as_ref()
                .map_or(0, |bytes| bytes.len().div_ceil(32) * 32);
    }
    for result in results {
        encoded.extend(word(u128::from(result.is_some())));
        encoded.extend(word(64));
        let mut data = result.unwrap_or_default();
        encoded.extend(word(data.len() as u128));
        data.resize(data.len().div_ceil(32) * 32, 0);
        encoded.extend(data);
    }
    json!(format!(
        "0x{}",
        encoded
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

fn graph(wrap: bool, unwrap: bool) -> Graph {
    let mut graph = Graph::new();
    graph.add_pool(Pool {
        kind: PoolKind::Conversion {
            kind: ConversionKind::WstEth,
            wrap,
            unwrap,
        },
        address: Address::with_last_byte(2),
        coins: vec![Address::with_last_byte(1), Address::with_last_byte(2)],
    });
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: Address::with_last_byte(10),
        coins: vec![Address::with_last_byte(2), Address::with_last_byte(3)],
    });
    graph
}

type Fixture = (
    Arc<SnapshotStore>,
    RpcClient,
    Arc<Mutex<BTreeMap<u64, Block>>>,
    tokio::task::JoinHandle<()>,
);

async fn setup(wrap: bool, unwrap: bool, block: Block) -> Fixture {
    let blocks = Arc::new(Mutex::new(BTreeMap::from([(123, block)])));
    let chain = blocks.clone();
    let app = Router::new().route(
        "/",
        post(move |Json(request): Json<Value>| {
            let chain = chain.clone();
            async move {
                let blocks = chain.lock().unwrap();
                let result = if request["method"] == "eth_getBlockByNumber" {
                    let tag = request["params"][0].as_str().unwrap();
                    if tag == "latest" {
                        blocks.last_key_value().unwrap().1.header.clone()
                    } else {
                        blocks[&u64::from_str_radix(tag.trim_start_matches("0x"), 16).unwrap()]
                            .header
                            .clone()
                    }
                } else {
                    assert_eq!(request["method"], "eth_call");
                    assert_eq!(request["params"][1]["requireCanonical"], true);
                    let block = blocks
                        .values()
                        .find(|b| b.header["hash"] == request["params"][1]["blockHash"])
                        .unwrap();
                    reply_calls(&request, block)
                };
                Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = RpcClient::new(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let store = Arc::new(
        SnapshotStore::new(
            graph(wrap, unwrap),
            RouterConfig::default(),
            LocalStateConfig::default(),
        )
        .unwrap(),
    );
    (store, rpc, blocks, server)
}

fn app(store: Arc<SnapshotStore>, rpc: RpcClient) -> Router {
    let snapshot = store.acquire().unwrap();
    api::app_with_local_state(
        Arc::new(AppState {
            graph: snapshot.graph().clone(),
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

async fn quote(
    app: Router,
    input: u8,
    output: u8,
    amount: u128,
    split: bool,
    min: Option<u128>,
) -> (u16, Value) {
    let mut body = json!({"token_in":Address::with_last_byte(input),"token_out":Address::with_last_byte(output),"amount_in":amount.to_string(),"split":split});
    if let Some(min) = min {
        body["amount_out_min"] = json!(min.to_string());
    }
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
    (
        status,
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap(),
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn offline_http_converts_both_directions_and_preserves_multi_hop_baseline() {
    let (store, rpc, _, server) = setup(true, true, Block::new(123, 10)).await;
    StateSynchronizer::new(rpc.clone(), store.clone())
        .sync_once()
        .await
        .unwrap();
    server.abort();
    let before = rpc.request_count();
    let app = app(store.clone(), rpc.clone());
    let (status, wrapped) = quote(app.clone(), 1, 2, 10, false, None).await;
    assert_eq!(status, 200);
    assert_eq!(wrapped["amount_out"], "7");
    assert_eq!(wrapped["path"][0]["operation"], "wrap");
    let (status, unwrapped) = quote(app.clone(), 2, 1, 7, true, Some(10)).await;
    assert_eq!(status, 200);
    assert_eq!(unwrapped["amount_out"], "10");
    assert_eq!(unwrapped["path"][0]["operation"], "unwrap");
    assert_eq!(unwrapped["search"]["eligible_pools"], 0);
    assert_eq!(quote(app.clone(), 1, 2, 10, false, Some(8)).await.0, 422);
    let graph_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/graph")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(graph_response.status(), 200);
    let graph: Value = serde_json::from_slice(
        &axum::body::to_bytes(graph_response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    let conversions = graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|edge| edge["platform"] == "wsteth")
        .collect::<Vec<_>>();
    assert_eq!(conversions.len(), 2);
    assert!(
        conversions
            .iter()
            .all(|edge| edge.get("reserve").is_none() && edge["mid_price"].as_f64().unwrap() > 0.0)
    );
    let (status, multi) = quote(app, 1, 3, 1001, true, None).await;
    assert_eq!(status, 200);
    assert_eq!(multi["plan_kind"], "baseline");
    assert_eq!(multi["allocations"].as_array().unwrap().len(), 1);
    assert_eq!(multi["path"].as_array().unwrap().len(), 2);
    assert_eq!(multi["path"][0]["amount_out"], "700");
    assert_eq!(multi["path"][1]["amount_in"], "700");
    assert_eq!(
        multi["state"]["conversion_state"][0]["internal_ether"],
        "10"
    );
    assert_eq!(store.status().loaded_pools, 2);
    assert_eq!(rpc.request_count(), before);
}

#[tokio::test]
async fn a_disabled_wrap_direction_is_not_available_but_unwrap_still_works() {
    let (store, rpc, _, server) = setup(false, true, Block::new(123, 10)).await;
    StateSynchronizer::new(rpc.clone(), store.clone())
        .sync_once()
        .await
        .unwrap();
    let app = app(store, rpc);
    assert_eq!(quote(app.clone(), 1, 2, 10, false, None).await.0, 404);
    assert_eq!(quote(app, 2, 1, 7, false, None).await.0, 200);
    server.abort();
}

#[tokio::test]
async fn ratio_changes_publish_a_new_version_without_mutating_old_readers() {
    let (store, rpc, blocks, server) = setup(true, true, Block::new(123, 10)).await;
    let sync = StateSynchronizer::new(rpc, store.clone());
    sync.sync_once().await.unwrap();
    let old = store.acquire().unwrap();
    blocks.lock().unwrap().insert(124, Block::new(124, 13));
    sync.sync_once().await.unwrap();
    let request = QuoteRequest {
        token_in: Address::with_last_byte(2),
        token_out: Address::with_last_byte(1),
        amount_in: U256::from(7),
        max_hops: None,
        amount_out_min: None,
    };
    assert_eq!(
        old.route(&request).await.unwrap().amount_out,
        U256::from(10)
    );
    let latest = store.acquire().unwrap();
    assert_eq!(latest.block().number, 124);
    assert_eq!(
        latest.route(&request).await.unwrap().amount_out,
        U256::from(13)
    );
    let mut bad = Block::new(125, 17);
    bad.missing_state = true;
    blocks.lock().unwrap().insert(125, bad);
    assert_eq!(sync.sync_once().await.unwrap_err(), StateError::Incomplete);
    assert_eq!(store.acquire().unwrap().block().number, 124);
    assert_eq!(
        latest.route(&request).await.unwrap().amount_out,
        U256::from(13)
    );
    server.abort();
}

#[tokio::test]
async fn invalid_identity_version_state_and_view_never_publish_partial_snapshots() {
    for failure in ["identity", "version", "state", "view"] {
        let mut block = Block::new(123, 10);
        match failure {
            "identity" => block.bad_identity = true,
            "version" => block.version = 5,
            "state" => block.missing_state = true,
            "view" => block.bad_view = true,
            _ => unreachable!(),
        }
        let (store, rpc, _, server) = setup(true, true, block).await;
        assert!(
            StateSynchronizer::new(rpc, store.clone())
                .sync_once()
                .await
                .is_err(),
            "{failure}"
        );
        assert_eq!(store.status().loaded_pools, 0);
        assert!(!store.status().ready);
        server.abort();
    }
}

#[tokio::test]
async fn background_sync_retries_block_dependent_protocol_version_mismatches() {
    let mut initial = Block::new(123, 10);
    initial.version = 5;
    let (_, rpc, blocks, server) = setup(true, true, initial).await;
    let store = Arc::new(
        SnapshotStore::new(
            graph(true, true),
            RouterConfig::default(),
            LocalStateConfig {
                poll_interval_ms: 10,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let background = tokio::spawn(StateSynchronizer::new(rpc, store.clone()).run());
    tokio::time::timeout(Duration::from_secs(5), async {
        while store.status().last_sync_error.as_deref() != Some("UnsupportedLocalPool") {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    blocks.lock().unwrap().insert(124, Block::new(124, 13));
    tokio::time::timeout(Duration::from_secs(5), async {
        while !store.status().ready {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(store.acquire().unwrap().block().number, 124);
    assert_eq!(store.status().conversion_state[0].protocol_version, 4);
    background.abort();
    server.abort();
}
