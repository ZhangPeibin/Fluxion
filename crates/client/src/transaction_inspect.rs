//! Deterministic pre-sign transaction inspection.
//!
//! The first slice decodes calldata and pins a block. It deliberately does not
//! claim safety: metadata, simulation and policy are reported as not run.

use std::collections::BTreeMap;

use alloy_primitives::{Address, B256, U256};
use serde_json::{Value, json};

use crate::capability::{Amount, CapabilityError};
use crate::chain::Chain;
use crate::transaction::{Rpc, normalize_data};
use crate::wallet::WalletConfig;

pub use fluxion_protocol::transaction::{
    TransactionInspectApprovalEffect, TransactionInspectArg, TransactionInspectCoverage,
    TransactionInspectEffects, TransactionInspectFinding, TransactionInspectIntent,
    TransactionInspectNativeEffect, TransactionInspectNftEffect, TransactionInspectResponse,
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
    let data = normalize_data(&request.data)?;
    let rpc = Rpc::new(&config.rpc_url_for(request.chain));
    let (block_number, block_hash, tag) = rpc.block(request.block).await?;
    let decoded = decode_intent(&data)?;
    let metadata = inspect_target(&rpc, request.to, &tag).await?;
    let simulation = inspect_effects(&rpc, &request, &data, &tag).await;
    let mut unknowns = decoded.unknowns;
    unknowns.extend(metadata.unknowns);
    unknowns.extend(simulation.unknowns);
    unknowns.push("no policy was configured".to_string());
    let mut warnings = decoded.warnings;
    warnings.extend(metadata.warnings);
    warnings.extend(simulation.warnings);
    let mut findings = detect_approvals(&decoded.intent);
    findings.extend(detect_nested_approvals(&decoded.intent));
    findings.extend(detect_contract(&decoded.intent, &metadata.target, &request));
    findings.extend(detect_address(&decoded.intent, &request));
    findings.extend(detect_asset_flow(
        &decoded.intent,
        &request,
        &simulation.effects,
        decoded.decode_coverage,
    ));
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
    sources.extend(simulation.sources);

    Ok(TransactionInspectResponse {
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
            policy: "not_configured".to_string(),
        },
        unknowns,
        sources,
        warnings,
    })
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

    let block = match rpc.simulate_v1(tx, tag).await {
        Ok(block) => block,
        Err(error) => {
            unknowns.push(format!("asset effects unavailable: {}", error.message));
            return SimulationMetadata {
                effects,
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
            coverage: "partial",
            sources,
            unknowns,
            warnings,
        };
    }

    if let Some(logs) = call.get("logs").and_then(Value::as_array) {
        for log in logs {
            decode_log(log, &mut effects, &mut warnings);
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

    SimulationMetadata {
        effects,
        coverage: "partial",
        sources,
        unknowns,
        warnings,
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
    unknowns: Vec<String>,
    warnings: Vec<String>,
}

async fn inspect_target(
    rpc: &Rpc,
    address: Address,
    tag: &str,
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
            },
            coverage: "complete",
            unknowns: Vec::new(),
            warnings: Vec::new(),
        });
    }

    let slot: B256 = EIP1967_IMPLEMENTATION_SLOT
        .parse()
        .map_err(|_| CapabilityError::internal("invalid EIP-1967 implementation slot"))?;
    let (proxy, implementation, mut unknowns, mut warnings) =
        match rpc.storage_at(address, slot, tag).await {
            Ok(value) if value != B256::ZERO => (
                Some(true),
                Some(Address::from_slice(&value.as_slice()[12..])),
                Vec::new(),
                Vec::new(),
            ),
            Ok(_) => (Some(false), None, Vec::new(), Vec::new()),
            Err(error) => (
                None,
                None,
                vec![format!(
                    "proxy storage could not be read: {}",
                    error.message
                )],
                Vec::new(),
            ),
        };

    unknowns.push("contract source verification was not configured".to_string());
    warnings.push(
        "verified=null means source verification was not checked, not that the contract is safe"
            .to_string(),
    );

    Ok(TargetMetadata {
        target: TransactionInspectTarget {
            address,
            kind: "contract".to_string(),
            verified: None,
            proxy,
            implementation,
        },
        coverage: "partial",
        unknowns,
        warnings,
    })
}

const EIP1967_IMPLEMENTATION_SLOT: &str =
    "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc";

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
