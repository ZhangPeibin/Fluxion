//! Deterministic inspection of typed data, EIP-7702 authorizations and ERC-4337 user operations.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use alloy_primitives::{Address, U256};
use serde_json::Value;

use crate::capability::CapabilityError;
use crate::chain::Chain;

pub use fluxion_protocol::signature::{SignatureInspectDomain, SignatureInspectResponse};
use fluxion_protocol::transaction::{
    TransactionInspectExplanation, TransactionInspectFinding, TransactionInspectSource,
};

#[derive(Debug, Clone)]
pub struct SignatureInspectRequest {
    pub chain: Chain,
    pub from: Option<Address>,
    pub typed_data: Option<Value>,
    pub authorization_list: Option<Value>,
    pub user_operation: Option<Value>,
}

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

    let findings = std::mem::take(&mut inspection.findings);
    let risk = aggregate_risk(&findings);
    let explanation = explain(
        inspection.kind,
        request.chain,
        &findings,
        &risk,
        &inspection.unknowns,
    );

    Ok(SignatureInspectResponse {
        chain: request.chain.as_str().to_string(),
        chain_id: request.chain.chain_id(),
        kind: inspection.kind.to_string(),
        from: request.from,
        domain: inspection.domain,
        findings,
        risk,
        coverage: inspection.coverage.to_string(),
        unknowns: inspection.unknowns,
        explanation,
        sources: vec![TransactionInspectSource {
            name: "signature-decoder".to_string(),
            scope: inspection.kind.to_string(),
        }],
        warnings: inspection.warnings,
    })
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
