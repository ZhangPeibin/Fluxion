use alloy_primitives::{Address, U256, keccak256};
use axum::Json;
use serde_json::{Value, json};

use fluxion::adapters::Dispatcher;
use fluxion::config::Config;
use fluxion::conversion::ProtocolConversions;
use fluxion::graph::Graph;
use fluxion::model::{ConversionKind, Pool, PoolKind, QuoteRequest};
use fluxion::router::Router;
use fluxion::rpc::RpcClient;
use fluxion::source::{MockV2, QuoteSource};

fn selector(signature: &str) -> String {
    keccak256(signature.as_bytes())[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn wrapper() -> Pool {
    Pool {
        kind: PoolKind::Conversion {
            kind: ConversionKind::WstEth,
            wrap: true,
            unwrap: true,
        },
        address: Address::with_last_byte(2),
        coins: vec![Address::with_last_byte(1), Address::with_last_byte(2)],
    }
}

async fn conversion_rpc() -> (RpcClient, tokio::task::JoinHandle<()>) {
    let app = axum::Router::new().route(
        "/",
        axum::routing::post(|Json(request): Json<Value>| async move {
            let data = request["params"][0]["data"]
                .as_str()
                .unwrap()
                .trim_start_matches("0x");
            let word = if data == selector("stETH()") {
                U256::from(1)
            } else {
                let amount = U256::from_str_radix(&data[8..], 16).unwrap();
                if data.starts_with(&selector("getWstETHByStETH(uint256)")) {
                    amount * U256::from(4) / U256::from(5)
                } else {
                    assert!(data.starts_with(&selector("getStETHByWstETH(uint256)")));
                    amount * U256::from(5) / U256::from(4)
                }
            };
            Json(json!({"jsonrpc": "2.0", "id": 1, "result": format!("0x{word:064x}")}))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = RpcClient::new(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (rpc, server)
}

#[tokio::test]
async fn conversion_uses_exact_amounts_and_enforces_direction_and_underlying() {
    let (rpc, server) = conversion_rpc().await;
    let source = ProtocolConversions::new(rpc);
    let mut pool = wrapper();
    source.validate_pool(&pool).await.unwrap();
    assert_eq!(
        source
            .amount_out(&pool, 0, 1, U256::from(101))
            .await
            .unwrap(),
        U256::from(80)
    );
    assert_eq!(
        source
            .amount_out(&pool, 1, 0, U256::from(101))
            .await
            .unwrap(),
        U256::from(126)
    );
    pool.kind = PoolKind::Conversion {
        kind: ConversionKind::WstEth,
        wrap: true,
        unwrap: false,
    };
    let mut graph = Graph::new();
    graph.add_pool(pool.clone());
    assert_eq!(graph.edge_count(), 1);
    assert!(source.amount_out(&pool, 1, 0, U256::from(1)).await.is_err());
    assert!(source.amount_out(&pool, 0, 1, U256::ZERO).await.is_err());
    pool.coins[0] = Address::with_last_byte(99);
    assert!(source.validate_pool(&pool).await.is_err());
    server.abort();
}

#[tokio::test]
async fn router_composes_conversion_with_existing_wsteth_pool() {
    let (rpc, server) = conversion_rpc().await;
    let conversion = wrapper();
    let weth = Address::with_last_byte(3);
    let swap = Pool {
        kind: PoolKind::UniswapV2,
        address: Address::with_last_byte(10),
        coins: vec![conversion.coins[1], weth],
    };
    let direct = Pool {
        kind: PoolKind::UniswapV2,
        address: Address::with_last_byte(11),
        coins: vec![conversion.coins[0], weth],
    };
    let source = Dispatcher::new(
        Box::new(MockV2::new(std::collections::HashMap::from([
            (swap.address, [U256::from(800_000), U256::from(1_000_000)]),
            (direct.address, [U256::from(1000), U256::from(500)]),
        ]))),
        None,
        None,
    )
    .with_conversions(Box::new(ProtocolConversions::new(rpc)));
    let mut graph = Graph::new();
    for pool in [conversion, swap, direct] {
        graph.add_pool(pool);
    }
    let req = QuoteRequest {
        token_in: Address::with_last_byte(1),
        token_out: weth,
        amount_in: U256::from(100),
        max_hops: None,
        amount_out_min: None,
    };
    let result = Router::default()
        .route(&graph, &source, &req)
        .await
        .unwrap();
    assert_eq!(result.hops.len(), 2);
    assert_eq!(result.hops[0].pool.kind.operation(0), "wrap");
    assert!(result.amount_out > U256::from(90));
    // The public API must expose conversion operations, and must not present a
    // synthetic infinite reserve as if the wrapper were an AMM pool.
    use tower::ServiceExt;
    let router = Router::default();
    router.set_snapshot(std::sync::Arc::new(
        fluxion::router::PriceSnapshot::build(&graph, &source).await,
    ));
    let app = fluxion::api::app(std::sync::Arc::new(fluxion::api::AppState {
        graph,
        source: std::sync::Arc::new(source),
        router,
        limiter: fluxion::api::RateLimiter::new(100, std::time::Duration::from_secs(1)),
        rpc: None,
        request_timeout: std::time::Duration::from_secs(3),
    }));
    let response = app.clone().oneshot(axum::http::Request::builder().method("POST").uri("/quote").header("content-type", "application/json").body(axum::body::Body::from(json!({"token_in": format!("{:#x}", req.token_in), "token_out": format!("{:#x}", weth), "amount_in": "100"}).to_string())).unwrap()).await.unwrap();
    let response: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(response["path"][0]["operation"], "wrap");
    assert_eq!(response["path"][1]["operation"], "swap");
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/graph")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let response: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    let conversions = response["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|edge| edge["platform"] == "wsteth")
        .collect::<Vec<_>>();
    assert_eq!(conversions.len(), 2);
    assert!(conversions.iter().all(|edge| edge.get("reserve").is_none()));
    server.abort();
}

#[test]
fn config_rejects_unknown_interfaces_and_inconsistent_wrapper_tokens() {
    assert!(serde_yaml::from_str::<Config>("protocol_conversions:\n- kind: made-up\n").is_err());
    let mut config = Config::default();
    config.protocol_conversions[0].wrapped = Address::with_last_byte(99);
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn conversion_path_survives_spot_better_but_thin_amm_paths() {
    use fluxion::router::RouterConfig;
    let (rpc, server) = conversion_rpc().await;
    let conversion = wrapper();
    let mut graph = Graph::new();
    graph.add_pool(conversion);
    let mut reserves = std::collections::HashMap::new();
    // Wrap 1->2, then deep swaps 2->3->4 yield ~197 for input 100.
    // Thin swaps 1->5->3->4 look 25x better at spot but yield only ~5.
    for (id, from, to, r0, r1) in [
        (10, 2, 3, 800_000, 1_000_000),
        (11, 3, 4, 1_000_000, 2_000_000),
        (12, 1, 5, 1, 5),
        (13, 5, 3, 1, 5),
    ] {
        let address = Address::with_last_byte(id);
        graph.add_pool(Pool {
            kind: PoolKind::UniswapV2,
            address,
            coins: vec![Address::with_last_byte(from), Address::with_last_byte(to)],
        });
        reserves.insert(address, [U256::from(r0), U256::from(r1)]);
    }
    let source = Dispatcher::new(Box::new(MockV2::new(reserves)), None, None)
        .with_conversions(Box::new(ProtocolConversions::new(rpc)));
    let result = Router::new(RouterConfig {
        top_k: 1,
        ..Default::default()
    })
    .route(
        &graph,
        &source,
        &QuoteRequest {
            token_in: Address::with_last_byte(1),
            token_out: Address::with_last_byte(4),
            amount_in: U256::from(100),
            max_hops: Some(3),
            amount_out_min: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(result.hops.len(), 3);
    assert_eq!(result.hops[0].pool.kind.operation(0), "wrap");
    assert!(result.amount_out > U256::from(190));
    server.abort();
}

#[tokio::test]
async fn adding_a_thin_shibaswap_pool_cannot_crowd_out_the_original_route() {
    use fluxion::router::RouterConfig;
    let mut graph = Graph::new();
    let mut reserves = std::collections::HashMap::new();
    for (id, from, to, r0, r1, kind) in [
        (10, 1, 3, 1_000_000, 2_000_000, PoolKind::UniswapV2),
        (11, 3, 4, 1_000_000, 2_000_000, PoolKind::UniswapV2),
        (12, 1, 3, 1, 10, PoolKind::ShibaSwapV2),
    ] {
        let address = Address::with_last_byte(id);
        graph.add_pool(Pool {
            kind,
            address,
            coins: vec![Address::with_last_byte(from), Address::with_last_byte(to)],
        });
        reserves.insert(address, [U256::from(r0), U256::from(r1)]);
    }
    let result = Router::new(RouterConfig {
        top_k: 1,
        ..Default::default()
    })
    .route(
        &graph,
        &MockV2::new(reserves),
        &QuoteRequest {
            token_in: Address::with_last_byte(1),
            token_out: Address::with_last_byte(4),
            amount_in: U256::from(100),
            max_hops: Some(2),
            amount_out_min: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(result.hops[0].pool.address, Address::with_last_byte(10));
    assert!(result.amount_out > U256::from(390));
}

#[tokio::test]
#[ignore]
async fn mainnet_wsteth_round_trip_and_shibaswap_router_agree() {
    use fluxion::adapters::UniswapV2;
    let rpc = RpcClient::new("https://eth.drpc.org".into());
    let block = rpc.block_number().await.unwrap();
    let rpc = rpc.at_block(block);
    let config = Config::default();
    let conversion = config.protocol_conversions[0].pool();
    let source = ProtocolConversions::new(rpc.clone());
    source.validate_pool(&conversion).await.unwrap();
    let amount = U256::from(1_000_000_000_000_000_000u128);
    let wrapped = source.amount_out(&conversion, 0, 1, amount).await.unwrap();
    let unwrapped = source.amount_out(&conversion, 1, 0, wrapped).await.unwrap();
    assert!(unwrapped <= amount && amount - unwrapped <= U256::from(10));
    let weth: Address = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        .parse()
        .unwrap();
    let shib: Address = "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce"
        .parse()
        .unwrap();
    let discovered = fluxion::discovery::discover_shibaswap_v2(
        &Config {
            token_allowlist: vec![format!("{weth:#x}"), format!("{shib:#x}")],
            ..Default::default()
        },
        &rpc,
    )
    .await
    .unwrap();
    assert_eq!(discovered.len(), 1);
    let pool = &discovered[0];
    let source = UniswapV2::new(std::sync::Arc::new(fluxion::adapters::RpcV2Reserves::new(
        rpc.clone(),
    )));
    let i = pool.coins.iter().position(|coin| *coin == weth).unwrap();
    let out = source.amount_out(pool, i, 1 - i, amount).await.unwrap();
    let router: Address = "0x03f7724180AA6b939894B5Ca4314783B0b36b329"
        .parse()
        .unwrap();
    let data = format!(
        "d06ca61f{amount:064x}{:064x}{:064x}{:064x}{:064x}",
        64,
        2,
        U256::from_be_slice(weth.as_slice()),
        U256::from_be_slice(shib.as_slice())
    );
    let result = rpc.eth_call(router, &data).await.unwrap();
    assert_eq!(out, U256::from_be_slice(&result[96..128]));
    eprintln!(
        "block {block}; ShibaSwap SHIB/WETH pool {}: router and adapter output {out}",
        pool.address
    );
}
