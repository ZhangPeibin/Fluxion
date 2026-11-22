# Capability Catalog Spec

**Status:** draft  
**Release:** v3  
**Artifact:** `capabilities/catalog.yaml`  
**Schema:** `schemas/capability_catalog.v2.json`

## 1. Purpose

The catalog is the machine-readable source of truth for:

- which capabilities exist;
- what each capability does;
- which chains it supports;
- where it runs;
- what side effect it has;
- which schema, skill, docs, and tests belong to it;
- which workflows it participates in;
- which user-facing surfaces are not capabilities.

The catalog does not replace capability docs or skills. It defines the facts those documents must
agree with. The `schema` and `skill` fields also act as the schema and skill index.

## 2. File layout

```text
capabilities/
  catalog.yaml
schemas/
  capability_catalog.v2.json
```

The catalog is reviewed like code. A capability change without a catalog change is incomplete.

## 3. Top-level shape

```yaml
version: 2
updated: 2026-11-20
envelope: v1
capabilities: []
surfaces: []
workflows: []
```

| Field | Required | Meaning |
|---|---|---|
| `version` | yes | Catalog format version. Currently `2`. |
| `updated` | yes | ISO date of the last review. |
| `envelope` | yes | Current response envelope for catalog entries: `v1` or `v2`. During migration this is `v1`. |
| `capabilities` | yes | Agent-callable capabilities. |
| `surfaces` | no | Entry points that are not capabilities. |
| `workflows` | no | Ordered compositions. |

## 4. Capability entry

```yaml
- id: swap.quote
  domain: swap
  action: quote
  status: stable
  summary: Exact-in swap quote on Ethereum.
  execution: remote
  transports: [mcp, cli, http]
  chains: [ethereum]
  default_chain: ethereum
  side_effects: read_only
  requires: []
  schema: schemas/swap_quote.v1.json
  skill: skills/swap/SKILL.md
  docs: docs/capabilities/swap/README.md
  tests:
    - crates/client/tests/client_remote.rs
    - tests/integration_mainnet.rs
  workflows: [swap]
  coverage_dimensions: [data, freshness]
```

| Field | Required | Rules |
|---|---|---|
| `id` | yes | Must equal `<domain>.<action>` and be unique. |
| `domain` | yes | One of `swap`, `wallet`, `transaction`, `signature`, `yield`, `position`, `fourmeme`. |
| `action` | yes | One of `quote`, `build`, `simulate`, `inspect`, `send`, `sign`, `read`, `discover`, `analyze`, `connect`, `disconnect`, `status`, `agent-id`, `withdraw`. |
| `status` | yes | `proposed`, `experimental`, `stable`, or `deprecated`. |
| `summary` | yes | One sentence; no marketing claims. |
| `envelope` | no | `v1` (default) or `v2`. Use `v2` only after the capability's v2 schema and tests exist. |
| `execution` | yes | `local` or `remote`. Must match the execution-routing table. |
| `transports` | yes | Non-empty subset of `mcp`, `cli`, `http`. |
| `chains` | yes | Non-empty subset of `ethereum`, `bsc`. |
| `default_chain` | no | If present, must be in `chains`. |
| `chain_required` | no | Boolean. If true, callers must pass `chain`; no default. |
| `side_effects` | yes | `read_only`, `local_session`, `prepares_calldata`, `signs`, or `broadcasts`. |
| `requires` | no | Capability ids or environment prerequisites such as `wallet_session`. |
| `schema` | yes | Path to a schema file. Must exist. |
| `skill` | yes | Path to a skill file. Must exist. |
| `docs` | yes | Path to a capability doc. Must exist. |
| `tests` | yes | Non-empty for `stable`; may be empty for `experimental` with a warning. |
| `workflows` | no | Workflow ids that include this capability. |
| `coverage_dimensions` | yes | Subset of standard dimensions. May be empty when the capability has no coverage dimensions, for example local session operations. |
| `notes` | no | Short clarification, not a substitute for docs. |

## 5. Execution routing

| Capability | Execution |
|---|---|
| `swap.quote` | remote |
| `yield.discover` | remote |
| `yield.history` | remote |
| `position.analyze` | remote |
| all other capabilities | local |

