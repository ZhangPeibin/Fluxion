# Fluxion capability runtime - Master Spec

**Status:** draft  
**Release:** v3  
**Scope:** capability catalog, registry, envelope v2, chain matrix, workflows, migration  
**Owners:** Fluxion maintainers

## 1. Purpose

Fluxion is a composable Web3 capability runtime for AI agents. This spec defines how the current
collection of capabilities becomes one layer:

- an agent can discover what Fluxion can do;
- every capability declares its chains, execution location, side effects, and contract;
- every capability returns the same envelope;
- agents can compose capabilities into workflows;
- documentation, schemas, skills, MCP, and CLI cannot drift from the runtime.

This is a platform refactor, not a feature cut. Swap, wallet, transaction, signature, yield,
position, and Four.meme remain part of the layer.

## 2. Problem

The project currently has working capabilities but no platform contract:

- Capability metadata is duplicated across Rust, MCP, CLI, docs, schemas, and skills.
- Chain defaults are inconsistent (`wallet` defaults to BSC; `yield`/`position` default to Ethereum;
  `swap` is hard-coded to Ethereum).
- Response shapes differ: `provenance`, `coverage`, `warnings`, `unknowns`, and errors are not
  uniform.
- Multi-step use is implicit in prose; there is no declared workflow.
- The catalog of capabilities is not machine-readable, so CI cannot detect drift.
- Documentation carries stale statements about delivery state and chain support.

The result is a set of endpoints that happens to live in one repository, not a layer an agent can
reason about.

## 3. Goals

1. Make the capability catalog the single source of truth.
2. Make every capability declare domain, action, status, chains, execution, and side effects.
3. Make every capability return envelope v2.
4. Make Ethereum and BSC support explicit through a chain matrix.
5. Make workflows first-class, with steps, checkpoints, and failure handling.
6. Make MCP, CLI, docs, schemas, and skills generated or validated from the catalog.
7. Make CI enforce workspace tests, catalog parity, and documentation links.

## 4. Non-goals

- Do not remove working capabilities.
- Do not add new chains or protocols in this refactor.
- Do not promise full Ethereum/BSC parity for every capability.
- Do not turn workflow recipes into hidden execution paths.
- Do not claim inspection is a safety guarantee.
- Do not build a general workflow DSL or a new agent runtime.

## 5. Target architecture

```text
Agent
  │ MCP stdio / CLI / HTTP
  ▼
Capability catalog  ── metadata, chain matrix, status, side effects
  │
  ▼
Capability registry ── id → handler + metadata
  │
  ├── swap.*        Ethereum
  ├── yield.*       Ethereum
  ├── position.*    Ethereum
  ├── fourmeme.*    BSC
  ├── wallet.*      Ethereum + BSC
  ├── transaction.* Ethereum + BSC
  └── signature.*   Ethereum + BSC
  │
  ├── local runtime  → RPC, WalletConnect, policy, simulation
  └── remote service → discovery, snapshot, yield intelligence
```

Execution location is fixed per capability and remains internal. Agents choose a chain and a
capability; they never choose local or remote.

## 6. Capability model

A capability is identified by `domain.action` and declares:

| Field | Meaning |
|---|---|
| `id` | Stable `domain.action` identifier |
| `domain` | `swap`, `wallet`, `transaction`, `signature`, `yield`, `position`, `fourmeme` |
| `action` | `quote`, `build`, `simulate`, `inspect`, `send`, `sign`, `read`, `discover`, `analyze` |
| `status` | `proposed`, `experimental`, `stable`, `deprecated` |
| `execution` | `local` or `remote`; internal only |
| `transports` | `mcp`, `cli`, `http` |
| `chains` | Declared chain support |
| `default_chain` | Optional default; must be in `chains` |
| `side_effects` | `read_only`, `local_session`, `prepares_calldata`, `signs`, `broadcasts` |
| `requires` | Prerequisites such as `wallet_session` or `swap.quote` |
| `schema` | JSON schema reference |
| `skill` | Agent usage instructions |
| `docs` | Capability contract |
| `tests` | Test selectors or paths |
| `workflows` | Workflow memberships |

The full field definition and validation rules live in the [catalog spec](catalog.md).

## 7. Catalog and registry

### Catalog

`capabilities/catalog.yaml` is the source of truth. It contains:

- `capabilities`: agent-callable actions.
- `surfaces`: non-capability entry points such as `fourmeme.ui`.
- `workflows`: ordered capability compositions.

