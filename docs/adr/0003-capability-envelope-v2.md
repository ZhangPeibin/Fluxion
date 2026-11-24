# ADR-0003: Every capability uses envelope v2

- **Status:** Accepted
- **Date:** 2026-11-20
- **Deciders:** Eollo maintainers

## Context

Current capability responses are inconsistent:

- `swap.quote` has `meta` and `provenance`.
- `transaction.inspect` has `coverage` and `unknowns`.
- `wallet.*`, `yield.*`, `position.analyze`, and `fourmeme.*` each define their own
  `coverage`/`warnings` shapes.
- Some responses expose `chain` as a string plus `chain_id`; others differ.
- Error responses are not uniform.

An agent should not have to learn a new result shape for every tool. The layer needs one envelope.

## Decision

Adopt **capability envelope v2** for every capability:

```json
{
  "meta": {"schema_version": 2, "capability": "swap.quote", "request_id": "..."},
  "chain": {"name": "ethereum", "id": 1},
  "block": {"number": 123, "hash": "0x..."},
  "as_of": "2026-11-20T10:00:00Z",
  "provenance": {"sources": [], "freshness": "fresh", "age_secs": 3},
  "coverage": {"dimensions": {"data": "complete"}},
  "warnings": [],
  "unknowns": [],
  "data": {}
}
```

Errors use the same envelope with `error` instead of `data`.

The migration is a one-time cutover:

- v2 schemas are the target for all capabilities.
- v1 schemas remain available for one release and are marked `deprecated`.
- The runtime does not maintain two response code paths.
- MCP maps the envelope to text content and, where supported, `structuredContent`.
- HTTP returns the envelope unchanged.

## Consequences

- Every capability schema, skill, doc, and test must be updated.
- `coverage` becomes a generic dimension map with a shared status vocabulary.
- `warnings` and `unknowns` become structured objects with codes.
- The MCP bridge and CLI human output need a shared renderer.
- This is a breaking change, accepted because the project is pre-1.0.

## Alternatives considered

- **Incremental additive fields.** Rejected: it preserves divergence and doubles the long-term
  surface.
- **Per-domain envelopes.** Rejected: agents still need per-domain knowledge.
- **Keep current shapes and document them.** Rejected: documentation cannot fix machine
  inconsistency.

## References

- [Envelope v2 spec](../feature/v3/capability-runtime/envelope-v2.md)
- [Migration spec](../feature/v3/capability-runtime/migration.md)
