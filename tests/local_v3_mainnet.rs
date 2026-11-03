use alloy_primitives::{Address, U256};
use fluxion::{
    graph::Graph,
    local_state::{LocalStateConfig, SnapshotStore, StateSynchronizer},
    model::{Pool, PoolKind, QuoteRequest},
    router::RouterConfig,
    rpc::RpcClient,
};

async fn independent_hop(rpc: &RpcClient, hop: &fluxion::model::Hop) -> U256 {
    let input = U256::from_be_slice(hop.token_in.as_slice());
    let output = U256::from_be_slice(hop.token_out.as_slice());
    let amount = hop.amount_in;
    let (target, data, first) = match hop.pool.kind {
        PoolKind::UniswapV3 { fee } => (
            "0x61ffe014ba17989e743c5f6cb21bf9697530b21e",
            format!(
                "c6a5026a{input:064x}{output:064x}{amount:064x}{fee:064x}{:064x}",
                0
            ),
            true,
        ),
        kind => {
            let target = match kind {
                PoolKind::UniswapV2 => "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
                PoolKind::ShibaSwapV2 => "0x03f7724180AA6b939894B5Ca4314783B0b36b329",
                _ => panic!("unsupported test venue"),
            };
            (
                target,
                format!(
                    "d06ca61f{amount:064x}{:064x}{:064x}{input:064x}{output:064x}",
                    64, 2
                ),
                false,
            )
        }
    };
    let result = rpc.eth_call(target.parse().unwrap(), &data).await.unwrap();
    if first {
        U256::from_be_slice(&result[..32])
    } else {
        U256::from_be_slice(&result[result.len() - 32..])
    }
}

