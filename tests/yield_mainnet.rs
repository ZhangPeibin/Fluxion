//! Live read-only yield provider check. Run with:
//! `cargo test --test yield_mainnet -- --ignored`

use fluxion_remote::rpc::RpcClient;
use fluxion_remote::yield_intel::Registry;
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
