# ADR-0005: The capability catalog is the single source of truth

- **Status:** Accepted
- **Date:** 2026-11-20
- **Deciders:** Eollo maintainers

## Context

Capability metadata is currently duplicated across:

- `CapabilityId` and `CapabilityDescriptor` in Rust.
- Hand-written MCP `tools/list` definitions.
- CLI help text.
- `docs/capabilities/` pages.
- `schemas/`.
- `skills/`.
- `docs/MCP.md`.

These copies drift. Examples: the catalog names 23 capabilities while the MCP file header still
mentions only `swap_quote`; chain defaults differ by capability; docs claim different delivery
states.

## Decision

`capabilities/catalog.yaml` is the **single source of truth** for capability metadata:

- id, domain, action, status, summary
- execution location, transports
- chains, default chain, chain requirement
- side effects, prerequisites
- schema, skill, docs, tests
- workflow membership
- non-capability surfaces (for example `fourmeme.ui`)

A JSON schema at `schemas/capability_catalog.v2.json` validates the catalog. CI enforces parity:

- Every catalog capability has a schema, skill, doc, and test.
- Every MCP tool maps to a catalog capability.
- Every CLI command maps to a catalog capability.
- Workflows reference existing capabilities.
- Surfaces are not treated as capabilities.
- Documented chain support matches the runtime.

Code may keep a typed `CapabilityId` enum for compile-time safety, but a parity test must prove it
matches the catalog exactly.

## Consequences

- New capability work starts with a catalog entry, not a tool definition.
- MCP, CLI, docs, schemas, and skills become generated or validated surfaces.
- The catalog becomes a review artifact; changes require an owner and status.
- A CI job is required to prevent the catalog from becoming another stale document.

## Alternatives considered

- **Docs-only catalog.** Rejected: it cannot enforce parity.
- **Code-first metadata.** Rejected: it leaves docs and skills behind.
- **Per-capability manifest files.** Rejected for now: a single catalog is easier to review and
  validate at this size.

## References

- [Catalog spec](../feature/v3/capability-runtime/catalog.md)
- [Capability catalog](../../capabilities/catalog.yaml)
- [Catalog schema](../../schemas/capability_catalog.v2.json)
