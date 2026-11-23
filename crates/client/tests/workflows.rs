//! End-to-end workflow recipes against mocked remote services and RPC.
//!
//! The tests exercise the capability sequence declared in `capabilities/catalog.yaml`. The wallet
//! confirmation boundary is asserted through the catalog (`transaction.send` requires a session)
//! rather than a real WalletConnect relay.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use alloy_primitives::{Address, U256};
use axum::extract::Path as AxumPath;
use axum::routing::{get, post};
use axum::{Json, Router};
use fluxion_client::agent_id::{self, AgentIdConfig, AgentIdLookup, AgentIdRequest};
use fluxion_client::capability::CapabilityRegistry;
use fluxion_client::chain::Chain;
use fluxion_client::client::RuntimeClient;
use fluxion_client::fourmeme::{self, FourMemeConfig, ListRequest};
use fluxion_client::swap::quote::QuoteRequest;
use fluxion_client::swap_build::{self, BuildRequest};
use fluxion_client::transaction::{self, SimulateRequest};
use fluxion_client::transaction_inspect::{self, InspectRequest};
use fluxion_client::transaction_verify::{self, VerifyRequest};
use fluxion_client::wallet::{
    self, AllowanceRequest, AssetsRequest, BalanceRequest, CollectiblesRequest, WalletConfig,
};
use fluxion_client::yield_build;
use fluxion_protocol::position::PositionAnalyzeRequest;
use fluxion_protocol::yield_capability::{RiskProfile, YieldBuildRequest, YieldDiscoverRequest};
use serde_json::{Value, json};

const RPC_BLOCK_HASH: &str = "0x0000000000000000000000000000000000000000000000000000000000000064";
const RECEIPT_BLOCK_HASH: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000062";
const TX_HASH: &str = "0x00000000000000000000000000000000000000000000000000000000000000aa";
const OWNER: &str = "0x0000000000000000000000000000000000000001";
const RECIPIENT: &str = "0x0000000000000000000000000000000000000002";
const TOKEN_IN: &str = "0x0000000000000000000000000000000000000011";
const TOKEN_OUT: &str = "0x0000000000000000000000000000000000000012";
const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const COLLECTION: &str = "0x0000000000000000000000000000000000000010";
const AGENT_REGISTRY: &str = "0x60B9262456dE011F7d15a54CdD8c48bAaa8B022F";
const OPENFOUR_CORE: &str = "0xebe7b6C1089D9F72aD07f34E36d898e44E5e27f3";
const VAULT: &str = "0x0000000000000000000000000000000000000077";
const FOURMEME_TOKEN: &str = "0x00000000000000000000000000000000000000a1";

fn word(value: U256) -> String {
    format!("0x{value:064x}")
}

