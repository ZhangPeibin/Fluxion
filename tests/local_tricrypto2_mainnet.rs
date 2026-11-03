use alloy_primitives::{Address, U256};
use fluxion::{
    adapters::Curve,
    conversion::ProtocolConversions,
    curve_local::CurveProfile,
    graph::Graph,
    local_state::{LocalStateConfig, SnapshotStore, StateSynchronizer},
    model::{ConversionKind, Hop, Pool, PoolKind, QuoteRequest},
    router::RouterConfig,
    rpc::RpcClient,
    source::QuoteSource,
    split::{SplitConfig, SplitOptimizer},
    tricrypto2_local,
};
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};

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
async fn all_six_tricrypto2_directions_match_live_view_integers_and_reverts() {
    let pool = tricrypto2_local::pool();
    let mut graph = Graph::new();
    graph.add_pool(pool.clone());
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
    let oracle_rpc =
        RpcClient::new("https://eth.drpc.org".into()).at_block_hash(snapshot.block().hash);
    let oracle = Curve::new(oracle_rpc.clone());
    let before = rpc.request_count();
    let optimizer = SplitOptimizer::new(SplitConfig::multi_pool()).unwrap();
    let small = [
        U256::from(1_000_000_000u64),
        U256::from(1_670_000),
        U256::from(333_333_333_333_333_333u128),
    ];
    let units = tricrypto2_local::units();
    let mut times = Vec::new();
    let mut success = 0;
    let mut reverted = 0;
    for i in 0..3 {
        let inputs = BTreeSet::from([
            U256::from(1),
            U256::from(1_000_000),
            units[i],
            units[i] + U256::from(1),
            small[i],
            small[i] + U256::from(1),
            small[i] * U256::from(10),
            small[i] * U256::from(100),
            small[i] * U256::from(1_000_000),
        ]);
        for j in 0..3 {
            if i != j {
                for amount in &inputs {
                    let start = Instant::now();
                    let actual = snapshot
                        .split(
                            &optimizer,
                            &request(pool.coins[i], pool.coins[j], *amount, 1),
                            tokio::time::Instant::now() + Duration::from_secs(8),
                        )
                        .await;
                    times.push(start.elapsed().as_micros());
                    match oracle.amount_out(&pool, i, j, *amount).await {
                        Ok(expected) => {
                            let actual = actual.unwrap();
                            assert_eq!(actual.amount_out, expected, "i={i} j={j} input={amount}");
                            assert_eq!(actual.allocations.len(), 1);
                            assert_eq!(actual.stats.eligible_pools, 0);
                            success += 1;
                            if *amount == small[i] * U256::from(100) {
                                eprintln!(
                                    "tricrypto2 i={i} j={j} input={amount} output={expected}"
                                );
                            }
                        }
                        Err(_) => {
                            assert!(
                                actual.is_err(),
                                "must not fabricate a quote at i={i} j={j} input={amount}"
                            );
                            reverted += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(success + reverted, 52);
    assert_eq!(rpc.request_count(), before);
    assert_eq!(
        oracle_rpc
            .block_hash(snapshot.block().number)
            .await
            .unwrap(),
        snapshot.block().hash.to_string()
    );
    eprintln!(
        "tricrypto2 block={} hash={} init_ms={} sync_rpc={before} success={success} reverted={reverted} compute_us={times:?} state={}",
        snapshot.block().number,
        snapshot.block().hash,
        init.as_millis(),
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
        _ => panic!("unexpected test venue"),
    }
}

#[tokio::test]
#[ignore]
async fn tricrypto2_classic_stable_v3_and_wsteth_share_one_snapshot() {
    let crypto = tricrypto2_local::pool();
    let mut graph = Graph::new();
    graph.add_pool(CurveProfile::Classic3.pool());
    graph.add_pool(crypto.clone());
    let usdc: Address = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        .parse()
        .unwrap();
    let weth = crypto.coins[2];
    let btc = crypto.coins[1];
    let wsteth: Address = "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"
        .parse()
        .unwrap();
    let steth: Address = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84"
        .parse()
        .unwrap();
    for (address, fee, coins) in [
        (
            "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640",
            500,
            vec![usdc, weth],
        ),
        (
            "0x109830a1aaad605bbf02a9dfa7b0b92ec2fb7daa",
            100,
            vec![wsteth, weth],
        ),
    ] {
        graph.add_pool(Pool {
            kind: PoolKind::UniswapV3 { fee },
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
    for (input, output, amount, hops) in [
        (usdc, btc, U256::from(100_000_000_000u64), 2),
        (btc, usdc, U256::from(167_000_000), 2),
        (btc, steth, U256::from(16_700_000), 3),
    ] {
        let start = Instant::now();
        let result = snapshot
            .split(
                &optimizer,
                &request(input, output, amount, hops),
                tokio::time::Instant::now() + Duration::from_secs(8),
            )
            .await
            .unwrap();
        times.push(start.elapsed().as_micros());
        assert_eq!(result.allocations.len(), 1);
        let path = &result.allocations[0].hops;
        assert!(path.iter().any(|h| h.pool.address == crypto.address));
        let mut previous = amount;
        for hop in path {
            assert_eq!(hop.amount_in, previous);
            previous = independent_hop(&oracle, hop).await;
            assert_eq!(previous, hop.amount_out);
        }
        assert_eq!(previous, result.amount_out);
        eprintln!(
            "tricrypto2 mixed hops={} input={amount} output={} venues={:?}",
            path.len(),
            result.amount_out,
            path.iter().map(|h| h.pool.kind.name()).collect::<Vec<_>>()
        );
    }
    assert_eq!(store.status().curve_state.len(), 2);
    assert_eq!(rpc.request_count(), before);
    assert_eq!(
        oracle.block_hash(snapshot.block().number).await.unwrap(),
        snapshot.block().hash.to_string()
    );
    eprintln!(
        "tricrypto2 mixed block={} hash={} sync_rpc={before} compute_us={times:?}",
        snapshot.block().number,
        snapshot.block().hash
    );
}
