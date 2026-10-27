use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{Address, U256};

use fluxion::adapters::{Curve, Dispatcher, UniswapV2, UniswapV3};
use fluxion::graph::Graph;
use fluxion::model::{Pool, PoolKind, QuoteRequest};
use fluxion::router::{Router, RouterConfig};
use fluxion::rpc::RpcClient;
use fluxion::state::StateCache;

fn addr(s: &str) -> Address {
    s.parse().unwrap()
}

const RPC: &str = "https://eth.drpc.org";
const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const USDC: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const USDT: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
const DAI: &str = "0x6B175474E89094C44Da98b954EedeAC495271d0F";

#[tokio::test]
#[ignore]
async fn end_to_end_multi_hop_v2_v3_curve() {
    let mut graph = Graph::new();
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: addr("0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc"),
        coins: vec![addr(USDC), addr(WETH)],
    });
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV3 { fee: 3000 },
        address: addr("0x8ad599c3A0ff1De082011EFDDc58f1908eb6e6D8"),
        coins: vec![addr(USDC), addr(WETH)],
    });
    graph.add_pool(Pool {
        kind: PoolKind::Curve,
        address: addr("0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7"),
        coins: vec![addr(DAI), addr(USDC), addr(USDT)],
    });

    let rpc = RpcClient::new(RPC.to_string());
    let state = Arc::new(StateCache::new(rpc.clone(), Duration::from_secs(3)));
    let dispatcher = Dispatcher::new(
        Box::new(UniswapV2::new(state)),
        Some(Box::new(UniswapV3::new(rpc.clone()))),
        Some(Box::new(Curve::new(rpc))),
    );

    let router = Router::new(RouterConfig {
        max_hops: 3,
        top_k: 4,
        lambda: 0.0,
        reserve_ratio_k: 0,
    });

    // WETH -> DAI: WETH->USDC (V2/V3) then USDC->DAI (Curve 3pool).
    let req = QuoteRequest {
        token_in: addr(WETH),
        token_out: addr(DAI),
        amount_in: U256::from(1_000_000_000_000_000_000u128), // 1 WETH
        max_hops: Some(3),
        amount_out_min: None,
    };
    let route = router.route(&graph, &dispatcher, &req).await.unwrap();
    assert!(!route.hops.is_empty());
    assert!(route.amount_out > U256::ZERO);
}
