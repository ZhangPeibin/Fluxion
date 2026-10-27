use std::sync::Arc;

use alloy_primitives::{Address, U256};

use fluxion::adapters::{Curve, Dispatcher, RpcV2Reserves, UniswapV2, UniswapV3};
use fluxion::graph::Graph;
use fluxion::model::{Pool, PoolKind, QuoteRequest};
use fluxion::router::Router;
use fluxion::rpc::RpcClient;
use fluxion::source::QuoteSource;

fn addr(s: &str) -> Address {
    s.parse().unwrap()
}

const RPC: &str = "https://eth.drpc.org";

const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const USDC: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const WETH_USDC_V2: &str = "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc";
const WETH_USDC_V3: &str = "0x8ad599c3A0ff1De082011EFDDc58f1908eb6e6D8";

fn one_weth() -> U256 {
    U256::from(1_000_000_000_000_000_000u128)
}

#[tokio::test]
#[ignore]
async fn real_v3_quote_against_mainnet() {
    let v3 = UniswapV3::new(RpcClient::new(RPC.to_string()));
    let pool = Pool {
        kind: PoolKind::UniswapV3 { fee: 3000 },
        address: addr(WETH_USDC_V3),
        coins: vec![addr(USDC), addr(WETH)],
    };
    let out = v3.amount_out(&pool, 1, 0, one_weth()).await.unwrap();
    assert!(out > U256::ZERO);
}

#[tokio::test]
#[ignore]
async fn v2_vs_v3_better_venue() {
    let mut graph = Graph::new();
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: addr(WETH_USDC_V2),
        coins: vec![addr(USDC), addr(WETH)],
    });
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV3 { fee: 3000 },
        address: addr(WETH_USDC_V3),
        coins: vec![addr(USDC), addr(WETH)],
    });

    let rpc = RpcClient::new(RPC.to_string());
    let dispatcher = Dispatcher::new(
        Box::new(UniswapV2::new(Arc::new(RpcV2Reserves::new(rpc.clone())))),
        Some(Box::new(UniswapV3::new(rpc))),
        None,
    );

    let req = QuoteRequest {
        token_in: addr(WETH),
        token_out: addr(USDC),
        amount_in: one_weth(),
        max_hops: Some(3),
        amount_out_min: None,
    };
    let route = Router::default()
        .route(&graph, &dispatcher, &req)
        .await
        .unwrap();
    assert_eq!(route.hops.len(), 1);
    assert!(route.amount_out > U256::ZERO);
}

#[tokio::test]
#[ignore]
async fn real_curve_3pool_quote() {
    let curve = Curve::new(RpcClient::new(RPC.to_string()));
    let pool = Pool {
        kind: PoolKind::Curve,
        address: addr("0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7"),
        coins: vec![
            addr("0x6B175474E89094C44Da98b954EedeAC495271d0F"), // DAI
            addr(USDC),
            addr("0xdAC17F958D2ee523a2206206994597C13D831ec7"), // USDT
        ],
    };
    // 1 USDC -> DAI
    let out = curve
        .amount_out(&pool, 1, 0, U256::from(1_000_000u64))
        .await
        .unwrap();
    assert!(out > U256::ZERO);
}

#[tokio::test]
#[ignore]
async fn real_curve_crypto_pool_quote() {
    // tricrypto2 (USDT/WBTC/WETH) uses get_dy(uint256,uint256,uint256).
    let curve = Curve::new(RpcClient::new(RPC.to_string()));
    let pool = Pool {
        kind: PoolKind::Curve,
        address: addr("0xD51a44d3FaE010294C616388b506AcdA1bfAAE46"),
        coins: vec![
            addr("0xdAC17F958D2ee523a2206206994597C13D831ec7"), // USDT
            addr("0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599"), // WBTC
            addr(WETH),
        ],
    };
    // 100,000 USDT -> WBTC
    let out = curve
        .amount_out(&pool, 0, 1, U256::from(100_000_000_000u64))
        .await
        .unwrap();
    assert!(out > U256::ZERO);
}
