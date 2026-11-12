//! `swap` capability family. The current implementation exposes `swap.quote`;
//! `swap.build` and execution belong to later phases.

pub mod build;
pub mod quote;

use serde::{Deserialize, Serialize};

use crate::capability::{CapabilityDescriptor, SCHEMA_VERSION};

pub fn describe() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "remote",
        transports: vec!["cli", "mcp-stdio"],
    }
}

pub use build::{SwapBuildHop, SwapBuildResponse};
pub use quote::{AllocationResponse, HopResponse, QuoteErrorResponse, QuoteRequest, QuoteResponse};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    Baseline,
    Direct,
    Split,
}

impl PlanKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Direct => "direct",
            Self::Split => "split",
        }
    }
}

pub const CAPABILITY: &str = "swap.quote";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_a_fixed_remote_execution() {
        let descriptor = describe();
        assert_eq!(descriptor.name, "swap.quote");
        assert_eq!(descriptor.schema_version, SCHEMA_VERSION);
        assert_eq!(descriptor.execution, "remote");
        assert_eq!(descriptor.transports, vec!["cli", "mcp-stdio"]);
    }
}
