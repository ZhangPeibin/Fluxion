//! Deterministic inspection of typed data, EIP-7702 authorizations and ERC-4337 user operations.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use alloy_primitives::{Address, U256};
use serde_json::{Value, json};

use crate::capability::CapabilityError;
use crate::chain::Chain;
use crate::transaction_inspect::InspectPolicy;

pub use fluxion_protocol::signature::{
    SignatureInspectAssurance, SignatureInspectDomain, SignatureInspectResponse,
};
use fluxion_protocol::transaction::{
    TransactionInspectExplanation, TransactionInspectFinding, TransactionInspectPolicyResult,
    TransactionInspectSource,
};

#[derive(Debug, Clone)]
pub struct SignatureInspectRequest {
    pub chain: Chain,
    pub from: Option<Address>,
    pub typed_data: Option<Value>,
    pub authorization_list: Option<Value>,
    pub user_operation: Option<Value>,
    pub policy: Option<InspectPolicy>,
}

const RULESET_VERSION: &str = "2026-11-11.1";

#[derive(Default)]
struct Inspection {
    domain: Option<SignatureInspectDomain>,
    findings: Vec<TransactionInspectFinding>,
    unknowns: Vec<String>,
    warnings: Vec<String>,
    coverage: &'static str,
    kind: &'static str,
}

