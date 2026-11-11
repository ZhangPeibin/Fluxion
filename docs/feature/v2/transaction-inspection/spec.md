# transaction.inspect - Technical Spec

**Status:** draft
**Release:** v2
**Capability:** `transaction.inspect`
**Execution:** local
**Default chain:** BSC
**Chains:** BSC (`eip155:56`), Ethereum (`eip155:1`)

## 1. Purpose

`transaction.inspect` answers a narrow question before signing:

> What is this transaction trying to do, what effects are observable, which risks are known, and
> what remains unknown?

It does not answer “is this safe?” and it never signs or broadcasts.

## 2. Product Rules

- The report is deterministic for the same chain, block, request, and decoder/rules version.
- Missing coverage produces `unknown`, not `low`.
- Findings carry rule IDs and evidence; no finding is emitted without evidence.
- LLM explanation is optional, receives only the structured report, and cannot mutate it.
- A successful inspection does not authorize execution.
- Unknown selectors, unverified contracts, unsupported proxy patterns, and unavailable trace data
  are first-class outputs.

## 3. Request

```json
{
  "chain": "bsc",
  "from": "0x...",
  "to": "0x...",
  "value": "0",
  "data": "0x...",
  "block": 132893125
}
```

Rules:

- `chain` defaults to BSC.
- `from` is optional for pure decoding, but recommended and required for effect-oriented findings.
- `to` and `data` are required. Native transfers use `data: "0x"`.
- `value` is human native units (`BNB`/`ETH`), converted without floats.
- `block` pins the inspection state; otherwise the latest block is used.

## 4. Response

```json
{
  "chain": "bsc",
  "chain_id": 56,
  "block_number": 132893125,
  "block_hash": "0x...",
  "from": "0x...",
  "to": "0x...",
  "value": {
    "raw": "0",
    "decimal": "0"
  },
  "intent": {
    "selector": "0x095ea7b3",
    "signature": "approve(address,uint256)",
    "name": "approve",
    "args": [
      {"name": "spender", "type": "address", "value": "0x..."},
      {"name": "amount", "type": "uint256", "value": "115792089..."}
    ]
  },
  "target": {
    "address": "0x...",
    "kind": "contract",
    "verified": true,
    "proxy": false,
    "implementation": null
  },
  "effects": {
    "native": [],
    "tokens": [],
    "approvals": [],
    "nfts": []
  },
  "findings": [],
  "risk": "unknown",
  "coverage": {
    "decode": "complete",
    "metadata": "complete",
    "simulation": "not_run",
    "policy": "not_configured"
  },
  "unknowns": [
    "asset effects were not simulated"
  ],
  "sources": [],
  "warnings": []
}
```

### 4.1 Risk

```text
critical | high | medium | low | unknown
```

Aggregation:

1. The highest finding severity wins when coverage is sufficient for that finding.
2. If required coverage is missing, the report is `unknown` unless a higher finding already exists.
3. `low` means “no known finding within declared coverage”; it does not mean safe.
4. There is no `safe` enum value.

### 4.2 Findings

```json
{
  "id": "AUTH-001",
  "category": "approval",
  "severity": "high",
  "title": "Unlimited token approval",
  "description": "The transaction grants an unlimited allowance.",
  "evidence": {
    "token": "0x...",
    "spender": "0x...",
    "amount": "max"
  },
  "confidence": "high"
}
```

Every finding must include:

- stable rule ID;
- category;
- severity;
- short title;
- evidence;
- confidence (`high`, `medium`, `low`);
- whether it is deterministic or heuristic.

### 4.3 Coverage

Coverage values:

```text
complete | partial | not_run | unavailable | not_configured
```

Coverage dimensions:

- `decode`: selector and argument decoding.
- `metadata`: contract type, verification, proxy/implementation.
- `simulation`: state effects and allowance deltas.
- `policy`: user/integrator policy evaluation.

`unknowns` explains every non-complete dimension that could change the conclusion.

## 5. Architecture

```text
request
  -> canonicalize chain, block, tx
  -> decode selector / ABI / args
  -> target metadata / proxy / verification
  -> simulate and trace effects when available
  -> deterministic detector pipeline
  -> coverage and unknowns
  -> structured report
  -> optional host-agent explanation
```

### 5.1 Deterministic detector pipeline

Detectors run in a stable order and see the same immutable `InspectionContext`:

```text
InspectionContext {
  request, block, target, bytecode, intent, effects, metadata
}
```

Each detector returns zero or more findings plus its coverage. A detector must never mutate the
request or another detector’s findings.

### 5.2 Simulation

Preferred order:

1. RPC simulation with state diff and logs (`eth_simulateV1`, `debug_traceCall`, or equivalent).
2. `eth_call` success/revert plus explicitly reduced coverage.
3. No simulation: `coverage.simulation = unavailable` and `unknowns` populated.

