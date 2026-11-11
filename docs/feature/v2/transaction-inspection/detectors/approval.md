# inspect.approval - Approval Safety

## Goal

Detect token and NFT approvals that materially expand an attacker-controlled or unknown spender's
power.

## Dependencies

`inspect.decode`, `inspect.effects`.

## Implemented Slice

The first detector uses decoded calldata, not forged logs:

- `approve` / `increaseAllowance` with `uint256::MAX` -> `AUTH-001` high.
- `setApprovalForAll(operator, true)` -> `AUTH-004` high.
- finite approvals and allowlist caps remain pending `inspect.policy`.

## Rules

- `AUTH-001`: ERC20 `approve` or `increaseAllowance` to `uint256::MAX` -> high.
- `AUTH-002`: ERC20 approval above configured cap -> high or medium by cap.
- `AUTH-003`: approval to a spender outside the allowlist -> medium; high if combined with unlimited
  amount.
- `AUTH-004`: `setApprovalForAll(operator, true)` -> high unless operator is explicitly allowed.
- `AUTH-005`: approval amount increases but simulation cannot confirm current allowance -> medium
  with reduced confidence.
- `AUTH-006`: `permit`/Permit2 path is handled by `inspect.signature`; do not duplicate it here.

## Evidence

- token, owner, spender, before/after allowance, raw amount, selector;
- whether unlimited, new approval, increased approval, or NFT operator approval;
- source of before/after values.

## Failure and Unknown Behaviour

- Missing before/after allowance -> finding may be emitted with `confidence: medium`, and coverage
  must say allowance delta is partial.
- Unknown spender reputation is not a sanction check; it means not in the allowlist/registry.
- No local allowlist means only the generic unlimited/operator checks run.

## Tests

- unlimited ERC20 approve;
- finite approve below/above cap;
- increaseAllowance;
- setApprovalForAll true/false;
- unknown spender;
- missing allowance delta.

## Acceptance

- findings map to schema and rule IDs;
- no finding claims to know spender intent;
- deterministic tests cover both unlimited and capped approvals.
