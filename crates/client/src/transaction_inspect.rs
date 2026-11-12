//! Deterministic pre-sign transaction inspection.
//!
//! The first slice decodes calldata and pins a block. It deliberately does not
//! claim safety: metadata, simulation and policy are reported as not run.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use alloy_primitives::{Address, B256, U256};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::capability::{Amount, CapabilityError};
use crate::chain::Chain;
use crate::token;
use crate::transaction::{Rpc, normalize_data};
use crate::wallet::WalletConfig;

pub use fluxion_protocol::transaction::{
    TransactionInspectApprovalEffect, TransactionInspectArg, TransactionInspectAssurance,
    TransactionInspectCoverage, TransactionInspectEffects, TransactionInspectExplanation,
    TransactionInspectFinding, TransactionInspectIntent, TransactionInspectNativeEffect,
    TransactionInspectNftEffect, TransactionInspectPolicyResult, TransactionInspectResponse,
    TransactionInspectSource, TransactionInspectTarget, TransactionInspectTokenEffect,
};

#[derive(Debug, Clone)]
pub struct InspectRequest {
    pub chain: Chain,
    pub from: Option<Address>,
    pub to: Address,
    pub data: String,
    pub value: U256,
    pub block: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SourceVerifier {
    pub url: String,
    pub api_key: String,
    pub label: String,
}

impl SourceVerifier {
    pub fn from_env(chain: Chain) -> Option<Self> {
        let (key_var, url_var, default_url, label) = match chain {
            Chain::Bsc => (
                "BSCSCAN_API_KEY",
                "FLUXION_BSCSCAN_API_URL",
                "https://api.bscscan.com/api",
                "bscscan",
            ),
            Chain::Ethereum => (
                "ETHERSCAN_API_KEY",
                "FLUXION_ETHERSCAN_API_URL",
                "https://api.etherscan.io/api",
                "etherscan",
            ),
        };
        let api_key = std::env::var(key_var).ok()?.trim().to_string();
        if api_key.is_empty() {
            return None;
        }
        let url = std::env::var(url_var).unwrap_or_else(|_| default_url.to_string());
        Some(Self {
            url,
            api_key,
            label: label.to_string(),
        })
    }

    async fn verify(&self, address: Address) -> SourceVerification {
        let client = reqwest::Client::new();
        let response = client
            .get(&self.url)
            .query(&[
                ("module", "contract"),
                ("action", "getsourcecode"),
                ("address", &address.to_string()),
                ("apikey", &self.api_key),
            ])
            .timeout(Duration::from_secs(10))
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                return SourceVerification {
                    unknowns: vec![format!("source verification request failed: {error}")],
                    ..Default::default()
                };
            }
        };
        let value: Value = match response.json().await {
            Ok(value) => value,
            Err(error) => {
                return SourceVerification {
                    unknowns: vec![format!(
                        "source verification returned invalid JSON: {error}"
                    )],
                    ..Default::default()
                };
            }
        };
        let result = value
            .get("result")
            .and_then(Value::as_array)
            .and_then(|items| items.first());
        let Some(result) = result else {
            let message = value
                .get("result")
                .and_then(Value::as_str)
                .or_else(|| value.get("message").and_then(Value::as_str))
                .unwrap_or("unknown response");
            if message.to_ascii_lowercase().contains("not verified") {
                return SourceVerification {
                    verified: Some(false),
                    warnings: vec!["contract source is not verified".to_string()],
                    ..Default::default()
                };
            }
            return SourceVerification {
                unknowns: vec![format!("source verification unavailable: {message}")],
                ..Default::default()
            };
        };
        let source = result
            .get("SourceCode")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let verified = if source.is_empty() || source.to_ascii_lowercase().contains("not verified")
        {
            Some(false)
        } else {
            Some(true)
        };
        let proxy = result
            .get("Proxy")
            .and_then(Value::as_str)
            .map(|value| value == "1");
        let implementation = result
            .get("Implementation")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse::<Address>().ok());
        SourceVerification {
            verified,
            proxy,
            implementation,
            warnings: if verified == Some(false) {
                vec!["contract source is not verified".to_string()]
            } else {
                Vec::new()
            },
            unknowns: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
struct SourceVerification {
    verified: Option<bool>,
    proxy: Option<bool>,
    implementation: Option<Address>,
    warnings: Vec<String>,
    unknowns: Vec<String>,
}

const RULESET_VERSION: &str = "2026-11-11.1";
const MAX_DATA_BYTES: usize = 256 * 1024;
const MAX_LOG_COUNT: usize = 512;
const MAX_EFFECT_PAIRS: usize = 128;
const MAX_TRACE_CALLS: usize = 4096;
const MAX_TRACE_DEPTH: usize = 32;

const BUILTIN_SELECTORS: &[&str] = &[
    "0xa9059cbb",
    "0x23b872dd",
    "0x095ea7b3",
    "0x39509351",
    "0xa457c2d7",
    "0xa22cb465",
    "0x42842e0e",
    "0xd0e30db0",
    "0x2e1a7d4d",
    "0xd505accf",
    "0xb88d4fde",
    "0xf242432a",
    "0xf2fde38b",
    "0x3659cfe6",
    "0xac9650d8",
    "0x38ed1739",
    "0x7ff36ab5",
    "0x18cbafe5",
    "0x5c11d795",
    "0xb6f9de95",
    "0x791ac947",
];

#[derive(Debug, Clone, Copy)]
enum ArgType {
    Address,
    Uint,
    Uint8,
    Bool,
    Bytes32,
}

#[derive(Debug, Clone, Copy)]
struct ArgSpec {
    name: &'static str,
    kind: &'static str,
    arg_type: ArgType,
}

const fn arg(name: &'static str, kind: &'static str, arg_type: ArgType) -> ArgSpec {
    ArgSpec {
        name,
        kind,
        arg_type,
    }
}

const TRANSFER: [ArgSpec; 2] = [
    arg("to", "address", ArgType::Address),
    arg("amount", "uint256", ArgType::Uint),
];
const TRANSFER_FROM: [ArgSpec; 3] = [
    arg("from", "address", ArgType::Address),
    arg("to", "address", ArgType::Address),
    arg("amount", "uint256", ArgType::Uint),
];
const APPROVE: [ArgSpec; 2] = [
    arg("spender", "address", ArgType::Address),
    arg("amount", "uint256", ArgType::Uint),
];
const ALLOWANCE_CHANGE: [ArgSpec; 2] = [
    arg("spender", "address", ArgType::Address),
    arg("addedValue", "uint256", ArgType::Uint),
];
const SET_APPROVAL_FOR_ALL: [ArgSpec; 2] = [
    arg("operator", "address", ArgType::Address),
    arg("approved", "bool", ArgType::Bool),
];
const SAFE_TRANSFER_FROM: [ArgSpec; 3] = [
    arg("from", "address", ArgType::Address),
    arg("to", "address", ArgType::Address),
    arg("tokenId", "uint256", ArgType::Uint),
];
const ERC1155_SAFE_TRANSFER_FROM: [ArgSpec; 4] = [
    arg("from", "address", ArgType::Address),
    arg("to", "address", ArgType::Address),
    arg("id", "uint256", ArgType::Uint),
    arg("amount", "uint256", ArgType::Uint),
];
const WITHDRAW: [ArgSpec; 1] = [arg("wad", "uint256", ArgType::Uint)];
const TRANSFER_OWNERSHIP: [ArgSpec; 1] = [arg("newOwner", "address", ArgType::Address)];
const UPGRADE_TO: [ArgSpec; 1] = [arg("newImplementation", "address", ArgType::Address)];
const PERMIT: [ArgSpec; 7] = [
    arg("owner", "address", ArgType::Address),
    arg("spender", "address", ArgType::Address),
    arg("value", "uint256", ArgType::Uint),
    arg("deadline", "uint256", ArgType::Uint),
    arg("v", "uint8", ArgType::Uint8),
    arg("r", "bytes32", ArgType::Bytes32),
    arg("s", "bytes32", ArgType::Bytes32),
];

struct DecodedIntent {
    intent: TransactionInspectIntent,
    decode_coverage: &'static str,
    unknowns: Vec<String>,
    warnings: Vec<String>,
}

pub async fn inspect(
    config: &WalletConfig,
    request: InspectRequest,
) -> Result<TransactionInspectResponse, CapabilityError> {
    inspect_with_policy(config, request, None).await
}

pub async fn inspect_with_verifier(
    config: &WalletConfig,
    request: InspectRequest,
    verifier: Option<SourceVerifier>,
) -> Result<TransactionInspectResponse, CapabilityError> {
    inspect_with_options(config, request, None, verifier).await
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct InspectPolicy {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub chains: Vec<String>,
    #[serde(default)]
    pub max_native_spend: Option<String>,
    #[serde(default)]
    pub max_token_spend: Vec<TokenSpendPolicy>,
    #[serde(default)]
    pub target_allowlist: Vec<Address>,
    #[serde(default)]
    pub method_allowlist: Vec<String>,
    #[serde(default)]
    pub spender_allowlist: Vec<Address>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenSpendPolicy {
    pub token: Address,
    pub amount: String,
    #[serde(default)]
    pub decimals: Option<u8>,
}

pub async fn inspect_with_policy(
    config: &WalletConfig,
    request: InspectRequest,
    policy: Option<InspectPolicy>,
) -> Result<TransactionInspectResponse, CapabilityError> {
    let verifier = SourceVerifier::from_env(request.chain);
    inspect_with_options(config, request, policy, verifier).await
}

async fn inspect_with_options(
    config: &WalletConfig,
    request: InspectRequest,
    policy: Option<InspectPolicy>,
    verifier: Option<SourceVerifier>,
) -> Result<TransactionInspectResponse, CapabilityError> {
    if let Some(policy) = policy.as_ref()
        && !policy.mode.is_empty()
        && !policy.mode.eq_ignore_ascii_case("advisory")
        && !policy.mode.eq_ignore_ascii_case("enforce")
    {
        return Err(CapabilityError::invalid_argument(
            "policy.mode must be advisory or enforce",
        ));
    }
    let data = normalize_data(&request.data)?;
    if data.len().saturating_sub(2) / 2 > MAX_DATA_BYTES {
        return Err(CapabilityError::invalid_argument(format!(
            "calldata exceeds {MAX_DATA_BYTES} bytes"
        )));
    }
    let rpc = Rpc::new(&config.rpc_url_for(request.chain));
    let (block_number, block_hash, tag) = rpc.block(request.block).await?;
    let decoded = decode_intent(&data)?;
    let metadata = inspect_target(&rpc, request.to, &tag, verifier.as_ref()).await?;
    let simulation = inspect_effects(&rpc, &request, &data, &tag).await;
    let trace = inspect_trace(&rpc, &request, &data, &tag, &metadata.target).await;
    let verify_rpc_url = std::env::var("FLUXION_VERIFY_RPC_URL")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let cross_check = cross_check(
        &rpc,
        verify_rpc_url.as_deref(),
        request.to,
        &tag,
        block_number,
        block_hash,
    )
    .await;
    let mut unknowns = decoded.unknowns;
    unknowns.extend(metadata.unknowns);
    unknowns.extend(simulation.unknowns);
    unknowns.extend(trace.unknowns);
    unknowns.extend(cross_check.unknowns);
    unknowns.push("no policy was configured".to_string());
    let mut warnings = decoded.warnings;
    warnings.extend(metadata.warnings);
    warnings.extend(simulation.warnings);
    warnings.extend(trace.warnings);
    warnings.extend(cross_check.warnings);
    let mut findings = simulation.findings.clone();
    findings.extend(trace.findings);
    findings.extend(detect_approvals(&decoded.intent));
    findings.extend(detect_nested_approvals(&decoded.intent));
    findings.extend(detect_contract(&decoded.intent, &metadata.target, &request));
    findings.extend(detect_address(&decoded.intent, &request));
    findings.extend(detect_swap(&decoded.intent, &request));
    findings.extend(detect_signature(&decoded.intent));
    findings.extend(detect_asset_flow(
        &decoded.intent,
        &request,
        &simulation.effects,
        decoded.decode_coverage,
    ));
    let policy_eval = evaluate_policy(
        policy.as_ref(),
        &request,
        &decoded.intent,
        &simulation.effects,
        simulation.coverage,
    )?;
    findings.extend(policy_eval.findings);
    unknowns.extend(policy_eval.unknowns);
    let policy_mode = policy
        .as_ref()
        .map(|policy| {
            if policy.mode.is_empty() {
                "advisory".to_string()
            } else {
                policy.mode.to_ascii_lowercase()
            }
        })
        .unwrap_or_else(|| "not_configured".to_string());
    let policy_result = TransactionInspectPolicyResult {
        mode: policy_mode,
        decision: policy_eval.decision.to_string(),
        blocked: policy_eval.blocked,
        reasons: policy_eval.reasons,
    };
    let risk = aggregate_risk(&findings);
    let mut sources = vec![
        TransactionInspectSource {
            name: "calldata-decoder".to_string(),
            scope: "intent".to_string(),
        },
        TransactionInspectSource {
            name: format!("{}-rpc", request.chain.as_str()),
            scope: "block-pinning".to_string(),
        },
        TransactionInspectSource {
            name: format!("{}-rpc", request.chain.as_str()),
            scope: "target-metadata".to_string(),
        },
    ];
    sources.extend(metadata.sources);
    sources.extend(simulation.sources);
    sources.extend(trace.sources);
    sources.extend(cross_check.sources);

    let mut response = TransactionInspectResponse {
        chain: request.chain.as_str().to_string(),
        chain_id: request.chain.chain_id(),
        block_number,
        block_hash,
        from: request.from,
        to: request.to,
        value: Amount::from_raw(request.value, 18),
        intent: decoded.intent,
        target: metadata.target,
        effects: simulation.effects,
        findings,
        risk,
        coverage: TransactionInspectCoverage {
            decode: decoded.decode_coverage.to_string(),
            metadata: metadata.coverage.to_string(),
            simulation: simulation.coverage.to_string(),
            trace: trace.coverage.to_string(),
            policy: policy_eval.coverage.to_string(),
        },
        unknowns,
        policy: policy_result,
        assurance: TransactionInspectAssurance {
            ruleset_version: RULESET_VERSION.to_string(),
            report_id: String::new(),
            deterministic: true,
            cross_check: cross_check.status.to_string(),
            limitations: Vec::new(),
        },
        explanation: TransactionInspectExplanation::default(),
        sources,
        warnings,
    };
    response.assurance.limitations = limitations(&response);
    response.assurance.report_id = report_id(&response, &request, &data);
    response.explanation = explain(&response);
    Ok(response)
}

struct CrossCheck {
    status: &'static str,
    sources: Vec<TransactionInspectSource>,
    unknowns: Vec<String>,
    warnings: Vec<String>,
}

async fn cross_check(
    primary: &Rpc,
    secondary_url: Option<&str>,
    target: Address,
    tag: &str,
    block_number: u64,
    primary_block_hash: B256,
) -> CrossCheck {
    let Some(url) = secondary_url else {
        return CrossCheck {
            status: "not_configured",
            sources: Vec::new(),
            unknowns: Vec::new(),
            warnings: Vec::new(),
        };
    };
    let secondary = Rpc::new(url);
    let secondary_block = match secondary.block(Some(block_number)).await {
        Ok((_, hash, _)) => hash,
        Err(error) => {
            return CrossCheck {
                status: "unavailable",
                sources: vec![TransactionInspectSource {
                    name: "secondary-rpc".to_string(),
                    scope: "cross-check".to_string(),
                }],
                unknowns: vec![format!(
                    "secondary RPC block check failed: {}",
                    error.message
                )],
                warnings: Vec::new(),
            };
        }
    };
    let primary_code = primary.code(target, tag).await;
    let secondary_code = secondary.code(target, tag).await;
    let mut warnings = Vec::new();
    let mut unknowns = Vec::new();
    let mut status = "match";
    if secondary_block != primary_block_hash {
        status = "mismatch";
        warnings.push("secondary RPC reports a different block hash".to_string());
        unknowns.push("block hash mismatch between primary and secondary RPC".to_string());
    }
    match (primary_code, secondary_code) {
        (Ok(primary_code), Ok(secondary_code)) if primary_code != secondary_code => {
            status = "mismatch";
            warnings.push("secondary RPC reports different target bytecode".to_string());
            unknowns.push("target code mismatch between primary and secondary RPC".to_string());
        }
        (Err(error), _) | (_, Err(error)) => {
            if status == "match" {
                status = "unavailable";
            }
            unknowns.push(format!(
                "target code cross-check unavailable: {}",
                error.message
            ));
        }
        _ => {}
    }
    CrossCheck {
        status,
        sources: vec![TransactionInspectSource {
            name: "secondary-rpc".to_string(),
            scope: "cross-check".to_string(),
        }],
        unknowns,
        warnings,
    }
}

fn report_id(
    response: &TransactionInspectResponse,
    request: &InspectRequest,
    data: &str,
) -> String {
    let payload = json!({
        "ruleset_version": RULESET_VERSION,
        "chain": response.chain,
        "chain_id": response.chain_id,
        "block_number": response.block_number,
        "block_hash": response.block_hash,
        "from": request.from,
        "to": request.to,
        "value": request.value.to_string(),
        "data": data,
        "findings": response.findings,
        "coverage": response.coverage,
        "unknowns": response.unknowns,
        "cross_check": response.assurance.cross_check,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let digest = Sha256::digest(bytes);
    format!("0x{}", data_encoding::HEXLOWER.encode(&digest))
}

fn limitations(response: &TransactionInspectResponse) -> Vec<String> {
    let mut items = Vec::new();
    if response.coverage.decode != "complete" {
        items.push(
            "calldata coverage is partial; unknown selectors or arguments remain".to_string(),
        );
    }
    if response.coverage.metadata != "complete" {
        items.push("target metadata or source verification is incomplete".to_string());
    }
    if response.coverage.simulation != "complete" {
        items.push("asset effects are log-derived and partially verified".to_string());
    }
    if response.coverage.trace != "complete" {
        items.push("internal calls were not fully traced".to_string());
    }
    if response.coverage.policy == "not_configured" {
        items.push("no caller policy was configured".to_string());
    }
    match response.assurance.cross_check.as_str() {
        "not_configured" => items.push("secondary RPC cross-check was not configured".to_string()),
        "mismatch" => items.push("secondary RPC disagreed with primary RPC".to_string()),
        "unavailable" => items.push("secondary RPC cross-check was unavailable".to_string()),
        _ => {}
    }
    items.push(
        "RPC responses are trusted for state reads; no light-client proof is used".to_string(),
    );
    items.push(
        "pre-sign inspection is advisory and is not an audit or safety guarantee".to_string(),
    );
    items
}

pub(crate) fn inspect_nested_calldata(
    chain: Chain,
    from: Option<Address>,
    to: Address,
    data: &str,
    value: U256,
) -> Result<(TransactionInspectIntent, Vec<TransactionInspectFinding>), CapabilityError> {
    let data = normalize_data(data)?;
    let decoded = decode_intent(&data)?;
    let request = InspectRequest {
        chain,
        from,
        to,
        data,
        value,
        block: None,
    };
    let mut findings = detect_approvals(&decoded.intent);
    findings.extend(detect_address(&decoded.intent, &request));
    findings.extend(detect_swap(&decoded.intent, &request));
    findings.extend(detect_signature(&decoded.intent));
    Ok((decoded.intent, findings))
}

fn decode_swap_v2(
    selector: &str,
    signature: &str,
    name: &str,
    payload: &[u8],
    token_in: bool,
) -> DecodedIntent {
    let static_len = if token_in { 5 * 32 } else { 4 * 32 };
    let Some(head) = payload.get(..static_len) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "swap static arguments are missing".to_string(),
        );
    };
    let amount_in = if token_in {
        Some(U256::from_be_slice(&head[..32]).to_string())
    } else {
        None
    };
    let amount_out_min_pos = if token_in { 32 } else { 0 };
    let path_offset_pos = if token_in { 64 } else { 32 };
    let to_pos = if token_in { 96 } else { 64 };
    let deadline_pos = if token_in { 128 } else { 96 };

    let amount_out_min =
        U256::from_be_slice(&head[amount_out_min_pos..amount_out_min_pos + 32]).to_string();
    let Some(path_offset) = word_to_usize(&head[path_offset_pos..path_offset_pos + 32]) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "swap path offset is invalid".to_string(),
        );
    };
    let Some(recipient) = decode_address(&head[to_pos..to_pos + 32]).ok() else {
        return known_but_partial(
            selector,
            signature,
            name,
            "swap recipient is invalid".to_string(),
        );
    };
    let deadline = U256::from_be_slice(&head[deadline_pos..deadline_pos + 32]).to_string();

    if path_offset < static_len || path_offset + 32 > payload.len() {
        return known_but_partial(
            selector,
            signature,
            name,
            "swap path offset points outside calldata".to_string(),
        );
    }
    let Some(count) = word_to_usize(&payload[path_offset..path_offset + 32]) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "swap path length is invalid".to_string(),
        );
    };
    if count > 64 {
        return known_but_partial(
            selector,
            signature,
            name,
            "swap path has too many tokens".to_string(),
        );
    }
    let path_start = path_offset + 32;
    let Some(path_end) = path_start.checked_add(count * 32) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "swap path length overflow".to_string(),
        );
    };
    if path_end > payload.len() {
        return known_but_partial(
            selector,
            signature,
            name,
            "swap path exceeds calldata".to_string(),
        );
    }
    let mut path = Vec::with_capacity(count);
    for index in 0..count {
        let word = &payload[path_start + index * 32..path_start + (index + 1) * 32];
        let Ok(address) = decode_address(word) else {
            return known_but_partial(
                selector,
                signature,
                name,
                format!("swap path token {index} is invalid"),
            );
        };
        path.push(address);
    }

    let mut args = Vec::new();
    if let Some(amount_in) = amount_in {
        args.push(TransactionInspectArg {
            name: "amountIn".to_string(),
            kind: "uint256".to_string(),
            value: amount_in,
        });
    }
    args.push(TransactionInspectArg {
        name: "amountOutMin".to_string(),
        kind: "uint256".to_string(),
        value: amount_out_min,
    });
    args.push(TransactionInspectArg {
        name: "path".to_string(),
        kind: "address[]".to_string(),
        value: format!(
            "[{}]",
            path.iter()
                .map(|address| format!("\"{address}\""))
                .collect::<Vec<_>>()
                .join(",")
        ),
    });
    args.push(TransactionInspectArg {
        name: "to".to_string(),
        kind: "address".to_string(),
        value: recipient,
    });
    args.push(TransactionInspectArg {
        name: "deadline".to_string(),
        kind: "uint256".to_string(),
        value: deadline,
    });

    let mut warnings = Vec::new();
    if payload.len() > path_end {
        warnings.push(format!(
            "{} trailing bytes after swap path",
            payload.len() - path_end
        ));
    }
    DecodedIntent {
        intent: TransactionInspectIntent {
            selector: selector.to_string(),
            signature: Some(signature.to_string()),
            name: Some(name.to_string()),
            args,
        },
        decode_coverage: if warnings.is_empty() {
            "complete"
        } else {
            "partial"
        },
        unknowns: Vec::new(),
        warnings,
    }
}

