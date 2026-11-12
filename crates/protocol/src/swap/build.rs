//! `swap.build` request/response contract. Local calldata construction only.

use alloy_primitives::Address;
use serde::Serialize;

use crate::capability::{Amount, CapabilityDescriptor, SCHEMA_VERSION};

pub const BUILD_CAPABILITY: &str = "swap.build";

#[derive(Debug, Clone, Serialize)]
pub struct SwapBuildHop {
    pub platform: String,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: Amount,
    pub amount_out: Amount,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwapBuildResponse {
    pub chain: String,
    pub chain_id: u64,
    pub quote_block: Option<u64>,
    pub router: Address,
    pub value: String,
    pub data: String,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: Amount,
    pub amount_out_min: Amount,
    pub recipient: Address,
    pub deadline: u64,
    pub path: Vec<Address>,
    pub hops: Vec<SwapBuildHop>,
}

pub fn describe() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: BUILD_CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "local",
        transports: vec!["cli", "mcp-stdio"],
    }
}
