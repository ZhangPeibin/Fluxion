# Capability Lifecycle and Admission Spec

**Status:** draft  
**Release:** v3

## 1. Purpose

This spec defines how a capability enters, matures, and leaves Fluxion. It prevents ad hoc growth
while preserving the layer's breadth.

## 2. Status model

```text
proposed → experimental → stable → deprecated → removed
```

| Status | Callable | Meaning |
|---|---|---|
| `proposed` | no | Designed but not implemented. Must not appear in MCP `tools/list`. |
| `experimental` | yes | Callable; contract, chains, or workflow may change. |
| `stable` | yes | Contract is expected to remain compatible; tests and docs are complete. |
| `deprecated` | yes | Callable for a compatibility window; a replacement is declared. |
| `removed` | no | Not in the catalog or MCP. Historical docs only. |

## 3. Admission checklist

A capability may be added only when all items are true:

- [ ] It serves a clear agent/user outcome.
- [ ] It has a `domain.action` id.
- [ ] It declares status, execution, transports, chains, default chain, and side effects.
- [ ] It has a stable input/output schema.
- [ ] It returns envelope v2.
- [ ] It has a skill and a capability doc.
- [ ] It has tests, including an unsupported-chain case.
- [ ] It declares coverage dimensions and error codes.
- [ ] It declares prerequisites.
- [ ] It is added to the catalog.
- [ ] It is added to at least one workflow or explicitly marked standalone.
- [ ] It has an owner.
- [ ] Signing/broadcasting capabilities have a threat model and policy checkpoint.

## 4. Review requirements

| Capability type | Review |
|---|---|
| Read-only | Maintainer review; schema, docs, tests |
| Prepares calldata | Above + simulation/inspection expectations |
| Local session | Above + wallet boundary and session storage review |
| Signs | Above + explicit confirmation, policy, and threat model |
| Broadcasts | Above + simulation, inspection, policy, confirmation, and failure handling |

Security-sensitive capabilities must not be marked `stable` without a documented threat model.

## 5. Ownership

- Every capability has a maintainer.
- The maintainer owns the catalog entry, schema, skill, docs, and tests.
- Changes to `status`, `chains`, `side_effects`, or `execution` require maintainer review.
- Ownership is recorded in the capability doc or catalog `notes` until a dedicated field exists.

## 6. Deprecation

A deprecated capability must:

- remain callable for the compatibility window;
- declare a replacement or an explicit "no replacement" reason;
- keep its schema and docs available;
- return a deprecation warning in `warnings`;
- be removed from MCP `tools/list` only after the window.

## 7. Metrics

A capability should report or be testable for:

- latency;
- error rate by code;
- coverage status distribution;
- freshness status;
- data source failures;
- policy block rate (for signing/broadcasting).

Metrics are not a substitute for tests or docs.

## 8. Catalog updates

- A new capability starts as `proposed` or `experimental`.
- Promotion to `stable` requires complete schema, skill, docs, tests, and chain coverage.
- A capability cannot be `stable` with empty `tests`.
- A capability cannot declare a chain without an adapter and test.
- A workflow cannot be `stable` with `planned_steps` or `gaps`.

## 9. Removal

- Mark `deprecated` first.
- Keep v1/v2 schemas for the compatibility window.
- Announce the replacement and migration path.
- Remove code, catalog entry, MCP tool, CLI command, schema, skill, and docs together.
- Keep an ADR or changelog note explaining the removal.

## 10. References

- [ADR-0001](../../../adr/0001-fluxion-capability-runtime.md)
- [Catalog spec](catalog.md)
- [Registry spec](registry.md)
- [Workflows spec](workflows.md)
