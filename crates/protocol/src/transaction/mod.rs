//! `transaction` capability family. `transaction.simulate` and
//! `transaction.inspect` are read-only and never sign, approve, or broadcast.

use std::collections::BTreeMap;

use alloy_primitives::{Address, B256};
use serde::Serialize;

use crate::capability::{Amount, CapabilityDescriptor, SCHEMA_VERSION};

pub const SIMULATE_CAPABILITY: &str = "transaction.simulate";
pub const INSPECT_CAPABILITY: &str = "transaction.inspect";

#[derive(Debug, Clone, Serialize)]
pub struct TransactionSimulateResponse {
    pub chain: String,
    pub chain_id: u64,
    pub block_number: u64,
    pub block_hash: B256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    pub to: Address,
    pub value: Amount,
    pub success: bool,
    pub return_data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_estimate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revert_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionInspectArg {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionInspectIntent {
    pub selector: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub args: Vec<TransactionInspectArg>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionInspectTarget {
    pub address: Address,
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TransactionInspectEffects {
    pub native: Vec<TransactionInspectNativeEffect>,
    pub tokens: Vec<TransactionInspectTokenEffect>,
    pub approvals: Vec<TransactionInspectApprovalEffect>,
    pub nfts: Vec<TransactionInspectNftEffect>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionInspectNativeEffect {
    pub account: Address,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionInspectTokenEffect {
    pub token: Address,
    pub account: Address,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionInspectApprovalEffect {
    pub token: Address,
    pub owner: Address,
    pub spender: Address,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    pub after: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionInspectNftEffect {
    pub collection: Address,
    pub token_id: String,
    pub from: Address,
    pub to: Address,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionInspectFinding {
    pub id: String,
    pub category: String,
    pub severity: String,
    pub title: String,
    pub description: String,
    pub evidence: BTreeMap<String, String>,
    pub confidence: String,
    pub deterministic: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionInspectCoverage {
    pub decode: String,
    pub metadata: String,
    pub simulation: String,
    pub policy: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionInspectSource {
    pub name: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionInspectResponse {
    pub chain: String,
    pub chain_id: u64,
    pub block_number: u64,
    pub block_hash: B256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    pub to: Address,
    pub value: Amount,
    pub intent: TransactionInspectIntent,
    pub target: TransactionInspectTarget,
    pub effects: TransactionInspectEffects,
    pub findings: Vec<TransactionInspectFinding>,
    pub risk: String,
    pub coverage: TransactionInspectCoverage,
    pub unknowns: Vec<String>,
    pub sources: Vec<TransactionInspectSource>,
    pub warnings: Vec<String>,
}

pub fn describe() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: SIMULATE_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}

pub fn describe_inspect() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: INSPECT_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_local_simulation() {
        let descriptor = describe();
        assert_eq!(descriptor.name, "transaction.simulate");
        assert_eq!(descriptor.execution, "local");
    }

    #[test]
    fn describes_local_inspection() {
        let descriptor = describe_inspect();
        assert_eq!(descriptor.name, "transaction.inspect");
        assert_eq!(descriptor.execution, "local");
    }
}