struct PolicyEvaluation {
    findings: Vec<TransactionInspectFinding>,
    coverage: &'static str,
    decision: &'static str,
    blocked: bool,
    reasons: Vec<String>,
    unknowns: Vec<String>,
}

fn evaluate_policy(
    policy: Option<&InspectPolicy>,
    request: &InspectRequest,
    intent: &TransactionInspectIntent,
    effects: &TransactionInspectEffects,
    simulation_coverage: &str,
) -> Result<PolicyEvaluation, CapabilityError> {
    let Some(policy) = policy else {
        return Ok(PolicyEvaluation {
            findings: Vec::new(),
            coverage: "not_configured",
            decision: "not_configured",
            blocked: false,
            reasons: Vec::new(),
            unknowns: Vec::new(),
        });
    };
    let enforce = policy.mode.eq_ignore_ascii_case("enforce");
    let mut findings = Vec::new();
    let mut unknowns = Vec::new();

    if !policy.chains.is_empty()
        && !policy
            .chains
            .iter()
            .any(|chain| chain.eq_ignore_ascii_case(request.chain.as_str()))
    {
        findings.push(policy_finding(
            "POLICY-001",
            "high",
            "Chain not allowed by policy",
            "The transaction chain is outside the configured policy.",
            [("chain", request.chain.as_str().to_string())],
        ));
    }

    if let Some(max) = policy.max_native_spend.as_deref() {
        let max = token::parse_amount(max, 18)?;
        if request.value > max {
            findings.push(policy_finding(
                "POLICY-002",
                "high",
                "Native spend exceeds policy cap",
                "The native value exceeds the configured maximum.",
                [
                    ("value", request.value.to_string()),
                    ("max", max.to_string()),
                ],
            ));
        }
    }

    if !policy.target_allowlist.is_empty() && !policy.target_allowlist.contains(&request.to) {
        findings.push(policy_finding(
            "POLICY-004",
            "high",
            "Target outside policy allowlist",
            "The target address is not allowed by policy.",
            [("target", request.to.to_string())],
        ));
    }

    if !policy.method_allowlist.is_empty() {
        let allowed = intent
            .signature
            .as_deref()
            .is_some_and(|signature| policy.method_allowlist.iter().any(|item| item == signature));
        if !allowed {
            findings.push(policy_finding(
                "POLICY-004",
                "high",
                "Method outside policy allowlist",
                "The decoded method is not allowed by policy.",
                [(
                    "method",
                    intent
                        .signature
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                )],
            ));
        }
    }

    if !policy.spender_allowlist.is_empty() {
        for arg in &intent.args {
            if arg.name != "spender" && arg.name != "operator" {
                continue;
            }
            let Some(spender) = parse_address(&arg.value) else {
                unknowns.push(format!("policy could not parse {} address", arg.name));
                continue;
            };
            if !policy.spender_allowlist.contains(&spender) {
                findings.push(policy_finding(
                    "POLICY-004",
                    "high",
                    "Spender outside policy allowlist",
                    "The decoded spender is not allowed by policy.",
                    [("spender", spender.to_string())],
                ));
            }
        }
    }

    if let Some(from) = request.from {
        for cap in &policy.max_token_spend {
            let decimals = cap
                .decimals
                .or_else(|| token::decimals_for(cap.token))
                .ok_or_else(|| {
                    CapabilityError::invalid_argument(format!(
                        "unknown decimals for policy token {}; set policy.max_token_spend[].decimals",
                        cap.token
                    ))
                })?;
            let max = token::parse_amount(&cap.amount, decimals)?;
            let (_, outflow) = token_delta(effects, cap.token, from);
            if outflow > max {
                findings.push(policy_finding(
                    "POLICY-003",
                    "high",
                    "Token spend exceeds policy cap",
                    "The simulated token outflow exceeds the configured maximum.",
                    [
                        ("token", cap.token.to_string()),
                        ("observed", outflow.to_string()),
                        ("max", max.to_string()),
                    ],
                ));
            }
        }
    }

    if enforce && simulation_coverage == "unavailable" {
        findings.push(policy_finding(
            "POLICY-006",
            "high",
            "Policy cannot be enforced without simulation",
            "The policy is in enforce mode but asset effects are unavailable.",
            [("simulation", simulation_coverage.to_string())],
        ));
        unknowns.push("policy enforcement could not verify asset effects".to_string());
    }

    let reasons = findings
        .iter()
        .map(|finding| format!("{}: {}", finding.id, finding.title))
        .collect::<Vec<_>>();
    let decision = if findings.is_empty() {
        "allow"
    } else if enforce {
        "block"
    } else {
        "warn"
    };
    Ok(PolicyEvaluation {
        findings,
        coverage: if unknowns.is_empty() {
            "complete"
        } else {
            "partial"
        },
        decision,
        blocked: enforce && !reasons.is_empty(),
        reasons,
        unknowns,
    })
}

fn policy_finding<const N: usize>(
    id: &str,
    severity: &str,
    title: &str,
    description: &str,
    evidence: [(&str, String); N],
) -> TransactionInspectFinding {
    let mut evidence_map = BTreeMap::new();
    for (key, value) in evidence {
        evidence_map.insert(key.to_string(), value);
    }
    evidence_map.insert("source".to_string(), "policy".to_string());
    TransactionInspectFinding {
        id: id.to_string(),
        category: "policy".to_string(),
        severity: severity.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        evidence: evidence_map,
        confidence: "high".to_string(),
        deterministic: true,
    }
}

fn explain(response: &TransactionInspectResponse) -> TransactionInspectExplanation {
    let label = response
        .intent
        .signature
        .as_deref()
        .or(response.intent.name.as_deref())
        .unwrap_or("unknown call");
    let summary = if response.findings.is_empty() {
        format!(
            "{label} on {}: no known findings within current coverage (risk: {}). Review the unknowns before signing.",
            response.chain, response.risk
        )
    } else {
        let titles = response
            .findings
            .iter()
            .take(3)
            .map(|finding| finding.title.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "{label} on {}: {} finding(s), risk {}: {}.",
            response.chain,
            response.findings.len(),
            response.risk,
            titles
        )
    };

    let mut key_changes = Vec::new();
    for effect in &response.effects.native {
        key_changes.push(format!(
            "{} native balance changes by {} raw units",
            effect.account, effect.delta
        ));
    }
    for effect in &response.effects.tokens {
        key_changes.push(format!(
            "{} token {} balance changes by {} raw units",
            effect.account, effect.token, effect.delta
        ));
    }
    for effect in &response.effects.approvals {
        key_changes.push(format!(
            "{} allowance for {} on token {} becomes {}",
            effect.owner, effect.spender, effect.token, effect.after
        ));
    }
    for effect in &response.effects.nfts {
        key_changes.push(format!(
            "NFT {} token {} moves from {} to {}",
            effect.collection, effect.token_id, effect.from, effect.to
        ));
    }
    if key_changes.is_empty() {
        key_changes.push("no asset or approval changes were observed".to_string());
    }

    TransactionInspectExplanation {
        summary,
        key_changes,
        unknowns: response.unknowns.clone(),
    }
}

fn detect_signature(intent: &TransactionInspectIntent) -> Vec<TransactionInspectFinding> {
    if intent.name.as_deref() != Some("permit") {
        return Vec::new();
    }
    let mut findings = Vec::new();
    let spender = arg_value(intent, "spender").unwrap_or_else(|| "unknown".to_string());
    let value = arg_value(intent, "value");
    let deadline = arg_value(intent, "deadline");

    if value.as_deref() == Some(U256::MAX.to_string().as_str()) {
        findings.push(signature_finding(
            "SIG-003",
            "high",
            "Unlimited permit approval",
            "The EIP-2612 permit grants an unlimited token allowance.",
            &spender,
            value.as_deref().unwrap_or("unknown"),
            deadline.as_deref().unwrap_or("unknown"),
        ));
    }
    if deadline
        .as_deref()
        .and_then(parse_decimal_u256)
        .is_some_and(|value| value.is_zero())
    {
        findings.push(signature_finding(
            "SIG-002",
            "high",
            "Expired permit deadline",
            "The permit deadline is zero, so the authorization cannot be valid for a future block.",
            &spender,
            value.as_deref().unwrap_or("unknown"),
            "0",
        ));
    }
    findings
}

fn signature_finding(
    id: &str,
    severity: &str,
    title: &str,
    description: &str,
    spender: &str,
    value: &str,
    deadline: &str,
) -> TransactionInspectFinding {
    let mut evidence = BTreeMap::new();
    evidence.insert("spender".to_string(), spender.to_string());
    evidence.insert("value".to_string(), value.to_string());
    evidence.insert("deadline".to_string(), deadline.to_string());
    evidence.insert("source".to_string(), "calldata".to_string());
    TransactionInspectFinding {
        id: id.to_string(),
        category: "signature".to_string(),
        severity: severity.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        evidence,
        confidence: "high".to_string(),
        deterministic: true,
    }
}

fn detect_swap(
    intent: &TransactionInspectIntent,
    request: &InspectRequest,
) -> Vec<TransactionInspectFinding> {
    if !intent
        .name
        .as_deref()
        .is_some_and(|name| name.starts_with("swapExact"))
    {
        return Vec::new();
    }
    let mut findings = Vec::new();
    if let Some(amount_out_min) = arg_value(intent, "amountOutMin")
        .and_then(|value| parse_decimal_u256(&value))
        .filter(|value| value.is_zero())
    {
        findings.push(swap_finding(
            "SWAP-001",
            "high",
            "Zero minimum output",
            "The swap accepts any output amount, exposing the transaction to slippage and MEV.",
            [("amountOutMin", amount_out_min.to_string())],
        ));
    }
    if let (Some(from), Some(recipient)) = (request.from, arg_value(intent, "to"))
        && let Some(recipient) = parse_address(&recipient)
        && recipient != from
    {
        findings.push(swap_finding(
            "SWAP-004",
            "high",
            "Swap recipient differs from sender",
            "The decoded swap sends output to an account other than the sender.",
            [
                ("sender", from.to_string()),
                ("recipient", recipient.to_string()),
            ],
        ));
    }
    if !is_known_swap_router(request.to) {
        findings.push(swap_finding(
            "SWAP-007",
            "medium",
            "Unknown swap router",
            "The swap target is not in the built-in router registry.",
            [("router", request.to.to_string())],
        ));
    }
    findings
}

