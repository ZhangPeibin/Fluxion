//! Runtime metadata registry for capabilities.
//!
//! The catalog is the source of truth; this registry gives adapters a stable lookup surface without
//! duplicating capability metadata in MCP, CLI, or HTTP code.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::capability::{CapabilityEntry, CapabilityId, Catalog, catalog::load_embedded};

#[derive(Debug)]
pub struct CapabilityRegistry {
    catalog: Catalog,
    by_id: HashMap<String, usize>,
}

impl CapabilityRegistry {
    pub fn from_catalog(catalog: Catalog) -> Self {
        let by_id = catalog
            .capabilities
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id.clone(), index))
            .collect();
        Self { catalog, by_id }
    }

    pub fn global() -> &'static Self {
        static REGISTRY: OnceLock<CapabilityRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| Self::from_catalog(load_embedded()))
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn get(&self, id: &str) -> Option<&CapabilityEntry> {
        self.by_id
            .get(id)
            .and_then(|index| self.catalog.capabilities.get(*index))
    }

    pub fn get_typed(&self, id: CapabilityId) -> Option<&CapabilityEntry> {
        self.get(id.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = &CapabilityEntry> {
        self.catalog.capabilities.iter()
    }

    pub fn mcp_capabilities(&self) -> impl Iterator<Item = &CapabilityEntry> {
        self.iter()
            .filter(|entry| entry.transports.iter().any(|transport| transport == "mcp"))
    }

    pub fn capability_for_tool(&self, tool: &str) -> Option<&CapabilityEntry> {
        self.mcp_capabilities()
            .find(|entry| mcp_tool_name(&entry.id) == tool)
    }
}

/// Derive the MCP tool name from a capability id.
///
/// `wallet.agent-id` becomes `wallet_agent_id`; `fourmeme.agent-tokens` becomes
/// `fourmeme_agent_tokens`.
pub fn mcp_tool_name(id: &str) -> String {
    id.chars()
        .map(|character| {
            if character == '.' || character == '-' {
                '_'
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_covers_every_capability() {
        let registry = CapabilityRegistry::global();
        for id in CapabilityId::all() {
            assert!(
                registry.get_typed(*id).is_some(),
                "catalog is missing {}",
                id.as_str()
            );
        }
    }

    #[test]
    fn mcp_tool_names_are_unique() {
        let registry = CapabilityRegistry::global();
        let mut names = registry
            .mcp_capabilities()
            .map(|entry| mcp_tool_name(&entry.id))
            .collect::<Vec<_>>();
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "MCP tool names must be unique");
    }

    #[test]
    fn mcp_tool_lookup_round_trips() {
        let registry = CapabilityRegistry::global();
        for entry in registry.mcp_capabilities() {
            let tool = mcp_tool_name(&entry.id);
            assert_eq!(
                registry
                    .capability_for_tool(&tool)
                    .map(|item| item.id.as_str()),
                Some(entry.id.as_str())
            );
        }
    }

    #[test]
    fn derives_stable_tool_names() {
        assert_eq!(mcp_tool_name("swap.quote"), "swap_quote");
        assert_eq!(mcp_tool_name("wallet.agent-id"), "wallet_agent_id");
        assert_eq!(
            mcp_tool_name("fourmeme.agent-tokens"),
            "fourmeme_agent_tokens"
        );
    }
}
