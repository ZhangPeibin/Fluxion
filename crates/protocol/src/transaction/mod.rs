//! `transaction` capability family. `transaction.simulate` is read-only and
//! never signs, approves, or broadcasts.

use alloy_primitives::{Address, B256};
use serde::Serialize;

use crate::capability::{Amount, CapabilityDescriptor, SCHEMA_VERSION};

pub const SIMULATE_CAPABILITY: &str = "transaction.simulate";

#[derive(Debug, Clone, Serialize)]
pub struct TransactionSimulateResponse {
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

pub fn describe() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: SIMULATE_CAPABILITY,
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
}