pub fn inspect(
    request: SignatureInspectRequest,
) -> Result<SignatureInspectResponse, CapabilityError> {
    let selected = [
        request.typed_data.is_some(),
        request.authorization_list.is_some(),
        request.user_operation.is_some(),
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count();
    if selected != 1 {
        return Err(CapabilityError::invalid_argument(
            "signature.inspect needs exactly one of typed_data, authorization_list or user_operation",
        ));
    }

    let mut inspection = if let Some(typed_data) = request.typed_data.as_ref() {
        inspect_typed_data(typed_data, request.chain)?
    } else if let Some(authorization_list) = request.authorization_list.as_ref() {
        inspect_authorizations(authorization_list, request.chain)?
    } else {
        inspect_user_operation(
            request
                .user_operation
                .as_ref()
                .expect("exactly one input was selected"),
        )?
    };

    let mut findings = std::mem::take(&mut inspection.findings);
    let policy_eval = evaluate_policy(&request, &inspection, &findings)?;
    let policy_mode = request
        .policy
        .as_ref()
        .map(|policy| {
            if policy.mode.is_empty() {
                "advisory".to_string()
            } else {
                policy.mode.to_ascii_lowercase()
            }
        })
        .unwrap_or_else(|| "not_configured".to_string());
    let policy = TransactionInspectPolicyResult {
        mode: policy_mode,
        decision: policy_eval.decision.to_string(),
        blocked: policy_eval.blocked,
        reasons: policy_eval.reasons,
    };
    findings.extend(policy_eval.findings);
    let mut unknowns = inspection.unknowns;
    unknowns.extend(policy_eval.unknowns);
    let risk = aggregate_risk(&findings);
    let explanation = explain(inspection.kind, request.chain, &findings, &risk, &unknowns);

    let mut response = SignatureInspectResponse {
        chain: request.chain.as_str().to_string(),
        chain_id: request.chain.chain_id(),
        kind: inspection.kind.to_string(),
        from: request.from,
        domain: inspection.domain,
        findings,
        risk,
        coverage: inspection.coverage.to_string(),
        unknowns,
        policy,
        assurance: SignatureInspectAssurance {
            ruleset_version: RULESET_VERSION.to_string(),
            report_id: String::new(),
            deterministic: true,
            limitations: Vec::new(),
        },
        explanation,
        sources: vec![TransactionInspectSource {
            name: "signature-decoder".to_string(),
            scope: inspection.kind.to_string(),
        }],
        warnings: inspection.warnings,
    };
    response.assurance.limitations = limitations(&response);
    response.assurance.report_id = report_id(&request, &response);
    Ok(response)
}

struct PolicyEvaluation {
    decision: &'static str,
    blocked: bool,
    reasons: Vec<String>,
    unknowns: Vec<String>,
    findings: Vec<TransactionInspectFinding>,
}

fn evaluate_policy(
    request: &SignatureInspectRequest,
    inspection: &Inspection,
    findings: &[TransactionInspectFinding],
) -> Result<PolicyEvaluation, CapabilityError> {
    let Some(policy) = request.policy.as_ref() else {
        return Ok(PolicyEvaluation {
            decision: "not_configured",
            blocked: false,
            reasons: Vec::new(),
            unknowns: Vec::new(),
            findings: Vec::new(),
        });
    };
    let enforce = policy.mode.eq_ignore_ascii_case("enforce");
    let mut policy_findings = Vec::new();
    let mut unknowns = Vec::new();

    if !policy.chains.is_empty()
        && !policy
            .chains
            .iter()
            .any(|chain| chain.eq_ignore_ascii_case(request.chain.as_str()))
    {
        policy_findings.push(signature_policy_finding(
            "POLICY-001",
            "Chain not allowed by policy",
            "The signature chain is outside the configured policy.",
            [("chain", request.chain.as_str().to_string())],
        ));
    }

    if let Some(domain) = inspection.domain.as_ref()
        && !policy.target_allowlist.is_empty()
        && let Some(target) = domain.verifying_contract
        && !policy.target_allowlist.contains(&target)
    {
        policy_findings.push(signature_policy_finding(
            "POLICY-004",
            "Verifying contract outside policy allowlist",
            "The typed-data verifying contract is not allowed by policy.",
            [("target", target.to_string())],
        ));
    }

    if !policy.spender_allowlist.is_empty()
        && let Some(typed_data) = request.typed_data.as_ref()
    {
        for spender in typed_data_spenders(typed_data) {
            if !policy.spender_allowlist.contains(&spender) {
                policy_findings.push(signature_policy_finding(
                    "POLICY-004",
                    "Spender outside policy allowlist",
                    "A typed-data spender is not allowed by policy.",
                    [("spender", spender.to_string())],
                ));
            }
        }
    }

    let high_findings = findings
        .iter()
        .filter(|finding| finding.severity == "high" || finding.severity == "critical")
        .map(|finding| finding.id.clone())
        .collect::<Vec<_>>();
    if !high_findings.is_empty() {
        policy_findings.push(signature_policy_finding(
            "POLICY-007",
            "High-risk signature finding",
            "One or more high-risk signature findings are present.",
            [("findings", high_findings.join(","))],
        ));
    }

    let reasons = policy_findings
        .iter()
        .map(|finding| format!("{}: {}", finding.id, finding.title))
        .collect::<Vec<_>>();
    let decision = if policy_findings.is_empty() {
        "allow"
    } else if enforce {
        "block"
    } else {
        "warn"
    };
    if enforce && inspection.coverage != "complete" && policy_findings.is_empty() {
        unknowns.push(
            "signature coverage is partial; policy allowed within known coverage".to_string(),
        );
    }
    Ok(PolicyEvaluation {
        decision,
        blocked: enforce && !reasons.is_empty(),
        reasons,
        unknowns,
        findings: policy_findings,
    })
}

fn typed_data_spenders(value: &Value) -> Vec<Address> {
    let Some(message) = value.get("message") else {
        return Vec::new();
    };
    let mut spenders = Vec::new();
    if let Some(spender) = message.get("spender").and_then(value_address) {
        spenders.push(spender);
    }
    if let Some(details) = message.get("details") {
        if let Some(spender) = details.get("spender").and_then(value_address) {
            spenders.push(spender);
        }
        if let Some(items) = details.as_array() {
            for item in items {
                if let Some(spender) = item.get("spender").and_then(value_address) {
                    spenders.push(spender);
                }
            }
        }
    }
    spenders
}

fn signature_policy_finding<const N: usize>(
    id: &str,
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
        severity: "high".to_string(),
        title: title.to_string(),
        description: description.to_string(),
        evidence: evidence_map,
        confidence: "high".to_string(),
        deterministic: true,
    }
}

fn limitations(response: &SignatureInspectResponse) -> Vec<String> {
    let mut items = Vec::new();
    if response.coverage != "complete" {
        items.push(
            "signature coverage is partial; unsupported typed-data types remain unknown"
                .to_string(),
        );
    }
    if response.policy.mode == "not_configured" {
        items.push("no caller policy was configured".to_string());
    }
    items.push(
        "signature inspection does not sign, broadcast, or decode every userOp field".to_string(),
    );
    items.push(
        "pre-sign inspection is advisory and is not an audit or safety guarantee".to_string(),
    );
    items
}

