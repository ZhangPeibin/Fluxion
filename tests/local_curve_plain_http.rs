use alloy_primitives::{Address, U256, keccak256};
use axum::{Json, Router, routing::post};
use fluxion::{
    api::{self, AppState, RateLimiter},
    curve_local::CurveProfile,
    curve_plain::PlainProfile,
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

fn frame(profile: CurveProfile, old: bool) -> Value {
    let text = match (profile, old) {
        (CurveProfile::Classic3, false) => include_str!("fixtures/curve-3pool-26331147.json"),
        (CurveProfile::Classic3, true) => include_str!("fixtures/curve-3pool-20000000.json"),
        (CurveProfile::Plain(PlainProfile::TwoCrv), false) => {
            include_str!("fixtures/curve-2crv-26331147.json")
        }
        (CurveProfile::Plain(PlainProfile::TwoCrv), true) => {
            include_str!("fixtures/curve-2crv-20000000.json")
        }
        (CurveProfile::Plain(PlainProfile::FraxUsdc), false) => {
            include_str!("fixtures/curve-frax-usdc-26331147.json")
        }
        (CurveProfile::Plain(PlainProfile::FraxUsdc), true) => {
            include_str!("fixtures/curve-frax-usdc-20000000.json")
        }
        (CurveProfile::Tricrypto2, true) => include_str!("fixtures/tricrypto2-20000000.json"),
        (CurveProfile::Tricrypto2, false) => panic!("no crypto fixture for block 26331147"),
    };
    serde_json::from_str(text).unwrap()
}
fn number(v: &Value) -> U256 {
    U256::from_str_radix(v.as_str().unwrap(), 10).unwrap()
}
fn word(v: U256) -> Vec<u8> {
    v.to_be_bytes::<32>().to_vec()
}
fn sig(name: &str) -> [u8; 4] {
    keccak256(name.as_bytes())[..4].try_into().unwrap()
}
struct Chain {
    head: u64,
    frames: BTreeMap<u64, BTreeMap<Address, Value>>,
    missing: bool,
    bad_probe: bool,
    fail_probe: bool,
}
type Fixture = (
    Arc<SnapshotStore>,
    RpcClient,
    Arc<Mutex<Chain>>,
    tokio::task::JoinHandle<()>,
);

fn answer(chain: &Chain, frame: &Value, data: &[u8]) -> Option<Vec<u8>> {
    let value = if data[..4] == sig("coins(uint256)") {
        let i = U256::from_be_slice(&data[4..36]).to::<usize>();
        U256::from_be_slice(
            frame["coins"][i]
                .as_str()
                .unwrap()
                .parse::<Address>()
                .unwrap()
                .as_slice(),
        )
    } else if data[..4] == sig("balances(uint256)") {
        if chain.missing {
            return None;
        }
        number(&frame["balances"][U256::from_be_slice(&data[4..36]).to::<usize>()])
    } else if data[..4] == sig("A_precise()") {
        if frame["a_precision"] != 100 {
            return None;
        }
        number(&frame["amplification"])
    } else if data[..4] == sig("A()") {
        if frame["profile"] == "tricrypto2" {
            number(&frame["parameters"]["amplification"])
        } else {
            number(&frame["amplification"]) / U256::from(frame["a_precision"].as_u64().unwrap_or(1))
        }
    } else if data[..4] == sig("fee()") {
        number(&frame["fee"])
    } else if data[..4] == sig("price_scale(uint256)") {
        number(&frame["parameters"]["price_scale"][U256::from_be_slice(&data[4..36]).to::<usize>()])
    } else if let Some((_, key)) = [
        ("gamma()", "gamma"),
        ("D()", "stored_d"),
        ("mid_fee()", "mid_fee"),
        ("out_fee()", "out_fee"),
        ("fee_gamma()", "fee_gamma"),
    ]
    .into_iter()
    .find(|(signature, _)| data[..4] == sig(signature))
    {
        number(&frame["parameters"][key])
    } else {
        assert_eq!(
            data[..4],
            sig(if frame["profile"] == "tricrypto2" {
                "get_dy(uint256,uint256,uint256)"
            } else {
                "get_dy(int128,int128,uint256)"
            })
        );
        if chain.fail_probe {
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

fn aggregate(chain: &Chain, frames: &BTreeMap<Address, Value>, request: &Value) -> Value {
    let hex = request["params"][0]["data"]
        .as_str()
        .unwrap()
        .trim_start_matches("0x");
    let data = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let n = |at: usize| U256::from_be_slice(&data[at..at + 32]).to::<usize>();
    let count = n(36);
    let results = (0..count)
        .map(|i| {
            let tuple = 68 + n(68 + i * 32);
            let target = Address::from_slice(&data[tuple + 12..tuple + 32]);
            let bytes = tuple + n(tuple + 64);
            answer(
                chain,
                &frames[&target],
                &data[bytes + 32..bytes + 32 + n(bytes)],
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
        let mut result = r.unwrap_or_default();
        bytes.extend(word(U256::from(result.len())));
        result.resize(result.len().div_ceil(32) * 32, 0);
        bytes.extend(result);
    }
    json!(format!(
        "0x{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

async fn setup(profiles: &[CurveProfile], old: bool) -> Fixture {
    let mut graph = Graph::new();
    let mut frames = BTreeMap::new();
    for profile in profiles {
        let pool = profile.pool();
        graph.add_pool(pool.clone());
        frames.insert(pool.address, frame(*profile, old));
    }
    let head = if old { 20_000_000 } else { 26_331_147 };
    let chain = Arc::new(Mutex::new(Chain {
        head,
        frames: BTreeMap::from([(head, frames)]),
        missing: false,
        bad_probe: false,
        fail_probe: false,
    }));
    let control = chain.clone();
    let app=Router::new().route("/",post(move |Json(request):Json<Value>| {
        let control=control.clone();async move {
            let chain=control.lock().unwrap();
            let result=if request["method"]=="eth_getBlockByNumber" {
                let tag=request["params"][0].as_str().unwrap();let height=if tag=="latest" {chain.head} else {u64::from_str_radix(tag.trim_start_matches("0x"),16).unwrap()};
                let h=&chain.frames[&height].values().next().unwrap()["block"];
                json!({"number":format!("0x{height:x}"),"hash":h["hash"],"parentHash":h["parent_hash"],"timestamp":format!("0x{:x}",h["timestamp"].as_u64().unwrap())})
            } else {
                assert_eq!(request["method"],"eth_call");assert_eq!(request["params"][1]["requireCanonical"],true);
                let frames=chain.frames.values().find(|f|f.values().next().unwrap()["block"]["hash"]==request["params"][1]["blockHash"]).unwrap();aggregate(&chain,frames,&request)
            };
            Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = RpcClient::new(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let store = Arc::new(
        SnapshotStore::new(
            graph,
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
    let snapshot = store.acquire().unwrap();
    api::app_with_local_state(
        Arc::new(AppState {
            graph: snapshot.graph().clone(),
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
    min: Option<&str>,
) -> (u16, Value) {
    let mut request =
        json!({"token_in":input,"token_out":output,"amount_in":amount,"split":true,"max_hops":1});
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
async fn two_profiles_quote_all_independent_vectors_with_rpc_disconnected() {
    for profile in [PlainProfile::TwoCrv, PlainProfile::FraxUsdc] {
        let p = CurveProfile::Plain(profile);
        let (store, rpc, _, server) = setup(&[p], false).await;
        StateSynchronizer::new(rpc.clone(), store.clone())
            .sync_once()
            .await
            .unwrap();
        server.abort();
        let before = rpc.request_count();
        let app = app(store.clone(), rpc.clone());
        let reference = frame(p, false);
        let pool = p.pool();
        for q in reference["quotes"].as_array().unwrap() {
            let (status, result) = quote(
                app.clone(),
                pool.coins[q["i"].as_u64().unwrap() as usize],
                pool.coins[q["j"].as_u64().unwrap() as usize],
                q["amount_in"].as_str().unwrap(),
                None,
            )
            .await;
            if q["success"] == true {
                assert_eq!(status, 200, "{result}");
                assert_eq!(result["amount_out"], q["amount_out"]);
                assert_eq!(result["search"]["eligible_pools"], 0);
            } else {
                assert_ne!(status, 200);
            }
        }
        let q = reference["quotes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["i"] == 0 && q["amount_in"] == "1000000")
            .unwrap();
        let min = (number(&q["amount_out"]) + U256::from(1)).to_string();
        assert_eq!(
            quote(app, pool.coins[0], pool.coins[1], "1000000", Some(&min))
                .await
                .0,
            422
        );
        assert_eq!(store.status().curve_state[0].a_precision, 100);
        assert_eq!(
            store.status().curve_state[0].balance_source,
            profile.balance_source()
        );
        assert_eq!(rpc.request_count(), before);
    }
}

#[tokio::test]
async fn heterogeneous_three_and_two_coin_batches_publish_one_consistent_snapshot() {
    let profiles = [
        CurveProfile::Plain(PlainProfile::TwoCrv),
        CurveProfile::Classic3,
        CurveProfile::Plain(PlainProfile::FraxUsdc),
    ];
    let (store, rpc, _, server) = setup(&profiles, false).await;
    StateSynchronizer::new(rpc, store.clone())
        .sync_once()
        .await
        .unwrap();
    assert_eq!(store.status().loaded_pools, 3);
    assert_eq!(store.status().curve_state.len(), 3);
    let classic = store
        .status()
        .curve_state
        .into_iter()
        .find(|p| p.profile == "classic-3pool")
        .unwrap();
    assert_eq!(classic.a_precision, 1);
    assert_eq!(classic.amplification, "4000");
    for p in store
        .status()
        .curve_state
        .into_iter()
        .filter(|p| p.profile != "classic-3pool")
    {
        assert_eq!(p.a_precision, 100);
        assert_eq!(p.amplification, "150000");
    }
    server.abort();
}

#[tokio::test]
async fn historical_balance_changes_and_cross_profile_multi_hop_keep_old_readers_immutable() {
    let profiles = [
        CurveProfile::Plain(PlainProfile::TwoCrv),
        CurveProfile::Plain(PlainProfile::FraxUsdc),
    ];
    let (store, rpc, chain, server) = setup(&profiles, true).await;
    let sync = StateSynchronizer::new(rpc.clone(), store.clone());
    sync.sync_once().await.unwrap();
    let old = store.acquire().unwrap();
    let two = PlainProfile::TwoCrv.pool();
    let frax = PlainProfile::FraxUsdc.pool();
    let request = QuoteRequest {
        token_in: two.coins[1],
        token_out: frax.coins[0],
        amount_in: U256::from(1_000_000_000),
        max_hops: Some(2),
        amount_out_min: None,
    };
    let original = old.route(&request).await.unwrap();
    assert_eq!(original.hops.len(), 2);
    {
        let mut chain = chain.lock().unwrap();
        chain.frames.insert(
            26_331_147,
            profiles
                .into_iter()
                .map(|p| (p.pool().address, frame(p, false)))
                .collect(),
        );
        chain.head = 26_331_147;
    }
    sync.sync_once().await.unwrap();
    let latest = store.acquire().unwrap();
    assert_eq!(
        old.route(&request).await.unwrap().amount_out,
        original.amount_out
    );
    let updated = latest.route(&request).await.unwrap();
    assert_ne!(updated.amount_out, original.amount_out);
    assert_eq!(updated.hops[0].amount_out, U256::from(54_894_841));
    assert_eq!(updated.hops[1].amount_in, updated.hops[0].amount_out);
    let before = rpc.request_count();
    server.abort();
    assert_eq!(
        latest.route(&request).await.unwrap().amount_out,
        updated.amount_out
    );
    assert_eq!(rpc.request_count(), before);
}

#[tokio::test]
async fn unexpected_probe_reverts_and_bad_metadata_do_not_publish_partial_batches() {
    let profiles = [
        CurveProfile::Classic3,
        CurveProfile::Plain(PlainProfile::TwoCrv),
        CurveProfile::Plain(PlainProfile::FraxUsdc),
    ];
    for failure in 0..3 {
        let (store, rpc, chain, server) = setup(&profiles, false).await;
        {
            let mut c = chain.lock().unwrap();
            match failure {
                0 => c.missing = true,
                1 => c.bad_probe = true,
                _ => c.fail_probe = true,
            }
        }
        assert_eq!(
            StateSynchronizer::new(rpc, store.clone()).sync_once().await,
            Err(StateError::Incomplete)
        );
        assert_eq!(store.status().loaded_pools, 0);
        server.abort();
    }
}

#[test]
fn only_legacy_tricrypto_remains_unsupported_without_removing_any_deployment() {
    let mut graph = Graph::new();
    for p in [
        CurveProfile::Classic3,
        CurveProfile::Plain(PlainProfile::TwoCrv),
        CurveProfile::Plain(PlainProfile::FraxUsdc),
    ] {
        graph.add_pool(p.pool());
    }
    for address in [
        "0x80466c64868e1ab14a1ddf27a676c3fcbe638fe5",
        "0xd51a44d3fae010294c616388b506acda1bfaae46",
    ] {
        graph.add_pool(Pool {
            kind: PoolKind::Curve,
            address: address.parse().unwrap(),
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
    }
    let store =
        SnapshotStore::new(graph, RouterConfig::default(), LocalStateConfig::default()).unwrap();
    assert_eq!(store.status().total_pools, 5);
    assert_eq!(store.status().unsupported_pools.len(), 1);
    assert_eq!(
        store.status().unsupported_pools[0].address,
        "0x80466c64868e1ab14a1ddf27a676c3fcbe638fe5"
            .parse::<Address>()
            .unwrap()
    );
    assert!(!store.status().ready);
}

#[tokio::test]
async fn all_four_curve_deployments_share_one_offline_http_snapshot() {
    let profiles = [
        CurveProfile::Plain(PlainProfile::TwoCrv),
        CurveProfile::Tricrypto2,
        CurveProfile::Classic3,
        CurveProfile::Plain(PlainProfile::FraxUsdc),
    ];
    let (store, rpc, _, server) = setup(&profiles, true).await;
    StateSynchronizer::new(rpc.clone(), store.clone())
        .sync_once()
        .await
        .unwrap();
    assert_eq!(store.status().loaded_pools, 4);
    assert!(store.status().unsupported_pools.is_empty());
    let state = store.status().curve_state;
    assert_eq!(state.len(), 4);
    for entry in &state {
        if entry.profile == "tricrypto2" {
            assert_eq!(entry.fee_model, "dynamic-post-trade");
            assert!(entry.fee.is_none());
            assert_eq!(
                entry.crypto.as_ref().unwrap().stored_d,
                frame(CurveProfile::Tricrypto2, true)["parameters"]["stored_d"]
                    .as_str()
                    .unwrap()
            );
        } else {
            assert_eq!(entry.fee_model, "fixed");
            assert!(entry.fee.is_some());
            assert!(entry.crypto.is_none());
        }
    }
    assert_eq!(
        state
            .iter()
            .find(|s| s.profile == "plain-2crv")
            .unwrap()
            .balance_source,
        "erc20-minus-admin"
    );
    server.abort();
    let before = rpc.request_count();
    let app = app(store, rpc.clone());
    for (profile, j) in [
        (CurveProfile::Tricrypto2, 1),
        (CurveProfile::Plain(PlainProfile::FraxUsdc), 1),
    ] {
        let f = frame(profile, true);
        let q = f["quotes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["i"] == 0 && q["j"] == j && q["amount_in"] == "1000000")
            .unwrap();
        let pool = profile.pool();
        let (status, result) =
            quote(app.clone(), pool.coins[0], pool.coins[j], "1000000", None).await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["amount_out"], q["amount_out"]);
        assert_eq!(result["quote_block"], 20_000_000);
        assert_eq!(result["state"]["curve_state"].as_array().unwrap().len(), 4);
        assert_eq!(result["search"]["baseline_rpc_requests"], 0);
        assert_eq!(result["search"]["additional_rpc_requests"], 0);
    }
    assert_eq!(rpc.request_count(), before);
}
