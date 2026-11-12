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

- RPC data is trusted for block, code, simulation, and trace. No light-client proof is used.
- `FLUXION_VERIFY_RPC_URL` can cross-check block hash and target code, but not every state read.
- `FLUXION_THREAT_INTEL_FILE` is operator-provided and may be incomplete or stale.
- `debug_traceCall` is optional; public BSC RPC does not provide it.
- Source verification is optional; missing key or provider failure is `verified=null`.
- Balance/allowance verification depends on standard getters; non-standard tokens degrade to
  unknown.
- Native gas, full state diff, MEV, oracle manipulation, and protocol economics are out of scope.
- A clean report is not a safety guarantee.

## Test boundary

- Rust unit/fixture tests: decode, effects, trace, approval, asset-flow, policy, signature.
- Foundry/Anvil red-team: fake Transfer/Approval logs, hidden delegatecall, proxy upgrade,
  unlimited approval, permit, 7702, 4337, unverified target.
- Benchmark: `docs/benchmarks/redteam-2026-11-11.md` and
  `docs/benchmarks/security-inspection-2026-11-11.md`.
- No independent mainnet exploit corpus has been measured; do not claim real-world recall.
