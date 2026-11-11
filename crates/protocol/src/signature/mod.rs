//! Signature and authorization inspection. Read-only: never signs or broadcasts.

use alloy_primitives::Address;
use serde::Serialize;

use crate::capability::{CapabilityDescriptor, SCHEMA_VERSION};
use crate::transaction::{
    TransactionInspectExplanation, TransactionInspectFinding, TransactionInspectSource,
};

pub const INSPECT_CAPABILITY: &str = "signature.inspect";

#[derive(Debug, Clone, Serialize)]
pub struct SignatureInspectDomain {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verifying_contract: Option<Address>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SignatureInspectResponse {
    pub chain: String,
    pub chain_id: u64,
    /// `typed_data`, `authorization_list`, or `user_operation`.
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<SignatureInspectDomain>,
    pub findings: Vec<TransactionInspectFinding>,
    pub risk: String,
    pub coverage: String,
    pub unknowns: Vec<String>,
    pub explanation: TransactionInspectExplanation,
    pub sources: Vec<TransactionInspectSource>,
    pub warnings: Vec<String>,
}

pub fn describe() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: INSPECT_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}
