use alloy_primitives::{Address, U256, keccak256};
use axum::{Json, Router, routing::post};
use fluxion::{
    api::{self, AppState, RateLimiter},
    curve_local::{COINS, THREE_POOL},
    graph::Graph,
    local_state::{LocalStateConfig, SnapshotStore, StateError, StateSynchronizer},
    model::{Pool, PoolKind, QuoteRequest},
    router::{Router as QuoteRouter, RouterConfig},
    rpc::RpcClient,
    source::FixedRate,
    split::{SplitConfig, SplitOptimizer},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tower::ServiceExt;

fn fixture(old: bool) -> Value {
    serde_json::from_str(if old {
        include_str!("fixtures/curve-3pool-20000000.json")
    } else {
        include_str!("fixtures/curve-3pool-26331147.json")
    })
    .unwrap()
}
fn integer(v: &Value) -> U256 {
    U256::from_str_radix(v.as_str().unwrap(), 10).unwrap()
}
fn word(n: U256) -> Vec<u8> {
    n.to_be_bytes::<32>().to_vec()
}
fn selector(signature: &str) -> [u8; 4] {
    keccak256(signature.as_bytes())[..4].try_into().unwrap()
}

struct Chain {
    head: u64,
    frames: BTreeMap<u64, Value>,
    missing: bool,
    bad_coins: bool,
    bad_probe: bool,
}
type Fixture = (
    Arc<SnapshotStore>,
    RpcClient,
    Arc<Mutex<Chain>>,
    tokio::task::JoinHandle<()>,
);

fn graph() -> Graph {
    let mut graph = Graph::new();
    graph.add_pool(Pool {
        kind: PoolKind::Curve,
        address: THREE_POOL.parse().unwrap(),
        coins: COINS.map(|s| s.parse().unwrap()).to_vec(),
    });
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: Address::with_last_byte(10),
        coins: vec![COINS[1].parse().unwrap(), Address::with_last_byte(4)],
    });
    graph
}

