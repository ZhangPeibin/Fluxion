use alloy_primitives::{Address, U256, keccak256};
use axum::{Json, Router, routing::post};
use fluxion::{
    api::{self, AppState, RateLimiter},
    graph::Graph,
    local_state::{LocalStateConfig, SnapshotStore, StateError, StateSynchronizer},
    model::{Pool, PoolKind, QuoteRequest},
    router::{Router as QuoteRouter, RouterConfig},
    rpc::RpcClient,
    source::FixedRate,
    split::{SplitConfig, SplitOptimizer},
    tricrypto2_local,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tower::ServiceExt;

fn frame(old: bool) -> Value {
    serde_json::from_str(if old {
        include_str!("fixtures/tricrypto2-20000000.json")
    } else {
        include_str!("fixtures/tricrypto2-26331640.json")
    })
    .unwrap()
}
fn number(value: &Value) -> U256 {
    U256::from_str_radix(value.as_str().unwrap(), 10).unwrap()
}
fn word(value: U256) -> Vec<u8> {
    value.to_be_bytes::<32>().to_vec()
}
fn sig(name: &str) -> [u8; 4] {
    keccak256(name.as_bytes())[..4].try_into().unwrap()
}
struct Chain {
    head: u64,
    frames: BTreeMap<u64, Value>,
    missing: bool,
    bad_identity: bool,
    bad_parameter: bool,
    bad_probe: bool,
    unexpected_revert: bool,
}
type Fixture = (
    Arc<SnapshotStore>,
    RpcClient,
    Arc<Mutex<Chain>>,
    tokio::task::JoinHandle<()>,
);

fn graph() -> Graph {
    let mut graph = Graph::new();
    graph.add_pool(tricrypto2_local::pool());
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: Address::with_last_byte(10),
        coins: vec![
            tricrypto2_local::COINS[2].parse().unwrap(),
            Address::with_last_byte(4),
        ],
    });
    graph
}