fn report_id(request: &SignatureInspectRequest, response: &SignatureInspectResponse) -> String {
    let input = request
        .typed_data
        .as_ref()
        .or(request.authorization_list.as_ref())
        .or(request.user_operation.as_ref());
    let payload = json!({
        "ruleset_version": RULESET_VERSION,
        "chain": response.chain,
        "chain_id": response.chain_id,
        "kind": response.kind,
        "from": request.from,
        "input": input,
        "domain": response.domain,
        "findings": response.findings,
        "risk": response.risk,
        "coverage": response.coverage,
        "unknowns": response.unknowns,
        "policy": response.policy,
    });
    let digest = Sha256::digest(serde_json::to_vec(&payload).unwrap_or_default());
    format!("0x{}", data_encoding::HEXLOWER.encode(&digest))
}

fn inspect_typed_data(typed_data: &Value, chain: Chain) -> Result<Inspection, CapabilityError> {
    if !typed_data.is_object() {
        return Err(CapabilityError::invalid_argument(
            "typed_data must be a JSON object",
        ));
    }
    let domain_value = typed_data.get("domain");
    let (domain, domain_chain_id) = parse_domain(domain_value);
    let mut inspection = Inspection {
        domain,
        coverage: "partial",
        kind: "typed_data",
        ..Default::default()
    };

    if let Some(domain_chain_id) = domain_chain_id
        && domain_chain_id != chain.chain_id()
    {
        inspection.findings.push(signature_finding(
            "SIG-001",
            "high",
            "Typed-data chain mismatch",
            "The EIP-712 domain chain ID differs from the selected chain.",
            [
                ("domain_chain_id", domain_chain_id.to_string()),
                ("selected_chain_id", chain.chain_id().to_string()),
            ],
        ));
    }

    let Some(primary_type) = typed_data.get("primaryType").and_then(Value::as_str) else {
        inspection
            .unknowns
            .push("typed data has no primaryType".to_string());
        inspection.findings.push(signature_finding(
            "SIG-006",
            "medium",
            "Typed data cannot be decoded",
            "The typed data has no primaryType.",
            [],
        ));
        return Ok(inspection);
    };

    match primary_type {
        "PermitSingle" => inspect_permit_single(typed_data, &mut inspection),
        "PermitBatch" => inspect_permit_batch(typed_data, &mut inspection),
        other if other.contains("Permit") => {
            inspection.unknowns.push(format!(
                "unsupported permit primaryType {other}; no permit-specific finding was produced"
            ));
        }
        _ => {
            inspection.unknowns.push(format!(
                "unsupported typed-data primaryType {primary_type}; only domain checks were applied"
            ));
        }
    }
    Ok(inspection)
}

fn inspect_permit_single(typed_data: &Value, inspection: &mut Inspection) {
    let Some(message) = typed_data.get("message") else {
        inspection
            .unknowns
            .push("PermitSingle has no message".to_string());
        return;
    };
    let spender = message.get("spender");
    if let Some(spender) = spender.and_then(value_address)
        && spender == Address::ZERO
    {
        inspection.findings.push(signature_finding(
            "SIG-003",
            "high",
            "Permit to zero spender",
            "The permit spender is the zero address.",
            [("spender", spender.to_string())],
        ));
    }
    if let Some(deadline) = message.get("sigDeadline").and_then(value_u256) {
        if deadline.is_zero() {
            inspection.findings.push(signature_finding(
                "SIG-002",
                "high",
                "Permit has no expiration",
                "The Permit2 signature deadline is zero.",
                [("sigDeadline", "0".to_string())],
            ));
        } else if deadline < now_seconds() {
            inspection.findings.push(signature_finding(
                "SIG-002",
                "high",
                "Permit is expired",
                "The Permit2 signature deadline is in the past.",
                [("sigDeadline", deadline.to_string())],
            ));
        }
    }
    if let Some(details) = message.get("details") {
        inspect_permit_details(details, &mut inspection.findings);
    }
}

