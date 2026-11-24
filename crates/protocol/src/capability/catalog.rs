//! The repository capability catalog.
//!
//! `capabilities/catalog.yaml` is the source of truth for capability metadata. This module embeds
//! it so tests can prove that the typed capability surface and the catalog do not drift.

use serde::{Deserialize, Serialize};

pub const CATALOG_YAML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../capabilities/catalog.yaml"
));

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: u32,
    pub updated: String,
    pub envelope: String,
    pub capabilities: Vec<CapabilityEntry>,
    #[serde(default)]
    pub surfaces: Vec<SurfaceEntry>,
    #[serde(default)]
    pub workflows: Vec<WorkflowEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityEntry {
    pub id: String,
    pub domain: String,
    pub action: String,
    pub status: String,
    pub summary: String,
    #[serde(default = "default_envelope")]
    pub envelope: String,
    pub execution: String,
    pub transports: Vec<String>,
    pub chains: Vec<String>,
    #[serde(default)]
    pub default_chain: Option<String>,
    #[serde(default)]
    pub chain_required: bool,
    pub side_effects: String,
    #[serde(default)]
    pub requires: Vec<String>,
    pub schema: String,
    pub skill: String,
    pub docs: String,
    #[serde(default)]
    pub tests: Vec<String>,
    #[serde(default)]
    pub workflows: Vec<String>,
    pub coverage_dimensions: Vec<String>,
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceEntry {
    pub id: String,
    pub kind: String,
    pub capability: bool,
    pub status: String,
    pub summary: String,
    pub transports: Vec<String>,
    pub chains: Vec<String>,
    pub docs: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowEntry {
    pub id: String,
    pub summary: String,
    pub status: String,
    pub chains: Vec<String>,
    pub steps: Vec<String>,
    #[serde(default)]
    pub planned_steps: Vec<String>,
    #[serde(default)]
    pub skill: Option<String>,
    #[serde(default)]
    pub docs: Option<String>,
    #[serde(default)]
    pub gaps: Vec<String>,
}

impl Catalog {
    pub fn parse(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    pub fn capability(&self, id: &str) -> Option<&CapabilityEntry> {
        self.capabilities.iter().find(|entry| entry.id == id)
    }

    pub fn workflow(&self, id: &str) -> Option<&WorkflowEntry> {
        self.workflows.iter().find(|entry| entry.id == id)
    }

    /// Render the capability table used by `docs/capabilities/README.md`.
    pub fn markdown_table(&self) -> String {
        let mut output = String::from(
            "| Capability | Status | Chains | Side effects | Docs |\n|---|---|---|---|---|\n",
        );
        for entry in &self.capabilities {
            let docs = entry
                .docs
                .strip_prefix("docs/capabilities/")
                .unwrap_or(&entry.docs);
            output.push_str(&format!(
                "| `{}` | {} | {} | {} | [{}]({}) |\n",
                entry.id,
                entry.status,
                entry.chains.join(", "),
                entry.side_effects,
                entry.domain,
                docs
            ));
        }
        output
    }

    /// Machine-readable subset consumed by the `site/` landing page.
    ///
    /// Keep this intentionally narrow: the site only needs identity, status,
    /// chains and links. Everything else stays in the catalog.
    pub fn site_catalog_json(&self) -> serde_json::Value {
        let capabilities = self
            .capabilities
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "id": entry.id,
                    "domain": entry.domain,
                    "action": entry.action,
                    "status": entry.status,
                    "summary": entry.summary,
                    "execution": entry.execution,
                    "chains": entry.chains,
                    "sideEffects": entry.side_effects,
                    "transports": entry.transports,
                    "requires": entry.requires,
                    "docs": entry.docs,
                    "skill": entry.skill,
                })
            })
            .collect::<Vec<_>>();
        let workflows = self
            .workflows
            .iter()
            .map(|workflow| {
                serde_json::json!({
                    "id": workflow.id,
                    "summary": workflow.summary,
                    "status": workflow.status,
                    "chains": workflow.chains,
                    "steps": workflow.steps,
                    "plannedSteps": workflow.planned_steps,
                    "gaps": workflow.gaps,
                })
            })
            .collect::<Vec<_>>();

        serde_json::json!({
            "version": self.version,
            "updated": self.updated,
            "capabilities": capabilities,
            "workflows": workflows,
        })
    }
}

