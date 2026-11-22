# Envelope v2 Migration Spec

**Status:** draft  
**Release:** v3  
**From:** v1 capability responses  
**To:** envelope v2

## 1. Purpose

This spec defines the one-time migration from the current mixed response shapes to envelope v2.
The migration covers 23 capabilities and their schemas, MCP tools, CLI output, skills, docs, and
tests.

## 2. Principles

- One runtime response path; no v1/v2 branching in handlers.
- v1 schemas remain as deprecated artifacts for one release, then are removed.
- Read-only capabilities migrate first; signing and broadcasting migrate last.
- Every step has a parity test.
- No capability is marked `stable` until its v2 schema, skill, docs, and tests are complete.

## 3. Common v1 → v2 changes

| v1 | v2 |
|---|---|
| `chain` string + `chain_id` | `chain: {name, id}` |
| Payload fields at top level | Payload under `data` |
| Per-capability coverage shapes | `coverage.dimensions` map |
| String warnings | Structured `warnings` entries |
| String unknowns | Structured `unknowns` entries |
| Mixed provenance | `provenance` object with sources/freshness/age |
| No common `as_of` | `as_of` timestamp |
| Mixed error shapes | `error` object with `code`, `message`, `retryable`, `details` |

## 4. Capability mapping

| Capability | v1 schema | v2 changes | Notes |
|---|---|---|---|
| `swap.quote` | `swap_quote.v2.json` | Migrated: payload under `data`, block/provenance on the envelope | Remote |
| `swap.build` | `swap_build.v2.json` | Migrated: payload under `data`, `quote_block` on the envelope | Local |
| `wallet.balance` | `wallet_balances.v1.json` | Migrated to `wallet_balances.v2.json`: `chain` object, payload under `data`, structured warnings | First v2 vertical slice |
| `wallet.assets` | `wallet_assets.v2.json` | Migrated: `chain` object, payload under `data`, structured warnings | Wallet read slice |
| `wallet.allowance` | `wallet_allowances.v2.json` | Migrated: `chain` object, payload under `data`, structured warnings | Wallet read slice |
| `wallet.collectibles` | `wallet_collectibles.v2.json` | Migrated: `chain` object, payload under `data`, structured warnings | Wallet read slice |
| `wallet.connect` | `wallet_connect.v2.json` | Migrated: `chain` object, payload under `data` | Session, not chain transaction |
| `wallet.status` | `wallet_status.v2.json` | Migrated: `chain` object, payload under `data` | |
| `wallet.disconnect` | `wallet_disconnect.v2.json` | Migrated: `chain` object, payload under `data` | |
| `wallet.agent-id` | `wallet_agent_id.v2.json` | Migrated: `chain` object, payload under `data` | BSC only |
| `wallet.sign` | `wallet_sign.v2.json` | Migrated: `chain` object, payload under `data` | Signing |
| `wallet.send` | `wallet_send.v2.json` | Migrated: `chain` object, payload under `data`, nested block | Broadcasting |
| `transaction.simulate` | `transaction_simulate.v2.json` | Migrated: `chain` object, `block` object, payload under `data` | Read-only slice |
| `transaction.inspect` | `transaction_inspect.v2.json` | Migrated: `chain` object, normalized coverage dimensions, structured unknowns | Advisory |
| `transaction.send` | `transaction_send.v2.json` | Migrated: `chain` object, payload under `data`, nested block/coverage | Broadcasting |
| `signature.inspect` | `signature_inspect.v2.json` | Migrated: `chain` object, normalized coverage dimensions, structured unknowns | Advisory |
| `fourmeme.agent-tokens` | `fourmeme_agent_tokens.v2.json` | Migrated: `chain` object, payload under `data`, structured warnings | BSC only |
| `fourmeme.token` | `fourmeme_token.v2.json` | Migrated: `chain` object, payload under `data`, structured warnings | BSC only |
| `yield.discover` | `yield_discover.v2.json` | Migrated: `chain` object, payload under `data`, provenance/freshness | Remote |
| `yield.build` | `yield_build.v2.json` | Migrated: `chain` object, payload under `data` | Local |
| `yield.history` | `yield_history.v2.json` | Migrated: `chain` object, payload under `data`, provenance/freshness | Remote |
| `yield.withdraw` | `yield_withdraw.v2.json` | Migrated: `chain` object, payload under `data` | Local |
| `position.analyze` | `position_analyze.v2.json` | Migrated: `chain` object, payload under `data`, provenance/freshness | Remote |

## 5. Schema migration

- Add `schemas/<capability>.v2.json` for every capability.
- The v2 schema must reference the common envelope definitions, not copy them.
- Keep `schemas/<capability>.v1.json` with a `deprecated: true` note for one release.
- The catalog points to v2 after the capability migrates; until then it points to v1 and
  `envelope: v1`.

## 6. Surface migration

### MCP

- `tools/list` metadata is validated against the catalog.
- `tools/call` returns the v2 envelope in the text content block.
- Add or update tests for every tool.

### CLI

- `--json` prints the v2 envelope.
- Human renderers read the envelope, not the old top-level fields.
- Update help text to show chain support and side effects.

### Skills

- Update each skill to describe the v2 envelope.
- Add chain constraints and workflow sections.
- Remove any statement that contradicts the catalog.

### Docs

- Update each `docs/capabilities/<name>/README.md` with the v2 shape.
- Update `docs/MCP.md`, `docs/API.md`, and `docs/CONFIGURATION.md`.
- Update `README.md` capability tables and links.

## 7. Rollout order

1. Catalog and parity tests (no response change).
2. Read-only capabilities: swap.quote, wallet reads, transaction.simulate/inspect,
   signature.inspect, fourmeme.*, yield.discover/history, position.analyze.
3. Prepare-only capabilities: swap.build, yield.build, yield.withdraw.
4. Session capabilities: wallet.connect/status/disconnect.
5. Signing and broadcasting: wallet.sign, wallet.send, transaction.send.
6. Remove v1 schemas after the compatibility window.

## 8. Compatibility window

- v1 schemas remain for one release and are marked deprecated.
- v1 response shapes are not produced by the runtime after cutover.
- External callers that need v1 must pin the previous release.
- The migration notes list the field mapping.

## 9. Test matrix

| Area | Required test |
|---|---|
| Catalog | id parity with `CapabilityId::all()` |
| Schema | every capability has a valid v2 schema |
| MCP | every tool returns the envelope |
| CLI | `--json` returns the envelope |
| Error | unsupported chain, invalid argument, policy block, timeout |
| Workflow | swap, yield-deposit, fourmeme-discovery happy path and one failure path |
| Compatibility | v1 schema files still parse, and are marked deprecated |

## 10. Rollback

- Keep the previous release tag available.
- If v2 breaks a critical path, revert the release, not individual handlers.
- Do not reintroduce dual response paths in the runtime.

## 11. References

- [ADR-0003](../../../adr/0003-capability-envelope-v2.md)
- [Envelope v2 spec](envelope-v2.md)
- [Catalog spec](catalog.md)
