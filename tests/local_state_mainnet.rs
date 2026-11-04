//! Read-only parity against deployed V2 Routers, not another copy of our math.
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use alloy_primitives::{Address, U256};
use fluxion::{
    graph::Graph,
    local_state::{LocalStateConfig, SnapshotStore, StateSynchronizer, SyncMode},
    model::{Pool, PoolKind, QuoteRequest},
    router::RouterConfig,
    rpc::RpcClient,
    split::{SplitConfig, SplitOptimizer},
};

async fn router_output(rpc: &RpcClient, kind: PoolKind, amount: U256, path: &[Address]) -> U256 {
    let router: Address = match kind {
        PoolKind::UniswapV2 => "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
        PoolKind::ShibaSwapV2 => "0x03f7724180AA6b939894B5Ca4314783B0b36b329",
        _ => panic!("not V2"),
    }
    .parse()
    .unwrap();
    let mut data = format!("d06ca61f{amount:064x}{:064x}{:064x}", 64, path.len());
    for token in path {
        data += &format!("{:064x}", U256::from_be_slice(token.as_slice()));
    }
    let result = rpc.eth_call(router, &data).await.unwrap();
    assert_eq!(U256::from_be_slice(&result[32..64]), U256::from(path.len()));
    U256::from_be_slice(&result[result.len() - 32..])
}

#[tokio::test]
#[ignore]
async fn local_v2_snapshots_match_mainnet_routers_for_splits_and_multi_hop() {
    let rpc = RpcClient::new("https://eth.drpc.org".into());
    let shib: Address = "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce"
        .parse()
        .unwrap();
    let weth: Address = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        .parse()
        .unwrap();
    let usdc: Address = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        .parse()
        .unwrap();
    let mut graph = Graph::new();
    for (address, kind, coins) in [
        (
            "0x811beed0119b4afce20d2583eb608c6f7af1954f",
            PoolKind::UniswapV2,
            vec![shib, weth],
        ),
        (
            "0xcf6daab95c476106eca715d48de4b13287ffdeaa",
            PoolKind::ShibaSwapV2,
            vec![shib, weth],
        ),
        (
            "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc",
            PoolKind::UniswapV2,
            vec![usdc, weth],
        ),
    ] {
        graph.add_pool(Pool {
            kind,
            address: address.parse().unwrap(),
            coins,
        });
    }
    let store = Arc::new(
        SnapshotStore::new(
            graph,
            RouterConfig::default(),
            LocalStateConfig {
                max_age_secs: 300,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let started = Instant::now();
    StateSynchronizer::new(rpc.clone(), store.clone())
        .sync_once()
        .await
        .unwrap();
    let initialization = started.elapsed();
    let snapshot = store.acquire().unwrap();
    let oracle = RpcClient::new("https://eth.drpc.org".into()).at_block_hash(snapshot.block().hash);
    let count = rpc.request_count();
    let optimizer = SplitOptimizer::new(SplitConfig::multi_pool()).unwrap();
    let mut samples = Vec::new();
    for (input, output, amount) in [
        (weth, shib, U256::from(333_333_333_333_333_333u128)),
        (weth, shib, U256::from(33_333_333_333_333_333_333u128)),
        (
            shib,
            weth,
            U256::from(8_000_000_000u128) * U256::from(10u128.pow(18)),
        ),
        (usdc, shib, U256::from(10_000_000_000u128)),
    ] {
        let request = QuoteRequest {
            token_in: input,
            token_out: output,
            amount_in: amount,
            max_hops: None,
            amount_out_min: None,
        };
        let started = Instant::now();
        let result = snapshot
            .split(
                &optimizer,
                &request,
                tokio::time::Instant::now() + Duration::from_secs(2),
            )
            .await
            .unwrap();
        samples.push(started.elapsed().as_micros());
        assert!(result.amount_out >= result.baseline.amount_out);
        assert_eq!(
            result.allocations.iter().map(|a| a.amount_in).sum::<U256>(),
            amount
        );
        for allocation in &result.allocations {
            let mut value = allocation.amount_in;
            for hop in &allocation.hops {
                value = router_output(
                    &oracle,
                    hop.pool.kind,
                    value,
                    &[hop.token_in, hop.token_out],
                )
                .await;
                assert_eq!(value, hop.amount_out);
            }
            assert_eq!(value, allocation.amount_out);
        }
        assert_eq!(
            result
                .allocations
                .iter()
                .map(|a| a.amount_out)
                .sum::<U256>(),
            result.amount_out
        );
        if input == usdc {
            assert!(result.allocations[0].hops.len() >= 2);
        }
    }
    assert_eq!(
        rpc.request_count(),
        count,
        "local calculations cannot issue RPC"
    );
    assert_eq!(
        oracle.block_hash(snapshot.block().number).await.unwrap(),
        snapshot.block().hash.to_string()
    );
    eprintln!(
        "block={} hash={} init_ms={} sync_rpc={} local_compute_us={samples:?}; V2/ShibaSwap subset, not the 90-row multi-venue benchmark",
        snapshot.block().number,
        snapshot.block().hash,
        initialization.as_millis(),
        store.status().sync_rpc_requests
    );
}

#[tokio::test]
#[ignore]
async fn runtime_queued_pool_is_quoted_after_atomic_universe_swap() {
    let rpc = RpcClient::new("https://eth.drpc.org".into());
    let weth: Address = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        .parse()
        .unwrap();
    let usdc: Address = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        .parse()
        .unwrap();
    let shib: Address = "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce"
        .parse()
        .unwrap();
    let usdc_weth: Address = "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc"
        .parse()
        .unwrap();
    let shib_weth: Address = "0x811beed0119b4afce20d2583eb608c6f7af1954f"
        .parse()
        .unwrap();

    let mut graph = Graph::new();
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: usdc_weth,
        coins: vec![usdc, weth],
    });
    let store = Arc::new(
        SnapshotStore::new(
            graph,
            RouterConfig::default(),
            LocalStateConfig {
                sync_mode: SyncMode::Incremental,
                max_age_secs: 300,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let sync = StateSynchronizer::new(rpc, store.clone());
    sync.sync_once().await.unwrap();
    let before_swap = store.acquire().unwrap();
    assert_eq!(before_swap.graph().pool_count(), 1);

    let added = store.queue_new_pools(vec![Pool {
        kind: PoolKind::UniswapV2,
        address: shib_weth,
        coins: vec![shib, weth],
    }]);
    assert_eq!(added, 1);
    assert!(store.has_pending_universe());
    sync.sync_incremental_once().await.unwrap();

    let after_swap = store.acquire().unwrap();
    assert_eq!(after_swap.graph().pool_count(), 2);
    let request = QuoteRequest {
        token_in: weth,
        token_out: shib,
        amount_in: U256::from(10u128.pow(18)),
        max_hops: None,
        amount_out_min: None,
    };
    let route = after_swap.route(&request).await.unwrap();
    assert!(!route.hops.is_empty());

    let old_request = QuoteRequest {
        token_in: weth,
        token_out: usdc,
        amount_in: U256::from(10u128.pow(18)),
        max_hops: None,
        amount_out_min: None,
    };
    assert!(
        !before_swap
            .route(&old_request)
            .await
            .unwrap()
            .hops
            .is_empty()
    );
}
