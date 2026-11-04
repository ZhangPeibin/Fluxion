//! `swap` capability family. The current implementation exposes `swap.quote`;
//! `swap.build` and execution belong to later phases.

pub mod quote;

use crate::capability::{CapabilityDescriptor, ProviderDescriptor, ProviderId, SCHEMA_VERSION};

pub fn describe() -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: CAPABILITY,
        schema_version: SCHEMA_VERSION,
        providers: vec![
            ProviderDescriptor {
                id: ProviderId::LocalCli,
                location: "local",
                transport: "cli",
                available: true,
                unavailable_reason: None,
                requires: vec!["rpc_url", "graph_api_key"],
            },
            ProviderDescriptor {
                id: ProviderId::LocalMcp,
                location: "local",
                transport: "mcp-stdio",
                available: true,
                unavailable_reason: None,
                requires: vec!["rpc_url", "graph_api_key"],
            },
            ProviderDescriptor {
                id: ProviderId::RemoteMcp,
                location: "remote",
                transport: "mcp-http",
                available: false,
                unavailable_reason: Some("remote provider is not implemented yet"),
                requires: vec!["fluxion_api_key"],
            },
        ],
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
    fn describes_local_and_unavailable_remote_providers() {
        let descriptor = describe();
        assert_eq!(descriptor.name, "swap.quote");
        assert_eq!(descriptor.schema_version, SCHEMA_VERSION);
        let local = descriptor
            .providers
            .iter()
            .find(|provider| provider.id == ProviderId::LocalMcp)
            .unwrap();
        assert!(local.available);
        let remote = descriptor
            .providers
            .iter()
            .find(|provider| provider.id == ProviderId::RemoteMcp)
            .unwrap();
        assert!(!remote.available);
        assert!(remote.unavailable_reason.is_some());
    }
}
