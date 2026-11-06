use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use alloy_primitives::{Address, U256};
use async_trait::async_trait;
use fluxion::error::QuoteError;
use fluxion::graph::Graph;
use fluxion::model::{Pool, PoolKind, QuoteRequest};
use fluxion::router::Router;
use fluxion::source::{FixedRate, MockV2, QuoteSource};
use fluxion::split::{PlanKind, SplitConfig, SplitOptimizer, StopReason};

fn addr(n: u8) -> Address {
    Address::with_last_byte(n)
}

fn request(amount: U256) -> QuoteRequest {
    QuoteRequest {
        token_in: addr(1),
        token_out: addr(2),
        amount_in: amount,
        max_hops: None,
        amount_out_min: None,
    }
}

fn pool(id: u8, from: u8, to: u8) -> Pool {
    Pool {
        kind: PoolKind::UniswapV2,
        address: addr(id),
        coins: vec![addr(from), addr(to)],
    }
}

fn parallel() -> (Graph, MockV2) {
    let mut graph = Graph::new();
    graph.add_pool(pool(10, 1, 2));
    graph.add_pool(pool(11, 1, 2));
    let reserves = [U256::from(1000), U256::from(100_000)];
    (
        graph,
        MockV2::new(HashMap::from([(addr(10), reserves), (addr(11), reserves)])),
    )
}

#[tokio::test]
async fn split_improves_output_and_conserves_odd_integer_input() {
    let (graph, source) = parallel();
    let result = SplitOptimizer::new(SplitConfig::default())
        .unwrap()
        .quote(
            &Router::default(),
            &graph,
            &source,
            &request(U256::from(1001)),
        )
        .await
        .unwrap();
    assert_eq!(result.kind, PlanKind::Split);
    assert!(result.amount_out > result.baseline.amount_out);
    assert_eq!(result.allocations.len(), 2);
    assert_eq!(
        result.allocations.iter().map(|a| a.amount_in).sum::<U256>(),
        U256::from(1001)
    );
    assert_eq!(
        result
            .allocations
            .iter()
            .map(|a| a.amount_out)
            .sum::<U256>(),
        result.amount_out
    );
    assert_ne!(
        result.allocations[0].hops[0].pool.address,
        result.allocations[1].hops[0].pool.address
    );
}

#[tokio::test]
async fn baseline_keeps_multi_hop_route_instead_of_only_best_direct_pool() {
    let (mut graph, _) = parallel();
    graph.add_pool(pool(12, 1, 3));
    graph.add_pool(pool(13, 3, 2));
    let source = MockV2::new(HashMap::from([
        (addr(10), [U256::from(1_000_000), U256::from(1_000_000)]),
        (addr(11), [U256::from(1_000_000), U256::from(1_000_000)]),
        (addr(12), [U256::from(1_000_000), U256::from(2_000_000)]),
        (addr(13), [U256::from(1_000_000), U256::from(2_000_000)]),
    ]));
    let result = SplitOptimizer::new(SplitConfig::default())
        .unwrap()
        .quote(
            &Router::default(),
            &graph,
            &source,
            &request(U256::from(1000)),
        )
        .await
        .unwrap();
    assert_eq!(result.kind, PlanKind::Baseline);
    assert_eq!(result.allocations[0].hops.len(), 2);
    assert_eq!(result.amount_out, result.baseline.amount_out);
}

struct GuardedSource {
    inner: MockV2,
    fail_fractional: bool,
    stall: bool,
    zero_calls: AtomicUsize,
}

#[async_trait]
impl QuoteSource for GuardedSource {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount: U256,
    ) -> Result<U256, QuoteError> {
        if amount.is_zero() {
            self.zero_calls.fetch_add(1, Ordering::Relaxed);
            panic!("zero amount sent to adapter");
        }
        if self.fail_fractional && pool.address == addr(11) && amount < U256::from(1000) {
            return Err(QuoteError::RpcError("fractional quote failed".into()));
        }
        if self.stall && pool.address == addr(10) && amount == U256::from(200) {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        self.inner.amount_out(pool, i, j, amount).await
    }
    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        self.inner.mid_price(pool, i, j).await
    }
    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError> {
        self.inner.reserve_in(pool, i).await
    }
}