fn reply_call(chain: &Chain, frame: &Value, target: Address, data: &[u8]) -> Option<Vec<u8>> {
    if data[..4] == selector("getReserves()") {
        assert_eq!(target, Address::with_last_byte(10));
        return Some(
            [
                word(U256::from(10u128.pow(12))),
                word(U256::from(10u128.pow(21))),
                word(U256::ZERO),
            ]
            .concat(),
        );
    }
    assert_eq!(target, THREE_POOL.parse::<Address>().unwrap());
    let value = if data[..4] == selector("coins(uint256)") {
        let i = U256::from_be_slice(&data[4..36]).to::<usize>();
        if chain.bad_coins {
            U256::ZERO
        } else {
            U256::from_be_slice(
                frame["coins"][i]
                    .as_str()
                    .unwrap()
                    .parse::<Address>()
                    .unwrap()
                    .as_slice(),
            )
        }
    } else if data[..4] == selector("balances(uint256)") {
        if chain.missing {
            return None;
        }
        integer(&frame["balances"][U256::from_be_slice(&data[4..36]).to::<usize>()])
    } else if data[..4] == selector("A()") {
        integer(&frame["amplification"])
    } else if data[..4] == selector("fee()") {
        integer(&frame["fee"])
    } else {
        assert_eq!(data[..4], selector("get_dy(int128,int128,uint256)"));
        let i = U256::from_be_slice(&data[4..36]).to::<u64>();
        let j = U256::from_be_slice(&data[36..68]).to::<u64>();
        let amount = U256::from_be_slice(&data[68..100]).to_string();
        let quote = frame["quotes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["i"] == i && q["j"] == j && q["amount_in"] == amount)
            .unwrap();
        integer(&quote["amount_out"]) + U256::from(chain.bad_probe)
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
    let number = |at: usize| U256::from_be_slice(&bytes[at..at + 32]).to::<usize>();
    let count = number(36);
    let results = (0..count)
        .map(|i| {
            let tuple = 68 + number(68 + i * 32);
            let data = tuple + number(tuple + 64);
            reply_call(
                chain,
                frame,
                Address::from_slice(&bytes[tuple + 12..tuple + 32]),
                &bytes[data + 32..data + 32 + number(data)],
            )
        })
        .collect::<Vec<_>>();
    let mut output = word(U256::from(32));
    output.extend(word(U256::from(count)));
    let mut offset = count * 32;
    for result in &results {
        output.extend(word(U256::from(offset)));
        offset += 96 + result.as_ref().map_or(0, |d| d.len().div_ceil(32) * 32);
    }
    for result in results {
        output.extend(word(U256::from(result.is_some())));
        output.extend(word(U256::from(64)));
        let mut data = result.unwrap_or_default();
        output.extend(word(U256::from(data.len())));
        data.resize(data.len().div_ceil(32) * 32, 0);
        output.extend(data);
    }
    json!(format!(
        "0x{}",
        output
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

async fn setup(old: bool) -> Fixture {
    let frame = fixture(old);
    let head = frame["block"]["number"].as_u64().unwrap();
    let chain = Arc::new(Mutex::new(Chain {
        head,
        frames: BTreeMap::from([(head, frame)]),
        missing: false,
        bad_coins: false,
        bad_probe: false,
    }));
    let control = chain.clone();
    let app=Router::new().route("/",post(move |Json(request):Json<Value>| {
        let control=control.clone();
        async move {
            let chain=control.lock().unwrap();
            let result=if request["method"]=="eth_getBlockByNumber" {
                let tag=request["params"][0].as_str().unwrap();let n=if tag=="latest" {chain.head} else {u64::from_str_radix(tag.trim_start_matches("0x"),16).unwrap()};
                let h=&chain.frames[&n]["block"];
                json!({"number":format!("0x{n:x}"),"hash":h["hash"],"parentHash":h["parent_hash"],"timestamp":format!("0x{:x}",h["timestamp"].as_u64().unwrap())})
            } else {
                assert_eq!(request["method"],"eth_call");assert_eq!(request["params"][1]["requireCanonical"],true);
                let frame=chain.frames.values().find(|f|f["block"]["hash"]==request["params"][1]["blockHash"]).unwrap();
                aggregate(&chain,frame,&request)
            };
            Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = RpcClient::new(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    // Historical oracle data is replayed honestly with its original timestamp;
    // the replay policy explicitly permits historical age, not fake freshness.
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
            request_timeout: Duration::from_secs(2),
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
    max_hops: usize,
    min: Option<&str>,
) -> (u16, Value) {
    let mut request = json!({"token_in":input,"token_out":output,"amount_in":amount,"split":true,"max_hops":max_hops});
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
async fn recorded_oracles_replay_offline_through_all_six_http_directions_and_multi_hop() {
    let (store, rpc, _, server) = setup(false).await;
    StateSynchronizer::new(rpc.clone(), store.clone())
        .sync_once()
        .await
        .unwrap();
    server.abort();
    let before = rpc.request_count();
    let app = app(store.clone(), rpc.clone());
    let frame = fixture(false);
    let coins = COINS.map(|s| s.parse::<Address>().unwrap());
    for q in frame["quotes"].as_array().unwrap() {
        let (status, result) = quote(
            app.clone(),
            coins[q["i"].as_u64().unwrap() as usize],
            coins[q["j"].as_u64().unwrap() as usize],
            q["amount_in"].as_str().unwrap(),
            1,
            None,
        )
        .await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["amount_out"], q["amount_out"]);
        assert_eq!(result["allocations"].as_array().unwrap().len(), 1);
        assert_eq!(result["plan_kind"], "baseline");
        assert_eq!(result["search"]["eligible_pools"], 0);
        assert_eq!(result["search"]["baseline_rpc_requests"], 0);
    }
    let (status, result) = quote(
        app.clone(),
        coins[2],
        Address::with_last_byte(4),
        "1000000000",
        2,
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(result["path"].as_array().unwrap().len(), 2);
    assert_eq!(result["path"][0]["amount_out"], "998629012");
    assert_eq!(result["path"][1]["amount_in"], "998629012");
    let minimum = (integer(&frame["quotes"][14]["amount_out"]) + U256::from(1)).to_string();
    assert_eq!(
        quote(
            app.clone(),
            coins[1],
            coins[0],
            "1000000000",
            1,
            Some(&minimum)
        )
        .await
        .0,
        422
    );
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/graph")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let graph: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    let edges = graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["platform"] == "curve")
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), 6);
    for e in edges {
        assert_eq!(
            e["reserve"],
            frame["balances"][e["i"].as_u64().unwrap() as usize]
        );
    }
    assert_eq!(store.status().curve_state[0].a_precision, 1);
    assert_eq!(rpc.request_count(), before);
}

#[tokio::test]
async fn real_historical_a_fee_and_balance_changes_do_not_mutate_old_snapshots() {
    let (store, rpc, chain, server) = setup(true).await;
    let sync = StateSynchronizer::new(rpc, store.clone());
    sync.sync_once().await.unwrap();
    let old = store.acquire().unwrap();
    let coins = COINS.map(|s| s.parse::<Address>().unwrap());
    let request = QuoteRequest {
        token_in: coins[1],
        token_out: coins[0],
        amount_in: U256::from(1_000_000_000),
        max_hops: Some(1),
        amount_out_min: None,
    };
    let first = old.route(&request).await.unwrap();
    {
        let mut chain = chain.lock().unwrap();
        let frame = fixture(false);
        let number = frame["block"]["number"].as_u64().unwrap();
        chain.frames.insert(number, frame);
        chain.head = number;
    }
    sync.sync_once().await.unwrap();
    assert_eq!(
        old.route(&request).await.unwrap().amount_out,
        first.amount_out
    );
    assert_ne!(
        store
            .acquire()
            .unwrap()
            .route(&request)
            .await
            .unwrap()
            .amount_out,
        first.amount_out
    );
    assert_eq!(
        store.quote_state(&old).unwrap().curve_state[0].amplification,
        "2000"
    );
    assert_eq!(store.status().curve_state[0].amplification, "4000");
    assert_eq!(
        store.status().curve_state[0].fee.as_deref(),
        Some("1500000")
    );
    server.abort();
}

#[tokio::test]
async fn missing_balances_wrong_coins_and_failed_model_probes_never_partially_publish() {
    for failure in [0, 1, 2] {
        let (store, rpc, chain, server) = setup(false).await;
        {
            let mut chain = chain.lock().unwrap();
            match failure {
                0 => chain.missing = true,
                1 => chain.bad_coins = true,
                _ => chain.bad_probe = true,
            }
        }
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
}

#[tokio::test]
async fn other_curve_deployments_remain_visible_and_unsupported() {
    let mut graph = graph();
    graph.add_pool(Pool {
        kind: PoolKind::Curve,
        address: "0x80466c64868e1ab14a1ddf27a676c3fcbe638fe5"
            .parse()
            .unwrap(),
        coins: vec![
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
                .parse()
                .unwrap(),
            "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599"
                .parse()
                .unwrap(),
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
                .parse()
                .unwrap(),
        ],
    });
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

#[tokio::test]
async fn configured_reserve_threshold_keeps_legacy_curve_multi_hop_behavior() {
    let (_, rpc, _, server) = setup(false).await;
    let store = Arc::new(
        SnapshotStore::new(
            graph(),
            RouterConfig {
                reserve_ratio_k: 1,
                ..Default::default()
            },
            LocalStateConfig {
                max_age_secs: u64::MAX,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    StateSynchronizer::new(rpc, store.clone())
        .sync_once()
        .await
        .unwrap();
    let request = QuoteRequest {
        token_in: Address::with_last_byte(4),
        token_out: COINS[2].parse().unwrap(),
        amount_in: U256::from(33_333_333_333_333_333_333u128),
        max_hops: Some(2),
        amount_out_min: None,
    };
    let result = store.acquire().unwrap().route(&request).await.unwrap();
    assert_eq!(result.hops.len(), 2);
    assert_eq!(result.hops[1].pool.kind, PoolKind::Curve);
    server.abort();
}
