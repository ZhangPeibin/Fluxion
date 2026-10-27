use std::collections::HashMap;

use alloy_primitives::{Address, U256};

use fluxion::graph::Graph;
use fluxion::model::{Pool, PoolKind, QuoteRequest};
use fluxion::router;
use fluxion::source::MockV2;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let eth = Address::with_last_byte(1);
    let usdc = Address::with_last_byte(2);
    let wbtc = Address::with_last_byte(3);

    let mut graph = Graph::new();

    let pool_a = Pool {
        kind: PoolKind::UniswapV2,
        address: Address::with_last_byte(10),
        coins: vec![eth, usdc],
    };
    let pool_b = Pool {
        kind: PoolKind::UniswapV2,
        address: Address::with_last_byte(11),
        coins: vec![eth, usdc],
    };
    let pool_c = Pool {
        kind: PoolKind::UniswapV2,
        address: Address::with_last_byte(12),
        coins: vec![usdc, wbtc],
    };
    graph.add_pool(pool_a.clone());
    graph.add_pool(pool_b.clone());
    graph.add_pool(pool_c.clone());

    let mut reserves = HashMap::new();
    reserves.insert(
        pool_a.address,
        [U256::from(1000u64), U256::from(3_000_000u64)],
    );
    reserves.insert(
        pool_b.address,
        [U256::from(500u64), U256::from(2_000_000u64)],
    );
    reserves.insert(
        pool_c.address,
        [U256::from(1_000_000u64), U256::from(40u64)],
    );
    let source = MockV2::new(reserves);

    let request = QuoteRequest {
        token_in: eth,
        token_out: usdc,
        amount_in: U256::from(1u64),
        max_hops: Some(3),
        amount_out_min: None,
    };

    let route = router::route(&graph, &source, &request).await.unwrap();
    let hop = &route.hops[0];
    println!("amount_in  = {}", request.amount_in);
    println!("amount_out = {}", route.amount_out);
    println!("pool       = {}", hop.pool.address);
    println!("hops       = {}", route.hops.len());
}
