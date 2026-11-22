# Workflows Spec

**Status:** draft  
**Release:** v3

## 1. Purpose

A workflow is an ordered composition of capabilities that produces a user result. Workflows make
multi-step use explicit, testable, and safe.

A workflow is not a new transport and not a hidden execution path. Each step is a normal capability
call with its own contract and envelope.

## 2. Workflow fields

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Stable workflow id. |
| `summary` | yes | User result in one sentence. |
| `status` | yes | `experimental`, `stable`, or `deprecated`. |
| `chains` | yes | Intersection of the steps' supported chains. |
| `steps` | yes | Existing capability ids in order. |
| `planned_steps` | no | Future capability ids; must not be available yet. |
| `skill` | yes | Agent instructions. |
| `docs` | yes | This workflow's documentation. |
| `gaps` | no | Known missing pieces. |

## 3. Common rules

1. Capabilities are called in the declared order.
2. Simulation and inspection happen before signing or broadcasting.
3. Policy is checked before the wallet request.
4. User confirmation happens in the wallet; Fluxion never signs silently.
5. A failed step stops the workflow unless the workflow explicitly says otherwise.
6. A workflow must not hide approvals or extra transactions.
7. A workflow with gaps or planned steps is `experimental`.
8. Every workflow has an end-to-end test or an explicit test gap.

## 4. `swap`

**Goal:** turn a swap quote into a confirmed on-chain swap and verify the result.

**Chains:** Ethereum.

**Status:** experimental.

**Preconditions:** Ethereum; a quote response; a connected WalletConnect session before send; an
optional caller policy.

**Steps:**

1. `swap.quote`
2. `swap.build`
3. `wallet.allowance`
4. `transaction.simulate`
5. `transaction.inspect`
6. `transaction.send`
7. `transaction.verify`

**Checkpoints:**

- Quote is fresh and the route shape is known before build.
- Build succeeded; no split/V3 route was approximated.
- Allowance covers the sell amount, or an explicit approval is requested first.
- Simulation succeeds before inspect and send.
- Inspection has no blocking policy finding.
- Verify returns `success` and matches the inspected effects.

**Planned steps:**

- optional `swap.execute` convenience wrapper

**Gaps:**

- `swap.build` currently supports one Uniswap V2 allocation; V3 and split allocations need support.
- Approval steps are documented but not a single capability.

**Failure handling:**

- No route → stop.
- Build unsupported → stop; do not ask for a signature.
- Insufficient allowance → request approval first.
- Simulation failure → stop; do not send.
- Policy block → stop; do not ask for confirmation.
- User rejection → stop.

## 5. `yield-deposit`

**Goal:** discover a curated ETH yield opportunity, prepare the deposit, execute it, and read the
resulting position.

**Chains:** Ethereum.

**Status:** experimental.

**Preconditions:** Ethereum; a curated opportunity id or discovery profile; a connected
WalletConnect session before send; an optional caller policy.

**Steps:**

1. `yield.discover`
2. `yield.build`
3. `wallet.allowance`
4. `transaction.inspect`
5. `transaction.simulate`
6. `transaction.send`
7. `transaction.verify`
8. `position.analyze`

**Checkpoints:**

- Discovery data is not stale and the user's risk profile is unchanged.
- Build returns explicit steps and approvals, not a hidden sequence.
- Each approval is requested explicitly before the deposit.
- Inspection has no blocking policy finding; simulation succeeds.
- Verify returns `success`.
- `position.analyze` reads at a block at or after the deposit.

**Gaps:**

- `yield.build` returns calldata and approvals, but the approval sequence is not a shared helper.
- `position.analyze` may need a block after execution; the workflow must state the observation block.

**Failure handling:**

- Stale yield data → stop with `STALE_STATE`.
- Unsupported market → stop with `UNSUPPORTED`.
- Approval failure → stop before deposit.
- Simulation failure → stop.
- Policy block → stop.

## 6. `fourmeme-discovery`

**Goal:** find recent Four.meme Binance Agent ID Exclusive tokens and check wallet eligibility.

**Chains:** BSC.

**Status:** experimental.

**Preconditions:** BSC; an optional wallet for eligibility.

**Steps:**

1. `fourmeme.agent-tokens`
2. `fourmeme.token`
3. `wallet.agent-id`

**Checkpoints:**

- `lookback_blocks` is explicit; partial ranges stay warnings.
- Template and phase come from the API or on-chain fields, never inferred.
- Eligibility is `unknown`, not `false`, when `AgentIdentifier` is unavailable.

**Nature:** read-only. There is no buy, sell, approve, or sign step.

**Failure handling:**

- API unavailable → return on-chain fields with warnings.
- AgentIdentifier unavailable → eligibility is `unknown`, not false.
- Partial log range → warning; never a silent zero.

## 7. `portfolio`

**Goal:** summarize what a wallet holds and where it has yield positions.

**Chains:** Ethereum or BSC, depending on step support.

**Status:** experimental.

**Preconditions:** a wallet address; a chain where the requested read capabilities are supported.

**Steps:**

1. `wallet.balance`
2. `wallet.assets`
3. `wallet.collectibles`
4. `position.analyze` (Ethereum only)

**Checkpoints:**

- The pinned block and discovery coverage are stated.
- NFTs are best-effort; non-enumerable collections may report only a count.
- BSC portfolios stop before `position.analyze` and say why.

**Gaps:**

- `position.analyze` is Ethereum-only; the workflow must declare chain-specific steps.

**Failure handling:**

- Provider unavailable → registry fallback with warnings.
- Unknown tokens stay unknown; do not infer balances.
- `position.analyze` failure is reported separately and does not invalidate wallet balances.

## 8. Workflow skills

Each workflow must be documented in a skill section or a dedicated skill:

- `skills/swap/SKILL.md`
- `skills/yield/SKILL.md`
- `skills/fourmeme/SKILL.md`
- `skills/wallet/SKILL.md` (portfolio)
- `skills/transaction/SKILL.md` (inspection and send checkpoints)

The skill must state:

- when to use the workflow;
- required inputs;
- chain constraints;
- which steps need confirmation;
- how to interpret partial results;
- what to do on failure.

## 9. End-to-end testing

- `crates/client/tests/workflows.rs` drives the catalog sequence for `swap`, `yield-deposit`,
  `fourmeme-discovery`, and `portfolio` against mocked remote services and RPC.
- Each workflow has a success path and a failure path: quote failure, unknown opportunity,
  RPC/log failure, and position failure.
- Catalog invariants are tested: steps exist, chains are the intersection, planned steps are not
  implemented, and workflows with gaps stay experimental.
- The wallet confirmation boundary is asserted through the catalog (`transaction.send` requires a
  `wallet_session`); a real WalletConnect relay and phone confirmation are not simulated.
- Signing and broadcasting tests use Anvil or a mock wallet; never real funds.
- Chain-unsupported paths are tested.
- Policy-blocked paths are tested.
- User-rejection paths are tested where the wallet boundary is mocked.

## 10. References

- [ADR-0004](../../../adr/0004-composable-workflows.md)
- [Catalog spec](catalog.md)
- [Envelope v2 spec](envelope-v2.md)
- [Chain matrix spec](chain-matrix.md)