fn default_envelope() -> String {
    "v1".to_string()
}

pub fn load_embedded() -> Catalog {
    Catalog::parse(CATALOG_YAML).expect("embedded capability catalog must be valid YAML")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityId;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    const CHAINS: &[&str] = &["ethereum", "bsc"];
    const STATUSES: &[&str] = &["proposed", "experimental", "stable", "deprecated"];
    const TRANSPORTS: &[&str] = &["mcp", "cli", "http"];
    const SIDE_EFFECTS: &[&str] = &[
        "read_only",
        "local_session",
        "prepares_calldata",
        "signs",
        "broadcasts",
    ];
    const COVERAGE: &[&str] = &[
        "data",
        "decode",
        "metadata",
        "simulation",
        "trace",
        "policy",
        "freshness",
        "eligibility",
        "effects",
    ];
    const REMOTE: &[&str] = &[
        "swap.quote",
        "yield.discover",
        "yield.history",
        "position.analyze",
    ];

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn assert_path(root: &Path, relative: &str) {
        assert!(
            root.join(relative).exists(),
            "catalog path does not exist: {relative}"
        );
    }

    #[test]
    fn catalog_ids_match_capability_ids() {
        let catalog = load_embedded();
        let catalog_ids = catalog
            .capabilities
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<HashSet<_>>();
        let typed_ids = CapabilityId::all()
            .iter()
            .map(|id| id.as_str())
            .collect::<HashSet<_>>();

        assert_eq!(
            catalog_ids.len(),
            catalog.capabilities.len(),
            "catalog contains duplicate capability ids"
        );
        assert_eq!(
            catalog_ids, typed_ids,
            "catalog and CapabilityId::all() must contain the same ids"
        );
    }

    #[test]
    fn catalog_semantics_are_consistent() {
        let catalog = load_embedded();
        assert_eq!(catalog.version, 2);
        assert_eq!(catalog.envelope, "v1");

        for entry in &catalog.capabilities {
            assert_eq!(
                entry.id,
                format!("{}.{}", entry.domain, entry.action),
                "{} must equal domain.action",
                entry.id
            );
            assert!(
                STATUSES.contains(&entry.status.as_str()),
                "{} has an unknown status",
                entry.id
            );
            assert!(
                matches!(entry.envelope.as_str(), "v1" | "v2"),
                "{} has an unknown envelope version",
                entry.id
            );
            if entry.envelope == "v2" {
                assert!(
                    entry.schema.contains(".v2."),
                    "{} is v2 but does not reference a v2 schema",
                    entry.id
                );
            }
            assert!(
                !entry.transports.is_empty()
                    && entry
                        .transports
                        .iter()
                        .all(|transport| TRANSPORTS.contains(&transport.as_str())),
                "{} has an unknown transport",
                entry.id
            );
            assert!(
                !entry.chains.is_empty()
                    && entry
                        .chains
                        .iter()
                        .all(|chain| CHAINS.contains(&chain.as_str())),
                "{} has an unknown chain",
                entry.id
            );
            assert!(
                SIDE_EFFECTS.contains(&entry.side_effects.as_str()),
                "{} has an unknown side effect",
                entry.id
            );
            assert!(
                entry
                    .coverage_dimensions
                    .iter()
                    .all(|dimension| COVERAGE.contains(&dimension.as_str())),
                "{} has an unknown coverage dimension",
                entry.id
            );

            if let Some(default) = &entry.default_chain {
                assert!(
                    entry.chains.contains(default),
                    "{} default_chain must be in chains",
                    entry.id
                );
            }
            assert!(
                !(entry.chain_required && entry.default_chain.is_some()),
                "{} cannot have both chain_required and default_chain",
                entry.id
            );

            let expected_execution = if REMOTE.contains(&entry.id.as_str()) {
                "remote"
            } else {
                "local"
            };
            assert_eq!(
                entry.execution, expected_execution,
                "{} has an unexpected execution location",
                entry.id
            );

            if entry.status == "stable" {
                assert!(
                    !entry.tests.is_empty(),
                    "stable capability {} must declare tests",
                    entry.id
                );
            }
            if entry.status == "deprecated" {
                assert!(
                    entry.replacement.is_some(),
                    "deprecated capability {} must declare a replacement",
                    entry.id
                );
            }

            for requirement in &entry.requires {
                assert!(
                    requirement == "wallet_session" || catalog.capability(requirement).is_some(),
                    "{} requires unknown prerequisite {}",
                    entry.id,
                    requirement
                );
            }
        }

        let ids = catalog
            .capabilities
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<HashSet<_>>();

        for workflow in &catalog.workflows {
            assert!(
                STATUSES.contains(&workflow.status.as_str()),
                "workflow {} has an unknown status",
                workflow.id
            );
            for step in &workflow.steps {
                let capability = catalog.capability(step).unwrap_or_else(|| {
                    panic!("workflow {} has unknown step {}", workflow.id, step)
                });
                assert!(
                    capability.workflows.iter().any(|item| item == &workflow.id),
                    "{} does not list workflow {}",
                    capability.id,
                    workflow.id
                );
            }
            for planned in &workflow.planned_steps {
                assert!(
                    !ids.contains(planned.as_str()),
                    "workflow {} planned step {} already exists",
                    workflow.id,
                    planned
                );
            }
            for chain in &workflow.chains {
                for step in &workflow.steps {
                    let capability = catalog.capability(step).expect("step checked above");
                    assert!(
                        capability.chains.contains(chain),
                        "workflow {} chain {} is not supported by {}",
                        workflow.id,
                        chain,
                        capability.id
                    );
                }
            }
            if workflow.status == "stable" {
                assert!(
                    workflow.planned_steps.is_empty() && workflow.gaps.is_empty(),
                    "stable workflow {} cannot have planned steps or gaps",
                    workflow.id
                );
            }
        }

        for surface in &catalog.surfaces {
            assert!(
                !surface.capability,
                "surface {} must not be a capability",
                surface.id
            );
            assert!(
                !ids.contains(surface.id.as_str()),
                "surface {} overlaps a capability id",
                surface.id
            );
        }
    }

    #[test]
    fn docs_capability_table_matches_catalog() {
        let catalog = load_embedded();
        let path = repo_root().join("docs/capabilities/README.md");
        let document = std::fs::read_to_string(&path).expect("capability index");
        let start = document
            .find("<!-- catalog-table:start -->")
            .expect("catalog table start marker")
            + "<!-- catalog-table:start -->".len();
        let end = document[start..]
            .find("<!-- catalog-table:end -->")
            .expect("catalog table end marker")
            + start;
        assert_eq!(
            document[start..end].trim(),
            catalog.markdown_table().trim(),
            "docs/capabilities/README.md is stale; update the catalog table"
        );
    }

    #[test]
    fn catalog_paths_exist() {
        let catalog = load_embedded();
        let root = repo_root();

        for entry in &catalog.capabilities {
            for path in [&entry.schema, &entry.skill, &entry.docs] {
                assert_path(&root, path);
            }
            for path in &entry.tests {
                assert_path(&root, path);
            }
        }
        for surface in &catalog.surfaces {
            assert_path(&root, &surface.docs);
        }
        for workflow in &catalog.workflows {
            if let Some(skill) = &workflow.skill {
                assert_path(&root, skill);
            }
            if let Some(docs) = &workflow.docs {
                assert_path(&root, docs);
            }
        }
    }

    #[test]
    fn site_catalog_matches_catalog() {
        let catalog = load_embedded();
        let path = repo_root().join("site/src/data/capabilities.json");
        let document = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "missing {}; generate it with `cargo run -p eollo-protocol --bin catalog-json > site/src/data/capabilities.json`",
                path.display()
            )
        });
        let expected = serde_json::to_string_pretty(&catalog.site_catalog_json())
            .expect("catalog serializes to JSON");
        assert_eq!(
            document.trim(),
            expected.trim(),
            "site/src/data/capabilities.json is stale; regenerate it from the catalog"
        );
    }
}