fn abi_string(value: &str) -> String {
    let mut bytes = Vec::new();
    let mut head = [0u8; 32];
    head[31] = 32;
    bytes.extend_from_slice(&head);
    let mut length = [0u8; 32];
    length[24..].copy_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&length);
    let mut data = vec![0u8; 32];
    data[..value.len()].copy_from_slice(value.as_bytes());
    bytes.extend_from_slice(&data);
    format!(
        "0x{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn config(base: &str) -> WalletConfig {
    WalletConfig {
        rpc_url: format!("{base}/rpc"),
        bsc_rpc_url: format!("{base}/rpc"),
        oneinch_api_key: None,
        oneinch_balance_url: "http://unused".to_string(),
        alchemy_api_key: None,
        alchemy_nft_url: "http://unused".to_string(),
    }
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

fn address_word_bytes(address: &str) -> [u8; 32] {
    let address: Address = address.parse().unwrap();
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(address.as_slice());
    word
}

fn core_token_state() -> String {
    let mut words = vec![U256::ZERO; 18];
    words[1] = U256::from_be_slice(&address_word_bytes(OWNER));
    words[2] = U256::from(1_778_027_615_735u64);
    words[6] = U256::from(1_000_000_000u64) * U256::from(10).pow(U256::from(18));
    words[7] = U256::from(800_000_000u64) * U256::from(10).pow(U256::from(18));
    words[10] = U256::from_be_slice(&address_word_bytes(VAULT));
    words[17] = U256::from(1u64);
    format!(
        "0x{}",
        words
            .iter()
            .map(|word| format!("{word:064x}"))
            .collect::<String>()
    )
}

async fn rpc(Json(body): Json<Value>) -> Json<Value> {
    let is_batch = body.is_array();
    let requests = body.as_array().cloned().unwrap_or_else(|| vec![body]);
    let items = requests
        .iter()
        .map(|request| {
            let id = request.get("id").cloned().unwrap_or(Value::Null);
            let method = request["method"].as_str().unwrap_or_default();
            let result = match method {
                "eth_blockNumber" => json!("0x64"),
                "eth_getBlockByNumber" => json!({"number": "0x64", "hash": RPC_BLOCK_HASH}),
                "eth_getBalance" => json!(word(U256::from(1_000_000_000_000_000_000u128))),
                "eth_getCode" => json!("0x"),
                "eth_getStorageAt" => json!(word(U256::ZERO)),
                "eth_estimateGas" => json!("0x5208"),
                "eth_getLogs" => json!([]),
                "eth_simulateV1" => Value::Null,
                "debug_traceCall" => Value::Null,
                "eth_getTransactionByHash" => json!({
                    "hash": TX_HASH,
                    "from": OWNER,
                    "to": RECIPIENT,
                    "value": "0x16345785d8a0000",
                    "gasPrice": "0x2e90edd000"
                }),
                "eth_getTransactionReceipt" => json!({
                    "transactionHash": TX_HASH,
                    "blockNumber": "0x62",
                    "blockHash": RECEIPT_BLOCK_HASH,
                    "status": "0x1",
                    "gasUsed": "0x5208",
                    "effectiveGasPrice": "0x2e90edd000",
                    "logs": []
                }),
                "eth_call" => {
                    let call = &request["params"][0];
                    let to = call["to"].as_str().unwrap_or_default().to_ascii_lowercase();
                    let data = call["data"].as_str().unwrap_or_default();
                    if to == OPENFOUR_CORE.to_ascii_lowercase() && data.starts_with("0xe4860339") {
                        json!(core_token_state())
                    } else if to == VAULT.to_ascii_lowercase() && data.starts_with("0xb1c9fe6e") {
                        json!(word(U256::ZERO))
                    } else if to == FOURMEME_TOKEN.to_ascii_lowercase()
                        && data.starts_with("0x70a08231")
                    {
                        json!(word(
                            U256::from(5_000_000u64) * U256::from(10).pow(U256::from(18))
                        ))
                    } else if to == AGENT_REGISTRY.to_ascii_lowercase() {
                        json!(word(
                            U256::from_str_radix(OWNER.trim_start_matches("0x"), 16).unwrap()
                        ))
                    } else if data.starts_with("0xdd62ed3e") {
                        json!(word(U256::from(1_000_000_000u64)))
                    } else if data.starts_with("0x70a08231") {
                        json!(word(U256::from(1_000_000u64)))
                    } else if data.starts_with("0x313ce567") {
                        json!(word(U256::from(18u64)))
                    } else if data.starts_with("0x95d89b41") {
                        json!(abi_string("TOKEN"))
                    } else if data.starts_with("0x06fdde03") {
                        json!(abi_string("Collection"))
                    } else if ["0x01ffc9a7", "0x2f745c59", "0x6352211e", "0x1ffbb064"]
                        .iter()
                        .any(|selector| data.starts_with(selector))
                    {
                        json!(word(U256::from(1u64)))
                    } else {
                        json!(word(U256::from(42u64)))
                    }
                }
                _ => json!("0x0"),
            };
            json!({"jsonrpc": "2.0", "id": id, "result": result})
        })
        .collect::<Vec<_>>();
    if is_batch {
        Json(Value::Array(items))
    } else {
        Json(items.into_iter().next().unwrap_or(Value::Null))
    }
}

async fn quote_ok() -> Json<Value> {
    Json(json!({
        "amount_out": "1000000000000000000",
        "price_impact_bps": 1,
        "path": [{
            "operation": "swap",
            "platform": "uniswap-v2",
            "pool": "0x0000000000000000000000000000000000000009",
            "i": 0,
            "j": 1,
            "token_in": TOKEN_IN,
            "token_out": TOKEN_OUT,
            "amount_in": "1000000000000000000",
            "amount_out": "1000000000000000000"
        }],
        "plan_kind": "baseline",
        "quote_block": 123,
        "state": {
            "pricing_mode": "local_snapshot",
            "block_hash": RPC_BLOCK_HASH,
            "block_timestamp": 1,
            "state_age_secs": 2,
            "lag_blocks": 0
        }
    }))
}

async fn quote_fail() -> (axum::http::StatusCode, Json<Value>) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "quote unavailable"})),
    )
}

