use std::sync::Arc;

use alloy_primitives::{Address, U256};

use fluxion::adapters::{RpcV2Reserves, UniswapV2};
use fluxion::graph::Graph;
use fluxion::model::{Pool, PoolKind, QuoteRequest};
use fluxion::router::Router;
use fluxion::rpc::RpcClient;

fn addr(s: &str) -> Address {
    s.parse().unwrap()
}

const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const USDC: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const USDT: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
const WETH_USDC: &str = "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc";
const WETH_USDT: &str = "0x0d4a11d5EEaaC28EC3F61d100daF4d40471f1852";

#[tokio::test]
#[ignore]
async fn real_v2_quote_against_mainnet() {
    let mut graph = Graph::new();
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: addr(WETH_USDC),
        coins: vec![addr(USDC), addr(WETH)],
    });
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: addr(WETH_USDT),
        coins: vec![addr(USDT), addr(WETH)],
    });

    let source = UniswapV2::new(Arc::new(RpcV2Reserves::new(RpcClient::new(
        "https://eth.drpc.org".to_string(),
    ))));
    let router = Router::default();

    let req = QuoteRequest {
        token_in: addr(WETH),
        token_out: addr(USDC),
        amount_in: U256::from(1_000_000_000_000_000_000u128), // 1 WETH
        max_hops: Some(3),
        amount_out_min: None,
    };
    let route = router.route(&graph, &source, &req).await.unwrap();
    assert_eq!(route.hops.len(), 1);
    assert!(route.amount_out > U256::ZERO);
}

#[tokio::test]
#[ignore]
async fn discover_v2_against_mainnet() {
    dotenvy::dotenv().ok();
    let key = std::env::var("GRAPH_API_KEY").expect("GRAPH_API_KEY not set");
    let config = fluxion::config::Config {
        min_liquidity: 10_000_000, // limit result size
        ..Default::default()
    };
    let pools = fluxion::discovery::discover_v2(&config, &key)
        .await
        .unwrap();
    assert!(!pools.is_empty());
}