fn inspect_permit_batch(typed_data: &Value, inspection: &mut Inspection) {
    let Some(message) = typed_data.get("message") else {
        inspection
            .unknowns
            .push("PermitBatch has no message".to_string());
        return;
    };
    if let Some(deadline) = message.get("sigDeadline").and_then(value_u256) {
        if deadline.is_zero() {
            inspection.findings.push(signature_finding(
                "SIG-002",
                "high",
                "Permit batch has no expiration",
                "The Permit2 signature deadline is zero.",
                [("sigDeadline", "0".to_string())],
            ));
        } else if deadline < now_seconds() {
            inspection.findings.push(signature_finding(
                "SIG-002",
                "high",
                "Permit batch is expired",
                "The Permit2 signature deadline is in the past.",
                [("sigDeadline", deadline.to_string())],
            ));
        }
    }
    if let Some(details) = message.get("details").and_then(Value::as_array) {
        for detail in details {
            inspect_permit_details(detail, &mut inspection.findings);
        }
    }
}

fn inspect_permit_details(details: &Value, findings: &mut Vec<TransactionInspectFinding>) {
    let amount = details.get("amount").and_then(value_u256);
    if amount.is_some_and(|amount| amount >= max_uint160()) {
        findings.push(signature_finding(
            "SIG-003",
            "high",
            "Unlimited Permit2 approval",
            "The Permit2 amount is the maximum uint160 value.",
            [("amount", amount.unwrap().to_string())],
        ));
    }
    if let Some(expiration) = details.get("expiration").and_then(value_u256) {
        if expiration.is_zero() {
            findings.push(signature_finding(
                "SIG-002",
                "high",
                "Permit2 token permission has no expiration",
                "The Permit2 token permission expiration is zero.",
                [("expiration", "0".to_string())],
            ));
        } else if expiration < now_seconds() {
            findings.push(signature_finding(
                "SIG-002",
                "high",
                "Permit2 token permission is expired",
                "The Permit2 token permission expiration is in the past.",
                [("expiration", expiration.to_string())],
            ));
        }
    }
}

fn inspect_authorizations(value: &Value, chain: Chain) -> Result<Inspection, CapabilityError> {
    let Some(items) = value.as_array() else {
        return Err(CapabilityError::invalid_argument(
            "authorization_list must be a JSON array",
        ));
    };
    let mut inspection = Inspection {
        coverage: "partial",
        kind: "authorization_list",
        ..Default::default()
    };
    if items.is_empty() {
        inspection
            .unknowns
            .push("authorization list is empty".to_string());
        return Ok(inspection);
    }
    let mut zero_addresses = 0;
    let mut chain_mismatches = 0;
    for item in items {
        if item
            .get("address")
            .and_then(value_address)
            .is_some_and(|address| address == Address::ZERO)
        {
            zero_addresses += 1;
        }
        if let Some(chain_id) = item.get("chainId").and_then(value_u256)
            && chain_id != U256::from(chain.chain_id())
        {
            chain_mismatches += 1;
        }
    }
    inspection.findings.push(signature_finding(
        "SIG-004",
        "high",
        "EIP-7702 delegation authorization",
        "The request contains EIP-7702 authorizations that can delegate account code.",
        [
            ("count", items.len().to_string()),
            ("zero_addresses", zero_addresses.to_string()),
            ("chain_mismatches", chain_mismatches.to_string()),
            ("selected_chain_id", chain.chain_id().to_string()),
        ],
    ));
    Ok(inspection)
}

fn inspect_user_operation(value: &Value) -> Result<Inspection, CapabilityError> {
    if !value.is_object() {
        return Err(CapabilityError::invalid_argument(
            "user_operation must be a JSON object",
        ));
    }
    let mut inspection = Inspection {
        coverage: "partial",
        kind: "user_operation",
        ..Default::default()
    };
    if value.get("sender").and_then(value_address).is_none() {
        inspection.findings.push(signature_finding(
            "SIG-006",
            "high",
            "User operation has no valid sender",
            "The ERC-4337 user operation sender is missing or invalid.",
            [],
        ));
    }
    let has_paymaster = value.get("paymaster").is_some() || value.get("paymasterAndData").is_some();
    if has_paymaster {
        inspection.findings.push(signature_finding(
            "SIG-005",
            "medium",
            "User operation has a paymaster",
            "A paymaster is attached; verify its sponsorship policy and trust.",
            [],
        ));
    }
    if value.get("factory").is_some() || value.get("factoryData").is_some() {
        inspection.findings.push(signature_finding(
            "SIG-005",
            "medium",
            "User operation deploys an account",
            "The user operation includes an account factory deployment.",
            [],
        ));
    }
    if value.get("callData").is_some() {
        inspection
            .unknowns
            .push("user operation callData was not decoded".to_string());
    }
    if value.get("signature").is_some() {
        inspection
            .unknowns
            .push("user operation signature was not inspected".to_string());
    }
    Ok(inspection)
}

