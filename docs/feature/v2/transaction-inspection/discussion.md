# Transaction Inspection - Requirements Discussion

**Status:** ready-for-prd
**Release:** v2
**Feature:** transaction-inspection

## Problem

Agents can construct arbitrary EVM calldata. Before signing, a user or host agent usually sees raw
data, an opaque wallet confirmation, or a simulation that only says whether the call reverts. There
is no durable, structured answer to:

- what function is being called;
- which assets may move;
- which approvals may be created or changed;
- whether the target is verified, proxied, or newly deployed;
- whether a batch hides additional calls;
- what the inspection does not cover.

Fluxion already exposes `transaction.simulate`, but it only performs `eth_call` plus
`eth_estimateGas`. It does not decode calldata, produce asset/allowance deltas, inspect contract
metadata, or evaluate policy.

## Users and Outcomes

- Agent developers: call `transaction.inspect` before asking the user to sign, then explain the
  structured report in their own UI.
- Wallet / agent-platform integrators: use findings and coverage as an additional confirmation
  surface.
- Advanced users: see intent, effects, approvals, and unknowns instead of signing raw calldata.

Desired outcome: a deterministic, block-pinned report that is useful when complete and explicitly
`unknown` when not. It must never imply that an inspected transaction is safe.

## Scope

### In Scope

- New local capability `transaction.inspect`.
- BSC by default; Ethereum via `--chain ethereum`.
- Arbitrary calldata, with unknown contracts and selectors allowed.
- Deterministic intent decoding, target metadata, simulation/state effects, detector findings,
  coverage, and unknowns.
- Advisory output for host agents and users.
- Optional LLM explanation only after deterministic findings exist.

### Out of Scope

- Signing, broadcasting, private-key handling, or automatic execution.
- A guarantee of safety, audit equivalence, or economic/MEV analysis.
- Replacing wallet confirmation.
- Full coverage of every chain, proxy pattern, protocol, or signature scheme.
- Treating LLM output as a detection result.

## Decisions

- **Capability name:** `transaction.inspect`, not `security agent`. The output is a risk report,
  not an autonomous security verdict.
- **Advisory first:** findings inform the signer; enforcement belongs at the signer, smart account,
  or wallet policy layer. `inspect.policy` may later expose policy evaluation, but blocking is not
  the default.
- **Arbitrary calldata:** support arbitrary `to`/`data`; unknown selectors and unverified contracts
  return `unknown`, not a guessed low risk.
- **BSC default:** reuse the local chain abstraction; Ethereum is explicit.
- **Deterministic first:** selectors, ABI decoding, metadata, effects, and rules are deterministic.
  LLM explanation is optional and cannot change risk or findings.
- **No `safe` value:** the highest risk tier is `unknown` when coverage is insufficient. The report
  may say “no known findings”, never “safe”.
- **Local RPC first:** use the local RPC and trace capabilities where available. Third-party
  simulation providers are an optional later adapter, not a v1 requirement.
- **Host-agent explanation:** the capability returns structured JSON. The calling agent may explain
  it; Fluxion does not embed a second general-purpose LLM agent in the default path.

## Constraints

- Rust edition 2024, tokio, `reqwest` raw JSON-RPC, `alloy-primitives` only.
- No private keys in the local runtime.
- Stable capability errors; inspection findings are not transport errors.
- Every response is pinned to a block number and hash.
- Tests must not require network access by default.
- Docs, schemas, CLI, MCP, and skills must stay in sync.

## Open Questions

- Which BSC RPCs support `debug_traceCall`, `eth_simulateV1`, or equivalent state-diff simulation?
- Which known ABI/contract registry should ship first, and how is it refreshed?
- Should a later policy layer support deny/allow enforcement, or remain advisory?
- Which public malicious/benign transaction corpus can be used for evaluation without leaking
  private user data?

## Evidence

- Existing `transaction.simulate` (`crates/client/src/transaction.rs`) returns only success,
  return data, gas estimate, revert data, and error.
- Existing `wallet.send` already depends on simulation before phone confirmation.
- `wallet.sign` and WalletConnect already define the signing boundary; `transaction.inspect` does
  not bypass it.
- The v1 RFQ scope is Ethereum-only; this feature is a local execution-safety capability and should
  not be mixed into `swap.quote`.

## Session Log

- 2026-11-11: Feature opened after reviewing the limits of raw `eth_call` simulation and LLM-only
  calldata analysis. Defaults accepted: v2, advisory, arbitrary calldata with explicit unknowns,
  deterministic-first, BSC default, host-agent explanation. Big spec and detector sub-specs created
  under this feature directory.