The catalog must fail validation if an entry contradicts this table.

## 6. Status semantics

| Status | Meaning |
|---|---|
| `proposed` | Designed but not callable. It must not appear in MCP `tools/list`. |
| `experimental` | Callable, but contract, chain support, or workflow may change. |
| `stable` | Contract is expected to remain compatible; tests and docs are complete. |
| `deprecated` | Callable for a compatibility window; a replacement must be declared. |

## 7. Side-effect semantics

| Side effect | Meaning | Confirmation |
|---|---|---|
| `read_only` | Reads chain or provider data; no signing, no transaction. | None |
| `local_session` | Creates or removes a local wallet session. | User action in the wallet flow |
| `prepares_calldata` | Returns calldata or approval steps; does not sign. | None, but inspect before send |
| `signs` | Requests a wallet signature. | Explicit phone confirmation |
| `broadcasts` | Sends a transaction or transfer. | Simulation + inspect + policy + confirmation |

A capability with `signs` or `broadcasts` must declare `wallet_session` in `requires`.

## 8. Workflow entry

```yaml
- id: swap
  summary: Quote, build, inspect, and execute a swap.
  status: experimental
  chains: [ethereum]
  steps:
    - swap.quote
    - swap.build
    - wallet.allowance
    - transaction.simulate
    - transaction.inspect
    - transaction.send
  planned_steps:
    - transaction.verify
  skill: skills/swap/SKILL.md
  docs: docs/feature/v3/capability-runtime/workflows.md
  gaps:
    - swap.build supports V2 only
    - transaction.verify is not implemented
```

Rules:

- `steps` must reference existing capability ids.
- `planned_steps` must not reference existing capability ids; they are future work.
- Every workflow must have a `skill` and `docs`.
- A workflow with `planned_steps` or `gaps` must be `experimental`.
- Workflows must declare `chains` that are the intersection of their steps' chain support.

## 9. Surface entry

```yaml
- id: fourmeme.ui
  kind: local_web_ui
  capability: false
  status: experimental
  chains: [bsc]
  transports: [cli]
  docs: docs/capabilities/fourmeme/README.md
```

Surfaces are not listed in MCP `tools/list` and do not count as capabilities. They exist so that
user-facing entry points are not orphaned from the catalog.

## 10. Validation rules

CI must enforce:

1. `catalog.yaml` is valid YAML and matches `schemas/capability_catalog.v2.json`.
2. Capability ids are unique and equal `<domain>.<action>`.
3. Every `schema`, `skill`, `docs`, and `tests` path exists.
4. Every standard capability has exactly one catalog entry.
5. `CapabilityId::all()` matches the catalog id set exactly.
6. Every catalog capability has an MCP tool unless `transports` excludes `mcp`.
7. Every catalog capability has a CLI command unless `transports` excludes `cli`.
8. `docs/capabilities/README.md` matches the catalog table generated between
   `<!-- catalog-table:start -->` and `<!-- catalog-table:end -->`.
9. Every workflow step exists; every `planned_steps` entry does not.
10. `surfaces` do not appear in `capabilities`.
11. `execution` matches the routing table.
12. `default_chain` is in `chains`.
13. `chain_required` and `default_chain` are not both set.
14. `stable` capabilities have non-empty `tests`.
15. `deprecated` capabilities declare `replacement`.
16. A capability with `envelope: v2` references a `.v2.json` schema.
17. `fourmeme.ui` remains a surface, not a capability.

## 11. Ownership and review

- Every capability entry has an implicit owner: the maintainer who last changed its schema or
  handler.
- Changes to `status`, `chains`, `side_effects`, or `execution` require maintainer review.
- Adding a capability requires the lifecycle admission checklist.
- Removing a capability requires a deprecation window and migration notes.

## 12. References

- [Lifecycle spec](lifecycle.md)
- [Chain matrix spec](chain-matrix.md)
- [Envelope v2 spec](envelope-v2.md)
- [Capability catalog](../../../../capabilities/catalog.yaml)
- [Catalog schema](../../../../schemas/capability_catalog.v2.json)
