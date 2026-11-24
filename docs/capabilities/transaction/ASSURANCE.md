# Transaction inspection assurance

This document defines the rule catalog, severity calibration, evidence model, coverage semantics,
and test boundary for `transaction.inspect` and `signature.inspect`.

It is not an audit report and does not claim complete exploit coverage.

## Assurance object

Every `transaction.inspect` response includes:

```json
"assurance": {
  "ruleset_version": "2026-11-11.1",
  "report_id": "0x...",
  "deterministic": true,
  "cross_check": "not_configured",
  "limitations": []
}
```

- `ruleset_version` changes when rule semantics or severity calibration changes.
- `report_id` is SHA-256 over request, block hash, ruleset version, findings, coverage, unknowns,
  and cross-check status. It is stable for the same request/block/code version.
- `cross_check` is `not_configured`, `match`, `mismatch`, or `unavailable`.
- `limitations` names missing coverage and trust assumptions that may change the conclusion.
- `policy` reports `mode`, `decision`, `blocked`, and reasons. `wallet.send --policy` uses the same
  decision and fails with `POLICY_BLOCKED` before the wallet request.

## Rule catalog

### Approval and authorization

| ID | Severity | Meaning | Evidence |
|---|---|---|---|
| `AUTH-001` | high | Unlimited ERC20 approval | token, spender, amount, selector |
| `AUTH-004` | high | NFT `setApprovalForAll(true)` | collection, operator |
| `SIG-002` | high | Permit deadline/expiration is zero or expired | deadline, expiration |
| `SIG-003` | high | Unlimited Permit2 amount or zero spender | spender, amount |
| `SIG-004` | high | EIP-7702 authorization present/mismatched/zero address | count, chain, address |
| `SIG-005` | medium | ERC-4337 paymaster or account factory | userOp fields |
| `SIG-006` | high/medium | Undecodable authorization or invalid userOp sender | input |

### Effects and asset flow

| ID | Severity | Meaning | Evidence |
|---|---|---|---|
| `EFFECT-007` | high | Simulated Transfer logs disagree with balance delta | token, account, expected/actual |
| `EFFECT-009` | high | Approval log disagrees with allowance | token, owner, spender, expected/actual |
| `EFFECT-010` | high | NFT Transfer log disagrees with `ownerOf` | collection, token id, expected/actual |
| `FLOW-001` | high | Native value on a known non-payable call | from, to, value |
| `FLOW-002` | high | Outflow exceeds decoded amount | token, sender, expected/actual |
| `FLOW-003` | high | Token outflow during an approval | from, target |
| `FLOW-004` | high | Recipient differs from decoded calldata | token, expected/actual recipient |
| `FLOW-005` | medium | Recipient receives less than decoded amount | token, expected/actual |
| `FLOW-006` | high | NFT transfer during an approval | from, collection |

### Contract and addresses

| ID | Severity | Meaning | Evidence |
|---|---|---|---|
| `CONTRACT-001` | medium/high | Source provider reports unverified contract | target, verified=false |
| `CONTRACT-003` | low/high | EIP-1967 proxy target; high for `upgradeTo` | target, implementation |
| `CONTRACT-004` | high | Unexpected internal `DELEGATECALL` | from, to, input, depth |
| `ADDR-001` | high | Zero address target/recipient | role, address |
| `ADDR-002` | high | Burn address target/recipient | role, address |
| `ADDR-005` | low | Target equals sender | address |
| `ADDR-007` | high | Address matches operator threat-intel | role, address, label, source |

### Swap and policy

| ID | Severity | Meaning | Evidence |
|---|---|---|---|
| `SWAP-001` | high | `amountOutMin == 0` | minOut |
| `SWAP-004` | high | Swap recipient differs from sender | sender, recipient |
| `SWAP-007` | medium | Unknown router | router |
| `POLICY-001` | high | Chain outside policy | chain |
| `POLICY-002` | high | Native spend exceeds cap | value, max |
| `POLICY-003` | high | Token spend exceeds cap | token, observed, max |
| `POLICY-004` | high | Target/method/spender outside allowlist | target/method/spender |
| `POLICY-005` | n/a | Policy not configured | `coverage.policy` |
| `POLICY-006` | high | Enforce policy cannot verify effects | simulation state |

## Severity calibration

- `critical`: not currently emitted.
- `high`: direct loss, unlimited approval, unexpected delegatecall, unverified target on an
  asset-moving call, forged logs with state mismatch.
- `medium`: missing data that materially reduces confidence, finite but policy-relevant risk,
  paymaster/factory deployment.
- `low`: informational proxy target or self-target.
- `unknown`: no finding within missing or incomplete coverage. This is not low risk.

## Coverage dependency

| Rule family | Requires |
|---|---|
| `AUTH-*` | decode |
| `EFFECT-*` | simulation + balance/allowance/owner reads |
| `FLOW-*` | decode complete + effects |
| `CONTRACT-*` | metadata; `CONTRACT-004` requires trace |
| `ADDR-*` | decode |
| `SWAP-*` | decode |
| `SIG-*` | calldata or signature input |
| `POLICY-*` | effects for spend caps |

If a prerequisite is unavailable, the rule is not guessed; the report adds `unknown` and the
corresponding coverage value.

## Trust and limitations

### Trust boundaries