#[tokio::test]
async fn fractional_failure_never_turns_one_branch_into_a_complete_split() {
    let (graph, inner) = parallel();
    let source = GuardedSource {
        inner,
        fail_fractional: true,
        stall: false,
        zero_calls: AtomicUsize::new(0),
    };
    let result = SplitOptimizer::new(SplitConfig::default())
        .unwrap()
        .quote(
            &Router::default(),
            &graph,
            &source,
            &request(U256::from(1000)),
        )
        .await
        .unwrap();
    assert_eq!(result.kind, PlanKind::Baseline);
    assert!(result.stats.failed_additional_quotes > 0);
    assert_eq!(source.zero_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn exhausted_quote_budget_keeps_a_completed_improvement() {
    let (graph, source) = parallel();
    let config = SplitConfig {
        max_quote_calls: 2,
        concurrency: 1,
        ..Default::default()
    };
    let result = SplitOptimizer::new(config)
        .unwrap()
        .quote(
            &Router::default(),
            &graph,
            &source,
            &request(U256::from(1000)),
        )
        .await
        .unwrap();
    assert_eq!(result.stats.stop_reason, StopReason::QuoteBudget);
    assert!(result.stats.additional_quote_calls <= 2);
    assert!(result.amount_out > result.baseline.amount_out);
}

#[tokio::test]
async fn deadline_keeps_completed_plans_and_drops_incomplete_ones() {
    let (graph, inner) = parallel();
    let source = GuardedSource {
        inner,
        fail_fractional: false,
        stall: true,
        zero_calls: AtomicUsize::new(0),
    };
    let config = SplitConfig {
        timeout_ms: 30,
        concurrency: 1,
        ..Default::default()
    };
    let result = SplitOptimizer::new(config)
        .unwrap()
        .quote(
            &Router::default(),
            &graph,
            &source,
            &request(U256::from(1000)),
        )
        .await
        .unwrap();
    assert_eq!(result.stats.stop_reason, StopReason::TimeBudget);
    assert!(result.amount_out > result.baseline.amount_out);
    assert_eq!(
        result.allocations.iter().map(|a| a.amount_in).sum::<U256>(),
        U256::from(1000)
    );
}

#[tokio::test]
async fn ties_tiny_inputs_and_u256_max_keep_the_baseline_without_overflow() {
    let (graph, _) = parallel();
    for amount in [U256::from(1), U256::from(101), U256::MAX] {
        let result = SplitOptimizer::new(SplitConfig::default())
            .unwrap()
            .quote(
                &Router::default(),
                &graph,
                &FixedRate::new(U256::from(1)),
                &request(amount),
            )
            .await
            .unwrap();
        assert_eq!(result.kind, PlanKind::Baseline);
        assert_eq!(result.amount_out, amount);
        if amount == U256::from(1) {
            assert_eq!(result.stats.additional_quote_calls, 0);
        }
    }
}

#[tokio::test]
async fn zero_budget_and_duplicate_pool_addresses_do_not_force_a_split() {
    let (graph, source) = parallel();
    let result = SplitOptimizer::new(SplitConfig {
        max_quote_calls: 0,
        ..Default::default()
    })
    .unwrap()
    .quote(
        &Router::default(),
        &graph,
        &source,
        &request(U256::from(1000)),
    )
    .await
    .unwrap();
    assert_eq!(result.kind, PlanKind::Baseline);
    assert_eq!(result.stats.additional_quote_calls, 0);
    let mut duplicate = Graph::new();
    duplicate.add_pool(pool(10, 1, 2));
    duplicate.add_pool(pool(10, 1, 2));
    let result = SplitOptimizer::new(SplitConfig::default())
        .unwrap()
        .quote(
            &Router::default(),
            &duplicate,
            &source,
            &request(U256::from(1000)),
        )
        .await
        .unwrap();
    assert_eq!(result.kind, PlanKind::Baseline);
    assert_eq!(result.stats.stop_reason, StopReason::NoPairs);
}

#[tokio::test]
async fn baseline_errors_are_preserved() {
    let (graph, source) = parallel();
    let error = SplitOptimizer::new(SplitConfig::default())
        .unwrap()
        .quote(&Router::default(), &graph, &source, &request(U256::ZERO))
        .await
        .unwrap_err();
    assert_eq!(error, QuoteError::ZeroAmount);
}

#[tokio::test]
async fn route_split_uses_disjoint_multi_hop_paths_for_large_inputs() {
    let mut graph = Graph::new();
    graph.add_pool(pool(10, 1, 2));
    graph.add_pool(pool(11, 1, 3));
    graph.add_pool(pool(12, 3, 2));
    graph.add_pool(pool(13, 1, 4));
    graph.add_pool(pool(14, 4, 2));
    let big = [U256::from(1_000_000), U256::from(1_000_000)];
    let tiny = [U256::from(1_000), U256::from(1_000)];
    let source = MockV2::new(HashMap::from([
        (addr(10), tiny),
        (addr(11), big),
        (addr(12), big),
        (addr(13), big),
        (addr(14), big),
    ]));
    let result = SplitOptimizer::new(SplitConfig::multi_pool())
        .unwrap()
        .quote(
            &Router::default(),
            &graph,
            &source,
            &request(U256::from(2_000_000)),
        )
        .await
        .unwrap();
    assert_eq!(result.kind, PlanKind::Split);
    assert!(result.amount_out > result.baseline.amount_out);
    assert_eq!(result.allocations.len(), 2);
    assert!(
        result
            .allocations
            .iter()
            .all(|allocation| allocation.hops.len() == 2)
    );
    assert_eq!(
        result.allocations.iter().map(|a| a.amount_in).sum::<U256>(),
        U256::from(2_000_000)
    );
}

#[tokio::test]
async fn multi_hop_path_enters_split_with_disjoint_pool_sets() {
    let mut graph = Graph::new();
    graph.add_pool(pool(10, 1, 2));
    graph.add_pool(pool(11, 1, 3));
    graph.add_pool(pool(12, 3, 2));
    let source = MockV2::new(HashMap::from([
        (addr(10), [U256::from(100_000), U256::from(2_000)]),
        (addr(11), [U256::from(1_000_000), U256::from(10_000_000)]),
        (addr(12), [U256::from(10_000_000), U256::from(10_000)]),
    ]));
    let amount = U256::from(100_001);
    for config in [SplitConfig::default(), SplitConfig::multi_pool()] {
        let max_calls = config.max_quote_calls;
        let result = SplitOptimizer::new(config)
            .unwrap()
            .quote(&Router::default(), &graph, &source, &request(amount))
            .await
            .unwrap();

        assert_eq!(result.kind, PlanKind::Split);
        assert!(result.stats.additional_quote_calls <= max_calls);
        assert_eq!(result.stats.failed_additional_quotes, 0);
        assert!(result.amount_out > result.baseline.amount_out);
        assert!(result.stats.multi_route_search);
        assert!(result.stats.route_branches >= 2);
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
        assert!(
            result
                .allocations
                .iter()
                .any(|allocation| allocation.hops.len() == 2)
        );
        let mut pools = std::collections::HashSet::new();
        for allocation in &result.allocations {
            for hop in &allocation.hops {
                assert!(
                    pools.insert(hop.pool.address),
                    "allocation paths must not share a pool"
                );
            }
        }
    }
}

#[tokio::test]
async fn equal_multi_hop_split_keeps_baseline() {
    let mut graph = Graph::new();
    graph.add_pool(pool(10, 1, 2));
    graph.add_pool(pool(11, 1, 3));
    graph.add_pool(pool(12, 3, 2));
    let amount = U256::from(1000);
    let result = SplitOptimizer::new(SplitConfig::multi_pool())
        .unwrap()
        .quote(
            &Router::default(),
            &graph,
            &FixedRate::new(U256::from(1)),
            &request(amount),
        )
        .await
        .unwrap();
    assert_eq!(result.kind, PlanKind::Baseline);
    assert_eq!(result.amount_out, result.baseline.amount_out);
    assert_eq!(result.allocations.len(), 1);
    assert_eq!(result.allocations[0].amount_in, amount);
}

struct AmountLimitedSource {
    inner: MockV2,
    limited_pool: Address,
    max_amount: U256,
}

#[async_trait]
impl QuoteSource for AmountLimitedSource {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount: U256,
    ) -> Result<U256, QuoteError> {
        if pool.address == self.limited_pool && amount > self.max_amount {
            return Err(QuoteError::InsufficientLiquidity);
        }
        self.inner.amount_out(pool, i, j, amount).await
    }
    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        self.inner.mid_price(pool, i, j).await
    }
    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError> {
        self.inner.reserve_in(pool, i).await
    }
}

