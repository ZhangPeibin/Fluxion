//! Behavioral checks for the v1 path-search acceptance criteria.
use std::collections::HashMap;

use alloy_primitives::{Address, U256};
use fluxion::graph::Graph;
use fluxion::model::{Pool, PoolKind, QuoteRequest};
use fluxion::router::{Router, RouterConfig};
use fluxion::source::MockV2;

fn add_pool(
    graph: &mut Graph,
    reserves: &mut HashMap<Address, [U256; 2]>,
    id: u8,
    from: u8,
    to: u8,
    r0: u64,
    r1: u64,
) {
    let address = Address::with_last_byte(id);
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address,
        coins: vec![Address::with_last_byte(from), Address::with_last_byte(to)],
    });
    reserves.insert(address, [U256::from(r0), U256::from(r1)]);
}

fn request(max_hops: usize) -> QuoteRequest {
    QuoteRequest {
        token_in: Address::with_last_byte(1),
        token_out: Address::with_last_byte(2),
        amount_in: U256::from(100),
        max_hops: Some(max_hops),
        amount_out_min: None,
    }
}

#[tokio::test]
async fn negative_weight_cycle_does_not_repeat_tokens() {
    let mut graph = Graph::new();
    let mut reserves = HashMap::new();
    // 1->3->2->1 has an attractive spot-rate product of 8. A route
    // must not exploit the cycle by revisiting the input or destination.
    add_pool(&mut graph, &mut reserves, 10, 1, 3, 1_000_000, 2_000_000);
    add_pool(&mut graph, &mut reserves, 11, 3, 2, 1_000_000, 2_000_000);
    add_pool(&mut graph, &mut reserves, 12, 2, 1, 1_000_000, 2_000_000);
    let result = Router::default()
        .route(&graph, &MockV2::new(reserves), &request(8))
        .await
        .unwrap();
    assert_eq!(result.hops.len(), 2);
    let mut tokens = vec![request(8).token_in];
    for hop in result.hops {
        assert!(!tokens.contains(&hop.token_out));
        tokens.push(hop.token_out);
    }
}

#[tokio::test]
async fn cheaper_long_path_cannot_displace_better_short_path_before_refinement() {
    let mut graph = Graph::new();
    let mut reserves = HashMap::new();
    // Deep two-hop route: spot 4, exact output ~397.
    add_pool(&mut graph, &mut reserves, 10, 1, 3, 1_000_000, 2_000_000);
    add_pool(&mut graph, &mut reserves, 11, 3, 2, 1_000_000, 2_000_000);
    // Thin three-hop route: apparently better spot 8, but poor real output.
    add_pool(&mut graph, &mut reserves, 12, 1, 4, 100, 200);
    add_pool(&mut graph, &mut reserves, 13, 4, 5, 100, 200);
    add_pool(&mut graph, &mut reserves, 14, 5, 2, 100, 200);
    let router = Router::new(RouterConfig {
        top_k: 1,
        ..Default::default()
    });
    let result = router
        .route(&graph, &MockV2::new(reserves), &request(3))
        .await
        .unwrap();
    assert_eq!(result.hops.len(), 2);
    assert!(result.amount_out > U256::from(390));
}