fn swap_finding<const N: usize>(
    id: &str,
    severity: &str,
    title: &str,
    description: &str,
    evidence: [(&str, String); N],
) -> TransactionInspectFinding {
    let mut evidence_map = BTreeMap::new();
    for (key, value) in evidence {
        evidence_map.insert(key.to_string(), value);
    }
    evidence_map.insert("source".to_string(), "calldata".to_string());
    TransactionInspectFinding {
        id: id.to_string(),
        category: "swap".to_string(),
        severity: severity.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        evidence: evidence_map,
        confidence: "high".to_string(),
        deterministic: true,
    }
}

fn is_known_swap_router(address: Address) -> bool {
    [
        "0x10ED43C718714eb63d5aA57B78B54704E256024E",
        "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
    ]
    .iter()
    .filter_map(|value| parse_address(value))
    .any(|known| known == address)
}

fn detect_address(
    intent: &TransactionInspectIntent,
    request: &InspectRequest,
) -> Vec<TransactionInspectFinding> {
    let mut findings = Vec::new();
    check_address(&mut findings, request.to, "target");
    if let Some(from) = request.from
        && from == request.to
    {
        findings.push(address_finding(
            "ADDR-005",
            "low",
            "Transaction target is the sender",
            "The target address is the same as the sender.",
            "target",
            request.to,
        ));
    }
    for arg in &intent.args {
        if arg.name != "to" {
            continue;
        }
        let Some(recipient) = parse_address(&arg.value) else {
            continue;
        };
        if recipient != request.to {
            check_address(&mut findings, recipient, "recipient");
        }
    }
    findings
}

fn check_address(findings: &mut Vec<TransactionInspectFinding>, address: Address, role: &str) {
    if address == Address::ZERO {
        findings.push(address_finding(
            "ADDR-001",
            "high",
            "Zero address recipient",
            "The transaction targets or pays the zero address, which cannot recover assets.",
            role,
            address,
        ));
    }
    if let Some(burn) = parse_address("0x000000000000000000000000000000000000dEaD")
        && address == burn
    {
        findings.push(address_finding(
            "ADDR-002",
            "high",
            "Burn address recipient",
            "The transaction targets or pays a known burn address.",
            role,
            address,
        ));
    }
}

fn address_finding(
    id: &str,
    severity: &str,
    title: &str,
    description: &str,
    role: &str,
    address: Address,
) -> TransactionInspectFinding {
    let mut evidence = BTreeMap::new();
    evidence.insert("role".to_string(), role.to_string());
    evidence.insert("address".to_string(), address.to_string());
    evidence.insert("source".to_string(), "calldata".to_string());
    TransactionInspectFinding {
        id: id.to_string(),
        category: "address".to_string(),
        severity: severity.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        evidence,
        confidence: "high".to_string(),
        deterministic: true,
    }
}

fn detect_contract(
    intent: &TransactionInspectIntent,
    target: &TransactionInspectTarget,
    request: &InspectRequest,
) -> Vec<TransactionInspectFinding> {
    let mut findings = Vec::new();
    if target.verified == Some(false) {
        let moves_assets = matches!(
            intent.name.as_deref(),
            Some("approve")
                | Some("increaseAllowance")
                | Some("decreaseAllowance")
                | Some("setApprovalForAll")
                | Some("transfer")
                | Some("transferFrom")
                | Some("safeTransferFrom")
        ) || !request.value.is_zero();
        findings.push(contract_finding(
            "CONTRACT-001",
            if moves_assets { "high" } else { "medium" },
            "Unverified contract target",
            "The target contract source is not verified. Verification is not proof of safety, but absence reduces review confidence.",
            [
                ("target", target.address.to_string()),
                ("verified", "false".to_string()),
            ],
        ));
    }
    if target.proxy == Some(true) {
        if intent.name.as_deref() == Some("upgradeTo") {
            findings.push(contract_finding(
                "CONTRACT-003",
                "high",
                "Proxy implementation upgrade",
                "The transaction changes the implementation behind an upgradeable proxy.",
                [
                    ("target", target.address.to_string()),
                    (
                        "implementation",
                        target
                            .implementation
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "unknown".to_string()),
                    ),
                ],
            ));
        } else {
            findings.push(contract_finding(
                "CONTRACT-003",
                "low",
                "Upgradeable proxy target",
                "The target uses an EIP-1967 proxy. Its implementation can change outside this transaction.",
                [
                    ("target", target.address.to_string()),
                    (
                        "implementation",
                        target
                            .implementation
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "unknown".to_string()),
                    ),
                ],
            ));
        }
    }
    findings
}

fn contract_finding<const N: usize>(
    id: &str,
    severity: &str,
    title: &str,
    description: &str,
    evidence: [(&str, String); N],
) -> TransactionInspectFinding {
    let mut evidence_map = BTreeMap::new();
    for (key, value) in evidence {
        evidence_map.insert(key.to_string(), value);
    }
    evidence_map.insert("source".to_string(), "onchain-metadata".to_string());
    TransactionInspectFinding {
        id: id.to_string(),
        category: "contract".to_string(),
        severity: severity.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        evidence: evidence_map,
        confidence: "high".to_string(),
        deterministic: true,
    }
}

fn detect_asset_flow(
    intent: &TransactionInspectIntent,
    request: &InspectRequest,
    effects: &TransactionInspectEffects,
    decode_coverage: &str,
) -> Vec<TransactionInspectFinding> {
    if decode_coverage != "complete" {
        return Vec::new();
    }
    let Some(name) = intent.name.as_deref() else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    let non_payable = matches!(
        name,
        "approve"
            | "increaseAllowance"
            | "decreaseAllowance"
            | "setApprovalForAll"
            | "transferOwnership"
            | "upgradeTo"
    );
    if non_payable
        && !request.value.is_zero()
        && let Some(from) = request.from
    {
        findings.push(flow_finding(
            "FLOW-001",
            "high",
            "Native value attached to a non-payable call",
            "The transaction sends native value to a call that is not expected to accept it.",
            [
                ("from", from.to_string()),
                ("to", request.to.to_string()),
                ("value", request.value.to_string()),
            ],
        ));
    }

    if matches!(
        name,
        "approve" | "increaseAllowance" | "decreaseAllowance" | "setApprovalForAll"
    ) && let Some(from) = request.from
    {
        if effects.tokens.iter().any(|effect| {
            effect.account == from && effect.delta.starts_with('-') && effect.delta != "-0"
        }) {
            findings.push(flow_finding(
                "FLOW-003",
                "high",
                "Unexpected token outflow during an approval",
                "Simulated logs show a token leaving the sender while the decoded call is only an approval.",
                [
                    ("from", from.to_string()),
                    ("target", request.to.to_string()),
                ],
            ));
        }
        if effects.nfts.iter().any(|effect| effect.from == from) {
            findings.push(flow_finding(
                "FLOW-006",
                "high",
                "Unexpected NFT transfer during an approval",
                "Simulated logs show an NFT leaving the sender while the decoded call is only an approval.",
                [("from", from.to_string()), ("target", request.to.to_string())],
            ));
        }
    }

    match name {
        "transfer" => {
            if let (Some(from), Some(recipient), Some(amount)) = (
                request.from,
                arg_value(intent, "to"),
                arg_value(intent, "amount").and_then(|value| parse_decimal_u256(&value)),
            ) {
                detect_erc20_transfer(&mut findings, request, effects, from, recipient, amount);
            }
        }
        "transferFrom" => {
            if let (Some(sender), Some(recipient), Some(amount)) = (
                arg_value(intent, "from"),
                arg_value(intent, "to"),
                arg_value(intent, "amount").and_then(|value| parse_decimal_u256(&value)),
            ) && let Some(sender) = parse_address(&sender)
            {
                detect_erc20_transfer(&mut findings, request, effects, sender, recipient, amount);
            }
        }
        "safeTransferFrom" => detect_nft_transfer(&mut findings, request, effects, intent),
        _ => {}
    }

    findings
}

fn detect_erc20_transfer(
    findings: &mut Vec<TransactionInspectFinding>,
    request: &InspectRequest,
    effects: &TransactionInspectEffects,
    sender: Address,
    recipient: String,
    amount: U256,
) {
    let token = request.to;
    let expected_recipient = parse_address(&recipient);
    let (_, negative) = token_delta(effects, token, sender);
    let recipient_positive = expected_recipient
        .map(|recipient| token_delta(effects, token, recipient).0)
        .unwrap_or(U256::ZERO);
    if negative > amount {
        findings.push(flow_finding(
            "FLOW-002",
            "high",
            "Token outflow exceeds decoded amount",
            "Simulated logs show more tokens leaving the sender than the calldata amount.",
            [
                ("token", token.to_string()),
                ("sender", sender.to_string()),
                ("expected", amount.to_string()),
                ("observed", negative.to_string()),
            ],
        ));
    }
    if recipient_positive < amount && !recipient_positive.is_zero() {
        findings.push(flow_finding(
            "FLOW-005",
            "medium",
            "Token amount differs from decoded amount",
            "The simulated recipient amount is lower than the calldata amount, which may indicate a fee-on-transfer token.",
            [
                ("token", token.to_string()),
                ("recipient", recipient.clone()),
                ("expected", amount.to_string()),
                ("observed", recipient_positive.to_string()),
            ],
        ));
    }
    if let Some(expected) = expected_recipient {
        for effect in &effects.tokens {
            if effect.token == token
                && effect.account != expected
                && !effect.delta.starts_with('-')
                && effect.delta != "0"
            {
                findings.push(flow_finding(
                    "FLOW-004",
                    "high",
                    "Unexpected token recipient",
                    "Simulated logs show a token recipient that differs from the decoded calldata.",
                    [
                        ("token", token.to_string()),
                        ("expected", expected.to_string()),
                        ("observed", effect.account.to_string()),
                    ],
                ));
            }
        }
    }
}

fn detect_nft_transfer(
    findings: &mut Vec<TransactionInspectFinding>,
    request: &InspectRequest,
    effects: &TransactionInspectEffects,
    intent: &TransactionInspectIntent,
) {
    let expected_recipient = arg_value(intent, "to").and_then(|value| parse_address(&value));
    let expected_from = arg_value(intent, "from").and_then(|value| parse_address(&value));
    for effect in &effects.nfts {
        if effect.collection != request.to {
            continue;
        }
        if let Some(expected) = expected_recipient
            && effect.to != expected
        {
            findings.push(flow_finding(
                "FLOW-004",
                "high",
                "Unexpected NFT recipient",
                "Simulated logs show an NFT recipient that differs from the decoded calldata.",
                [
                    ("collection", effect.collection.to_string()),
                    ("expected", expected.to_string()),
                    ("observed", effect.to.to_string()),
                ],
            ));
        }
        if let Some(expected) = expected_from
            && effect.from != expected
        {
            findings.push(flow_finding(
                "FLOW-002",
                "high",
                "NFT sender differs from decoded calldata",
                "Simulated logs show the NFT leaving a different account than the decoded sender.",
                [
                    ("collection", effect.collection.to_string()),
                    ("expected", expected.to_string()),
                    ("observed", effect.from.to_string()),
                ],
            ));
        }
    }
}

fn token_delta(
    effects: &TransactionInspectEffects,
    token: Address,
    account: Address,
) -> (U256, U256) {
    let mut positive = U256::ZERO;
    let mut negative = U256::ZERO;
    for effect in &effects.tokens {
        if effect.token != token || effect.account != account {
            continue;
        }
        if let Some(value) = parse_signed_u256(&effect.delta) {
            if value.0 {
                negative = negative.saturating_add(value.1);
            } else {
                positive = positive.saturating_add(value.1);
            }
        }
    }
    (positive, negative)
}

fn parse_signed_u256(value: &str) -> Option<(bool, U256)> {
    if let Some(value) = value.strip_prefix('-') {
        Some((true, U256::from_str_radix(value, 10).ok()?))
    } else {
        Some((false, U256::from_str_radix(value, 10).ok()?))
    }
}

fn parse_decimal_u256(value: &str) -> Option<U256> {
    U256::from_str_radix(value, 10).ok()
}

fn flow_finding<const N: usize>(
    id: &str,
    severity: &str,
    title: &str,
    description: &str,
    evidence: [(&str, String); N],
) -> TransactionInspectFinding {
    let mut evidence_map = BTreeMap::new();
    for (key, value) in evidence {
        evidence_map.insert(key.to_string(), value);
    }
    evidence_map.insert("source".to_string(), "calldata+simulated-logs".to_string());
    TransactionInspectFinding {
        id: id.to_string(),
        category: "asset-flow".to_string(),
        severity: severity.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        evidence: evidence_map,
        confidence: "medium".to_string(),
        deterministic: true,
    }
}

fn detect_approvals(intent: &TransactionInspectIntent) -> Vec<TransactionInspectFinding> {
    let mut findings = Vec::new();
    match intent.name.as_deref() {
        Some("approve") | Some("increaseAllowance") => {
            let spender = arg_value(intent, "spender");
            let amount = arg_value(intent, "amount").or_else(|| arg_value(intent, "addedValue"));
            let Some(spender) = spender else {
                return findings;
            };
            let Some(amount) = amount else {
                return findings;
            };
            if amount == U256::MAX.to_string() {
                findings.push(approval_finding(
                    "AUTH-001",
                    "Unlimited token approval",
                    "The transaction grants an unlimited token allowance.",
                    spender,
                    &amount,
                    "calldata",
                ));
            }
        }
        Some("setApprovalForAll") => {
            let Some(operator) = arg_value(intent, "operator") else {
                return findings;
            };
            if arg_value(intent, "approved").as_deref() == Some("true") {
                findings.push(approval_finding(
                    "AUTH-004",
                    "NFT operator approval",
                    "The transaction grants an operator control over all NFTs in this collection.",
                    operator,
                    "all",
                    "calldata",
                ));
            }
        }
        _ => {}
    }
    findings
}

fn approval_finding(
    id: &str,
    title: &str,
    description: &str,
    spender: String,
    amount: &str,
    source: &str,
) -> TransactionInspectFinding {
    let mut evidence = BTreeMap::new();
    evidence.insert("spender".to_string(), spender);
    evidence.insert("amount".to_string(), amount.to_string());
    evidence.insert("source".to_string(), source.to_string());
    TransactionInspectFinding {
        id: id.to_string(),
        category: "approval".to_string(),
        severity: "high".to_string(),
        title: title.to_string(),
        description: description.to_string(),
        evidence,
        confidence: "high".to_string(),
        deterministic: true,
    }
}

fn arg_value(intent: &TransactionInspectIntent, name: &str) -> Option<String> {
    intent
        .args
        .iter()
        .find(|arg| arg.name == name)
        .map(|arg| arg.value.clone())
}

fn aggregate_risk(findings: &[TransactionInspectFinding]) -> String {
    let mut level = 0;
    for finding in findings {
        level = level.max(match finding.severity.as_str() {
            "critical" => 4,
            "high" => 3,
            "medium" => 2,
            "low" => 1,
            _ => 0,
        });
    }
    match level {
        4 => "critical",
        3 => "high",
        2 => "medium",
        1 => "low",
        _ => "unknown",
    }
    .to_string()
}

struct SimulationMetadata {
    effects: TransactionInspectEffects,
    findings: Vec<TransactionInspectFinding>,
    coverage: &'static str,
    sources: Vec<TransactionInspectSource>,
    unknowns: Vec<String>,
    warnings: Vec<String>,
}

async fn inspect_effects(
    rpc: &Rpc,
    request: &InspectRequest,
    data: &str,
    tag: &str,
) -> SimulationMetadata {
    let mut effects = TransactionInspectEffects::default();
    let mut findings = Vec::new();
    let mut unknowns = Vec::new();
    let mut warnings = Vec::new();
    let mut sources = Vec::new();

    if !request.value.is_zero() {
        if let Some(from) = request.from {
            effects.native.push(TransactionInspectNativeEffect {
                account: from,
                delta: format!("-{}", request.value),
            });
            effects.native.push(TransactionInspectNativeEffect {
                account: request.to,
                delta: request.value.to_string(),
            });
            sources.push(TransactionInspectSource {
                name: "transaction-value".to_string(),
                scope: "native-effects".to_string(),
            });
        } else {
            unknowns.push(
                "sender was not provided; native value effects were not attributed".to_string(),
            );
        }
    }

    let mut tx = json!({
        "to": request.to,
        "data": data,
    });
    if let Some(from) = request.from {
        tx["from"] = json!(from);
    } else {
        warnings.push("sender was not provided; simulation may not reflect the signer".to_string());
    }
    if !request.value.is_zero() {
        tx["value"] = json!(format!("0x{:x}", request.value));
    }

    let block = match rpc.simulate_v1(tx.clone(), tag).await {
        Ok(block) => block,
        Err(error) => {
            unknowns.push(format!("asset effects unavailable: {}", error.message));
            return SimulationMetadata {
                effects,
                findings,
                coverage: "unavailable",
                sources,
                unknowns,
                warnings,
            };
        }
    };
    sources.push(TransactionInspectSource {
        name: "eth_simulateV1".to_string(),
        scope: "simulated-logs".to_string(),
    });

    let Some(call) = block
        .get("calls")
        .and_then(Value::as_array)
        .and_then(|calls| calls.first())
    else {
        unknowns.push("simulation returned no call result".to_string());
        return SimulationMetadata {
            effects,
            findings,
            coverage: "partial",
            sources,
            unknowns,
            warnings,
        };
    };
    if call.get("status").and_then(Value::as_str) != Some("0x1") {
        let detail = call
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("simulation reverted");
        unknowns.push(format!("transaction simulation failed: {detail}"));
        return SimulationMetadata {
            effects,
            findings,
            coverage: "partial",
            sources,
            unknowns,
            warnings,
        };
    }

    if let Some(logs) = call.get("logs").and_then(Value::as_array) {
        for log in logs.iter().take(MAX_LOG_COUNT) {
            decode_log(log, &mut effects, &mut warnings);
        }
        if logs.len() > MAX_LOG_COUNT {
            warnings.push(format!(
                "simulation produced {} logs; only the first {MAX_LOG_COUNT} were decoded",
                logs.len()
            ));
        }
        if logs.is_empty() {
            warnings
                .push("simulation produced no logs; token effects may be incomplete".to_string());
        }
    } else {
        unknowns.push("simulation returned no logs; token effects are unknown".to_string());
    }
    unknowns.push(
        "effects are derived from simulated logs; gas fees and full state changes are not included"
            .to_string(),
    );
    warnings.push(
        "simulated logs can be emitted by arbitrary contracts and must be cross-checked"
            .to_string(),
    );

    let verification = verify_effects(rpc, tag, &tx, &effects).await;
    findings.extend(verification.findings);
    unknowns.extend(verification.unknowns);
    warnings.extend(verification.warnings);
    sources.extend(verification.sources);

    SimulationMetadata {
        effects,
        findings,
        coverage: "partial",
        sources,
        unknowns,
        warnings,
    }
}