#[tokio::test]
#[ignore]
async fn mixed_v2_v3_splits_and_multi_hop_match_independent_mainnet_calls() {
    use fluxion::split::{SplitConfig, SplitOptimizer};
    let weth: Address = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        .parse()
        .unwrap();
    let shib: Address = "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce"
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
            "0x2f62f2b4c5fcd7570a709dec05d68ea19c82a9ec",
            PoolKind::UniswapV3 { fee: 3000 },
            vec![shib, weth],
        ),
        (
            "0x5764a6f2212d502bc5970f9f129ffcd61e5d7563",
            PoolKind::UniswapV3 { fee: 10000 },
            vec![shib, weth],
        ),
        (
            "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc",
            PoolKind::UniswapV2,
            vec![usdc, weth],
        ),
        (
            "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640",
            PoolKind::UniswapV3 { fee: 500 },
            vec![usdc, weth],
        ),
    ] {
        graph.add_pool(Pool {
            kind,
            address: address.parse().unwrap(),
            coins,
        });
    }
    let rpc = RpcClient::new("https://eth.drpc.org".into());
    let store = Arc::new(
        SnapshotStore::new(
            graph,
            RouterConfig::default(),
            LocalStateConfig {
                max_age_secs: 300,
                sync_timeout_ms: 120_000,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    StateSynchronizer::new(rpc.clone(), store.clone())
        .sync_once()
        .await
        .unwrap();
    let snapshot = store.acquire().unwrap();
    let oracle = RpcClient::new("https://eth.drpc.org".into()).at_block_hash(snapshot.block().hash);
    let before = rpc.request_count();
    let optimizer = SplitOptimizer::new(SplitConfig::multi_pool()).unwrap();
    let mut times = Vec::new();
    for (input, output, amount) in [
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
        let start = Instant::now();
        let result = snapshot
            .split(
                &optimizer,
                &request,
                tokio::time::Instant::now() + std::time::Duration::from_secs(8),
            )
            .await
            .unwrap();
        times.push(start.elapsed().as_micros());
        assert!(result.amount_out >= result.baseline.amount_out);
        assert_eq!(
            result.allocations.iter().map(|a| a.amount_in).sum::<U256>(),
            amount
        );
        assert_eq!(
            result
                .allocations
                .iter()
                .map(|a| a.amount_out)
                .sum::<U256>(),
            result.amount_out
        );
        for branch in &result.allocations {
            for hop in &branch.hops {
                assert_eq!(independent_hop(&oracle, hop).await, hop.amount_out);
            }
        }
        if input == usdc {
            assert!(result.allocations[0].hops.len() >= 2);
        }
        eprintln!(
            "mixed kind={:?} branches={} in={amount} out={} baseline={} extra_quotes={}",
            result.kind,
            result.allocations.len(),
            result.amount_out,
            result.baseline.amount_out,
            result.stats.additional_quote_calls
        );
    }
    assert_eq!(rpc.request_count(), before);
    assert_eq!(
        oracle.block_hash(snapshot.block().number).await.unwrap(),
        snapshot.block().hash.to_string()
    );
    eprintln!(
        "mixed block={} hash={} compute_us={times:?} sync_rpc={before}",
        snapshot.block().number,
        snapshot.block().hash
    );
}
use std::{sync::Arc, time::Instant};

#[tokio::test]
#[ignore]
async fn local_v3_exact_amounts_match_quoter_both_directions_and_multiple_fees() {
    let usdc: Address = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        .parse()
        .unwrap();
    let weth: Address = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        .parse()
        .unwrap();
    let shib: Address = "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce"
        .parse()
        .unwrap();
    let usdt: Address = "0xdac17f958d2ee523a2206206994597c13d831ec7"
        .parse()
        .unwrap();
    let quoter: Address = "0x61ffe014ba17989e743c5f6cb21bf9697530b21e"
        .parse()
        .unwrap();
    let pools = [
        (
            "0x3416cf6c708da44db2624d63ea0aaef7113527c6",
            100,
            vec![usdc, usdt],
        ),
        (
            "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640",
            500,
            vec![usdc, weth],
        ),
        (
            "0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8",
            3000,
            vec![usdc, weth],
        ),
        (
            "0x2f62f2b4c5fcd7570a709dec05d68ea19c82a9ec",
            3000,
            vec![shib, weth],
        ),
        (
            "0x5764a6f2212d502bc5970f9f129ffcd61e5d7563",
            10000,
            vec![shib, weth],
        ),
    ];
    let mut total = 0;
    let mut crossed = 0;
    for (address, fee, coins) in pools {
        let mut graph = Graph::new();
        graph.add_pool(Pool {
            kind: PoolKind::UniswapV3 { fee },
            address: address.parse().unwrap(),
            coins: coins.clone(),
        });
        let rpc = RpcClient::new("https://eth.drpc.org".into());
        let store = Arc::new(
            SnapshotStore::new(
                graph,
                RouterConfig::default(),
                LocalStateConfig {
                    max_age_secs: 300,
                    sync_timeout_ms: 120_000,
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        let start = Instant::now();
        StateSynchronizer::new(rpc.clone(), store.clone())
            .sync_once()
            .await
            .unwrap();
        let init = start.elapsed();
        let snapshot = store.acquire().unwrap();
        let oracle =
            RpcClient::new("https://eth.drpc.org".into()).at_block_hash(snapshot.block().hash);
        let before = rpc.request_count();
        let mut timings = Vec::new();
        for direction in [0, 1] {
            let input = coins[direction];
            let output = coins[1 - direction];
            let amount = if input == usdc || input == usdt {
                U256::from(1_000_000_000u128)
            } else if input == shib {
                U256::from(80_000_000u128) * U256::from(10u128.pow(18))
            } else {
                U256::from(333_333_333_333_333_333u128)
            };
            for scale in [1, 10, 100] {
                let amount = amount * U256::from(scale);
                let request = QuoteRequest {
                    token_in: input,
                    token_out: output,
                    amount_in: amount,
                    max_hops: None,
                    amount_out_min: None,
                };
                let start = Instant::now();
                let local = snapshot.route(&request).await.unwrap();
                timings.push(start.elapsed().as_micros());
                let data = format!(
                    "c6a5026a{:064x}{:064x}{amount:064x}{:064x}{:064x}",
                    U256::from_be_slice(input.as_slice()),
                    U256::from_be_slice(output.as_slice()),
                    fee,
                    0
                );
                let result = oracle.eth_call(quoter, &data).await.unwrap();
                let expected = U256::from_be_slice(&result[..32]);
                assert_eq!(
                    local.amount_out,
                    expected,
                    "pool={address} fee={fee} direction={direction} amount={amount} block={}",
                    snapshot.block().number
                );
                crossed += usize::from(!U256::from_be_slice(&result[64..96]).is_zero());
                total += 1;
            }
        }
        assert_eq!(rpc.request_count(), before);
        assert_eq!(
            oracle.block_hash(snapshot.block().number).await.unwrap(),
            snapshot.block().hash.to_string()
        );
        eprintln!(
            "V3 pool={address} block={} init_ms={} rpc={} compute_us={timings:?} coverage={}",
            snapshot.block().number,
            init.as_millis(),
            before,
            serde_json::to_string(&store.status().v3_coverage).unwrap()
        );
    }
    assert_eq!(total, 30);
    assert!(
        crossed > 0,
        "the parity corpus must actually exercise initialized tick crossings"
    );
    eprintln!("V3 parity={total}/{total}; samples with initialized tick crossings={crossed}");
}