async fn yield_discover_ok() -> Json<Value> {
    Json(json!({
        "schema_version": 1,
        "capability": "yield.discover",
        "request_id": "test",
        "chain": "ethereum",
        "asset": {"symbol": "ETH", "address": "native", "decimals": 18},
        "amount": {"raw": "1000000000000000000", "decimal": "1"},
        "risk_profile": "conservative",
        "objective": "highest_apy_within_eligible_safety_tiers",
        "opportunities": [],
        "excluded": [],
        "as_of": 1,
        "age_secs": 0,
        "stale": false,
        "coverage": "curated-universe",
        "sources": [],
        "disclaimer": "test",
        "warnings": []
    }))
}

async fn position_ok() -> Json<Value> {
    Json(json!({
        "schema_version": 1,
        "capability": "position.analyze",
        "request_id": "test",
        "chain": "ethereum",
        "owner": OWNER,
        "block_number": 100,
        "block_hash": RPC_BLOCK_HASH,
        "as_of": 1,
        "yield_data_as_of": 1,
        "positions": [],
        "totals": {"position_count": 0, "active_count": 0},
        "coverage": "test",
        "sources": [],
        "disclaimer": "test",
        "warnings": []
    }))
}

async fn position_fail() -> (axum::http::StatusCode, Json<Value>) {
    (
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error": "position unavailable"})),
    )
}

async fn agent_id_api(AxumPath(handle): AxumPath<String>) -> Json<Value> {
    Json(json!({
        "handle": handle,
        "owner": OWNER,
        "tokenId": "1",
        "txHash": TX_HASH,
        "blockNumber": 1,
        "asOfBlock": 1
    }))
}

async fn fourmeme_detail_api() -> Json<Value> {
    Json(json!({
        "code": 0,
        "data": {
            "name": "Agent Token",
            "shortName": "AGT",
            "status": "PUBLISH",
            "tokenPrice": {
                "progress": "0.42",
                "marketCap": "100",
                "price": "1",
                "maxPrice": "2",
                "dayTrading": "1234",
                "dayIncrease": "5.2",
                "liquidity": "50",
                "raisedAmount": "10"
            },
            "tokenTemplateVo": {"id": 1778027615735i64, "name": "Binance Agent ID Exclusive"},
            "userAddress": OWNER,
            "createDate": 1,
            "launchTime": 1
        }
    }))
}

fn catalog_workflows() -> BTreeMap<String, fluxion_client::capability::WorkflowEntry> {
    CapabilityRegistry::global()
        .catalog()
        .workflows
        .iter()
        .map(|workflow| (workflow.id.clone(), workflow.clone()))
        .collect()
}

