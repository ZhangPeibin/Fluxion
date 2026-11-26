//! Live read-only BSC yield checks. Run with:
//! `cargo test --test bsc_yield_mainnet -- --ignored`

use alloy_primitives::{Address, U256};
use eollo_client::chain::Chain;
use eollo_client::transaction::{self, SimulateRequest};
use eollo_client::wallet::WalletConfig;
use eollo_client::yield_build;
use eollo_protocol::yield_capability::YieldBuildRequest;

const BSC_RPC: &str = "https://bsc-rpc.publicnode.com";

fn bsc_config() -> WalletConfig {
    let mut config = WalletConfig::new(BSC_RPC);
    config.bsc_rpc_url = BSC_RPC.to_string();
    config
}

fn vbnb_deposit() -> YieldBuildRequest {
    YieldBuildRequest {
        opportunity_id: "venus-vbnb".to_string(),
        amount: "1".to_string(),
        recipient: Address::with_last_byte(0x42),
        funding_asset: None,
    }
}

#[test]
fn builds_venus_vbnb_deposit_calldata() {
    let response = yield_build::build(&vbnb_deposit()).expect("build");
    assert_eq!(response.chain, "bsc");
    assert_eq!(response.steps.len(), 1);
    assert_eq!(response.steps[0].kind, "supply");
    assert_eq!(response.steps[0].data, "0x1249c58b");
    assert_eq!(response.steps[0].value, "1000000000000000000");
    assert!(response.approvals.is_empty());
}

#[test]
fn builds_venus_vusdt_deposit_calldata() {
    let response = yield_build::build(&YieldBuildRequest {
        opportunity_id: "venus-vusdt".to_string(),
        amount: "100".to_string(),
        recipient: Address::with_last_byte(0x42),
        funding_asset: None,
    })
    .expect("build");
    assert_eq!(response.chain, "bsc");
    assert_eq!(response.steps.len(), 2);
    assert_eq!(response.steps[0].kind, "approve");
    assert_eq!(response.steps[1].kind, "supply");
    assert!(response.steps[1].data.starts_with("0xa0712d68"));
    assert_eq!(response.approvals.len(), 1);
}

#[tokio::test]
#[ignore]
async fn venus_vbnb_deposit_reaches_the_contract() {
    let response = yield_build::build(&vbnb_deposit()).expect("build");
    let step = &response.steps[0];
    let result = transaction::simulate(
        &bsc_config(),
        SimulateRequest {
            chain: Chain::Bsc,
            from: Some(Address::with_last_byte(0x42)),
            to: step.to,
            data: step.data.clone(),
            value: U256::from_str_radix(&step.value, 10).expect("value"),
            block: None,
        },
    )
    .await
    .expect("simulate");

    assert!(!result.success, "expected a revert without BNB balance");
    assert!(
        result.error.is_some() || result.revert_data.is_some(),
        "expected a revert reason or revert data"
    );
}