#[tokio::test]
async fn partial_only_multi_hop_path_still_enters_split() {
    let mut graph = Graph::new();
    graph.add_pool(pool(10, 1, 2));
    graph.add_pool(pool(11, 1, 3));
    graph.add_pool(pool(12, 3, 2));
    let source = AmountLimitedSource {
        inner: MockV2::new(HashMap::from([
            (addr(10), [U256::from(100_000), U256::from(2_000)]),
            (addr(11), [U256::from(1_000_000), U256::from(10_000_000)]),
            (addr(12), [U256::from(10_000_000), U256::from(10_000)]),
        ])),
        limited_pool: addr(11),
        max_amount: U256::from(300_000),
    };
    let amount = U256::from(1_000_000);
    let result = SplitOptimizer::new(SplitConfig::multi_pool())
        .unwrap()
        .quote(&Router::default(), &graph, &source, &request(amount))
        .await
        .unwrap();

    assert_eq!(result.kind, PlanKind::Split);
    assert!(result.amount_out > result.baseline.amount_out);
    assert!(result.stats.multi_route_search);
    assert!(
        result
            .allocations
            .iter()
            .any(|allocation| allocation.hops.len() == 2)
    );
    assert_eq!(
        result.allocations.iter().map(|a| a.amount_in).sum::<U256>(),
        amount
    );
}