#[test]
fn catalog_workflows_are_consistent() {
    let registry = CapabilityRegistry::global();
    let catalog = registry.catalog();
    let workflows = catalog_workflows();
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_eq!(workflows.len(), 4);

    for workflow in catalog.workflows.iter() {
        let skill = workflow
            .skill
            .as_deref()
            .unwrap_or_else(|| panic!("{} has no skill", workflow.id));
        let docs = workflow
            .docs
            .as_deref()
            .unwrap_or_else(|| panic!("{} has no docs", workflow.id));
        assert!(
            repo_root.join(skill).exists(),
            "{} skill missing",
            workflow.id
        );
        assert!(
            repo_root.join(docs).exists(),
            "{} docs missing",
            workflow.id
        );
        assert!(!workflow.steps.is_empty());
        let mut chains: Option<BTreeSet<String>> = None;
        for step in &workflow.steps {
            let entry = catalog
                .capability(step)
                .unwrap_or_else(|| panic!("{} references missing step {}", workflow.id, step));
            let step_chains = entry.chains.iter().cloned().collect::<BTreeSet<_>>();
            chains = Some(match chains {
                Some(existing) => existing.intersection(&step_chains).cloned().collect(),
                None => step_chains,
            });
            assert!(
                entry.workflows.iter().any(|id| id == &workflow.id),
                "{} is a step of {} but does not list it in workflows",
                step,
                workflow.id
            );
        }
        let expected = chains.unwrap_or_default();
        let declared = workflow.chains.iter().cloned().collect::<BTreeSet<_>>();
        assert_eq!(declared, expected, "{} chain intersection", workflow.id);
        for planned in &workflow.planned_steps {
            assert!(
                catalog.capability(planned).is_none(),
                "{} planned step {} is already implemented",
                workflow.id,
                planned
            );
        }
        if !workflow.gaps.is_empty() || !workflow.planned_steps.is_empty() {
            assert_eq!(workflow.status, "experimental");
        }
    }

    let swap = &workflows["swap"];
    let swap_steps = swap.steps.iter().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(
        swap_steps,
        vec![
            "swap.quote",
            "swap.build",
            "wallet.allowance",
            "transaction.simulate",
            "transaction.inspect",
            "transaction.send",
            "transaction.verify",
        ]
    );
    assert_eq!(swap.planned_steps, vec!["swap.execute"]);

    let yield_deposit = &workflows["yield-deposit"];
    let yield_steps = yield_deposit
        .steps
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(
        yield_steps,
        vec![
            "yield.discover",
            "yield.build",
            "wallet.allowance",
            "transaction.inspect",
            "transaction.simulate",
            "transaction.send",
            "transaction.verify",
            "position.analyze",
        ]
    );

    let send = catalog.capability("transaction.send").unwrap();
    assert!(send.requires.iter().any(|item| item == "wallet_session"));
    assert_eq!(send.side_effects, "broadcasts");
    let verify = catalog.capability("transaction.verify").unwrap();
    assert_eq!(verify.side_effects, "read_only");
}