struct TraceInspection {
    findings: Vec<TransactionInspectFinding>,
    coverage: &'static str,
    sources: Vec<TransactionInspectSource>,
    unknowns: Vec<String>,
    warnings: Vec<String>,
}

async fn inspect_trace(
    rpc: &Rpc,
    request: &InspectRequest,
    data: &str,
    tag: &str,
    target: &TransactionInspectTarget,
) -> TraceInspection {
    let mut tx = json!({
        "to": request.to,
        "data": data,
    });
    if let Some(from) = request.from {
        tx["from"] = json!(from);
    }
    if !request.value.is_zero() {
        tx["value"] = json!(format!("0x{:x}", request.value));
    }

    let trace = match rpc.trace_call(tx, tag).await {
        Ok(trace) => trace,
        Err(error) => {
            return TraceInspection {
                findings: Vec::new(),
                coverage: "unavailable",
                sources: Vec::new(),
                unknowns: vec![format!(
                    "internal-call trace unavailable: {}",
                    error.message
                )],
                warnings: Vec::new(),
            };
        }
    };
    if !trace.is_object() {
        return TraceInspection {
            findings: Vec::new(),
            coverage: "unavailable",
            sources: Vec::new(),
            unknowns: vec!["internal-call trace returned an invalid result".to_string()],
            warnings: Vec::new(),
        };
    }

    let mut findings = Vec::new();
    let mut budget = TraceBudget::default();
    collect_delegatecalls(&trace, target, 0, &mut findings, &mut budget);
    let mut unknowns = Vec::new();
    if budget.truncated {
        unknowns.push(format!(
            "internal-call trace truncated at {MAX_TRACE_CALLS} calls or depth {MAX_TRACE_DEPTH}"
        ));
    }
    TraceInspection {
        findings,
        coverage: if budget.truncated {
            "partial"
        } else {
            "complete"
        },
        sources: vec![TransactionInspectSource {
            name: "debug_traceCall".to_string(),
            scope: "internal-calls".to_string(),
        }],
        unknowns,
        warnings: Vec::new(),
    }
}

#[derive(Default)]
struct TraceBudget {
    calls: usize,
    truncated: bool,
}

fn collect_delegatecalls(
    call: &Value,
    target: &TransactionInspectTarget,
    depth: usize,
    findings: &mut Vec<TransactionInspectFinding>,
    budget: &mut TraceBudget,
) {
    if depth > MAX_TRACE_DEPTH || budget.calls >= MAX_TRACE_CALLS {
        budget.truncated = true;
        return;
    }
    budget.calls += 1;
    if call.get("type").and_then(Value::as_str) == Some("DELEGATECALL") {
        let to = call
            .get("to")
            .and_then(Value::as_str)
            .and_then(parse_address);
        let from = call
            .get("from")
            .and_then(Value::as_str)
            .and_then(parse_address);
        let input = call.get("input").and_then(Value::as_str).unwrap_or("0x");
        let normal_proxy =
            target.proxy == Some(true) && to.is_some() && target.implementation == to;
        if !normal_proxy {
            let mut evidence = BTreeMap::new();
            evidence.insert("type".to_string(), "DELEGATECALL".to_string());
            evidence.insert(
                "from".to_string(),
                from.map(|value| value.to_string()).unwrap_or_default(),
            );
            evidence.insert(
                "to".to_string(),
                to.map(|value| value.to_string()).unwrap_or_default(),
            );
            evidence.insert("input".to_string(), input.to_string());
            evidence.insert("depth".to_string(), depth.to_string());
            evidence.insert("source".to_string(), "debug_traceCall".to_string());
            findings.push(TransactionInspectFinding {
                id: "CONTRACT-004".to_string(),
                category: "contract".to_string(),
                severity: "high".to_string(),
                title: "Internal delegatecall".to_string(),
                description: "The transaction performs a delegatecall that is not a recognized proxy implementation call.".to_string(),
                evidence,
                confidence: "high".to_string(),
                deterministic: true,
            });
        }
    }
    if let Some(calls) = call.get("calls").and_then(Value::as_array) {
        for nested in calls {
            collect_delegatecalls(nested, target, depth + 1, findings, budget);
        }
    }
}

struct EffectVerification {
    findings: Vec<TransactionInspectFinding>,
    sources: Vec<TransactionInspectSource>,
    unknowns: Vec<String>,
    warnings: Vec<String>,
}

async fn verify_effects(
    rpc: &Rpc,
    tag: &str,
    tx: &Value,
    effects: &TransactionInspectEffects,
) -> EffectVerification {
    let mut token_pairs = BTreeSet::new();
    for effect in &effects.tokens {
        if effect.account != Address::ZERO && effect.delta != "0" && effect.delta != "-0" {
            token_pairs.insert((effect.token, effect.account));
        }
    }
    let mut approval_triples = BTreeSet::new();
    for effect in &effects.approvals {
        approval_triples.insert((effect.token, effect.owner, effect.spender));
    }
    let mut nft_tokens = BTreeSet::new();
    for effect in &effects.nfts {
        if let Ok(token_id) = effect.token_id.parse::<U256>() {
            nft_tokens.insert((effect.collection, token_id));
        }
    }
    if token_pairs.is_empty() && approval_triples.is_empty() && nft_tokens.is_empty() {
        return EffectVerification {
            findings: Vec::new(),
            sources: Vec::new(),
            unknowns: Vec::new(),
            warnings: Vec::new(),
        };
    }
    let token_pairs = token_pairs
        .into_iter()
        .take(MAX_EFFECT_PAIRS)
        .collect::<BTreeSet<_>>();
    let approval_triples = approval_triples
        .into_iter()
        .take(MAX_EFFECT_PAIRS)
        .collect::<BTreeSet<_>>();
    let nft_tokens = nft_tokens
        .into_iter()
        .take(MAX_EFFECT_PAIRS)
        .collect::<BTreeSet<_>>();

    let mut calls = vec![tx.clone()];
    for (token, account) in &token_pairs {
        calls.push(json!({
            "to": token,
            "data": encode_balance_of(*account),
        }));
    }
    for (token, owner, spender) in &approval_triples {
        calls.push(json!({
            "to": token,
            "data": encode_allowance(*owner, *spender),
        }));
    }
    for (collection, token_id) in &nft_tokens {
        calls.push(json!({
            "to": collection,
            "data": encode_owner_of(*token_id),
        }));
    }

    let mut unknowns = Vec::new();
    let warnings = Vec::new();
    let mut findings = Vec::new();

    let balance_pre_offset = 1;
    let approval_pre_offset = balance_pre_offset + token_pairs.len();
    let nft_pre_offset = approval_pre_offset + approval_triples.len();

    let mut pre_values = Vec::new();
    for (index, (token, account)) in token_pairs.iter().enumerate() {
        match rpc
            .call_hex(*token, &encode_balance_of(*account), tag)
            .await
        {
            Ok(value) => pre_values.push((balance_pre_offset + index, value)),
            Err(error) => unknowns.push(format!(
                "effect verification could not read balance for {token}/{account}: {}",
                error.message
            )),
        }
    }
    for (index, (token, owner, spender)) in approval_triples.iter().enumerate() {
        match rpc
            .call_hex(*token, &encode_allowance(*owner, *spender), tag)
            .await
        {
            Ok(value) => pre_values.push((approval_pre_offset + index, value)),
            Err(error) => unknowns.push(format!(
                "effect verification could not read allowance for {token}/{owner}/{spender}: {}",
                error.message
            )),
        }
    }
    for (index, (collection, token_id)) in nft_tokens.iter().enumerate() {
        match rpc
            .call_hex(*collection, &encode_owner_of(*token_id), tag)
            .await
        {
            Ok(value) => pre_values.push((nft_pre_offset + index, value)),
            Err(error) => unknowns.push(format!(
                "effect verification could not read owner for {collection}/{token_id}: {}",
                error.message
            )),
        }
    }

    let post = match rpc.simulate_v1_calls(calls, tag).await {
        Ok(calls) => calls,
        Err(error) => {
            unknowns.push(format!(
                "effect verification simulation failed: {}",
                error.message
            ));
            return EffectVerification {
                findings,
                sources: Vec::new(),
                unknowns,
                warnings,
            };
        }
    };
    if post.len() < 1 + token_pairs.len() + approval_triples.len() + nft_tokens.len() {
        unknowns.push("effect verification returned an incomplete call result".to_string());
        return EffectVerification {
            findings,
            sources: Vec::new(),
            unknowns,
            warnings,
        };
    }

    for (index, (token, account)) in token_pairs.iter().enumerate() {
        let Some(pre) = pre_values
            .iter()
            .find(|(pre_index, _)| *pre_index == balance_pre_offset + index)
            .and_then(|(_, value)| parse_u256(value))
        else {
            continue;
        };
        let Some(post) = post
            .get(balance_pre_offset + index)
            .and_then(|call| call.get("returnData"))
            .and_then(Value::as_str)
            .and_then(parse_u256)
        else {
            unknowns.push(format!(
                "effect verification could not read post balance for {token}/{account}"
            ));
            continue;
        };
        let expected = expected_token_delta(effects, *token, *account);
        let actual = signed_delta(post, pre);
        if actual != expected {
            findings.push(effect_finding(
                "EFFECT-007",
                "Log amount does not match balance change",
                "The simulated token balance change differs from the Transfer log amount.",
                [
                    ("token", token.to_string()),
                    ("account", account.to_string()),
                    ("expected", format_signed_delta(expected)),
                    ("actual", format_signed_delta(actual)),
                ],
            ));
        }
    }

    for (index, (token, owner, spender)) in approval_triples.iter().enumerate() {
        let expected = effects
            .approvals
            .iter()
            .find(|effect| {
                effect.token == *token && effect.owner == *owner && effect.spender == *spender
            })
            .and_then(|effect| parse_decimal_u256(&effect.after));
        let Some(expected) = expected else {
            continue;
        };
        let Some(post) = post
            .get(approval_pre_offset + index)
            .and_then(|call| call.get("returnData"))
            .and_then(Value::as_str)
            .and_then(parse_u256)
        else {
            unknowns.push(format!(
                "effect verification could not read post allowance for {token}/{owner}/{spender}"
            ));
            continue;
        };
        if post != expected {
            findings.push(effect_finding(
                "EFFECT-009",
                "Approval log does not match allowance",
                "The simulated allowance differs from the Approval log amount.",
                [
                    ("token", token.to_string()),
                    ("owner", owner.to_string()),
                    ("spender", spender.to_string()),
                    ("expected", expected.to_string()),
                    ("actual", post.to_string()),
                ],
            ));
        }
    }

    for (index, (collection, token_id)) in nft_tokens.iter().enumerate() {
        let expected = effects
            .nfts
            .iter()
            .find(|effect| {
                effect.collection == *collection
                    && effect.token_id.parse::<U256>().ok() == Some(*token_id)
            })
            .map(|effect| effect.to);
        let Some(expected) = expected else {
            continue;
        };
        let Some(post) = post
            .get(nft_pre_offset + index)
            .and_then(|call| call.get("returnData"))
            .and_then(Value::as_str)
            .and_then(parse_topic_address)
        else {
            unknowns.push(format!(
                "effect verification could not read post owner for {collection}/{token_id}"
            ));
            continue;
        };
        if post != expected {
            findings.push(effect_finding(
                "EFFECT-010",
                "NFT log does not match owner",
                "The simulated NFT owner differs from the Transfer log recipient.",
                [
                    ("collection", collection.to_string()),
                    ("token_id", token_id.to_string()),
                    ("expected", expected.to_string()),
                    ("actual", post.to_string()),
                ],
            ));
        }
    }

    if findings.is_empty() {
        unknowns.push(
            "effect verification is limited to simulated token balances and allowances".to_string(),
        );
    }
    let sources = vec![TransactionInspectSource {
        name: "effect-verifier".to_string(),
        scope: "balance-and-allowance".to_string(),
    }];
    EffectVerification {
        findings,
        sources,
        unknowns,
        warnings,
    }
}

fn encode_balance_of(account: Address) -> String {
    format!("0x70a08231{}", address_word(account))
}

fn encode_allowance(owner: Address, spender: Address) -> String {
    format!("0xdd62ed3e{}{}", address_word(owner), address_word(spender))
}

fn encode_owner_of(token_id: U256) -> String {
    format!("0x6352211e{:064x}", token_id)
}

fn address_word(address: Address) -> String {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(address.as_slice());
    data_encoding::HEXLOWER.encode(&word)
}

fn expected_token_delta(
    effects: &TransactionInspectEffects,
    token: Address,
    account: Address,
) -> (bool, U256) {
    let (positive, negative) = token_delta(effects, token, account);
    if positive >= negative {
        (false, positive - negative)
    } else {
        (true, negative - positive)
    }
}

fn signed_delta(after: U256, before: U256) -> (bool, U256) {
    if after >= before {
        (false, after - before)
    } else {
        (true, before - after)
    }
}

fn format_signed_delta((negative, value): (bool, U256)) -> String {
    if negative {
        format!("-{value}")
    } else {
        value.to_string()
    }
}

fn effect_finding<const N: usize>(
    id: &str,
    title: &str,
    description: &str,
    evidence: [(&str, String); N],
) -> TransactionInspectFinding {
    let mut evidence_map = BTreeMap::new();
    for (key, value) in evidence {
        evidence_map.insert(key.to_string(), value);
    }
    evidence_map.insert(
        "source".to_string(),
        "eth_simulateV1+balance-check".to_string(),
    );
    TransactionInspectFinding {
        id: id.to_string(),
        category: "effects".to_string(),
        severity: "high".to_string(),
        title: title.to_string(),
        description: description.to_string(),
        evidence: evidence_map,
        confidence: "high".to_string(),
        deterministic: true,
    }
}

const TRANSFER_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
const APPROVAL_TOPIC: &str = "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925";