#[tokio::test]
async fn shared_prefix_fans_out_to_multiple_suffix_pools() {
    let mut graph = Graph::new();
    graph.add_pool(pool(11, 1, 3));
    graph.add_pool(pool(12, 3, 2));
    graph.add_pool(pool(13, 3, 2));
    let source = MockV2::new(HashMap::from([
        (addr(11), [U256::from(1_000_000), U256::from(10_000_000)]),
        (addr(12), [U256::from(1_000_000), U256::from(100)]),
        (addr(13), [U256::from(100_000), U256::from(10)]),
    ]));
    let amount = U256::from(100_000);
    let result = SplitOptimizer::new(SplitConfig::multi_pool())
        .unwrap()
        .quote(&Router::default(), &graph, &source, &request(amount))
        .await
        .unwrap();

    assert_eq!(result.kind, PlanKind::Split);
    assert!(result.amount_out > result.baseline.amount_out);
    assert!(result.stats.route_tree);
    assert_eq!(result.allocations.len(), 1);
    let root = &result.allocations[0];
    assert_eq!(root.amount_in, amount);
    assert_eq!(root.hops.len(), 1);
    assert_eq!(root.children.len(), 2);
    assert_eq!(
        root.children.iter().map(|c| c.amount_in).sum::<U256>(),
        root.hops[0].amount_out
    );
    assert!(
        root.children
            .iter()
            .all(|child| child.hops.len() == 1 && child.hops[0].token_in == addr(3))
    );
    assert_eq!(
        root.children.iter().map(|c| c.amount_out).sum::<U256>(),
        result.amount_out
    );
}