Public BSC/Ethereum RPC support for trace APIs must be verified before relying on them. A provider
adapter may be added later, but the report must disclose which backend was used.

### 5.3 Explanation

The default capability returns structured findings only. If `inspect.explain` is implemented:

- it receives the structured report, not raw calldata;
- it is tool-less and has no key access;
- it cannot add, remove, or downgrade findings;
- its output is marked `explanation`, not `risk`;
- deterministic templates remain available when no model is configured.

## 6. Detector Sub-Specs

Foundation:

- [core](detectors/core.md) - request/response, capability plumbing, block pinning, coverage.
- [decode](detectors/decode.md) - selector, ABI, argument and intent decoding.
- [metadata](detectors/metadata.md) - contract type, verification, proxy and implementation.
- [effects](detectors/effects.md) - native/token/NFT/approval deltas from simulation.
- [eval](detectors/eval.md) - fixtures, false-positive/false-negative measurement.

Detectors:

- [approval](detectors/approval.md) - approvals, Permit2, NFT operator approval.
- [asset-flow](detectors/asset-flow.md) - unexpected outflows, net flow, fee-on-transfer.
- [contract](detectors/contract.md) - unverified/new/proxy/upgrade/delegatecall risk.
- [batch](detectors/batch.md) - multicall, Safe batches and hidden nested calls.
- [address](detectors/address.md) - zero/burn/self/session mismatch/allowlist.
- [swap](detectors/swap.md) - router path, minOut, deadline, recipient and price impact.
- [signature](detectors/signature.md) - EIP-2612 permit calldata in `transaction.inspect`;
  typed data, Permit2 signatures, EIP-7702 and 4337 are exposed by the companion
  `signature.inspect` capability.
- [policy](detectors/policy.md) - spend/chain/target/method/spender constraints.
- [explain](detectors/explain.md) - optional plain-language explanation contract.

## 7. Rule ID Convention

```text
CORE-xxx
DECODE-xxx
META-xxx
EFFECT-xxx
AUTH-xxx
FLOW-xxx
CONTRACT-xxx
BATCH-xxx
ADDR-xxx
SWAP-xxx
SIG-xxx
POLICY-xxx
```

IDs are append-only. Changing a rule’s meaning requires a new ID.

## 8. Implementation Order

Each item is a separately reviewed change.

1. `inspect.core` + basic selector decoding: capability contract, schema, CLI/MCP, block pinning,
   report, coverage, unknown handling, tests.
2. `inspect.decode`: ABI signatures, static args, dynamic arg handling, unknown selectors.
3. `inspect.metadata`: contract/EOA, verified source, proxy/implementation, new contract.
4. `inspect.effects`: trace/state-diff spike and asset/allowance effects; reduced coverage fallback.
5. `inspect.approval`: approval findings.
6. `inspect.asset-flow`: asset-flow findings.
7. `inspect.contract`: contract/proxy/delegatecall findings.
8. `inspect.batch`: nested-call expansion.
9. `inspect.address`: address findings.
10. `inspect.swap`: swap-specific checks when swap construction exists.
11. `inspect.signature`: signature-based authorization checks.
12. `inspect.policy`: advisory policy evaluation.
13. `inspect.explain`: optional templated/LLM explanation.
14. `inspect.eval`: gate for any claim beyond deterministic coverage.

## 9. First Implementation Slice

The first slice is intentionally small:

- add `transaction.inspect`;
- fetch and pin the block;
- decode empty calldata as a native transfer;
- decode a built-in set of common selectors and static arguments;
- return unknown selectors without guessing;
- return `risk: unknown`, no findings, and coverage that states simulation/metadata/policy were not
  run;
- add CLI, MCP, schema, docs, skill, unit tests and a mocked RPC test.

This slice must not claim safety and must not require a trace-capable RPC.

## 10. Acceptance

- `transaction.inspect` is available via CLI and MCP.
- BSC is the default; Ethereum works with `--chain ethereum`.
- Known selectors decode into signature/name/args.
- Unknown selector returns `signature: null`, `name: null`, empty args, and an unknown entry.
- Empty data returns a native-transfer intent.
- Response is block-pinned.
- `risk` is never `safe`.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets`, and
  `cargo test --workspace` pass.
- Schema, docs, MCP tool list, skill and capability registry are updated.

## 11. Evaluation Gate

Before adding a “risk” conclusion beyond deterministic decoding:

- maintain a labeled fixture corpus of malicious and benign transactions;
- measure false negatives and false positives per detector;
- include adversarial calldata, proxy upgrades, multicall wrappers, Permit2 and approval cases;
- red-team the LLM explanation separately from the deterministic detector;
- publish coverage and known limitations in the capability docs.

## 12. Non-Goals

- No guarantee of safety.
- No automatic signing or execution.
- No protocol audit.
- No MEV or economic attack simulation.
- No cross-chain intent analysis.
- No inference that an unverified contract is safe because simulation succeeded.