| Component | Assumption | Failure / limit |
|---|---|---|
| Caller (agent or user) | Untrusted planner; can construct arbitrary calldata and bypass local MCP by calling the CLI or RPC directly | Not treated as a security boundary |
| Phone wallet / WalletConnect | Private keys and final signature confirmation stay in the wallet | Rejection -> `USER_REJECTED`; timeout -> `TIMEOUT` |
| Local runtime | No keys; read-only RPC; process and config are controlled by the local machine | Does not defend against a malicious local process or tampered binary |
| RPC | Responses are used for block, code, simulation and trace; no light-client proof | Failure -> `RPC_ERROR` or explicit `unavailable` |
| Remote quote service | Returns quotes over a published immutable snapshot | Failure -> `RPC_ERROR`; no local quote fallback |
| Source verifier | Optional BscScan / Etherscan; returns `verified: true/false/null` | Missing key or request failure -> `null`, never inferred |
| Trace backend | Optional `debug_traceCall`; public BSC RPC does not provide it | Unsupported -> `coverage.trace = unavailable` |

- RPC data is trusted for block, code, simulation, and trace. No light-client proof is used.
- `EOLLO_VERIFY_RPC_URL` can cross-check block hash and target code, but not every state read.
- `EOLLO_THREAT_INTEL_FILE` is operator-provided and may be incomplete or stale. Configure
  `EOLLO_THREAT_INTEL_PUBKEY` to require an Ed25519-signed feed.
- `debug_traceCall` is optional; public BSC RPC does not provide it.
- Source verification is optional; missing key or provider failure is `verified=null`.
- Balance/allowance verification depends on standard getters; non-standard tokens degrade to
  unknown.
- Native gas, full state diff, MEV, oracle manipulation, and protocol economics are out of scope.
- A clean report is not a safety guarantee.

## Threat model

Covered:

- Malicious or malformed calldata: unknown selectors, wrong argument layout, hidden multicall,
  proxy upgrades.
- Assets and approvals: unintended transfer, forged Transfer/Approval logs, unlimited approval,
  NFT operator approval.
- Contract execution: unexpected internal `DELEGATECALL`, unverified source, EIP-1967 proxy.
- Addresses and policy: zero address, burn address, recipient mismatch, policy caps/allowlists.

Not covered:

- Private-key compromise, a malicious wallet app, or a controlled signing device.
- Protocol-level economic attacks, oracle manipulation, MEV, liquidations, and bridge risk.
- Off-chain signatures not supplied to the inspector, full semantics of non-standard tokens, or
  every real-world attack pattern.
- "No finding" does not mean safe; it means nothing was found within current coverage.

## Inspection pipeline

| Stage | Output | Failure / degradation |
|---|---|---|
| Canonicalize + pin block | `chain`, `block_number`, `block_hash` | RPC failure -> capability error |
| Decode | selector, signature, arguments, unknowns | Unknown selector -> `decode=partial` + `unknown` |
| Metadata | EOA/contract, EIP-1967 proxy/implementation | Code/storage failure -> error/unknown |
| Simulation | call status, logs, effects | Unsupported RPC/trace -> `simulation=unavailable/partial` |
| Effect verification | `balanceOf`, `allowance`, `ownerOf` before/after | Non-standard token -> verified=false/unknown |
| Internal trace | internal calls, delegatecall | No `debug_traceCall` -> `trace=unavailable` |
| Source verification | `verified`, provider source | Missing key/provider failure -> `verified=null` |
| Detectors | finding ID, severity, evidence, confidence | Missing prerequisite -> no guess, write `unknown` |
| Policy | cap/allowlist decision | Missing effects or policy -> `not_configured`/`partial` |

Per-detector goals, rule IDs, evidence fields, failure semantics, and test boundaries are in
[detectors](detectors/README.md).

## Risk aggregation

Every finding carries a stable `id`, `category`, `severity`, `evidence`, `confidence`, and
`deterministic` flag. Results aggregate as follows:

1. With findings, `risk` is the highest severity.
2. Without findings, `risk` is `unknown`, never `low` or `safe`.
3. Missing critical coverage keeps `unknown`; it cannot be downgraded to low risk.
4. `unknowns` must state what is missing; `warnings` are non-blocking notes.

## Determinism and reproducibility

- The same request, block, and code version should produce the same finding set.
- Simulations and reads use a pinned block; no mixed-block state.
- LLMs do not participate in detectors, risk, or coverage; `explanation` is a deterministic
  template.
- External providers supply input only; they do not change rule semantics.

## Test boundary

- Rust unit/fixture tests: decode, effects, trace, approval, asset-flow, policy, signature.
- Foundry/Anvil red-team: fake Transfer/Approval logs, hidden delegatecall, proxy upgrade,
  unlimited approval, permit, 7702, 4337, unverified target.
- Benchmark: `docs/benchmarks/redteam-2026-11-11.md` and
  `docs/benchmarks/security-inspection-2026-11-11.md`.
- Current authored red-team corpus: 17/17 detect, 3/3 controls with no findings.
- Real historical BSC calldata: two unlimited-approval phishing transactions hit `AUTH-001`;
  two opaque exploit entrypoints remain explicitly `unknown`.
- No independent mainnet exploit corpus has been measured; do not claim real-world recall.
