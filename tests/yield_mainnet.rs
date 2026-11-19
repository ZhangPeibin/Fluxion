//! Live read-only yield provider check. Run with:
//! `cargo test --test yield_mainnet -- --ignored`

use fluxion_protocol::yield_capability::YieldHistoryRequest;
use fluxion_remote::rpc::RpcClient;
use fluxion_remote::yield_intel::Registry;
use fluxion_remote::yield_intel::history;
use fluxion_remote::yield_intel::provider::{YieldProvider, now};
use fluxion_remote::yield_intel::score::build_snapshot;

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
