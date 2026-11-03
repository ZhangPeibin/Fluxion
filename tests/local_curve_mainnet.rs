use alloy_primitives::{Address, U256};
use fluxion::{
    adapters::Curve,
    conversion::ProtocolConversions,
    curve_local::{COINS, THREE_POOL},
    graph::Graph,
    local_state::{LocalStateConfig, SnapshotStore, StateSynchronizer},
    model::{ConversionKind, Hop, Pool, PoolKind, QuoteRequest},
    router::RouterConfig,
    rpc::RpcClient,
    source::QuoteSource,
    split::{SplitConfig, SplitOptimizer},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn curve_pool() -> Pool {
    Pool {
        kind: PoolKind::Curve,
        address: THREE_POOL.parse().unwrap(),
        coins: COINS.map(|s| s.parse().unwrap()).to_vec(),
    }
}
fn request(input: Address, output: Address, amount: U256, hops: usize) -> QuoteRequest {
    QuoteRequest {
        token_in: input,
        token_out: output,
        amount_in: amount,
        max_hops: Some(hops),
        amount_out_min: None,
    }
}

#[tokio::test]
#[ignore]
async fn classic_3pool_local_quotes_match_all_six_directions_at_one_mainnet_block() {
    let rpc = RpcClient::new("https://eth.drpc.org".into());
    let pool = curve_pool();
    let mut graph = Graph::new();
    graph.add_pool(pool.clone());
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
    let start = Instant::now();
    StateSynchronizer::new(rpc.clone(), store.clone())
        .sync_once()
        .await
        .unwrap();
    let initialization = start.elapsed();
    let snapshot = store.acquire().unwrap();
    let oracle_rpc =
        RpcClient::new("https://eth.drpc.org".into()).at_block_hash(snapshot.block().hash);
    let oracle = Curve::new(oracle_rpc.clone());
    let optimizer = SplitOptimizer::new(SplitConfig::multi_pool()).unwrap();
    let before = rpc.request_count();
    let mut times = Vec::new();
    let mut checked = 0;
    for i in 0..3 {
        let unit = if i == 0 {
            U256::from(10u128.pow(18))
        } else {
            U256::from(1_000_000)
        };
        for j in 0..3 {
            if i == j {
                continue;
            }
            for scale in [1000, 10000, 100000] {
                let amount = unit * U256::from(scale);
                let start = Instant::now();
                let result = snapshot
                    .split(
                        &optimizer,
                        &request(pool.coins[i], pool.coins[j], amount, 1),
                        tokio::time::Instant::now() + Duration::from_secs(8),
                    )
                    .await
                    .unwrap();
                times.push(start.elapsed().as_micros());
                assert_eq!(
                    result.amount_out,
                    oracle.amount_out(&pool, i, j, amount).await.unwrap()
                );
                assert_eq!(result.allocations.len(), 1);
                assert_eq!(result.allocations[0].amount_in, amount);
                assert_eq!(result.stats.eligible_pools, 0);
                if scale == 100000 {
                    eprintln!(
                        "3pool i={i} j={j} input={amount} output={}",
                        result.amount_out
                    );
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 18);
    assert_eq!(rpc.request_count(), before);
    assert_eq!(
        oracle_rpc
            .block_hash(snapshot.block().number)
            .await
            .unwrap(),
        snapshot.block().hash.to_string()
    );
    eprintln!(
        "3pool block={} hash={} checked={checked} init_ms={} sync_rpc={before} local_us={times:?} state={}",
        snapshot.block().number,
        snapshot.block().hash,
        initialization.as_millis(),
        serde_json::to_string(&store.status().curve_state).unwrap()
    );
}

async fn independent_hop(rpc: &RpcClient, hop: &Hop) -> U256 {
    match hop.pool.kind {
        PoolKind::Curve => Curve::new(rpc.clone())
            .amount_out(&hop.pool, hop.i, hop.j, hop.amount_in)
            .await
            .unwrap(),
        PoolKind::Conversion { .. } => ProtocolConversions::new(rpc.clone())
            .amount_out(&hop.pool, hop.i, hop.j, hop.amount_in)
            .await
            .unwrap(),
        PoolKind::UniswapV3 { fee } => {
            let data = format!(
                "c6a5026a{:064x}{:064x}{:064x}{fee:064x}{:064x}",
                U256::from_be_slice(hop.token_in.as_slice()),
                U256::from_be_slice(hop.token_out.as_slice()),
                hop.amount_in,
                0
            );
            U256::from_be_slice(
                &rpc.eth_call(
                    "0x61ffe014ba17989e743c5f6cb21bf9697530b21e"
                        .parse()
                        .unwrap(),
                    &data,
                )
                .await
                .unwrap()[..32],
            )
        }
        PoolKind::UniswapV2 => {
            let data = format!(
                "d06ca61f{:064x}{:064x}{:064x}{:064x}{:064x}",
                hop.amount_in,
                64,
                2,
                U256::from_be_slice(hop.token_in.as_slice()),
                U256::from_be_slice(hop.token_out.as_slice())
            );
            let out = rpc
                .eth_call(
                    "0x7a250d5630b4cf539739df2c5dacb4c659f2488d"
                        .parse()
                        .unwrap(),
                    &data,
                )
                .await
                .unwrap();
            U256::from_be_slice(&out[out.len() - 32..])
        }
        _ => panic!("unsupported test venue"),
    }
}

#[tokio::test]
#[ignore]
async fn curve_v2_v3_and_conversion_paths_preserve_independent_integer_hop_outputs() {
    let weth: Address = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        .parse()
        .unwrap();
    let steth: Address = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84"
        .parse()
        .unwrap();
    let wsteth: Address = "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"
        .parse()
        .unwrap();
    let usdc: Address = COINS[1].parse().unwrap();
    let usdt: Address = COINS[2].parse().unwrap();
    let mut graph = Graph::new();
    graph.add_pool(curve_pool());
    for (address, kind, coins) in [
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
        (
            "0x109830a1aaad605bbf02a9dfa7b0b92ec2fb7daa",
            PoolKind::UniswapV3 { fee: 100 },
            vec![wsteth, weth],
        ),
    ] {
        graph.add_pool(Pool {
            kind,
            address: address.parse().unwrap(),
            coins,
        });
    }
    graph.add_pool(Pool {
        kind: PoolKind::Conversion {
            kind: ConversionKind::WstEth,
            wrap: true,
            unwrap: true,
        },
        address: wsteth,
        coins: vec![steth, wsteth],
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
        (usdt, weth, U256::from(100_000_000_000u128)),
        (weth, usdt, U256::from(33_333_333_333_333_333_333u128)),
        (usdt, steth, U256::from(100_000_000_000u128)),
        (steth, usdt, U256::from(33_333_333_333_333_333_333u128)),
    ] {
        let start = Instant::now();
        let result = snapshot
            .split(
                &optimizer,
                &request(input, output, amount, 4),
                tokio::time::Instant::now() + Duration::from_secs(8),
            )
            .await
            .unwrap();
        times.push(start.elapsed().as_micros());
        assert_eq!(result.allocations.len(), 1);
        let hops = &result.allocations[0].hops;
        assert!(hops.iter().any(|h| h.pool.kind == PoolKind::Curve));
        let mut previous = amount;
        for hop in hops {
            assert_eq!(hop.amount_in, previous);
            previous = independent_hop(&oracle, hop).await;
            assert_eq!(previous, hop.amount_out);
        }
        assert_eq!(previous, result.amount_out);
        assert_eq!(result.stats.baseline_rpc_requests, Some(0));
        eprintln!(
            "curve mixed hops={} input={amount} output={} venues={:?}",
            hops.len(),
            result.amount_out,
            hops.iter().map(|h| h.pool.kind.name()).collect::<Vec<_>>()
        );
    }
    assert_eq!(rpc.request_count(), before);
    assert_eq!(
        oracle.block_hash(snapshot.block().number).await.unwrap(),
        snapshot.block().hash.to_string()
    );
    eprintln!(
        "curve mixed block={} hash={} sync_rpc={before} local_us={times:?}",
        snapshot.block().number,
        snapshot.block().hash
    );
}

#[tokio::test]
#[ignore]
async fn curve_plus_v2_only_paths_match_deployed_router_and_get_dy() {
    let usdc: Address = COINS[1].parse().unwrap();
    let usdt: Address = COINS[2].parse().unwrap();
    let weth: Address = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        .parse()
        .unwrap();
    let mut graph = Graph::new();
    graph.add_pool(curve_pool());
    graph.add_pool(Pool {
        kind: PoolKind::UniswapV2,
        address: "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc"
            .parse()
            .unwrap(),
        coins: vec![usdc, weth],
    });
    let rpc = RpcClient::new("https://eth.drpc.org".into());
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
    StateSynchronizer::new(rpc.clone(), store.clone())
        .sync_once()
        .await
        .unwrap();
    let snapshot = store.acquire().unwrap();
    let oracle = RpcClient::new("https://eth.drpc.org".into()).at_block_hash(snapshot.block().hash);
    let before = rpc.request_count();
    let mut timings = Vec::new();
    for (input, output, amount) in [
        (usdt, weth, U256::from(100_000_000_000u128)),
        (weth, usdt, U256::from(33_333_333_333_333_333_333u128)),
    ] {
        let start = Instant::now();
        let result = snapshot
            .route(&request(input, output, amount, 2))
            .await
            .unwrap();
        timings.push(start.elapsed().as_micros());
        assert_eq!(result.hops.len(), 2);
        assert!(
            result
                .hops
                .iter()
                .any(|h| h.pool.kind == PoolKind::UniswapV2)
        );
        for hop in &result.hops {
            assert_eq!(independent_hop(&oracle, hop).await, hop.amount_out);
        }
        eprintln!("curve+V2 input={amount} output={}", result.amount_out);
    }
    assert_eq!(rpc.request_count(), before);
    assert_eq!(
        oracle.block_hash(snapshot.block().number).await.unwrap(),
        snapshot.block().hash.to_string()
    );
    eprintln!(
        "curve+V2 block={} hash={} local_us={timings:?}",
        snapshot.block().number,
        snapshot.block().hash
    );
}
