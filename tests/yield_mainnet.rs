//! Live read-only yield provider check. Run with:
//! `cargo test --test yield_mainnet -- --ignored`

use eollo_protocol::position::PositionAnalyzeRequest;
use eollo_protocol::yield_capability::YieldHistoryRequest;
use eollo_remote::rpc::RpcClient;
use eollo_remote::yield_intel::Registry;
use eollo_remote::yield_intel::history;
use eollo_remote::yield_intel::provider::{YieldProvider, now};
use eollo_remote::yield_intel::score::build_snapshot;

#[tokio::test]
#[ignore]
async fn live_defillama_and_rpc_produce_yield_opportunities() {
    let registry = Registry::load_default().expect("registry");
    let provider = YieldProvider::new(RpcClient::new(
        "https://ethereum-rpc.publicnode.com".to_string(),
    ));
    let data = provider.fetch().await.expect("DefiLlama data");
    let mut checks = std::collections::HashMap::new();
    for entry in registry.entries() {
        checks.insert(entry.id.clone(), provider.verify(entry).await);
    }
    let snapshot = build_snapshot(
        registry.entries(),
        &data.pools,
        &data.protocol_meta,
        &checks,
        now(),
        data.sources,
        data.warnings,
    );
    assert!(
        snapshot.opportunities.len() >= 2,
        "expected at least two live opportunities, got {}",
        snapshot.opportunities.len()
    );
    assert!(
        snapshot
            .opportunities
            .iter()
            .any(|item| item.ranking_apy_base > 0.0),
        "expected a positive live APY"
    );
    assert!(
        snapshot.opportunities.iter().any(|item| item
            .evidence
            .iter()
            .any(|fact| fact.fact == "contract_code")),
        "expected on-chain evidence"
    );
}

#[tokio::test]
#[ignore]
async fn live_history_returns_a_series() {
    let registry = Registry::load_default().expect("registry");
    let entry = registry
        .entries()
        .iter()
        .find(|entry| entry.id == "lido-steth")
        .expect("lido-steth");
    let provider = YieldProvider::new(RpcClient::new(
        "https://ethereum-rpc.publicnode.com".to_string(),
    ));
    let points = provider
        .fetch_history(entry.defillama_pool.as_deref().expect("pool id"))
        .await
        .expect("history");
    let response = history::build_response(
        entry,
        points,
        &YieldHistoryRequest {
            opportunity_id: "lido-steth".to_string(),
            days: 30,
            max_points: 100,
        },
        now(),
        now(),
    )
    .expect("history response");
    assert!(!response.series.is_empty());
    assert!(response.summary.points > 0);
}

#[tokio::test]
#[ignore]
async fn live_position_analyze_reads_a_pinned_block() {
    let registry = Registry::load_default().expect("registry");
    let provider = YieldProvider::new(RpcClient::new(
        "https://ethereum-rpc.publicnode.com".to_string(),
    ));
    let data = provider.fetch().await.expect("DefiLlama data");
    let mut checks = std::collections::HashMap::new();
    for entry in registry.entries() {
        checks.insert(entry.id.clone(), provider.verify(entry).await);
    }
    let snapshot = build_snapshot(
        registry.entries(),
        &data.pools,
        &data.protocol_meta,
        &checks,
        now(),
        data.sources,
        data.warnings,
    );
    let rpc = RpcClient::new("https://ethereum-rpc.publicnode.com".to_string());
    let response = eollo_remote::position_intel::analyze(
        &registry,
        &snapshot,
        &rpc,
        &PositionAnalyzeRequest {
            owner: "0xDC24316b9AE028F1497c275EB9192a3Ea0f67022"
                .parse()
                .unwrap(),
            chain: "ethereum".to_string(),
            opportunity_ids: vec!["lido-steth".to_string()],
            include_zero: false,
            cost_basis: vec![eollo_protocol::position::PositionCostBasis {
                opportunity_id: "lido-steth".to_string(),
                amount: "10000".to_string(),
            }],
        },
    )
    .await
    .expect("position analysis");
    assert!(response.block_number > 0);
    assert_eq!(response.positions.len(), 1);
    let position = &response.positions[0];
    assert!(position.value_eth.as_ref().unwrap().decimal != "0");
    assert!(position.pnl_eth.is_some());
}
