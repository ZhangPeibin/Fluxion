use alloy_primitives::{Address, U256, keccak256};
use fluxion::{
    conversion::ProtocolConversions,
    graph::Graph,
    local_state::{LocalStateConfig, SnapshotStore, StateSynchronizer},
    model::{ConversionKind, Pool, PoolKind, QuoteRequest},
    router::RouterConfig,
    rpc::RpcClient,
    source::QuoteSource,
    split::{SplitConfig, SplitOptimizer},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn tokens() -> (Address, Address, Address, Address) {
    (
        "0xae7ab96520de3a18e5e111b5eaab095312d7fe84"
            .parse()
            .unwrap(),
        "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"
            .parse()
            .unwrap(),
        "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
            .parse()
            .unwrap(),
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
            .parse()
            .unwrap(),
    )
}

fn wrapper() -> Pool {
    let (steth, wsteth, _, _) = tokens();
    Pool {
        kind: PoolKind::Conversion {
            kind: ConversionKind::WstEth,
            wrap: true,
            unwrap: true,
        },
        address: wsteth,
        coins: vec![steth, wsteth],
    }
}

async fn read_word(rpc: &RpcClient, contract: Address, signature: &str) -> U256 {
    let digest = keccak256(signature.as_bytes());
    let selector = digest[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let bytes = rpc.eth_call(contract, &selector).await.unwrap();
    assert_eq!(bytes.len(), 32);
    U256::from_be_slice(&bytes)
}

fn request(input: Address, output: Address, amount: U256) -> QuoteRequest {
    QuoteRequest {
        token_in: input,
        token_out: output,
        amount_in: amount,
        max_hops: None,
        amount_out_min: None,
    }
}

#[tokio::test]
#[ignore]
async fn lido_v4_local_conversions_match_deployed_views_including_internal_ratio_boundaries() {
    let mut graph = Graph::new();
    let pool = wrapper();
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
    let oracle = ProtocolConversions::new(oracle_rpc.clone());
    let metadata = &store.status().conversion_state[0];
    assert_eq!(metadata.protocol_version, 4);
    let numerator = U256::from_str_radix(&metadata.internal_ether, 10).unwrap();
    let denominator = U256::from_str_radix(&metadata.internal_shares, 10).unwrap();
    let total_ether = read_word(&oracle_rpc, pool.coins[0], "getTotalPooledEther()").await;
    let total_shares = read_word(&oracle_rpc, pool.coins[0], "getTotalShares()").await;
    let naive = denominator * total_ether / total_shares;
    assert!(naive <= numerator);
    let before = rpc.request_count();
    let mut timings = Vec::new();
    let mut checked = 0;
    let unit = U256::from(10u64).pow(U256::from(18));
    for (i, center) in [(0, numerator), (1, denominator)] {
        for amount in [
            U256::from(1),
            U256::from(2),
            unit,
            unit + U256::from(1),
            unit * U256::from(100_000),
            center - U256::from(1),
            center,
            center + U256::from(1),
            U256::from(u128::MAX - 1),
        ] {
            let start = Instant::now();
            let local = snapshot
                .route(&request(pool.coins[i], pool.coins[1 - i], amount))
                .await;
            timings.push(start.elapsed().as_micros());
            let expected = oracle.amount_out(&pool, i, 1 - i, amount).await;
            match expected {
                Ok(expected) => assert_eq!(
                    local.unwrap().amount_out,
                    expected,
                    "direction={i} amount={amount}"
                ),
                Err(error) => {
                    assert_eq!(error, fluxion::error::QuoteError::InsufficientLiquidity);
                    assert!(
                        local.is_err(),
                        "zero-output integer rounding cannot invent a positive route"
                    );
                }
            }
            checked += 1;
        }
        let limit = U256::from(u128::MAX);
        assert!(
            snapshot
                .route(&request(pool.coins[i], pool.coins[1 - i], limit))
                .await
                .is_err()
        );
        assert!(oracle.amount_out(&pool, i, 1 - i, limit).await.is_err());
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
        "wstETH block={} hash={} protocol=4 init_ms={} sync_rpc={} internal_ether={numerator} internal_shares={denominator} naive_total_ratio_at_internal_shares={naive} checked={checked} local_us={timings:?}",
        snapshot.block().number,
        snapshot.block().hash,
        init.as_millis(),
        store.status().sync_rpc_requests
    );
}

#[tokio::test]
#[ignore]
async fn steth_v3_multi_hop_and_split_baselines_match_independent_mainnet_hops() {
    let (steth, wsteth, weth, usdc) = tokens();
    let rpc = RpcClient::new("https://eth.drpc.org".into());
    let mut graph = Graph::new();
    graph.add_pool(wrapper());
    for (address, coins) in [
        (
            "0x109830a1aaad605bbf02a9dfa7b0b92ec2fb7daa",
            vec![wsteth, weth],
        ),
        (
            "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640",
            vec![usdc, weth],
        ),
    ] {
        let address = address.parse().unwrap();
        let fee = read_word(&rpc, address, "fee()").await.to::<u32>();
        graph.add_pool(Pool {
            kind: PoolKind::UniswapV3 { fee },
            address,
            coins,
        });
    }
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
    let conversion = ProtocolConversions::new(oracle_rpc.clone());
    let quoter: Address = "0x61ffe014ba17989e743c5f6cb21bf9697530b21e"
        .parse()
        .unwrap();
    let optimizer = SplitOptimizer::new(SplitConfig::multi_pool()).unwrap();
    let before = rpc.request_count();
    let mut timings = Vec::new();
    let amount = U256::from(33_333_333_333_333_333_333u128);
    for (input, output, amount) in [
        (steth, weth, amount),
        (weth, steth, amount),
        (steth, usdc, amount),
        (usdc, steth, U256::from(100_000_000_000u128)),
    ] {
        let start = Instant::now();
        let result = snapshot
            .split(
                &optimizer,
                &request(input, output, amount),
                tokio::time::Instant::now() + Duration::from_secs(8),
            )
            .await
            .unwrap();
        timings.push(start.elapsed().as_micros());
        assert_eq!(result.allocations.len(), 1);
        let hops = &result.allocations[0].hops;
        assert!(hops.len() >= 2);
        assert_eq!(result.stats.eligible_pools, 0);
        assert_eq!(result.stats.baseline_rpc_requests, Some(0));
        let mut previous = amount;
        let mut operations = Vec::new();
        for hop in hops {
            assert_eq!(hop.amount_in, previous);
            operations.push(hop.pool.kind.operation(hop.i));
            previous = if hop.pool.kind.is_conversion() {
                conversion
                    .amount_out(&hop.pool, hop.i, hop.j, previous)
                    .await
                    .unwrap()
            } else {
                let PoolKind::UniswapV3 { fee } = hop.pool.kind else {
                    panic!("unexpected venue");
                };
                let data = format!(
                    "c6a5026a{:064x}{:064x}{previous:064x}{fee:064x}{:064x}",
                    U256::from_be_slice(hop.token_in.as_slice()),
                    U256::from_be_slice(hop.token_out.as_slice()),
                    0
                );
                U256::from_be_slice(&oracle_rpc.eth_call(quoter, &data).await.unwrap()[..32])
            };
            assert_eq!(previous, hop.amount_out);
        }
        assert_eq!(previous, result.amount_out);
        assert!(operations.contains(&"wrap") || operations.contains(&"unwrap"));
        eprintln!(
            "conversion path={operations:?} input={amount} output={} block={}",
            result.amount_out,
            snapshot.block().number
        );
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
        "conversion+V3 block={} hash={} init_ms={} sync_rpc={} local_us={timings:?}",
        snapshot.block().number,
        snapshot.block().hash,
        init.as_millis(),
        store.status().sync_rpc_requests
    );
}
