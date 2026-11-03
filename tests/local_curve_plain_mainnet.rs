use alloy_primitives::{Address, U256, keccak256};
use fluxion::{
    adapters::Curve,
    curve_plain::PlainProfile,
    graph::Graph,
    local_state::{LocalStateConfig, SnapshotStore, StateSynchronizer},
    model::{Pool, PoolKind, QuoteRequest},
    router::RouterConfig,
    rpc::RpcClient,
    source::QuoteSource,
    split::{SplitConfig, SplitOptimizer},
};
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};

fn data(signature: &str, args: &[U256]) -> String {
    let hash = keccak256(signature.as_bytes());
    let mut result = hash[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    for arg in args {
        result += &format!("{arg:064x}");
    }
    result
}

#[tokio::test]
#[ignore]
async fn both_two_coin_deployments_match_mainnet_including_tiny_and_large_relative_inputs() {
    let mut total = 0;
    for profile in [PlainProfile::TwoCrv, PlainProfile::FraxUsdc] {
        let pool = profile.pool();
        let mut graph = Graph::new();
        graph.add_pool(pool.clone());
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
        let mut times = Vec::new();
        if profile == PlainProfile::TwoCrv {
            for (i, token) in pool.coins.iter().enumerate() {
                let balance = U256::from_be_slice(
                    &oracle_rpc
                        .eth_call(
                            *token,
                            &data(
                                "balanceOf(address)",
                                &[U256::from_be_slice(pool.address.as_slice())],
                            ),
                        )
                        .await
                        .unwrap(),
                );
                let admin = U256::from_be_slice(
                    &oracle_rpc
                        .eth_call(
                            pool.address,
                            &data("admin_balances(uint256)", &[U256::from(i)]),
                        )
                        .await
                        .unwrap(),
                );
                assert_eq!(
                    balance - admin,
                    U256::from_str_radix(&store.status().curve_state[0].balances[i], 10).unwrap()
                );
            }
        }
        let optimizer = SplitOptimizer::new(SplitConfig::multi_pool()).unwrap();
        let mut successes = 0;
        let mut failures = 0;
        for (i, unit) in profile.units().into_iter().enumerate() {
            let inputs = BTreeSet::from([
                U256::from(1),
                U256::from(2),
                U256::from(1_000_000),
                unit,
                unit + U256::from(1),
                unit * U256::from(1000),
                unit * U256::from(10000),
                unit * U256::from(100000),
            ]);
            for amount in inputs {
                let request = QuoteRequest {
                    token_in: pool.coins[i],
                    token_out: pool.coins[1 - i],
                    amount_in: amount,
                    max_hops: Some(1),
                    amount_out_min: None,
                };
                let start = Instant::now();
                let actual = snapshot
                    .split(
                        &optimizer,
                        &request,
                        tokio::time::Instant::now() + Duration::from_secs(8),
                    )
                    .await;
                times.push(start.elapsed().as_micros());
                let expected = oracle.amount_out(&pool, i, 1 - i, amount).await;
                match expected {
                    Ok(out) => {
                        let actual = actual.unwrap();
                        assert_eq!(
                            actual.amount_out,
                            out,
                            "{} i={i} input={amount}",
                            profile.name()
                        );
                        assert_eq!(actual.stats.eligible_pools, 0);
                        successes += 1;
                        if amount == unit * U256::from(100000) {
                            eprintln!("{} i={i} input={amount} output={out}", profile.name());
                        }
                    }
                    Err(_) => {
                        assert!(actual.is_err());
                        failures += 1;
                    }
                }
                total += 1;
            }
        }
        assert_eq!(rpc.request_count(), before);
        assert_eq!(
            oracle_rpc
                .block_hash(snapshot.block().number)
                .await
                .unwrap(),
            snapshot.block().hash.to_string()
        );
        eprintln!(
            "plain profile={} block={} hash={} init_ms={} sync_rpc={before} success={successes} reverted={failures} compute_us={times:?} state={}",
            profile.name(),
            snapshot.block().number,
            snapshot.block().hash,
            init.as_millis(),
            serde_json::to_string(&store.status().curve_state).unwrap()
        );
    }
    assert_eq!(total, 29);
}

#[tokio::test]
#[ignore]
async fn each_plain_profile_composes_with_v3_in_one_pinned_snapshot() {
    let weth: Address = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        .parse()
        .unwrap();
    let usdc: Address = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        .parse()
        .unwrap();
    let quoter: Address = "0x61ffe014ba17989e743c5f6cb21bf9697530b21e"
        .parse()
        .unwrap();
    for profile in [PlainProfile::TwoCrv, PlainProfile::FraxUsdc] {
        let pool = profile.pool();
        let index = pool.coins.iter().position(|c| *c == usdc).unwrap();
        let mut graph = Graph::new();
        graph.add_pool(pool.clone());
        graph.add_pool(Pool {
            kind: PoolKind::UniswapV3 { fee: 500 },
            address: "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"
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
        let oracle =
            RpcClient::new("https://eth.drpc.org".into()).at_block_hash(snapshot.block().hash);
        let before = rpc.request_count();
        let amount = U256::from(1_000_000_000_000_000_000u128);
        let request = QuoteRequest {
            token_in: weth,
            token_out: pool.coins[1 - index],
            amount_in: amount,
            max_hops: Some(2),
            amount_out_min: None,
        };
        let start = Instant::now();
        let actual = snapshot.route(&request).await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(actual.hops.len(), 2);
        let expected_first = U256::from_be_slice(
            &oracle
                .eth_call(
                    quoter,
                    &data(
                        "quoteExactInputSingle((address,address,uint256,uint24,uint160))",
                        &[
                            U256::from_be_slice(weth.as_slice()),
                            U256::from_be_slice(usdc.as_slice()),
                            amount,
                            U256::from(500),
                            U256::ZERO,
                        ],
                    ),
                )
                .await
                .unwrap()[..32],
        );
        assert_eq!(expected_first, actual.hops[0].amount_out);
        assert_eq!(expected_first, actual.hops[1].amount_in);
        let expected = Curve::new(oracle.clone())
            .amount_out(&pool, index, 1 - index, expected_first)
            .await
            .unwrap();
        assert_eq!(actual.amount_out, expected);
        assert_eq!(rpc.request_count(), before);
        assert_eq!(
            oracle.block_hash(snapshot.block().number).await.unwrap(),
            snapshot.block().hash.to_string()
        );
        eprintln!(
            "plain+V3 profile={} block={} hash={} input={amount} output={expected} compute_us={} sync_rpc={before}",
            profile.name(),
            snapshot.block().number,
            snapshot.block().hash,
            elapsed.as_micros()
        );
    }
}