fn call(chain: &Chain, frame: &Value, target: Address, data: &[u8]) -> Option<Vec<u8>> {
    if data[..4] == sig("getReserves()") {
        assert_eq!(target, Address::with_last_byte(10));
        return Some(
            [
                word(U256::from(10u128.pow(21))),
                word(U256::from(3_000_000_000_000u64)),
                word(U256::ZERO),
            ]
            .concat(),
        );
    }
    assert_eq!(target, tricrypto2_local::POOL.parse::<Address>().unwrap());
    let selector = &data[..4];
    let p = &frame["parameters"];
    let value = if selector == sig("coins(uint256)") {
        if chain.bad_identity {
            U256::ZERO
        } else {
            U256::from_be_slice(
                frame["coins"][U256::from_be_slice(&data[4..36]).to::<usize>()]
                    .as_str()
                    .unwrap()
                    .parse::<Address>()
                    .unwrap()
                    .as_slice(),
            )
        }
    } else if selector == sig("balances(uint256)") {
        if chain.missing {
            return None;
        }
        number(&frame["balances"][U256::from_be_slice(&data[4..36]).to::<usize>()])
    } else if selector == sig("A()") {
        number(&p["amplification"])
    } else if selector == sig("gamma()") {
        number(&p["gamma"])
    } else if selector == sig("D()") {
        if chain.bad_parameter {
            U256::MAX
        } else {
            number(&p["stored_d"])
        }
    } else if selector == sig("price_scale(uint256)") {
        number(&p["price_scale"][U256::from_be_slice(&data[4..36]).to::<usize>()])
    } else if selector == sig("mid_fee()") {
        number(&p["mid_fee"])
    } else if selector == sig("out_fee()") {
        number(&p["out_fee"])
    } else if selector == sig("fee_gamma()") {
        number(&p["fee_gamma"])
    } else {
        assert_eq!(selector, sig("get_dy(uint256,uint256,uint256)"));
        if chain.unexpected_revert {
            return None;
        }
        let i = U256::from_be_slice(&data[4..36]).to::<u64>();
        let j = U256::from_be_slice(&data[36..68]).to::<u64>();
        let amount = U256::from_be_slice(&data[68..100]).to_string();
        let q = frame["quotes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["i"] == i && q["j"] == j && q["amount_in"] == amount)
            .unwrap();
        if q["success"] == false {
            return None;
        }
        number(&q["amount_out"]) + U256::from(chain.bad_probe)
    };
    Some(word(value))
}

fn aggregate(chain: &Chain, frame: &Value, request: &Value) -> Value {
    let hex = request["params"][0]["data"]
        .as_str()
        .unwrap()
        .trim_start_matches("0x");
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let n = |at: usize| U256::from_be_slice(&bytes[at..at + 32]).to::<usize>();
    let count = n(36);
    let results = (0..count)
        .map(|i| {
            let tuple = 68 + n(68 + i * 32);
            let body = tuple + n(tuple + 64);
            call(
                chain,
                frame,
                Address::from_slice(&bytes[tuple + 12..tuple + 32]),
                &bytes[body + 32..body + 32 + n(body)],
            )
        })
        .collect::<Vec<_>>();
    let mut bytes = word(U256::from(32));
    bytes.extend(word(U256::from(count)));
    let mut offset = count * 32;
    for r in &results {
        bytes.extend(word(U256::from(offset)));
        offset += 96 + r.as_ref().map_or(0, |r| r.len().div_ceil(32) * 32);
    }
    for r in results {
        bytes.extend(word(U256::from(r.is_some())));
        bytes.extend(word(U256::from(64)));
        let mut data = r.unwrap_or_default();
        bytes.extend(word(U256::from(data.len())));
        data.resize(data.len().div_ceil(32) * 32, 0);
        bytes.extend(data);
    }
    json!(format!(
        "0x{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

async fn setup(old: bool) -> Fixture {
    let f = frame(old);
    let head = f["block"]["number"].as_u64().unwrap();
    let chain = Arc::new(Mutex::new(Chain {
        head,
        frames: BTreeMap::from([(head, f)]),
        missing: false,
        bad_identity: false,
        bad_parameter: false,
        bad_probe: false,
        unexpected_revert: false,
    }));
    let control = chain.clone();
    let app=Router::new().route("/",post(move |Json(request):Json<Value>| {let control=control.clone();async move {
        let chain=control.lock().unwrap();let result=if request["method"]=="eth_getBlockByNumber" {
            let tag=request["params"][0].as_str().unwrap();let n=if tag=="latest" {chain.head} else {u64::from_str_radix(tag.trim_start_matches("0x"),16).unwrap()};let h=&chain.frames[&n]["block"];
            json!({"number":format!("0x{n:x}"),"hash":h["hash"],"parentHash":h["parent_hash"],"timestamp":format!("0x{:x}",h["timestamp"].as_u64().unwrap())})
        } else {assert_eq!(request["method"],"eth_call");assert_eq!(request["params"][1]["requireCanonical"],true);let f=chain.frames.values().find(|f|f["block"]["hash"]==request["params"][1]["blockHash"]).unwrap();aggregate(&chain,f,&request)};
        Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = RpcClient::new(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let store = Arc::new(
        SnapshotStore::new(
            graph(),
            RouterConfig::default(),
            LocalStateConfig {
                max_age_secs: u64::MAX,
                max_lag_blocks: u64::MAX,
                batch_size: 3,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    (store, rpc, chain, server)
}
fn app(store: Arc<SnapshotStore>, rpc: RpcClient) -> Router {
    api::app_with_local_state(
        Arc::new(AppState {
            graph: graph(),
            source: Arc::new(FixedRate::new(U256::from(1))),
            router: QuoteRouter::default(),
            rpc: Some(rpc),
            limiter: RateLimiter::new(1000, Duration::from_secs(1)),
            request_timeout: Duration::from_secs(3),
        }),
        SplitOptimizer::new(SplitConfig::multi_pool()).unwrap(),
        store,
    )
}
async fn quote(
    app: Router,
    input: Address,
    output: Address,
    amount: &str,
    hops: usize,
    min: Option<&str>,
) -> (u16, Value) {
    let mut request = json!({"token_in":input,"token_out":output,"amount_in":amount,"split":true,"max_hops":hops});
    if let Some(min) = min {
        request["amount_out_min"] = json!(min);
    }
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/quote")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status().as_u16(),
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap(),
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn all_six_directions_and_expected_reverts_replay_through_http_with_rpc_offline() {
    let (store, rpc, _, server) = setup(false).await;
    StateSynchronizer::new(rpc.clone(), store.clone())
        .sync_once()
        .await
        .unwrap();
    server.abort();
    let before = rpc.request_count();
    let app = app(store, rpc.clone());
    let f = frame(false);
    let coins = tricrypto2_local::COINS.map(|s| s.parse::<Address>().unwrap());
    for q in f["quotes"].as_array().unwrap() {
        let (status, result) = quote(
            app.clone(),
            coins[q["i"].as_u64().unwrap() as usize],
            coins[q["j"].as_u64().unwrap() as usize],
            q["amount_in"].as_str().unwrap(),
            1,
            None,
        )
        .await;
        if q["success"] == true {
            assert_eq!(status, 200, "{q} {result}");
            assert_eq!(result["amount_out"], q["amount_out"]);
            assert_eq!(result["search"]["eligible_pools"], 0);
            assert_eq!(result["allocations"].as_array().unwrap().len(), 1);
            let info = &result["state"]["curve_state"][0];
            assert_eq!(info["fee_model"], "dynamic-post-trade");
            assert!(info.get("fee").is_none());
            assert_eq!(info["crypto"]["stored_d"], f["parameters"]["stored_d"]);
        } else {
            assert_ne!(
                status, 200,
                "a reverted quote must not become a fabricated zero output"
            );
        }
    }
    let q = f["quotes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["i"] == 0 && q["j"] == 2 && q["amount_in"] == "1000000000")
        .unwrap();
    let minimum = (number(&q["amount_out"]) + U256::from(1)).to_string();
    assert_eq!(
        quote(
            app.clone(),
            coins[0],
            coins[2],
            "1000000000",
            1,
            Some(&minimum)
        )
        .await
        .0,
        422
    );
    let (status, path) = quote(
        app,
        coins[0],
        Address::with_last_byte(4),
        "1000000000",
        2,
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(path["path"].as_array().unwrap().len(), 2);
    assert_eq!(path["path"][0]["amount_out"], q["amount_out"]);
    assert_eq!(path["path"][1]["amount_in"], q["amount_out"]);
    assert_eq!(rpc.request_count(), before);
}

#[tokio::test]
async fn prices_and_stored_d_update_atomically_and_leave_old_readers_unchanged() {
    let (store, rpc, chain, server) = setup(true).await;
    let sync = StateSynchronizer::new(rpc, store.clone());
    sync.sync_once().await.unwrap();
    let old = store.acquire().unwrap();
    let pool = tricrypto2_local::pool();
    let request = QuoteRequest {
        token_in: pool.coins[0],
        token_out: pool.coins[2],
        amount_in: U256::from(1_000_000_000),
        max_hops: Some(1),
        amount_out_min: None,
    };
    let first = old.route(&request).await.unwrap();
    {
        let mut chain = chain.lock().unwrap();
        chain.frames.insert(26_331_640, frame(false));
        chain.head = 26_331_640;
    }
    sync.sync_once().await.unwrap();
    let latest = store.acquire().unwrap();
    assert_eq!(
        old.route(&request).await.unwrap().amount_out,
        first.amount_out
    );
    assert_ne!(
        latest.route(&request).await.unwrap().amount_out,
        first.amount_out
    );
    assert_eq!(
        store.quote_state(&old).unwrap().curve_state[0]
            .crypto
            .as_ref()
            .unwrap()
            .stored_d,
        frame(true)["parameters"]["stored_d"].as_str().unwrap()
    );
    assert_eq!(
        store.status().curve_state[0]
            .crypto
            .as_ref()
            .unwrap()
            .stored_d,
        frame(false)["parameters"]["stored_d"].as_str().unwrap()
    );
    server.abort();
}

#[tokio::test]
async fn invalid_fields_identity_or_probes_never_publish_partial_data() {
    for failure in 0..5 {
        let (store, rpc, chain, server) = setup(false).await;
        {
            let mut c = chain.lock().unwrap();
            match failure {
                0 => c.missing = true,
                1 => c.bad_identity = true,
                2 => c.bad_parameter = true,
                3 => c.bad_probe = true,
                _ => c.unexpected_revert = true,
            }
        }
        assert!(
            StateSynchronizer::new(rpc, store.clone())
                .sync_once()
                .await
                .is_err()
        );
        assert_eq!(store.status().loaded_pools, 0);
        assert!(!store.status().ready);
        server.abort();
    }
}

#[tokio::test]
async fn legacy_tricrypto_with_identical_coins_is_not_aliased_to_this_model() {
    let mut graph = graph();
    let mut old = tricrypto2_local::pool();
    old.address = "0x80466c64868e1ab14a1ddf27a676c3fcbe638fe5"
        .parse()
        .unwrap();
    graph.add_pool(old);
    let store = Arc::new(
        SnapshotStore::new(graph, RouterConfig::default(), LocalStateConfig::default()).unwrap(),
    );
    let rpc = RpcClient::new("http://127.0.0.1:1".into());
    assert_eq!(
        StateSynchronizer::new(rpc.clone(), store.clone())
            .sync_once()
            .await,
        Err(StateError::Unsupported)
    );
    assert_eq!(store.status().total_pools, 3);
    assert_eq!(store.status().unsupported_pools.len(), 1);
    assert_eq!(rpc.request_count(), 0);
}