### Registry

The runtime exposes a `CapabilityRegistry` that maps a capability id to metadata and a handler.
MCP and CLI adapters call the registry instead of maintaining separate dispatch tables.

The registry is specified in [registry.md](registry.md).

## 8. Chain strategy

Ethereum (`eip155:1`) and BSC (`eip155:56`) are the first supported chains.

- Every capability declares its chains.
- Unsupported chains return `UNSUPPORTED` with `details.supported_chains`.
- No silent fallback between chains.
- Every response declares the chain that produced it.
- Gaps such as BSC swap, BSC yield, and BSC position are explicit.

The matrix and defaults are specified in [chain-matrix.md](chain-matrix.md).

## 9. Envelope v2

Every capability uses one success envelope and one error envelope. The common fields are:

- `meta`: schema version, capability id, request id.
- `chain`: name and id.
- `block`: optional pinned block.
- `as_of`: observation time.
- `provenance`: sources, freshness, age.
- `coverage`: dimension map.
- `warnings` / `unknowns`: structured, code-bearing entries.
- `data` or `error`: capability payload or structured failure.

The exact shape and semantics are specified in [envelope-v2.md](envelope-v2.md).

## 10. Workflows

Workflows are ordered compositions of capabilities. The first workflows are:

- `swap`: quote → build → allowance → simulate → inspect → send → verify.
- `yield-deposit`: discover → build → inspect → simulate → send → position.
- `fourmeme-discovery`: agent-tokens → token → agent-id.
- `portfolio`: balances/assets/collectibles + position analysis.

Workflows preserve security order: simulate and inspect before send, policy before wallet
confirmation, no hidden signing. See [workflows.md](workflows.md).

## 11. Security boundary

- Read-only capabilities do not sign or broadcast.
- `prepares_calldata` capabilities return calldata and approvals but do not sign.
- `signs` and `broadcasts` capabilities require a wallet session and explicit user confirmation.
- `transaction.inspect` and `signature.inspect` remain advisory; they do not guarantee safety.
- Policy gates can block before the wallet request with `POLICY_BLOCKED`.
- Workflows must not skip inspection or policy checkpoints.

## 12. Rollout

| Phase | Deliverable | Exit criterion |
|---|---|---|
| 0 | CONTEXT and ADRs | Decisions accepted |
| 1 | Catalog and schema | CI validates all capability metadata |
| 2 | Registry and envelope v2 | All capabilities return the same envelope |
| 3 | Chain matrix enforcement | Unsupported chains fail explicitly |
| 4 | Workflows | Three workflows documented and tested |
| 5 | Docs and CI cleanup | README landing page, workspace CI, no stale docs |

The detailed migration is in [migration.md](migration.md).

## 13. Acceptance criteria

- `capabilities/catalog.yaml` lists all capabilities, surfaces, and workflows.
- Every capability has a schema, skill, doc, test, and declared chain support.
- MCP `tools/list`, CLI help, and `docs/capabilities/README.md` match the catalog.
- Every capability returns envelope v2.
- Unsupported chain calls return `UNSUPPORTED` with supported chains.
- At least three workflows have docs and end-to-end tests.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets` pass in CI.
- README is a landing page; `CONTEXT.md` and ADRs are linked.

## 14. Risks

| Risk | Mitigation |
|---|---|
| Envelope v2 is a breaking change | Pre-1.0 cutover; one-release deprecated v1 schemas; no dual code path |
| Catalog becomes stale | CI parity checks; catalog is the only metadata source |
| Workflow claims exceed capability support | `planned_steps` and `gaps` are explicit; workflows stay experimental until complete |
| BSC gaps look like missing support | Chain matrix and `UNSUPPORTED` details make gaps explicit |
| Monolithic CLI/MCP slows migration | Registry first; split handlers by capability afterwards |
| Security expectations inflate | Advisory/enforcement language remains explicit |

## 15. References

- [CONTEXT.md](../../../../CONTEXT.md)
- [Catalog spec](catalog.md)
- [Envelope v2 spec](envelope-v2.md)
- [Chain matrix spec](chain-matrix.md)
- [Registry spec](registry.md)
- [Workflows spec](workflows.md)
- [Migration spec](migration.md)
- [Lifecycle spec](lifecycle.md)
- [Capability catalog](../../../../capabilities/catalog.yaml)