#[tokio::test]
async fn swap_workflow_reaches_verify() {
    let app = Router::new()
        .route("/quote", post(quote_ok))
        .route("/rpc", post(rpc));
    let base = serve(app).await;
    let config = config(&base);
    let runtime = RuntimeClient::new(base.clone());

    let quote = runtime
        .swap_quote(
            &QuoteRequest {
                token_in: TOKEN_IN.parse().unwrap(),
                token_out: TOKEN_OUT.parse().unwrap(),
                amount_in: U256::from(1_000_000_000_000_000_000u128),
                max_hops: None,
                amount_out_min: None,
                split: false,
                split_max_branches: None,
            },
            fluxion_client::capability::TokenRef::new(
                Some("TIN".to_string()),
                TOKEN_IN.parse().unwrap(),
                18,
            ),
            fluxion_client::capability::TokenRef::new(
                Some("TOUT".to_string()),
                TOKEN_OUT.parse().unwrap(),
                18,
            ),
            "ethereum",
            "remote",
        )
        .await
        .unwrap();
    assert_eq!(quote.allocations.len(), 1);

    let built = swap_build::build(
        &quote,
        BuildRequest {
            recipient: RECIPIENT.parse().unwrap(),
            slippage_bps: 50,
            deadline: 4_000_000_000,
        },
    )
    .unwrap();
    assert!(built.data.starts_with("0x"));

    let allowance = wallet::allowance(
        &config,
        AllowanceRequest {
            chain: Chain::Ethereum,
            owner: OWNER.parse().unwrap(),
            spender: built.router,
            tokens: vec![WETH.to_string()],
            block: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(allowance.allowances.len(), 1);

    let simulation = transaction::simulate(
        &config,
        SimulateRequest {
            chain: Chain::Ethereum,
            from: Some(OWNER.parse().unwrap()),
            to: built.router,
            data: built.data.clone(),
            value: U256::ZERO,
            block: None,
        },
    )
    .await
    .unwrap();
    assert!(simulation.success);

    let inspection = transaction_inspect::inspect_with_policy(
        &config,
        InspectRequest {
            chain: Chain::Ethereum,
            from: Some(OWNER.parse().unwrap()),
            to: built.router,
            data: built.data,
            value: U256::ZERO,
            block: None,
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(inspection.intent.selector.len(), 10);

    let verified = transaction_verify::verify(
        &config,
        VerifyRequest {
            chain: Chain::Ethereum,
            tx_hash: TX_HASH.to_string(),
            confirmations: 1,
            expected: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(verified.status, "success");
    assert!(verified.confirmations >= 1);
}

#[tokio::test]
async fn swap_workflow_stops_when_quote_fails() {
    let app = Router::new().route("/quote", post(quote_fail));
    let base = serve(app).await;
    let runtime = RuntimeClient::new(base);

    let error = runtime
        .swap_quote(
            &QuoteRequest {
                token_in: TOKEN_IN.parse().unwrap(),
                token_out: TOKEN_OUT.parse().unwrap(),
                amount_in: U256::from(1_000_000u64),
                max_hops: None,
                amount_out_min: None,
                split: false,
                split_max_branches: None,
            },
            fluxion_client::capability::TokenRef::new(
                Some("TIN".to_string()),
                TOKEN_IN.parse().unwrap(),
                18,
            ),
            fluxion_client::capability::TokenRef::new(
                Some("TOUT".to_string()),
                TOKEN_OUT.parse().unwrap(),
                18,
            ),
            "ethereum",
            "remote",
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        fluxion_client::capability::CapabilityErrorCode::RpcError
    );
}

#[tokio::test]
async fn yield_workflow_reaches_verify_and_position() {
    let app = Router::new()
        .route("/yield/discover", post(yield_discover_ok))
        .route("/position/analyze", post(position_ok))
        .route("/rpc", post(rpc));
    let base = serve(app).await;
    let config = config(&base);
    let runtime = RuntimeClient::new(base);

    let discovered = runtime
        .yield_discover(&YieldDiscoverRequest {
            chain: "ethereum".to_string(),
            asset: "ETH".to_string(),
            amount: Some("1".to_string()),
            risk_profile: RiskProfile::Conservative,
            min_tvl_usd: None,
            max_lockup_days: None,
            include_reward_apy: None,
            protocols: vec![],
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(discovered.capability, "yield.discover");

    let built = yield_build::build(&YieldBuildRequest {
        opportunity_id: "lido-steth".to_string(),
        amount: "1".to_string(),
        recipient: RECIPIENT.parse().unwrap(),
        funding_asset: None,
    })
    .unwrap();
    assert!(!built.steps.is_empty());

    let allowance = wallet::allowance(
        &config,
        AllowanceRequest {
            chain: Chain::Ethereum,
            owner: OWNER.parse().unwrap(),
            spender: OWNER.parse().unwrap(),
            tokens: vec![WETH.to_string()],
            block: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(allowance.allowances.len(), 1);

    let step = built
        .steps
        .iter()
        .find(|step| step.kind != "approve")
        .expect("yield build has a signing step");
    let simulation = transaction::simulate(
        &config,
        SimulateRequest {
            chain: Chain::Ethereum,
            from: Some(OWNER.parse().unwrap()),
            to: step.to,
            data: step.data.clone(),
            value: U256::ZERO,
            block: None,
        },
    )
    .await
    .unwrap();
    assert!(simulation.success);

    let verified = transaction_verify::verify(
        &config,
        VerifyRequest {
            chain: Chain::Ethereum,
            tx_hash: TX_HASH.to_string(),
            confirmations: 1,
            expected: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(verified.status, "success");

    let position = runtime
        .position_analyze(&PositionAnalyzeRequest {
            owner: OWNER.parse().unwrap(),
            chain: "ethereum".to_string(),
            opportunity_ids: vec!["lido-steth".to_string()],
            include_zero: false,
            cost_basis: vec![],
        })
        .await
        .unwrap();
    assert_eq!(position.totals.position_count, 0);
}

#[tokio::test]
async fn yield_workflow_stops_on_unknown_opportunity() {
    let error = yield_build::build(&YieldBuildRequest {
        opportunity_id: "not-a-market".to_string(),
        amount: "1".to_string(),
        recipient: RECIPIENT.parse().unwrap(),
        funding_asset: None,
    })
    .unwrap_err();
    assert_eq!(
        error.code,
        fluxion_client::capability::CapabilityErrorCode::InvalidArgument
    );
}

#[tokio::test]
async fn portfolio_workflow_reads_holdings_and_positions() {
    let app = Router::new()
        .route("/position/analyze", post(position_ok))
        .route("/rpc", post(rpc));
    let base = serve(app).await;
    let config = config(&base);
    let runtime = RuntimeClient::new(base);

    let balances = wallet::balance(
        &config,
        BalanceRequest {
            chain: Chain::Ethereum,
            address: OWNER.parse().unwrap(),
            tokens: vec![USDC.to_string()],
            include_native: true,
            block: None,
        },
    )
    .await
    .unwrap();
    assert!(balances.native.is_some());
    assert_eq!(balances.tokens.len(), 1);

    let assets = wallet::assets(
        &config,
        AssetsRequest {
            chain: Chain::Ethereum,
            address: OWNER.parse().unwrap(),
            include_native: false,
            include_unknown: false,
            block: None,
        },
    )
    .await
    .unwrap();
    assert!(!assets.tokens.is_empty());

    let collection: Address = COLLECTION.parse().unwrap();
    let collectibles = wallet::collectibles(
        &config,
        CollectiblesRequest {
            chain: Chain::Ethereum,
            owner: OWNER.parse().unwrap(),
            collections: vec![collection],
            include_empty: true,
            block: None,
        },
    )
    .await
    .unwrap();
    assert!(
        collectibles
            .collections
            .iter()
            .any(|item| item.collection == collection)
    );

    let position = runtime
        .position_analyze(&PositionAnalyzeRequest {
            owner: OWNER.parse().unwrap(),
            chain: "ethereum".to_string(),
            opportunity_ids: vec![],
            include_zero: false,
            cost_basis: vec![],
        })
        .await
        .unwrap();
    assert_eq!(position.totals.position_count, 0);
}

#[tokio::test]
async fn portfolio_workflow_reports_position_failure() {
    let app = Router::new()
        .route("/position/analyze", post(position_fail))
        .route("/rpc", post(rpc));
    let base = serve(app).await;
    let runtime = RuntimeClient::new(base);

    let error = runtime
        .position_analyze(&PositionAnalyzeRequest {
            owner: OWNER.parse().unwrap(),
            chain: "ethereum".to_string(),
            opportunity_ids: vec![],
            include_zero: false,
            cost_basis: vec![],
        })
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        fluxion_client::capability::CapabilityErrorCode::StaleState
    );
}

#[tokio::test]
async fn fourmeme_workflow_discovers_tokens_and_agent_id() {
    let app = Router::new()
        .route("/rpc", post(rpc))
        .route("/private/token/get/v2", get(fourmeme_detail_api))
        .route("/v1/handles/{handle}", get(agent_id_api));
    let base = serve(app).await;
    let fourmeme_config = FourMemeConfig {
        api_url: base.clone(),
        bsc_rpc_url: format!("{base}/rpc"),
    };
    let agent_config = AgentIdConfig {
        api_url: base,
        bsc_rpc_url: fourmeme_config.bsc_rpc_url.clone(),
    };

    let discovered = fourmeme::agent_tokens(
        &fourmeme_config,
        ListRequest {
            wallet: None,
            limit: 5,
            lookback_blocks: 1_000,
        },
    )
    .await
    .unwrap();
    assert!(discovered.tokens.is_empty());

    let token = fourmeme::token(
        &fourmeme_config,
        FOURMEME_TOKEN.parse().unwrap(),
        Some(OWNER.parse().unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(token.phase, "bonding");
    assert_eq!(token.agent_id_exclusive, Some(true));
    assert_eq!(token.wallet_eligible, Some(true));
    let tradability = token.tradability.as_ref().unwrap();
    assert_eq!(tradability.transfer_locked, Some(true));
    assert_eq!(tradability.progress_percent, Some(42.0));
    let market = token.market.as_ref().unwrap();
    assert_eq!(market.max_price.as_deref(), Some("2"));
    assert_eq!(market.volume_24h.as_deref(), Some("1234"));
    assert_eq!(market.change_24h_percent, Some(5.2));
    assert_eq!(market.liquidity.as_deref(), Some("50"));
    let cap = token.wallet_cap.as_ref().unwrap();
    assert_eq!(cap.cap.raw, "10000000000000000000000000");
    assert_eq!(cap.balance.raw, "5000000000000000000000000");
    assert_eq!(cap.remaining.raw, "5000000000000000000000000");
    assert!(!cap.reached);

    let resolved = agent_id::resolve(
        &agent_config,
        AgentIdRequest {
            lookup: AgentIdLookup::Handle("alice".to_string()),
        },
    )
    .await
    .unwrap();
    assert_eq!(resolved.handle.as_deref(), Some("alice"));
    assert_eq!(resolved.owner, Some(OWNER.parse().unwrap()));
}

#[tokio::test]
async fn fourmeme_workflow_reports_rpc_failure() {
    async fn failing_logs(Json(body): Json<Value>) -> Json<Value> {
        let is_batch = body.is_array();
        let requests = body.as_array().cloned().unwrap_or_else(|| vec![body]);
        let mut items = Vec::new();
        for request in requests {
            let id = request.get("id").cloned().unwrap_or(Value::Null);
            if request["method"] == "eth_getLogs" {
                items.push(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32000, "message": "logs unavailable"}
                }));
            } else {
                items.push(rpc(Json(request)).await.0);
            }
        }
        if is_batch {
            Json(Value::Array(items))
        } else {
            Json(items.into_iter().next().unwrap_or(Value::Null))
        }
    }

    let app = Router::new().route("/rpc", post(failing_logs));
    let base = serve(app).await;
    let fourmeme_config = FourMemeConfig {
        api_url: base.clone(),
        bsc_rpc_url: format!("{base}/rpc"),
    };
    let error = fourmeme::agent_tokens(
        &fourmeme_config,
        ListRequest {
            wallet: None,
            limit: 5,
            lookback_blocks: 1_000,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.code,
        fluxion_client::capability::CapabilityErrorCode::RpcError
    );
}
