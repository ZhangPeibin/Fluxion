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

**Steps:**

1. `swap.quote`
2. `swap.build`
3. `wallet.allowance`
4. `transaction.simulate`
5. `transaction.inspect`
6. `transaction.send`

**Planned steps:**

- `transaction.verify`
- optional `swap.execute` convenience wrapper

**Gaps:**

- `swap.build` currently supports one Uniswap V2 allocation; V3 and split allocations need support.
- `transaction.verify` is not implemented.
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

**Steps:**

1. `yield.discover`
2. `yield.build`
3. `wallet.allowance`
4. `transaction.inspect`
5. `transaction.simulate`
6. `transaction.send`
7. `position.analyze`

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

**Steps:**

1. `fourmeme.agent-tokens`
2. `fourmeme.token`
3. `wallet.agent-id`

**Nature:** read-only. There is no buy, sell, approve, or sign step.

**Failure handling:**

- API unavailable → return on-chain fields with warnings.
- AgentIdentifier unavailable → eligibility is `unknown`, not false.
- Partial log range → warning; never a silent zero.

## 7. `portfolio`

**Goal:** summarize what a wallet holds and where it has yield positions.

**Chains:** Ethereum or BSC, depending on step support.

**Status:** experimental.

**Steps:**

1. `wallet.balance`
2. `wallet.assets`
3. `wallet.collectibles`
4. `position.analyze` (Ethereum only)

**Gaps:**

- `position.analyze` is Ethereum-only; the workflow must declare chain-specific steps.

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

- Every workflow has at least one deterministic test using fixtures or a fork.
- Signing and broadcasting tests use Anvil or a mock wallet; never real funds.
- Chain-unsupported paths are tested.
- Policy-blocked paths are tested.
- User-rejection paths are tested where the wallet boundary is mocked.

## 10. References

- [ADR-0004](../../../adr/0004-composable-workflows.md)
- [Catalog spec](catalog.md)
- [Envelope v2 spec](envelope-v2.md)
- [Chain matrix spec](chain-matrix.md)
