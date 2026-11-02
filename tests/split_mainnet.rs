use alloy_primitives::{Address, U256};
use fluxion::config::Config;
use fluxion::graph::Graph;
use fluxion::model::{Pool, PoolKind, QuoteRequest};
use fluxion::router::{PriceSnapshot, Router};
use fluxion::rpc::RpcClient;
use fluxion::source::QuoteSource;
use fluxion::split::{PlanKind, SplitConfig, SplitOptimizer};
use fluxion::universe::{pricing_source, warm_coarse_prices};
use std::sync::Arc;

#[tokio::test]
#[ignore]
async fn pinned_two_pool_allocation_matches_independent_adapter_quotes() {
    let rpc = RpcClient::new("https://eth.drpc.org".into());
    let block = rpc.block_number().await.unwrap();
    let rpc = rpc.at_block(block);
    let shib: Address = "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce"
        .parse()
        .unwrap();
    let weth: Address = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        .parse()
        .unwrap();
    let mut graph = Graph::new();
    for (address, kind) in [
        (
            "0x811beed0119b4afce20d2583eb608c6f7af1954f",
            PoolKind::UniswapV2,
        ),
        (
            "0xcf6daab95c476106eca715d48de4b13287ffdeaa",
            PoolKind::ShibaSwapV2,
        ),
    ] {
        graph.add_pool(Pool {
            kind,
            address: address.parse().unwrap(),
            coins: vec![shib, weth],
        });
    }
    let (source, _) = pricing_source(&Config::default(), rpc.clone());
    warm_coarse_prices(&graph, &source, &rpc).await;
    let router = Router::default();
    router.set_snapshot(Arc::new(
        PriceSnapshot::build(&graph, source.as_ref()).await,
    ));
    let amount = U256::from(33_333_333_333_333_333_333u128);
    let result = SplitOptimizer::new(SplitConfig::default())
        .unwrap()
        .quote_measured(
            &router,
            &graph,
            source.as_ref(),
            &QuoteRequest {
                token_in: weth,
                token_out: shib,
                amount_in: amount,
                max_hops: None,
                amount_out_min: None,
            },
            Some(&|| rpc.request_count()),
        )
        .await
        .unwrap();
    assert!(result.amount_out >= result.baseline.amount_out);
    assert_eq!(
        result.allocations.iter().map(|a| a.amount_in).sum::<U256>(),
        amount
    );
    assert!(result.stats.additional_quote_calls <= 128);
    if result.kind == PlanKind::Split {
        assert_ne!(
            result.allocations[0].hops[0].pool.address,
            result.allocations[1].hops[0].pool.address
        );
    }
    let mut total = U256::ZERO;
    for allocation in result.allocations {
        assert_eq!(allocation.hops.len(), 1);
        let hop = &allocation.hops[0];
        let actual = source
            .amount_out(&hop.pool, hop.i, hop.j, allocation.amount_in)
            .await
            .unwrap();
        assert_eq!(actual, allocation.amount_out);
        total += actual;
    }
    assert_eq!(total, result.amount_out);
    eprintln!(
        "pinned block {block}; baseline={}, selected={}",
        result.baseline.amount_out, result.amount_out
    );
}

#[tokio::test]
#[ignore]
async fn pinned_multi_pool_plan_has_independent_exact_branch_totals() {
    let rpc = RpcClient::new("https://eth.drpc.org".into());
    let block = rpc.block_number().await.unwrap().saturating_sub(2);
    let rpc = rpc.at_block(block);
    let shib: Address = "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce"
        .parse()
        .unwrap();
    let weth: Address = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        .parse()
        .unwrap();
    let mut graph = Graph::new();
    for (address, kind) in [
        (
            "0x811beed0119b4afce20d2583eb608c6f7af1954f",
            PoolKind::UniswapV2,
        ),
        (
            "0xcf6daab95c476106eca715d48de4b13287ffdeaa",
            PoolKind::ShibaSwapV2,
        ),
        (
            "0x2f62f2b4c5fcd7570a709dec05d68ea19c82a9ec",
            PoolKind::UniswapV3 { fee: 3000 },
        ),
        (
            "0x5764a6f2212d502bc5970f9f129ffcd61e5d7563",
            PoolKind::UniswapV3 { fee: 10000 },
        ),
    ] {
        graph.add_pool(Pool {
            kind,
            address: address.parse().unwrap(),
            coins: vec![shib, weth],
        });
    }
    let (source, _) = pricing_source(&Config::default(), rpc.clone());
    warm_coarse_prices(&graph, &source, &rpc).await;
    let router = Router::default();
    router.set_snapshot(Arc::new(
        PriceSnapshot::build(&graph, source.as_ref()).await,
    ));
    let amount = U256::from(33_333_333_333_333_333_333u128);
    let request = QuoteRequest {
        token_in: weth,
        token_out: shib,
        amount_in: amount,
        max_hops: None,
        amount_out_min: None,
    };
    let result = SplitOptimizer::new(SplitConfig::multi_pool())
        .unwrap()
        .quote(&router, &graph, source.as_ref(), &request)
        .await
        .unwrap();
    assert!(result.amount_out >= result.baseline.amount_out);
    assert!(result.allocations.len() <= 4);
    assert!(result.stats.additional_quote_calls <= 256);
    let mut addresses = std::collections::HashSet::new();
    let mut input = U256::ZERO;
    let mut output = U256::ZERO;
    for allocation in result.allocations {
        let hop = &allocation.hops[0];
        assert!(addresses.insert(hop.pool.address));
        input += allocation.amount_in;
        let actual = source
            .amount_out(&hop.pool, hop.i, hop.j, allocation.amount_in)
            .await
            .unwrap();
        assert_eq!(actual, allocation.amount_out);
        output += actual;
    }
    assert_eq!(input, amount);
    assert_eq!(output, result.amount_out);
}