fn parse_domain(value: Option<&Value>) -> (Option<SignatureInspectDomain>, Option<u64>) {
    let Some(value) = value else {
        return (None, None);
    };
    let chain_id = value.get("chainId").and_then(value_u256);
    let domain = SignatureInspectDomain {
        name: value
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        version: value
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_string),
        chain_id: chain_id.map(|value| value.to_string()),
        verifying_contract: value.get("verifyingContract").and_then(value_address),
    };
    (
        Some(domain),
        chain_id.and_then(|value| u64::try_from(value).ok()),
    )
}

fn value_address(value: &Value) -> Option<Address> {
    value.as_str().and_then(|value| value.parse().ok())
}

fn value_u256(value: &Value) -> Option<U256> {
    if let Some(value) = value.as_u64() {
        return Some(U256::from(value));
    }
    let value = value.as_str()?;
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        U256::from_str_radix(hex, 16).ok()
    } else {
        U256::from_str_radix(value, 10).ok()
    }
}

fn max_uint160() -> U256 {
    (U256::from(1) << 160) - U256::from(1)
}

fn now_seconds() -> U256 {
    U256::from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0),
    )
}

fn signature_finding<const N: usize>(
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
    evidence_map.insert("source".to_string(), "signature-input".to_string());
    TransactionInspectFinding {
        id: id.to_string(),
        category: "signature".to_string(),
        severity: severity.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        evidence: evidence_map,
        confidence: "high".to_string(),
        deterministic: true,
    }
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

fn explain(
    kind: &str,
    chain: Chain,
    findings: &[TransactionInspectFinding],
    risk: &str,
    unknowns: &[String],
) -> TransactionInspectExplanation {
    let summary = if findings.is_empty() {
        format!(
            "{kind} inspection on {}: no known findings within current coverage (risk: {risk}). Review the unknowns before signing.",
            chain.as_str()
        )
    } else {
        let titles = findings
            .iter()
            .take(3)
            .map(|finding| finding.title.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "{kind} inspection on {}: {} finding(s), risk {risk}: {titles}.",
            chain.as_str(),
            findings.len()
        )
    };
    TransactionInspectExplanation {
        summary,
        key_changes: findings
            .iter()
            .map(|finding| format!("{}: {}", finding.id, finding.title))
            .collect(),
        unknowns: unknowns.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(typed_data: Value) -> SignatureInspectRequest {
        SignatureInspectRequest {
            chain: Chain::Bsc,
            from: None,
            typed_data: Some(typed_data),
            authorization_list: None,
            user_operation: None,
            policy: None,
        }
    }

    #[test]
    fn flags_domain_chain_mismatch() {
        let response = inspect(request(json!({
            "domain": {"name": "Test", "version": "1", "chainId": 1},
            "primaryType": "Mail",
            "message": {}
        })))
        .unwrap();
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "SIG-001")
        );
        assert_eq!(response.risk, "high");
    }

    #[test]
    fn flags_unlimited_permit2() {
        let response = inspect(request(json!({
            "domain": {"chainId": 56},
            "primaryType": "PermitSingle",
            "message": {
                "spender": "0x0000000000000000000000000000000000000002",
                "sigDeadline": "9999999999",
                "details": {
                    "token": "0x0000000000000000000000000000000000000010",
                    "amount": "1461501637330902918203684832716283019655932542975",
                    "expiration": "9999999999",
                    "nonce": 0
                }
            }
        })))
        .unwrap();
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "SIG-003")
        );
    }

    #[test]
    fn flags_expired_permit2() {
        let response = inspect(request(json!({
            "domain": {"chainId": 56},
            "primaryType": "PermitSingle",
            "message": {
                "spender": "0x0000000000000000000000000000000000000002",
                "sigDeadline": "1",
                "details": {
                    "token": "0x0000000000000000000000000000000000000010",
                    "amount": "1",
                    "expiration": "1",
                    "nonce": 0
                }
            }
        })))
        .unwrap();
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "SIG-002")
        );
    }

    #[test]
    fn flags_7702_authorization() {
        let response = inspect(SignatureInspectRequest {
            chain: Chain::Bsc,
            from: None,
            typed_data: None,
            authorization_list: Some(json!([{
                "chainId": 1,
                "address": "0x0000000000000000000000000000000000000002"
            }])),
            user_operation: None,
            policy: None,
        })
        .unwrap();
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "SIG-004")
        );
        assert_eq!(response.risk, "high");
    }

    #[test]
    fn flags_paymaster_user_operation() {
        let response = inspect(SignatureInspectRequest {
            chain: Chain::Bsc,
            from: None,
            typed_data: None,
            authorization_list: None,
            user_operation: Some(json!({
                "sender": "0x0000000000000000000000000000000000000001",
                "paymaster": "0x0000000000000000000000000000000000000003",
                "callData": "0x"
            })),
            policy: None,
        })
        .unwrap();
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "SIG-005")
        );
        assert_eq!(response.risk, "medium");
    }

    #[test]
    fn fixture_corpus_matches_expected_findings() {
        use std::collections::BTreeSet;

        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../tests/fixtures/signature_inspect/cases.json"
        ))
        .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let typed_data = case["input"].get("typed_data").cloned();
            let authorization_list = case["input"].get("authorization_list").cloned();
            let user_operation = case["input"].get("user_operation").cloned();
            let response = inspect(SignatureInspectRequest {
                chain: Chain::Bsc,
                from: None,
                typed_data,
                authorization_list,
                user_operation,
                policy: None,
            })
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
            assert_eq!(
                response.kind,
                case["kind"].as_str().unwrap(),
                "{name}: kind"
            );
            assert_eq!(
                response.risk,
                case["expected_risk"].as_str().unwrap(),
                "{name}: risk"
            );
            let actual = response
                .findings
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

    #[test]
    fn enforce_policy_blocks_high_signature_findings() {
        let mut request = request(json!({
            "domain": {"chainId": 56},
            "primaryType": "PermitSingle",
            "message": {
                "spender": "0x0000000000000000000000000000000000000002",
                "sigDeadline": "9999999999",
                "details": {
                    "token": "0x0000000000000000000000000000000000000010",
                    "amount": "1461501637330902918203684832716283019655932542975",
                    "expiration": "9999999999",
                    "nonce": 0
                }
            }
        }));
        request.policy = Some(InspectPolicy {
            mode: "enforce".to_string(),
            ..Default::default()
        });
        let response = inspect(request).unwrap();
        assert_eq!(response.policy.decision, "block");
        assert!(response.policy.blocked);
        assert!(!response.policy.reasons.is_empty());
    }

    #[test]
    fn policy_spender_allowlist_blocks_unknown_spender() {
        let mut request = request(json!({
            "domain": {"chainId": 56},
            "primaryType": "PermitSingle",
            "message": {
                "spender": "0x0000000000000000000000000000000000000002",
                "sigDeadline": "9999999999",
                "details": {
                    "token": "0x0000000000000000000000000000000000000010",
                    "amount": "1",
                    "expiration": "9999999999",
                    "nonce": 0
                }
            }
        }));
        request.policy = Some(InspectPolicy {
            mode: "enforce".to_string(),
            spender_allowlist: vec![
                "0x0000000000000000000000000000000000000003"
                    .parse()
                    .unwrap(),
            ],
            ..Default::default()
        });
        let response = inspect(request).unwrap();
        assert!(
            response
                .findings
                .iter()
                .any(|finding| finding.id == "POLICY-004")
        );
        assert_eq!(response.policy.decision, "block");
    }

    #[test]
    fn signature_report_id_and_assurance_are_stable() {
        let first = inspect(request(json!({
            "domain": {"chainId": 56},
            "primaryType": "Mail",
            "message": {}
        })))
        .unwrap();
        let second = inspect(request(json!({
            "domain": {"chainId": 56},
            "primaryType": "Mail",
            "message": {}
        })))
        .unwrap();
        assert_eq!(first.assurance.report_id, second.assurance.report_id);
        assert_eq!(first.assurance.ruleset_version, "2026-11-11.1");
        assert!(first.assurance.deterministic);
        assert_eq!(first.assurance.report_id.len(), 66);
    }

    #[test]
    fn unknown_typed_data_stays_unknown() {
        let response = inspect(request(json!({
            "domain": {"chainId": 56},
            "primaryType": "Mail",
            "message": {}
        })))
        .unwrap();
        assert!(response.findings.is_empty());
        assert_eq!(response.risk, "unknown");
        assert!(!response.unknowns.is_empty());
    }
}