fn decode_log(log: &Value, effects: &mut TransactionInspectEffects, warnings: &mut Vec<String>) {
    let Some(token) = log
        .get("address")
        .and_then(Value::as_str)
        .and_then(parse_address)
    else {
        warnings.push("simulation log has an invalid address".to_string());
        return;
    };
    let Some(topics) = log.get("topics").and_then(Value::as_array) else {
        warnings.push("simulation log has no topics".to_string());
        return;
    };
    let Some(topic0) = topics.first().and_then(Value::as_str) else {
        warnings.push("simulation log has no topic0".to_string());
        return;
    };
    let data = log.get("data").and_then(Value::as_str).unwrap_or("0x");

    match topic0 {
        TRANSFER_TOPIC if topics.len() == 3 => {
            let Some(from) = topics
                .get(1)
                .and_then(Value::as_str)
                .and_then(parse_topic_address)
            else {
                warnings.push("ERC20 Transfer log has an invalid from address".to_string());
                return;
            };
            let Some(to) = topics
                .get(2)
                .and_then(Value::as_str)
                .and_then(parse_topic_address)
            else {
                warnings.push("ERC20 Transfer log has an invalid to address".to_string());
                return;
            };
            let Some(raw) = parse_u256(data) else {
                warnings.push("ERC20 Transfer log has invalid amount data".to_string());
                return;
            };
            effects.tokens.push(TransactionInspectTokenEffect {
                token,
                account: from,
                delta: format!("-{raw}"),
            });
            effects.tokens.push(TransactionInspectTokenEffect {
                token,
                account: to,
                delta: raw.to_string(),
            });
        }
        TRANSFER_TOPIC if topics.len() == 4 => {
            let Some(from) = topics
                .get(1)
                .and_then(Value::as_str)
                .and_then(parse_topic_address)
            else {
                warnings.push("ERC721 Transfer log has an invalid from address".to_string());
                return;
            };
            let Some(to) = topics
                .get(2)
                .and_then(Value::as_str)
                .and_then(parse_topic_address)
            else {
                warnings.push("ERC721 Transfer log has an invalid to address".to_string());
                return;
            };
            let Some(token_id) = topics.get(3).and_then(Value::as_str).and_then(parse_u256) else {
                warnings.push("ERC721 Transfer log has an invalid token id".to_string());
                return;
            };
            effects.nfts.push(TransactionInspectNftEffect {
                collection: token,
                token_id: token_id.to_string(),
                from,
                to,
            });
        }
        APPROVAL_TOPIC if topics.len() == 3 => {
            let Some(owner) = topics
                .get(1)
                .and_then(Value::as_str)
                .and_then(parse_topic_address)
            else {
                warnings.push("Approval log has an invalid owner address".to_string());
                return;
            };
            let Some(spender) = topics
                .get(2)
                .and_then(Value::as_str)
                .and_then(parse_topic_address)
            else {
                warnings.push("Approval log has an invalid spender address".to_string());
                return;
            };
            let Some(after) = parse_u256(data) else {
                warnings.push("Approval log has invalid amount data".to_string());
                return;
            };
            effects.approvals.push(TransactionInspectApprovalEffect {
                token,
                owner,
                spender,
                before: None,
                after: after.to_string(),
            });
        }
        _ => {}
    }
}

fn parse_topic_address(topic: &str) -> Option<Address> {
    let bytes = hex_to_bytes(topic).ok()?;
    (bytes.len() == 32).then(|| Address::from_slice(&bytes[12..]))
}

fn parse_address(value: &str) -> Option<Address> {
    value.parse().ok()
}

fn parse_u256(value: &str) -> Option<U256> {
    let hex = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))?;
    U256::from_str_radix(hex, 16).ok()
}

struct TargetMetadata {
    target: TransactionInspectTarget,
    coverage: &'static str,
    sources: Vec<TransactionInspectSource>,
    unknowns: Vec<String>,
    warnings: Vec<String>,
}

async fn inspect_target(
    rpc: &Rpc,
    address: Address,
    tag: &str,
    verifier: Option<&SourceVerifier>,
) -> Result<TargetMetadata, CapabilityError> {
    let code = rpc.code(address, tag).await?;
    if code == "0x" {
        return Ok(TargetMetadata {
            target: TransactionInspectTarget {
                address,
                kind: "eoa".to_string(),
                verified: None,
                proxy: Some(false),
                implementation: None,
                proxy_kind: None,
            },
            coverage: "complete",
            sources: Vec::new(),
            unknowns: Vec::new(),
            warnings: Vec::new(),
        });
    }

    let (proxy, implementation, proxy_kind, mut unknowns, mut warnings) =
        detect_proxy(rpc, address, &code, tag).await;

    let (verified, source_proxy, source_implementation, sources) = match verifier {
        Some(verifier) => {
            let source = verifier.verify(address).await;
            unknowns.extend(source.unknowns);
            warnings.extend(source.warnings);
            (
                source.verified,
                source.proxy,
                source.implementation,
                vec![TransactionInspectSource {
                    name: verifier.label.clone(),
                    scope: "source-verification".to_string(),
                }],
            )
        }
        None => {
            unknowns.push("contract source verification was not configured".to_string());
            warnings.push(
                "verified=null means source verification was not checked, not that the contract is safe"
                    .to_string(),
            );
            (None, None, None, Vec::new())
        }
    };
    let proxy = proxy.or(source_proxy);
    let implementation = implementation.or(source_implementation);
    let proxy_kind = proxy_kind.or_else(|| source_proxy.map(|_| "provider".to_string()));
    let coverage = if verified.is_some() && proxy.is_some() {
        "complete"
    } else {
        "partial"
    };

    Ok(TargetMetadata {
        target: TransactionInspectTarget {
            address,
            kind: "contract".to_string(),
            verified,
            proxy,
            implementation,
            proxy_kind,
        },
        coverage,
        sources,
        unknowns,
        warnings,
    })
}

async fn detect_proxy(
    rpc: &Rpc,
    address: Address,
    code: &str,
    tag: &str,
) -> (
    Option<bool>,
    Option<Address>,
    Option<String>,
    Vec<String>,
    Vec<String>,
) {
    if let Some(implementation) = minimal_proxy_implementation(code) {
        return (
            Some(true),
            Some(implementation),
            Some("eip1167".to_string()),
            Vec::new(),
            Vec::new(),
        );
    }

    let impl_slot: B256 = match EIP1967_IMPLEMENTATION_SLOT.parse() {
        Ok(slot) => slot,
        Err(_) => {
            return (
                None,
                None,
                None,
                vec!["invalid EIP-1967 slot".to_string()],
                Vec::new(),
            );
        }
    };
    match rpc.storage_at(address, impl_slot, tag).await {
        Ok(value) if value != B256::ZERO => {
            return (
                Some(true),
                Some(Address::from_slice(&value.as_slice()[12..])),
                Some("eip1967".to_string()),
                Vec::new(),
                Vec::new(),
            );
        }
        Ok(_) => {}
        Err(error) => {
            return (
                None,
                None,
                None,
                vec![format!(
                    "proxy storage could not be read: {}",
                    error.message
                )],
                Vec::new(),
            );
        }
    }

    let proviable_slot: B256 = match EIP1822_PROXIABLE_SLOT.parse() {
        Ok(slot) => slot,
        Err(_) => {
            return (
                None,
                None,
                None,
                vec!["invalid EIP-1822 slot".to_string()],
                Vec::new(),
            );
        }
    };
    match rpc.storage_at(address, proviable_slot, tag).await {
        Ok(value) if value != B256::ZERO => {
            return (
                Some(true),
                Some(Address::from_slice(&value.as_slice()[12..])),
                Some("eip1822".to_string()),
                Vec::new(),
                Vec::new(),
            );
        }
        Ok(_) => {}
        Err(error) => {
            return (
                None,
                None,
                None,
                vec![format!(
                    "EIP-1822 storage could not be read: {}",
                    error.message
                )],
                Vec::new(),
            );
        }
    }

    let beacon_slot: B256 = match EIP1967_BEACON_SLOT.parse() {
        Ok(slot) => slot,
        Err(_) => {
            return (
                None,
                None,
                None,
                vec!["invalid beacon slot".to_string()],
                Vec::new(),
            );
        }
    };
    match rpc.storage_at(address, beacon_slot, tag).await {
        Ok(value) if value != B256::ZERO => {
            let beacon = Address::from_slice(&value.as_slice()[12..]);
            return match rpc
                .call_hex(beacon, "0x5c60da1b", tag)
                .await
                .ok()
                .and_then(|value| parse_topic_address(&value))
            {
                Some(implementation) => (
                    Some(true),
                    Some(implementation),
                    Some("eip1967-beacon".to_string()),
                    Vec::new(),
                    Vec::new(),
                ),
                None => (
                    Some(true),
                    None,
                    Some("eip1967-beacon".to_string()),
                    vec!["beacon implementation could not be read".to_string()],
                    Vec::new(),
                ),
            };
        }
        Ok(_) => {}
        Err(error) => {
            return (
                None,
                None,
                None,
                vec![format!(
                    "beacon storage could not be read: {}",
                    error.message
                )],
                Vec::new(),
            );
        }
    }

    (Some(false), None, None, Vec::new(), Vec::new())
}

fn minimal_proxy_implementation(code: &str) -> Option<Address> {
    let code = code.strip_prefix("0x").unwrap_or(code);
    let prefix = "363d3d373d3d3d363d73";
    let suffix = "5af43d82803e903d91602b57fd5bf3";
    if code.len() != 90
        || !code[..20].eq_ignore_ascii_case(prefix)
        || !code[60..].eq_ignore_ascii_case(suffix)
    {
        return None;
    }
    format!("0x{}", &code[20..60]).parse().ok()
}

const EIP1967_IMPLEMENTATION_SLOT: &str =
    "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc";
const EIP1967_BEACON_SLOT: &str =
    "0xa3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50";
const EIP1822_PROXIABLE_SLOT: &str =
    "0xc5f16f0fcc639fa48a6947836d9850f504798523bf8c9a3a87d5876cf622bcf7";

fn selector_is_ambiguous(selector: &str) -> bool {
    BUILTIN_SELECTORS
        .iter()
        .filter(|candidate| **candidate == selector)
        .count()
        > 1
}

fn decode_intent(data: &str) -> Result<DecodedIntent, CapabilityError> {
    let bytes = hex_to_bytes(data)?;
    if bytes.is_empty() {
        return Ok(DecodedIntent {
            intent: TransactionInspectIntent {
                selector: "0x".to_string(),
                signature: None,
                name: Some("native_transfer".to_string()),
                args: Vec::new(),
            },
            decode_coverage: "complete",
            unknowns: Vec::new(),
            warnings: Vec::new(),
        });
    }
    if bytes.len() < 4 {
        return Err(CapabilityError::invalid_argument(
            "calldata must be empty or contain a 4-byte selector",
        ));
    }

    let selector = format!("0x{}", data_encoding::HEXLOWER.encode(&bytes[..4]));
    if selector_is_ambiguous(&selector) {
        return Ok(DecodedIntent {
            intent: TransactionInspectIntent {
                selector: selector.clone(),
                signature: None,
                name: None,
                args: Vec::new(),
            },
            decode_coverage: "partial",
            unknowns: vec![format!("selector {selector} is ambiguous")],
            warnings: vec![format!(
                "selector {selector} appears more than once in the built-in registry"
            )],
        });
    }
    let payload = &bytes[4..];
    let (signature, name, specs) = match selector.as_str() {
        "0xa9059cbb" => ("transfer(address,uint256)", "transfer", &TRANSFER[..]),
        "0x23b872dd" => (
            "transferFrom(address,address,uint256)",
            "transferFrom",
            &TRANSFER_FROM[..],
        ),
        "0x095ea7b3" => ("approve(address,uint256)", "approve", &APPROVE[..]),
        "0x39509351" => (
            "increaseAllowance(address,uint256)",
            "increaseAllowance",
            &ALLOWANCE_CHANGE[..],
        ),
        "0xa457c2d7" => (
            "decreaseAllowance(address,uint256)",
            "decreaseAllowance",
            &ALLOWANCE_CHANGE[..],
        ),
        "0xa22cb465" => (
            "setApprovalForAll(address,bool)",
            "setApprovalForAll",
            &SET_APPROVAL_FOR_ALL[..],
        ),
        "0x42842e0e" => (
            "safeTransferFrom(address,address,uint256)",
            "safeTransferFrom",
            &SAFE_TRANSFER_FROM[..],
        ),
        "0xd0e30db0" => ("deposit()", "deposit", &[][..]),
        "0x2e1a7d4d" => ("withdraw(uint256)", "withdraw", &WITHDRAW[..]),
        "0xd505accf" => (
            "permit(address,address,uint256,uint256,uint8,bytes32,bytes32)",
            "permit",
            &PERMIT[..],
        ),
        "0xb88d4fde" => {
            return Ok(decode_with_dynamic_bytes(
                &selector,
                "safeTransferFrom(address,address,uint256,bytes)",
                "safeTransferFrom",
                &SAFE_TRANSFER_FROM,
                payload,
            ));
        }
        "0xf242432a" => {
            return Ok(decode_with_dynamic_bytes(
                &selector,
                "safeTransferFrom(address,address,uint256,uint256,bytes)",
                "safeTransferFrom",
                &ERC1155_SAFE_TRANSFER_FROM,
                payload,
            ));
        }
        "0xf2fde38b" => (
            "transferOwnership(address)",
            "transferOwnership",
            &TRANSFER_OWNERSHIP[..],
        ),
        "0x3659cfe6" => ("upgradeTo(address)", "upgradeTo", &UPGRADE_TO[..]),
        "0x38ed1739" => {
            return Ok(decode_swap_v2(
                &selector,
                "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)",
                "swapExactTokensForTokens",
                payload,
                true,
            ));
        }
        "0x7ff36ab5" => {
            return Ok(decode_swap_v2(
                &selector,
                "swapExactETHForTokens(uint256,address[],address,uint256)",
                "swapExactETHForTokens",
                payload,
                false,
            ));
        }
        "0x18cbafe5" => {
            return Ok(decode_swap_v2(
                &selector,
                "swapExactTokensForETH(uint256,uint256,address[],address,uint256)",
                "swapExactTokensForETH",
                payload,
                true,
            ));
        }
        "0x5c11d795" => {
            return Ok(decode_swap_v2(
                &selector,
                "swapExactTokensForTokensSupportingFeeOnTransferTokens(uint256,uint256,address[],address,uint256)",
                "swapExactTokensForTokensSupportingFeeOnTransferTokens",
                payload,
                true,
            ));
        }
        "0xb6f9de95" => {
            return Ok(decode_swap_v2(
                &selector,
                "swapExactETHForTokensSupportingFeeOnTransferTokens(uint256,address[],address,uint256)",
                "swapExactETHForTokensSupportingFeeOnTransferTokens",
                payload,
                false,
            ));
        }
        "0x791ac947" => {
            return Ok(decode_swap_v2(
                &selector,
                "swapExactTokensForETHSupportingFeeOnTransferTokens(uint256,uint256,address[],address,uint256)",
                "swapExactTokensForETHSupportingFeeOnTransferTokens",
                payload,
                true,
            ));
        }
        "0xac9650d8" => return Ok(decode_multicall(&selector, payload)),
        _ => {
            return Ok(DecodedIntent {
                intent: TransactionInspectIntent {
                    selector: selector.clone(),
                    signature: None,
                    name: None,
                    args: Vec::new(),
                },
                decode_coverage: "partial",
                unknowns: vec![format!(
                    "selector {selector} is not in the built-in ABI registry"
                )],
                warnings: Vec::new(),
            });
        }
    };

    match decode_args(payload, specs) {
        Ok(args) => Ok(DecodedIntent {
            intent: TransactionInspectIntent {
                selector,
                signature: Some(signature.to_string()),
                name: Some(name.to_string()),
                args,
            },
            decode_coverage: "complete",
            unknowns: Vec::new(),
            warnings: Vec::new(),
        }),
        Err(error) => Ok(DecodedIntent {
            intent: TransactionInspectIntent {
                selector: selector.clone(),
                signature: Some(signature.to_string()),
                name: Some(name.to_string()),
                args: Vec::new(),
            },
            decode_coverage: "partial",
            unknowns: vec![format!(
                "arguments for selector {selector} could not be decoded"
            )],
            warnings: vec![error],
        }),
    }
}

fn decode_multicall(selector: &str, payload: &[u8]) -> DecodedIntent {
    let signature = "multicall(bytes[])";
    let name = "multicall";
    let Some(array_offset) = payload.get(..32).and_then(word_to_usize) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "multicall array offset is missing".to_string(),
        );
    };
    let Some(array_start) = array_offset.checked_add(0) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "multicall array offset overflow".to_string(),
        );
    };
    let Some(length_word) = payload.get(array_start..array_start.saturating_add(32)) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "multicall array length is missing".to_string(),
        );
    };
    let Some(count) = word_to_usize(length_word) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "multicall array length is invalid".to_string(),
        );
    };
    if count > 256 {
        return known_but_partial(
            selector,
            signature,
            name,
            "multicall has too many nested calls".to_string(),
        );
    }

    let mut args = Vec::with_capacity(count);
    let mut warnings = Vec::new();
    let mut unknowns = Vec::new();
    for index in 0..count {
        let offset_pos = match array_start
            .checked_add(32)
            .and_then(|value| value.checked_add(index * 32))
        {
            Some(value) => value,
            None => {
                unknowns.push("multicall element offset overflow".to_string());
                break;
            }
        };
        let Some(offset) = payload
            .get(offset_pos..offset_pos + 32)
            .and_then(word_to_usize)
        else {
            unknowns.push(format!("multicall element {index} offset is invalid"));
            continue;
        };
        let Some(element_start) = array_start.checked_add(offset) else {
            unknowns.push(format!("multicall element {index} offset overflow"));
            continue;
        };
        let Some(length) = payload
            .get(element_start..element_start.saturating_add(32))
            .and_then(word_to_usize)
        else {
            unknowns.push(format!("multicall element {index} length is invalid"));
            continue;
        };
        let Some(data_start) = element_start.checked_add(32) else {
            unknowns.push(format!("multicall element {index} data offset overflow"));
            continue;
        };
        let Some(data_end) = data_start.checked_add(length) else {
            unknowns.push(format!("multicall element {index} data length overflow"));
            continue;
        };
        let Some(data) = payload.get(data_start..data_end) else {
            unknowns.push(format!("multicall element {index} exceeds calldata"));
            continue;
        };
        let call = format!("0x{}", data_encoding::HEXLOWER.encode(data));
        let nested = match decode_intent(&call) {
            Ok(decoded) => decoded,
            Err(error) => {
                unknowns.push(format!(
                    "multicall element {index} is invalid: {}",
                    error.message
                ));
                continue;
            }
        };
        if nested.decode_coverage != "complete" {
            warnings.push(format!(
                "multicall element {index} has partial decode coverage"
            ));
        }
        args.push(TransactionInspectArg {
            name: format!("call[{index}]"),
            kind: "bytes".to_string(),
            value: call,
        });
    }

    DecodedIntent {
        intent: TransactionInspectIntent {
            selector: selector.to_string(),
            signature: Some(signature.to_string()),
            name: Some(name.to_string()),
            args,
        },
        decode_coverage: if unknowns.is_empty() && warnings.is_empty() {
            "complete"
        } else {
            "partial"
        },
        unknowns,
        warnings,
    }
}

