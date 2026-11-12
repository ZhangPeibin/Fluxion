# Security assurance hardening - Spec

**Status:** draft
**Release:** v2
**Capabilities:** `transaction.inspect`, `signature.inspect`

## Problem

The current inspection pipeline is deterministic and tested, but an external reviewer cannot yet
answer:

- Which ruleset produced this report?
- Is this exact report reproducible?
- Did more than one RPC agree on the block and target code?
- What resource limits prevent oversized inputs from exhausting the process?
- What is the published rule catalog and severity calibration?

Without those, the capability is a useful detector but not an auditable assurance product.

## Goal

Raise the security capability from “advisory detector” to **auditable pre-sign inspection**:

1. Every report carries provenance and a stable report ID.
2. Optional secondary RPC cross-checks block and code.
3. Input, log, candidate, and trace budgets are bounded.
4. A public rule catalog documents IDs, severity, evidence, and coverage.
5. The red-team corpus covers more asset and policy cases.
6. Decoder fuzz/property tests prove malformed input does not panic.

## Non-goals

- No claim of audit equivalence or complete exploit coverage.
- No signer-side enforcement in this phase.
- No real-funds transaction execution.
- No LLM risk decision.

## Required Behaviour

### 1. Assurance metadata

`transaction.inspect` adds:

```json
"assurance": {
  "ruleset_version": "2026-11-11.1",
  "report_id": "0x...",
  "deterministic": true,
  "cross_check": "match",
  "limitations": []
}
```

- `report_id` is a SHA-256 over the request, block hash, ruleset version, findings, coverage, and
  unknowns.
- Same request/block/code version produces the same ID.
- `cross_check` is `not_configured | match | mismatch | unavailable`.
- `limitations` lists missing coverage and trust boundaries that could change the conclusion.

### 2. Secondary RPC cross-check

Optional `FLUXION_VERIFY_RPC_URL`.

- Fetch the same block by number and compare block hash.
- Compare target code at the pinned tag.
- Mismatch adds a warning and `cross_check=mismatch`; it does not silently pass.
- Failure adds `cross_check=unavailable`; inspection continues.

### 3. Resource limits

- Max calldata: 256 KiB.
- Max simulated logs retained: 512.
- Max token/account verification pairs: 128.
- Max approval checks: 128.
- Max NFT owner checks: 128.
- Max trace depth: 32; max trace calls: 4096.
- Exceeding a limit returns a structured warning/unknown; never panic or allocate unboundedly.

### 4. Rule catalog

New `docs/capabilities/transaction/ASSURANCE.md`:

- rule IDs and meanings;
- severity and confidence calibration;
- evidence fields;
- coverage dependency;
- known false-positive/false-negative limits;
- red-team methodology.

### 5. Red-team expansion

Add at least:

- fake ERC721 Transfer log -> `EFFECT-010`;
- policy cap/allowlist cases;
- NFT control transfer;
- unknown proxy pattern stays unknown.

Report TP/FP/FN/unknown counts and per-case latency.

### 6. Decoder robustness

- Property test arbitrary hex/bytes through `decode_intent` without panic.
- Malformed multicall, swap path, dynamic bytes and oversized input tests.
- No unbounded recursion.

## Acceptance

- `assurance` present in schema, CLI/MCP JSON, docs and tests.
- Same input/block yields the same `report_id`.
- Secondary RPC match/mismatch/unavailable tested.
- Oversized calldata fails with `INVALID_ARGUMENT`.
- Red-team no longer has only the 15 original cases; fake NFT and policy are covered.
- `cargo fmt`, `clippy --workspace --all-targets`, `cargo test --workspace`, `forge fmt`,
  `forge build`, and `bench/security/redteam.py` pass.
