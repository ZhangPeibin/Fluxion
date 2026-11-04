//! Capability-first surface shared by CLI and HTTP adapters.
//!
//! Capability identifiers describe user actions (`swap.quote`), not protocols
//! or transport. Capability-specific code lives in its own top-level module
//! (`crate::swap`), while this module holds the stable contract.

pub mod error;
pub mod types;

pub use error::{CapabilityError, CapabilityErrorCode};
pub use types::{Amount, CapabilityMeta, Provenance, TokenRef};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityId {
    #[serde(rename = "swap.quote")]
    SwapQuote,
}

impl CapabilityId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SwapQuote => "swap.quote",
        }
    }

    pub const fn all() -> &'static [Self] {
        &[Self::SwapQuote]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderId {
    #[serde(rename = "local-cli")]
    LocalCli,
    #[serde(rename = "local-mcp")]
    LocalMcp,
    #[serde(rename = "fluxion-remote")]
    Remote,
}

impl ProviderId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalCli => "local-cli",
            Self::LocalMcp => "local-mcp",
            Self::Remote => "fluxion-remote",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub location: &'static str,
    pub transport: &'static str,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<&'static str>,
    pub requires: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityDescriptor {
    pub name: &'static str,
    pub schema_version: u32,
    pub providers: Vec<ProviderDescriptor>,
}

pub fn names() -> impl Iterator<Item = &'static str> {
    CapabilityId::all()
        .iter()
        .copied()
        .map(CapabilityId::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_stable_capability_names() {
        assert_eq!(names().collect::<Vec<_>>(), vec!["swap.quote"]);
        assert_eq!(CapabilityId::SwapQuote.as_str(), "swap.quote");
    }
}