fn detect_nested_approvals(intent: &TransactionInspectIntent) -> Vec<TransactionInspectFinding> {
    let mut findings = Vec::new();
    for arg in &intent.args {
        if !arg.name.starts_with("call[") || arg.kind != "bytes" {
            continue;
        }
        let Ok(nested) = decode_intent(&arg.value) else {
            continue;
        };
        let mut nested_findings = detect_approvals(&nested.intent);
        for finding in &mut nested_findings {
            finding
                .evidence
                .insert("nested_call".to_string(), arg.name.clone());
        }
        findings.extend(nested_findings);
    }
    findings
}

fn decode_with_dynamic_bytes(
    selector: &str,
    signature: &str,
    name: &str,
    specs: &[ArgSpec],
    payload: &[u8],
) -> DecodedIntent {
    let static_len = specs.len() * 32;
    let Some(head) = payload.get(..static_len) else {
        return known_but_partial(
            selector,
            signature,
            name,
            format!(
                "expected at least {static_len} argument bytes, found {}",
                payload.len()
            ),
        );
    };
    let mut args = match decode_args(head, specs) {
        Ok(args) => args,
        Err(error) => return known_but_partial(selector, signature, name, error),
    };
    let Some(offset_word) = payload.get(static_len..static_len + 32) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "dynamic bytes offset is missing".to_string(),
        );
    };
    let Some(offset) = word_to_usize(offset_word) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "dynamic bytes offset is out of range".to_string(),
        );
    };
    if offset < static_len + 32 || offset + 32 > payload.len() {
        return known_but_partial(
            selector,
            signature,
            name,
            "dynamic bytes offset points outside calldata".to_string(),
        );
    }
    let Some(length) = word_to_usize(&payload[offset..offset + 32]) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "dynamic bytes length is out of range".to_string(),
        );
    };
    let Some(data_start) = offset.checked_add(32) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "dynamic bytes offset overflow".to_string(),
        );
    };
    let Some(data_end) = data_start.checked_add(length) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "dynamic bytes length overflow".to_string(),
        );
    };
    let Some(padded_length) = length.checked_add(31).map(|value| value / 32 * 32) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "dynamic bytes padding overflow".to_string(),
        );
    };
    let Some(padded_end) = data_start.checked_add(padded_length) else {
        return known_but_partial(
            selector,
            signature,
            name,
            "dynamic bytes padding overflow".to_string(),
        );
    };
    if padded_end > payload.len() {
        return known_but_partial(
            selector,
            signature,
            name,
            "dynamic bytes length exceeds calldata".to_string(),
        );
    }
    args.push(TransactionInspectArg {
        name: "data".to_string(),
        kind: "bytes".to_string(),
        value: format!(
            "0x{}",
            data_encoding::HEXLOWER.encode(&payload[data_start..data_end])
        ),
    });
    let mut warnings = Vec::new();
    if payload.len() > padded_end {
        warnings.push(format!(
            "{} trailing bytes after dynamic argument",
            payload.len() - padded_end
        ));
    }
    DecodedIntent {
        intent: TransactionInspectIntent {
            selector: selector.to_string(),
            signature: Some(signature.to_string()),
            name: Some(name.to_string()),
            args,
        },
        decode_coverage: if warnings.is_empty() {
            "complete"
        } else {
            "partial"
        },
        unknowns: Vec::new(),
        warnings,
    }
}

fn known_but_partial(
    selector: &str,
    signature: &str,
    name: &str,
    warning: String,
) -> DecodedIntent {
    DecodedIntent {
        intent: TransactionInspectIntent {
            selector: selector.to_string(),
            signature: Some(signature.to_string()),
            name: Some(name.to_string()),
            args: Vec::new(),
        },
        decode_coverage: "partial",
        unknowns: vec![format!(
            "arguments for selector {selector} could not be decoded"
        )],
        warnings: vec![warning],
    }
}

fn word_to_usize(word: &[u8]) -> Option<usize> {
    if word.len() != 32 {
        return None;
    }
    usize::try_from(U256::from_be_slice(word)).ok()
}

fn decode_args(payload: &[u8], specs: &[ArgSpec]) -> Result<Vec<TransactionInspectArg>, String> {
    let expected = specs.len() * 32;
    if payload.len() < expected {
        return Err(format!(
            "expected {expected} argument bytes, found {}",
            payload.len()
        ));
    }
    if payload.len() > expected {
        return Err(format!(
            "expected {expected} argument bytes, found {} trailing bytes",
            payload.len() - expected
        ));
    }

    specs
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            let word = &payload[index * 32..(index + 1) * 32];
            let value = match spec.arg_type {
                ArgType::Address => decode_address(word)?,
                ArgType::Uint => U256::from_be_slice(word).to_string(),
                ArgType::Uint8 => decode_uint8(word)?,
                ArgType::Bool => decode_bool(word)?,
                ArgType::Bytes32 => format!("0x{}", data_encoding::HEXLOWER.encode(word)),
            };
            Ok(TransactionInspectArg {
                name: spec.name.to_string(),
                kind: spec.kind.to_string(),
                value,
            })
        })
        .collect()
}

fn decode_address(word: &[u8]) -> Result<String, String> {
    if word[..12].iter().any(|byte| *byte != 0) {
        return Err("address argument has nonzero padding".to_string());
    }
    Ok(Address::from_slice(&word[12..]).to_string())
}

fn decode_uint8(word: &[u8]) -> Result<String, String> {
    if word[..31].iter().any(|byte| *byte != 0) {
        return Err("uint8 argument is out of range".to_string());
    }
    Ok(word[31].to_string())
}

fn decode_bool(word: &[u8]) -> Result<String, String> {
    if word[..31].iter().any(|byte| *byte != 0) {
        return Err("bool argument is out of range".to_string());
    }
    match word[31] {
        0 => Ok("false".to_string()),
        1 => Ok("true".to_string()),
        _ => Err("bool argument must be 0 or 1".to_string()),
    }
}

