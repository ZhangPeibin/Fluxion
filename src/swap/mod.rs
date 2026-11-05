//! `swap` capability family. The current implementation exposes `swap.quote`;
//! `swap.build` and execution belong to later phases.

pub mod quote;

use crate::capability::{CapabilityDescriptor, SCHEMA_VERSION};

pub fn describe() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: CAPABILITY,
        schema_version: SCHEMA_VERSION,
        execution: "remote",
        transports: vec!["cli", "mcp-stdio"],
    }
}

pub use quote::{
    AllocationResponse, HopResponse, QuoteContext, QuoteErrorResponse, QuoteRequest, QuoteResponse,
};

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