fn hex_to_bytes(data: &str) -> Result<Vec<u8>, CapabilityError> {
    let hex = data
        .strip_prefix("0x")
        .or_else(|| data.strip_prefix("0X"))
        .unwrap_or(data);
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let chars = hex.as_bytes();
    for pair in chars.as_chunks::<2>().0 {
        let high = hex_nibble(pair[0]).ok_or_else(invalid_hex)?;
        let low = hex_nibble(pair[1]).ok_or_else(invalid_hex)?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn invalid_hex() -> CapabilityError {
    CapabilityError::invalid_argument("data must be 0x-prefixed hex")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::extract::Json;
    use axum::routing::post;
    use serde_json::{Value, json};

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

    async fn rpc(Json(body): Json<Value>) -> Json<Value> {
        let result = match body["method"].as_str().unwrap_or_default() {
            "eth_blockNumber" => json!("0x64"),
            "eth_getBlockByNumber" => json!({
                "number": "0x64",
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000064"
            }),
            "eth_simulateV1" => {
                json!([{"calls": [{"status": "0x1", "logs": [], "returnData": "0x"}]}])
            }
            _ => json!("0x0"),
        };
        Json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    }

    async fn base_url() -> String {
        let app = Router::new().route("/rpc", post(rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    async fn metadata_rpc(Json(body): Json<Value>) -> Json<Value> {
        let method = body["method"].as_str().unwrap_or_default();
        let address = body["params"][0].as_str().unwrap_or_default();
        let result = match method {
            "eth_blockNumber" => json!("0x64"),
            "eth_getBlockByNumber" => json!({
                "number": "0x64",
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000064"
            }),
            "eth_getCode" if address.ends_with("01") => json!("0x"),
            "eth_getCode" => json!("0x6000"),
            "eth_getStorageAt" if address.ends_with("03") => {
                json!("0x0000000000000000000000000000000000000000000000000000000000000004")
            }
            "eth_getStorageAt" => {
                json!("0x0000000000000000000000000000000000000000000000000000000000000000")
            }
            _ => json!("0x0"),
        };
        Json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    }

    async fn metadata_base_url() -> String {
        let app = Router::new().route("/rpc", post(metadata_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    fn request_to(to: &str, data: &str) -> InspectRequest {
        InspectRequest {
            chain: Chain::Bsc,
            from: None,
            to: to.parse().unwrap(),
            data: data.to_string(),
            value: U256::ZERO,
            block: None,
        }
    }

    fn request(data: &str) -> InspectRequest {
        InspectRequest {
            chain: Chain::Bsc,
            from: None,
            to: "0x0000000000000000000000000000000000000001"
                .parse()
                .unwrap(),
            data: data.to_string(),
            value: U256::ZERO,
            block: None,
        }
    }

    #[tokio::test]
    async fn native_transfer_is_unknown_not_safe() {
        let base = base_url().await;
        let response = inspect(&config(&base), request("0x")).await.unwrap();
        assert_eq!(response.intent.selector, "0x");
        assert_eq!(response.intent.name.as_deref(), Some("native_transfer"));
        assert_eq!(response.risk, "unknown");
        assert_eq!(response.coverage.decode, "complete");
        assert_eq!(response.block_number, 100);
        assert!(response.explanation.summary.contains("no known findings"));
        assert!(!response.explanation.unknowns.is_empty());
        assert!(response.findings.is_empty());
        assert_eq!(response.coverage.simulation, "partial");
        assert!(
            response
                .unknowns
                .iter()
                .any(|item| item.contains("simulated logs"))
        );
    }

    #[tokio::test]
    async fn decodes_erc20_transfer_args() {
        let base = base_url().await;
        let data = "0xa9059cbb00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003";
        let response = inspect(&config(&base), request(data)).await.unwrap();
        assert_eq!(
            response.intent.signature.as_deref(),
            Some("transfer(address,uint256)")
        );
        assert_eq!(response.intent.args[0].name, "to");
        assert_eq!(
            response.intent.args[0].value,
            "0x0000000000000000000000000000000000000002"
        );
        assert_eq!(response.intent.args[1].value, "3");
        assert_eq!(response.coverage.decode, "complete");
    }

    #[tokio::test]
    async fn unknown_selector_stays_unknown() {
        let base = base_url().await;
        let response = inspect(&config(&base), request("0xdeadbeef"))
            .await
            .unwrap();
        assert!(response.intent.signature.is_none());
        assert!(response.intent.name.is_none());
        assert_eq!(response.risk, "unknown");
        assert_eq!(response.coverage.decode, "partial");
    }

    #[tokio::test]
    async fn rejects_short_calldata() {
        let base = base_url().await;
        let error = inspect(&config(&base), request("0x123456"))
            .await
            .unwrap_err();
        assert_eq!(
            error.code,
            crate::capability::CapabilityErrorCode::InvalidArgument
        );
    }

    fn address_word(address: &str) -> String {
        format!("{:0>64}", address.trim_start_matches("0x"))
    }

    fn uint_word(value: u64) -> String {
        format!("{value:064x}")
    }

    #[tokio::test]
    async fn decodes_erc721_safe_transfer_from_dynamic_bytes() {
        let base = base_url().await;
        let data = format!(
            "0xb88d4fde{}{}{}{}{}1234{}",
            address_word("0x0000000000000000000000000000000000000001"),
            address_word("0x0000000000000000000000000000000000000002"),
            uint_word(3),
            uint_word(128),
            uint_word(2),
            "00".repeat(30)
        );
        let response = inspect(&config(&base), request(&data)).await.unwrap();
        assert_eq!(response.coverage.decode, "complete");
        assert_eq!(
            response.intent.signature.as_deref(),
            Some("safeTransferFrom(address,address,uint256,bytes)")
        );
        assert_eq!(response.intent.args.len(), 4);
        assert_eq!(response.intent.args[3].value, "0x1234");
    }

    #[tokio::test]
    async fn decodes_erc1155_safe_transfer_from_empty_bytes() {
        let base = base_url().await;
        let data = format!(
            "0xf242432a{}{}{}{}{}{}",
            address_word("0x0000000000000000000000000000000000000001"),
            address_word("0x0000000000000000000000000000000000000002"),
            uint_word(7),
            uint_word(9),
            uint_word(160),
            uint_word(0)
        );
        let response = inspect(&config(&base), request(&data)).await.unwrap();
        assert_eq!(response.coverage.decode, "complete");
        assert_eq!(response.intent.args.len(), 5);
        assert_eq!(response.intent.args[4].value, "0x");
    }

    #[tokio::test]
    async fn malformed_dynamic_offset_is_partial() {
        let base = base_url().await;
        let data = format!(
            "0xb88d4fde{}{}{}{}",
            address_word("0x0000000000000000000000000000000000000001"),
            address_word("0x0000000000000000000000000000000000000002"),
            uint_word(3),
            uint_word(4096)
        );
        let response = inspect(&config(&base), request(&data)).await.unwrap();
        assert_eq!(response.coverage.decode, "partial");
        assert!(response.warnings[0].contains("outside calldata"));
    }

    #[tokio::test]
    async fn decodes_ownership_and_upgrade_selectors() {
        let base = base_url().await;
        let ownership = format!(
            "0xf2fde38b{}",
            address_word("0x0000000000000000000000000000000000000003")
        );
        let response = inspect(&config(&base), request(&ownership)).await.unwrap();
        assert_eq!(
            response.intent.signature.as_deref(),
            Some("transferOwnership(address)")
        );

        let upgrade = format!(
            "0x3659cfe6{}",
            address_word("0x0000000000000000000000000000000000000004")
        );
        let response = inspect(&config(&base), request(&upgrade)).await.unwrap();
        assert_eq!(
            response.intent.signature.as_deref(),
            Some("upgradeTo(address)")
        );
    }

    #[test]
    fn builtin_selectors_are_unique() {
        let mut selectors = BUILTIN_SELECTORS.to_vec();
        selectors.sort_unstable();
        selectors.dedup();
        assert_eq!(selectors.len(), BUILTIN_SELECTORS.len());
        assert!(!selector_is_ambiguous("0xa9059cbb"));
    }

    #[tokio::test]
    async fn distinguishes_eoa_and_contract() {
        let base = metadata_base_url().await;
        let eoa = inspect(
            &config(&base),
            request_to("0x0000000000000000000000000000000000000001", "0x"),
        )
        .await
        .unwrap();
        assert_eq!(eoa.target.kind, "eoa");
        assert_eq!(eoa.target.proxy, Some(false));
        assert_eq!(eoa.coverage.metadata, "complete");

        let contract = inspect(
            &config(&base),
            request_to("0x0000000000000000000000000000000000000002", "0x"),
        )
        .await
        .unwrap();
        assert_eq!(contract.target.kind, "contract");
        assert_eq!(contract.target.verified, None);
        assert_eq!(contract.target.proxy, Some(false));
        assert_eq!(contract.coverage.metadata, "partial");
        assert!(
            contract
                .unknowns
                .iter()
                .any(|item| item.contains("source verification"))
        );
    }

    #[tokio::test]
    async fn detects_eip1967_proxy_implementation() {
        let base = metadata_base_url().await;
        let response = inspect(
            &config(&base),
            request_to("0x0000000000000000000000000000000000000003", "0x"),
        )
        .await
        .unwrap();
        assert_eq!(response.target.kind, "contract");
        assert_eq!(response.target.proxy, Some(true));
        assert_eq!(
            response.target.implementation,
            Some(
                "0x0000000000000000000000000000000000000004"
                    .parse()
                    .unwrap()
            )
        );
    }

    async fn effects_rpc(Json(body): Json<Value>) -> Json<Value> {
        let result = match body["method"].as_str().unwrap_or_default() {
            "eth_blockNumber" => json!("0x64"),
            "eth_getBlockByNumber" => json!({
                "number": "0x64",
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000064"
            }),
            "eth_getCode" => json!("0x6000"),
            "eth_getStorageAt" => {
                json!("0x0000000000000000000000000000000000000000000000000000000000000000")
            }
            "eth_simulateV1" => json!([{
                "calls": [{
                    "status": "0x1",
                    "returnData": "0x",
                    "logs": [
                        {
                            "address": "0x0000000000000000000000000000000000000010",
                            "topics": [
                                "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925",
                                format!("0x{}", address_word("0x0000000000000000000000000000000000000001")),
                                format!("0x{}", address_word("0x0000000000000000000000000000000000000002"))
                            ],
                            "data": format!("0x{}", uint_word(100))
                        },
                        {
                            "address": "0x0000000000000000000000000000000000000020",
                            "topics": [
                                "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
                                format!("0x{}", address_word("0x0000000000000000000000000000000000000001")),
                                format!("0x{}", address_word("0x0000000000000000000000000000000000000002"))
                            ],
                            "data": format!("0x{}", uint_word(5))
                        }
                    ]
                }]
            }]),
            _ => json!("0x0"),
        };
        Json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    }

    #[tokio::test]
    async fn decodes_approval_and_transfer_logs_into_effects() {
        let app = Router::new().route("/rpc", post(effects_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let base = format!("http://{address}");
        let response = inspect(&config(&base), request("0x")).await.unwrap();
        assert_eq!(response.coverage.simulation, "partial");
        assert_eq!(response.effects.approvals.len(), 1);
        assert_eq!(response.effects.approvals[0].after, "100");
        assert_eq!(response.effects.tokens.len(), 2);
        assert_eq!(response.effects.tokens[0].delta, "-5");
        assert_eq!(response.effects.tokens[1].delta, "5");
        assert!(
            response
                .warnings
                .iter()
                .any(|item| item.contains("cross-checked"))
        );
    }

    #[tokio::test]
    async fn native_value_is_reported_as_intended_effect() {
        let base = base_url().await;
        let mut request = request("0x");
        request.from = Some(
            "0x0000000000000000000000000000000000000001"
                .parse()
                .unwrap(),
        );
        request.value = U256::from(1_000_000_000_000_000_000u64);
        let response = inspect(&config(&base), request).await.unwrap();
        assert_eq!(response.effects.native.len(), 2);
        assert_eq!(response.effects.native[0].delta, "-1000000000000000000");
        assert_eq!(response.effects.native[1].delta, "1000000000000000000");
    }

    #[tokio::test]
    async fn flags_unlimited_erc20_approval() {
        let base = base_url().await;
        let data = format!(
            "0x095ea7b3{}{:064x}",
            address_word("0x0000000000000000000000000000000000000002"),
            U256::MAX
        );
        let response = inspect(&config(&base), request(&data)).await.unwrap();
        assert_eq!(response.risk, "high");
        assert_eq!(response.findings.len(), 1);
        assert_eq!(response.findings[0].id, "AUTH-001");
        assert!(
            response
                .explanation
                .summary
                .contains("Unlimited token approval")
        );
        assert!(
            !response
                .explanation
                .summary
                .to_ascii_lowercase()
                .contains("safe")
        );
        assert!(!response.explanation.key_changes.is_empty());
        assert_eq!(response.findings[0].severity, "high");
        assert_eq!(
            response.findings[0].evidence.get("spender").unwrap(),
            "0x0000000000000000000000000000000000000002"
        );
    }

    #[tokio::test]
    async fn flags_increase_allowance_unlimited() {
        let base = base_url().await;
        let data = format!(
            "0x39509351{}{:064x}",
            address_word("0x0000000000000000000000000000000000000002"),
            U256::MAX
        );
        let response = inspect(&config(&base), request(&data)).await.unwrap();
        assert_eq!(response.risk, "high");
        assert_eq!(response.findings[0].id, "AUTH-001");
    }

    #[tokio::test]
    async fn flags_nft_operator_approval() {
        let base = base_url().await;
        let data = format!(
            "0xa22cb465{}{}",
            address_word("0x0000000000000000000000000000000000000002"),
            uint_word(1)
        );
        let response = inspect(&config(&base), request(&data)).await.unwrap();
        assert_eq!(response.risk, "high");
        assert_eq!(response.findings.len(), 1);
        assert_eq!(response.findings[0].id, "AUTH-004");
        assert_eq!(response.findings[0].evidence.get("amount").unwrap(), "all");
    }

    #[tokio::test]
    async fn finite_approval_has_no_finding_yet() {
        let base = base_url().await;
        let data = format!(
            "0x095ea7b3{}{}",
            address_word("0x0000000000000000000000000000000000000002"),
            uint_word(100)
        );
        let response = inspect(&config(&base), request(&data)).await.unwrap();
        assert!(response.findings.is_empty());
        assert_eq!(response.risk, "unknown");
    }

    fn transfer_intent(recipient: &str, amount: u64) -> TransactionInspectIntent {
        let data = format!("0xa9059cbb{}{}", address_word(recipient), uint_word(amount));
        decode_intent(&data).unwrap().intent
    }

    fn token_effect(
        token: Address,
        account: Address,
        delta: &str,
    ) -> TransactionInspectTokenEffect {
        TransactionInspectTokenEffect {
            token,
            account,
            delta: delta.to_string(),
        }
    }

    fn flow_request() -> InspectRequest {
        InspectRequest {
            chain: Chain::Bsc,
            from: Some(
                "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
            ),
            to: "0x0000000000000000000000000000000000000010"
                .parse()
                .unwrap(),
            data: "0x".to_string(),
            value: U256::ZERO,
            block: None,
        }
    }

    #[test]
    fn flags_token_outflow_during_approval() {
        let data = format!(
            "0x095ea7b3{}{}",
            address_word("0x0000000000000000000000000000000000000002"),
            uint_word(100)
        );
        let intent = decode_intent(&data).unwrap().intent;
        let request = flow_request();
        let mut effects = TransactionInspectEffects::default();
        effects
            .tokens
            .push(token_effect(request.to, request.from.unwrap(), "-5"));
        let findings = detect_asset_flow(&intent, &request, &effects, "complete");
        assert!(findings.iter().any(|finding| finding.id == "FLOW-003"));
    }

    #[test]
    fn flags_unexpected_transfer_recipient() {
        let intent = transfer_intent("0x0000000000000000000000000000000000000002", 5);
        let request = flow_request();
        let mut effects = TransactionInspectEffects::default();
        effects
            .tokens
            .push(token_effect(request.to, request.from.unwrap(), "-5"));
        effects.tokens.push(token_effect(
            request.to,
            "0x0000000000000000000000000000000000000003"
                .parse()
                .unwrap(),
            "5",
        ));
        let findings = detect_asset_flow(&intent, &request, &effects, "complete");
        assert!(findings.iter().any(|finding| finding.id == "FLOW-004"));
    }

    #[test]
    fn flags_fee_on_transfer() {
        let intent = transfer_intent("0x0000000000000000000000000000000000000002", 10);
        let request = flow_request();
        let mut effects = TransactionInspectEffects::default();
        effects
            .tokens
            .push(token_effect(request.to, request.from.unwrap(), "-10"));
        effects.tokens.push(token_effect(
            request.to,
            "0x0000000000000000000000000000000000000002"
                .parse()
                .unwrap(),
            "9",
        ));
        let findings = detect_asset_flow(&intent, &request, &effects, "complete");
        assert!(findings.iter().any(|finding| finding.id == "FLOW-005"));
    }

    #[test]
    fn flags_outflow_above_decoded_amount() {
        let intent = transfer_intent("0x0000000000000000000000000000000000000002", 5);
        let request = flow_request();
        let mut effects = TransactionInspectEffects::default();
        effects
            .tokens
            .push(token_effect(request.to, request.from.unwrap(), "-6"));
        let findings = detect_asset_flow(&intent, &request, &effects, "complete");
        assert!(findings.iter().any(|finding| finding.id == "FLOW-002"));
    }

    #[test]
    fn flags_native_value_on_approval() {
        let data = format!(
            "0x095ea7b3{}{}",
            address_word("0x0000000000000000000000000000000000000002"),
            uint_word(100)
        );
        let intent = decode_intent(&data).unwrap().intent;
        let mut request = flow_request();
        request.value = U256::from(1u64);
        let findings = detect_asset_flow(
            &intent,
            &request,
            &TransactionInspectEffects::default(),
            "complete",
        );
        assert!(findings.iter().any(|finding| finding.id == "FLOW-001"));
    }

    fn contract_target(proxy: bool) -> TransactionInspectTarget {
        TransactionInspectTarget {
            address: "0x0000000000000000000000000000000000000010"
                .parse()
                .unwrap(),
            kind: "contract".to_string(),
            verified: None,
            proxy: Some(proxy),
            implementation: proxy.then(|| {
                "0x0000000000000000000000000000000000000020"
                    .parse()
                    .unwrap()
            }),
            proxy_kind: proxy.then(|| "eip1967".to_string()),
        }
    }

    #[test]
    fn flags_unverified_contract() {
        let intent = decode_intent("0x").unwrap().intent;
        let request = flow_request();
        let mut target = contract_target(false);
        target.verified = Some(false);
        let findings = detect_contract(&intent, &target, &request);
        assert!(findings.iter().any(|finding| finding.id == "CONTRACT-001"));
        assert_eq!(findings[0].severity, "medium");
    }

    #[test]
    fn flags_proxy_upgrade_as_high() {
        let data = format!(
            "0x3659cfe6{}",
            address_word("0x0000000000000000000000000000000000000020")
        );
        let intent = decode_intent(&data).unwrap().intent;
        let findings = detect_contract(&intent, &contract_target(true), &flow_request());
        assert!(
            findings
                .iter()
                .any(|finding| finding.id == "CONTRACT-003" && finding.severity == "high")
        );
    }

    #[test]
    fn flags_proxy_target_as_low() {
        let data = format!(
            "0x095ea7b3{}{}",
            address_word("0x0000000000000000000000000000000000000002"),
            uint_word(100)
        );
        let intent = decode_intent(&data).unwrap().intent;
        let findings = detect_contract(&intent, &contract_target(true), &flow_request());
        assert!(
            findings
                .iter()
                .any(|finding| finding.id == "CONTRACT-003" && finding.severity == "low")
        );
    }

    fn multicall_data(calls: &[&str]) -> String {
        let calls = calls
            .iter()
            .map(|call| hex_to_bytes(call).unwrap())
            .collect::<Vec<_>>();
        let mut payload = Vec::new();
        payload.extend_from_slice(&[0u8; 31]);
        payload.push(32);
        payload.extend_from_slice(&[0u8; 31]);
        payload.push(calls.len() as u8);

        let header_size = 32 + calls.len() * 32;
        let mut offset = header_size;
        for call in &calls {
            let mut offset_word = [0u8; 32];
            offset_word[30] = (offset >> 8) as u8;
            offset_word[31] = offset as u8;
            payload.extend_from_slice(&offset_word);
            offset += 32 + call.len().div_ceil(32) * 32;
        }
        for call in &calls {
            payload.extend_from_slice(&[0u8; 31]);
            payload.push(call.len() as u8);
            payload.extend_from_slice(call);
            let padding = call.len().div_ceil(32) * 32 - call.len();
            payload.extend(std::iter::repeat_n(0u8, padding));
        }
        let mut data = vec![0xac, 0x96, 0x50, 0xd8];
        data.extend_from_slice(&payload);
        format!("0x{}", data_encoding::HEXLOWER.encode(&data))
    }

    #[test]
    fn decodes_multicall_and_nested_approval() {
        let approve = format!(
            "0x095ea7b3{}{:064x}",
            address_word("0x0000000000000000000000000000000000000002"),
            U256::MAX
        );
        let data = multicall_data(&[approve.as_str()]);
        let decoded = decode_intent(&data).unwrap();
        assert_eq!(decoded.decode_coverage, "complete");
        assert_eq!(
            decoded.intent.signature.as_deref(),
            Some("multicall(bytes[])")
        );
        assert_eq!(decoded.intent.args.len(), 1);
        let nested = decode_intent(&decoded.intent.args[0].value).unwrap();
        assert_eq!(
            nested.intent.signature.as_deref(),
            Some("approve(address,uint256)")
        );
    }

    #[tokio::test]
    async fn nested_approval_produces_auth_finding() {
        let base = base_url().await;
        let approve = format!(
            "0x095ea7b3{}{:064x}",
            address_word("0x0000000000000000000000000000000000000002"),
            U256::MAX
        );
        let data = multicall_data(&[approve.as_str()]);
        let response = inspect(&config(&base), request(&data)).await.unwrap();
        assert_eq!(response.risk, "high");
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "AUTH-001")
        );
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.evidence.contains_key("nested_call"))
        );
    }

    #[test]
    fn malformed_multicall_offset_is_partial() {
        let data = "0xac9650d8".to_string()
            + &format!("{:064x}", 32)
            + &format!("{:064x}", 1)
            + &format!("{:064x}", 4096);
        let decoded = decode_intent(&data).unwrap();
        assert_eq!(decoded.decode_coverage, "partial");
        assert!(!decoded.unknowns.is_empty());
    }

    #[test]
    fn flags_zero_target() {
        let intent = decode_intent("0x").unwrap().intent;
        let mut request = flow_request();
        request.to = Address::ZERO;
        let findings = detect_address(&intent, &request);
        assert!(findings.iter().any(|finding| finding.id == "ADDR-001"));
    }

    #[test]
    fn flags_burn_recipient() {
        let intent = transfer_intent("0x000000000000000000000000000000000000dEaD", 1);
        let request = flow_request();
        let findings = detect_address(&intent, &request);
        assert!(findings.iter().any(|finding| finding.id == "ADDR-002"));
    }

    #[test]
    fn flags_self_target_as_low() {
        let intent = decode_intent("0x").unwrap().intent;
        let mut request = flow_request();
        request.to = request.from.unwrap();
        let findings = detect_address(&intent, &request);
        assert!(
            findings
                .iter()
                .any(|finding| finding.id == "ADDR-005" && finding.severity == "low")
        );
    }

    fn swap_v2_data(
        selector: &str,
        token_in: bool,
        amount_in: u64,
        amount_out_min: u64,
        recipient: &str,
        path: &[&str],
    ) -> String {
        let mut payload = String::new();
        if token_in {
            payload.push_str(&uint_word(amount_in));
        }
        payload.push_str(&uint_word(amount_out_min));
        let static_words = if token_in { 5 } else { 4 };
        payload.push_str(&uint_word(static_words * 32));
        payload.push_str(&address_word(recipient));
        payload.push_str(&uint_word(u64::MAX));
        payload.push_str(&uint_word(path.len() as u64));
        for token in path {
            payload.push_str(&address_word(token));
        }
        format!("0x{selector}{payload}")
    }

    #[test]
    fn decodes_v2_swap_path_and_checks_min_out() {
        let data = swap_v2_data(
            "38ed1739",
            true,
            1000,
            0,
            "0x0000000000000000000000000000000000000001",
            &[
                "0x0000000000000000000000000000000000000010",
                "0x0000000000000000000000000000000000000020",
            ],
        );
        let decoded = decode_intent(&data).unwrap();
        assert_eq!(decoded.decode_coverage, "complete");
        assert_eq!(decoded.intent.args.len(), 5);
        assert_eq!(decoded.intent.args[2].kind, "address[]");
        assert!(
            decoded.intent.args[2]
                .value
                .contains("0x0000000000000000000000000000000000000020")
        );

        let mut request = flow_request();
        request.to = "0x10ED43C718714eb63d5aA57B78B54704E256024E"
            .parse()
            .unwrap();
        let findings = detect_swap(&decoded.intent, &request);
        assert!(findings.iter().any(|finding| finding.id == "SWAP-001"));
        assert!(!findings.iter().any(|finding| finding.id == "SWAP-007"));
    }

    #[test]
    fn flags_swap_recipient_mismatch_and_unknown_router() {
        let data = swap_v2_data(
            "38ed1739",
            true,
            1000,
            1,
            "0x0000000000000000000000000000000000000002",
            &[
                "0x0000000000000000000000000000000000000010",
                "0x0000000000000000000000000000000000000020",
            ],
        );
        let intent = decode_intent(&data).unwrap().intent;
        let request = flow_request();
        let findings = detect_swap(&intent, &request);
        assert!(findings.iter().any(|finding| finding.id == "SWAP-004"));
        assert!(findings.iter().any(|finding| finding.id == "SWAP-007"));
    }

    #[test]
    fn decodes_eth_swap_without_amount_in() {
        let data = swap_v2_data(
            "7ff36ab5",
            false,
            0,
            1,
            "0x0000000000000000000000000000000000000001",
            &[
                "0x0000000000000000000000000000000000000010",
                "0x0000000000000000000000000000000000000020",
            ],
        );
        let decoded = decode_intent(&data).unwrap();
        assert_eq!(decoded.intent.args.len(), 4);
        assert_eq!(decoded.intent.args[0].name, "amountOutMin");
    }

    fn permit_data(value: U256, deadline: u64) -> String {
        format!(
            "0xd505accf{}{}{:064x}{}{}{}{}",
            address_word("0x0000000000000000000000000000000000000001"),
            address_word("0x0000000000000000000000000000000000000002"),
            value,
            uint_word(deadline),
            uint_word(27),
            "11".repeat(32),
            "22".repeat(32)
        )
    }

    #[tokio::test]
    async fn flags_unlimited_permit() {
        let base = base_url().await;
        let response = inspect(&config(&base), request(&permit_data(U256::MAX, 100)))
            .await
            .unwrap();
        assert_eq!(response.risk, "high");
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "SIG-003")
        );
    }

    #[tokio::test]
    async fn flags_expired_permit_deadline() {
        let base = base_url().await;
        let response = inspect(&config(&base), request(&permit_data(U256::from(100u64), 0)))
            .await
            .unwrap();
        assert_eq!(response.risk, "high");
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "SIG-002")
        );
    }

    #[tokio::test]
    async fn normal_permit_has_no_signature_finding() {
        let base = base_url().await;
        let response = inspect(
            &config(&base),
            request(&permit_data(U256::from(100u64), 999)),
        )
        .await
        .unwrap();
        assert!(
            !response
                .findings
                .iter()
                .any(|finding| finding.category == "signature")
        );
    }

    #[tokio::test]
    async fn policy_enforces_native_spend_cap() {
        let base = base_url().await;
        let mut request = flow_request();
        request.value = U256::from(2_000_000_000_000_000_000u64);
        let policy = InspectPolicy {
            mode: "advisory".to_string(),
            max_native_spend: Some("1".to_string()),
            ..Default::default()
        };
        let response = inspect_with_policy(&config(&base), request, Some(policy))
            .await
            .unwrap();
        assert_eq!(response.risk, "high");
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "POLICY-002")
        );
        assert_eq!(response.coverage.policy, "complete");
    }

    #[tokio::test]
    async fn policy_checks_target_and_method_allowlists() {
        let base = base_url().await;
        let intent = transfer_intent("0x0000000000000000000000000000000000000002", 1);
        let data = format!(
            "0xa9059cbb{}{}",
            address_word("0x0000000000000000000000000000000000000002"),
            uint_word(1)
        );
        let policy = InspectPolicy {
            mode: "advisory".to_string(),
            target_allowlist: vec![
                "0x0000000000000000000000000000000000000099"
                    .parse()
                    .unwrap(),
            ],
            method_allowlist: vec!["approve(address,uint256)".to_string()],
            ..Default::default()
        };
        let response = inspect_with_policy(&config(&base), request(&data), Some(policy))
            .await
            .unwrap();
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "POLICY-004")
        );
        assert_eq!(intent.name.as_deref(), Some("transfer"));
    }

    #[tokio::test]
    async fn policy_absent_is_not_configured() {
        let base = base_url().await;
        let response = inspect(&config(&base), request("0x")).await.unwrap();
        assert_eq!(response.coverage.policy, "not_configured");
    }

    #[test]
    fn fixture_corpus_matches_expected_findings() {
        use std::collections::BTreeSet;

        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../tests/fixtures/transaction_inspect/cases.json"
        ))
        .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let data = case["data"].as_str().unwrap();
            let decoded = decode_intent(data).unwrap_or_else(|error| {
                panic!("{name}: decode failed: {error:?}");
            });
            assert_eq!(
                decoded.decode_coverage,
                case["expected_decode"].as_str().unwrap(),
                "{name}: decode coverage"
            );
            assert_eq!(
                decoded.intent.signature.as_deref(),
                case["expected_signature"].as_str(),
                "{name}: signature"
            );

            let request = InspectRequest {
                chain: Chain::Bsc,
                from: Some(
                    "0x0000000000000000000000000000000000000001"
                        .parse()
                        .unwrap(),
                ),
                to: case["to"].as_str().unwrap().parse().unwrap(),
                data: data.to_string(),
                value: U256::ZERO,
                block: None,
            };
            let target = TransactionInspectTarget {
                address: request.to,
                kind: "contract".to_string(),
                verified: None,
                proxy: None,
                implementation: None,
                proxy_kind: None,
            };
            let mut findings = detect_approvals(&decoded.intent);
            findings.extend(detect_nested_approvals(&decoded.intent));
            findings.extend(detect_signature(&decoded.intent));
            findings.extend(detect_contract(&decoded.intent, &target, &request));
            findings.extend(detect_address(&decoded.intent, &request));
            findings.extend(detect_swap(&decoded.intent, &request));

            let actual = findings
                .iter()
                .map(|finding| finding.id.as_str())
                .collect::<BTreeSet<_>>();
            let expected = case["expected_findings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<BTreeSet<_>>();
            assert_eq!(actual, expected, "{name}: findings");
        }
    }

    async fn verification_rpc(Json(body): Json<Value>) -> Json<Value> {
        let method = body["method"].as_str().unwrap_or_default();
        let result = match method {
            "eth_blockNumber" => json!("0x64"),
            "eth_getBlockByNumber" => json!({
                "number": "0x64",
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000064"
            }),
            "eth_getCode" => json!("0x6000"),
            "eth_getStorageAt" => {
                json!("0x0000000000000000000000000000000000000000000000000000000000000000")
            }
            "eth_call" => {
                let data = body["params"][0]["data"].as_str().unwrap_or_default();
                let balance = if data.ends_with("0000000000000000000000000000000000000001") {
                    1000
                } else {
                    0
                };
                json!(format!("0x{}", uint_word(balance)))
            }
            "eth_simulateV1" => {
                let calls = body["params"][0]["blockStateCalls"][0]["calls"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let log = json!({
                    "address": "0x0000000000000000000000000000000000000010",
                    "topics": [
                        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
                        format!("0x{}", address_word("0x0000000000000000000000000000000000000001")),
                        format!("0x{}", address_word("0x0000000000000000000000000000000000000002"))
                    ],
                    "data": format!("0x{}", uint_word(100))
                });
                let mut results = vec![json!({
                    "status": "0x1",
                    "returnData": "0x",
                    "logs": [log]
                })];
                for _ in calls.iter().skip(1) {
                    results.push(json!({
                        "status": "0x1",
                        "returnData": format!("0x{}", uint_word(1000)),
                        "logs": []
                    }));
                }
                json!([{"calls": results}])
            }
            _ => json!("0x0"),
        };
        Json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    }

    #[tokio::test]
    async fn flags_log_balance_mismatch() {
        let app = Router::new().route("/rpc", post(verification_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let base = format!("http://{address}");
        let mut request = request("0x");
        request.from = Some(
            "0x0000000000000000000000000000000000000001"
                .parse()
                .unwrap(),
        );
        request.to = "0x0000000000000000000000000000000000000010"
            .parse()
            .unwrap();
        let response = inspect(&config(&base), request).await.unwrap();
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "EFFECT-007")
        );
        assert_eq!(response.risk, "high");
    }

    async fn trace_rpc(Json(body): Json<Value>) -> Json<Value> {
        let result = match body["method"].as_str().unwrap_or_default() {
            "eth_blockNumber" => json!("0x64"),
            "eth_getBlockByNumber" => json!({
                "number": "0x64",
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000064"
            }),
            "eth_getCode" => json!("0x6000"),
            "eth_getStorageAt" => {
                json!("0x0000000000000000000000000000000000000000000000000000000000000000")
            }
            "eth_simulateV1" => {
                json!([{"calls": [{"status": "0x1", "logs": [], "returnData": "0x"}]}])
            }
            "debug_traceCall" => json!({
                "type": "CALL",
                "from": "0x0000000000000000000000000000000000000001",
                "to": "0x0000000000000000000000000000000000000010",
                "input": "0x",
                "calls": [{
                    "type": "DELEGATECALL",
                    "from": "0x0000000000000000000000000000000000000010",
                    "to": "0x0000000000000000000000000000000000000099",
                    "input": "0x12345678"
                }]
            }),
            _ => json!("0x0"),
        };
        Json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    }

    #[tokio::test]
    async fn flags_internal_delegatecall_from_trace() {
        let app = Router::new().route("/rpc", post(trace_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let base = format!("http://{address}");
        let response = inspect(&config(&base), request("0x")).await.unwrap();
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "CONTRACT-004")
        );
        assert_eq!(response.coverage.trace, "complete");
    }

    async fn source_verified() -> Json<Value> {
        Json(json!({
            "status": "1",
            "message": "OK",
            "result": [{
                "SourceCode": "contract Test {}",
                "ABI": "[]",
                "Proxy": "0",
                "Implementation": ""
            }]
        }))
    }

    async fn source_unverified() -> Json<Value> {
        Json(json!({
            "status": "1",
            "message": "OK",
            "result": [{
                "SourceCode": "",
                "ABI": "Contract source code not verified",
                "Proxy": "0",
                "Implementation": ""
            }]
        }))
    }

    async fn source_base_url(verified: bool) -> String {
        let app = if verified {
            Router::new().route("/api", axum::routing::get(source_verified))
        } else {
            Router::new().route("/api", axum::routing::get(source_unverified))
        }
        .route("/rpc", post(rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    fn verifier(base: &str) -> SourceVerifier {
        SourceVerifier {
            url: format!("{base}/api"),
            api_key: "test".to_string(),
            label: "testscan".to_string(),
        }
    }

    #[tokio::test]
    async fn configures_source_verifier_as_verified() {
        let base = source_base_url(true).await;
        let response = inspect_with_verifier(&config(&base), request("0x"), Some(verifier(&base)))
            .await
            .unwrap();
        assert_eq!(response.target.verified, Some(true));
        assert_eq!(response.coverage.metadata, "complete");
        assert!(
            response
                .sources
                .iter()
                .any(|source| source.name == "testscan")
        );
    }

    #[tokio::test]
    async fn unverified_source_produces_contract_finding() {
        let base = source_base_url(false).await;
        let response = inspect_with_verifier(&config(&base), request("0x"), Some(verifier(&base)))
            .await
            .unwrap();
        assert_eq!(response.target.verified, Some(false));
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "CONTRACT-001")
        );
    }

    #[tokio::test]
    async fn report_id_is_stable_and_assurance_is_explicit() {
        let base = base_url().await;
        let first = inspect(&config(&base), request("0x")).await.unwrap();
        let second = inspect(&config(&base), request("0x")).await.unwrap();
        assert_eq!(first.assurance.report_id, second.assurance.report_id);
        assert_eq!(first.assurance.ruleset_version, "2026-11-11.1");
        assert_eq!(first.assurance.cross_check, "not_configured");
        assert!(first.assurance.deterministic);
        assert!(!first.assurance.limitations.is_empty());
        assert_eq!(first.assurance.report_id.len(), 66);
    }

    #[tokio::test]
    async fn secondary_rpc_cross_check_matches() {
        let primary_base = base_url().await;
        let secondary_base = base_url().await;
        let primary = Rpc::new(&format!("{primary_base}/rpc"));
        let secondary_url = format!("{secondary_base}/rpc");
        let (number, hash, tag) = primary.block(None).await.unwrap();
        let result = cross_check(
            &primary,
            Some(&secondary_url),
            request("0x").to,
            &tag,
            number,
            hash,
        )
        .await;
        assert_eq!(result.status, "match");
    }

    async fn mismatch_rpc(Json(body): Json<Value>) -> Json<Value> {
        let result = match body["method"].as_str().unwrap_or_default() {
            "eth_blockNumber" => json!("0x64"),
            "eth_getBlockByNumber" => json!({
                "number": "0x64",
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000065"
            }),
            "eth_getCode" => json!("0x6000"),
            _ => json!("0x0"),
        };
        Json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    }

    async fn mismatch_base_url() -> String {
        let app = Router::new().route("/rpc", post(mismatch_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn secondary_rpc_cross_check_detects_mismatch() {
        let primary_base = base_url().await;
        let mismatch_base = mismatch_base_url().await;
        let primary = Rpc::new(&format!("{primary_base}/rpc"));
        let (number, hash, tag) = primary.block(None).await.unwrap();
        let result = cross_check(
            &primary,
            Some(&format!("{mismatch_base}/rpc")),
            request("0x").to,
            &tag,
            number,
            hash,
        )
        .await;
        assert_eq!(result.status, "mismatch");
    }

    #[tokio::test]
    async fn secondary_rpc_cross_check_reports_unavailable() {
        let primary_base = base_url().await;
        let primary = Rpc::new(&format!("{primary_base}/rpc"));
        let (number, hash, tag) = primary.block(None).await.unwrap();
        let result = cross_check(
            &primary,
            Some("http://127.0.0.1:1"),
            request("0x").to,
            &tag,
            number,
            hash,
        )
        .await;
        assert_eq!(result.status, "unavailable");
    }

    #[tokio::test]
    async fn enforce_policy_blocks_high_findings() {
        let base = base_url().await;
        let mut request = flow_request();
        request.value = U256::from(2_000_000_000_000_000_000u64);
        let policy = InspectPolicy {
            mode: "enforce".to_string(),
            max_native_spend: Some("1".to_string()),
            ..Default::default()
        };
        let response = inspect_with_policy(&config(&base), request, Some(policy))
            .await
            .unwrap();
        assert_eq!(response.policy.mode, "enforce");
        assert_eq!(response.policy.decision, "block");
        assert!(response.policy.blocked);
        assert!(!response.policy.reasons.is_empty());
    }

    #[tokio::test]
    async fn enforce_policy_allows_clean_transaction() {
        let base = base_url().await;
        let policy = InspectPolicy {
            mode: "enforce".to_string(),
            max_native_spend: Some("1".to_string()),
            ..Default::default()
        };
        let response = inspect_with_policy(&config(&base), request("0x"), Some(policy))
            .await
            .unwrap();
        assert_eq!(response.policy.decision, "allow");
        assert!(!response.policy.blocked);
    }

    #[tokio::test]
    async fn rejects_oversized_calldata() {
        let data = format!("0x{}", "00".repeat(MAX_DATA_BYTES + 1));
        let error = inspect(&config("http://unused"), request(&data))
            .await
            .unwrap_err();
        assert_eq!(
            error.code,
            crate::capability::CapabilityErrorCode::InvalidArgument
        );
        assert!(error.message.contains("262144"));
    }

    #[test]
    fn detects_eip1167_minimal_proxy_bytecode() {
        let implementation = "1234567890abcdef1234567890abcdef12345678";
        let code = format!("0x363d3d373d3d3d363d73{implementation}5af43d82803e903d91602b57fd5bf3");
        assert_eq!(
            minimal_proxy_implementation(&code),
            Some(format!("0x{implementation}").parse().unwrap())
        );
        assert!(minimal_proxy_implementation("0x6000").is_none());
    }

    async fn eip1822_rpc(Json(body): Json<Value>) -> Json<Value> {
        let method = body["method"].as_str().unwrap_or_default();
        let result = match method {
            "eth_blockNumber" => json!("0x64"),
            "eth_getBlockByNumber" => json!({
                "number": "0x64",
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000064"
            }),
            "eth_getCode" => json!("0x6000"),
            "eth_getStorageAt" => {
                let slot = body["params"][1].as_str().unwrap_or_default();
                if slot.eq_ignore_ascii_case(EIP1822_PROXIABLE_SLOT) {
                    json!("0x0000000000000000000000000000000000000000000000000000000000000020")
                } else {
                    json!("0x0000000000000000000000000000000000000000000000000000000000000000")
                }
            }
            "eth_simulateV1" => {
                json!([{"calls": [{"status": "0x1", "logs": [], "returnData": "0x"}]}])
            }
            "debug_traceCall" => json!({
                "type": "CALL",
                "from": "0x0000000000000000000000000000000000000001",
                "to": "0x0000000000000000000000000000000000000010",
                "input": "0x"
            }),
            _ => json!("0x0"),
        };
        Json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    }

    #[tokio::test]
    async fn detects_eip1822_proxy_slot() {
        let app = Router::new().route("/rpc", post(eip1822_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let base = format!("http://{address}");
        let response = inspect(&config(&base), request("0x")).await.unwrap();
        assert_eq!(response.target.proxy, Some(true));
        assert_eq!(response.target.proxy_kind.as_deref(), Some("eip1822"));
        assert_eq!(
            response.target.implementation,
            Some(
                "0x0000000000000000000000000000000000000020"
                    .parse()
                    .unwrap()
            )
        );
    }

    #[test]
    fn arbitrary_short_calldata_does_not_panic() {
        for length in 0..64 {
            let bytes = (0..length)
                .map(|index| ((index * 37 + 11) % 256) as u8)
                .collect::<Vec<_>>();
            let data = format!("0x{}", data_encoding::HEXLOWER.encode(&bytes));
            let _ = decode_intent(&data);
        }
    }

    #[tokio::test]
    async fn malformed_arguments_are_partial() {
        let base = base_url().await;
        let response = inspect(&config(&base), request("0xa9059cbb00"))
            .await
            .unwrap();
        assert_eq!(response.coverage.decode, "partial");
        assert!(response.intent.args.is_empty());
        assert!(response.warnings[0].contains("expected 64"));
    }
}
