# inspect.policy - Advisory Policy Evaluation

## Goal

Evaluate findings against caller policy: spend caps, chain, target/method allowlists, spender
allowlists and session account.

## Dependencies

`inspect.effects`, `inspect.approval`, `inspect.address`, `inspect.contract`.

## Policy Inputs

```json
{
  "chain": "bsc",
  "max_native_spend": "0.1",
  "max_token_spend": [{"token": "0x...", "amount": "100"}],
  "target_allowlist": ["0x..."],
  "method_allowlist": ["transfer(address,uint256)"],
  "spender_allowlist": ["0x..."]
}
```

Policy is optional and supplied by the caller. There is no global hidden policy.

## Implemented Slice

Policy is optional JSON. Amounts are human units; native uses 18 decimals and known tokens use
registry decimals. Unknown token caps require `max_token_spend[].decimals`.

- `POLICY-001`: chain outside `chains`.
- `POLICY-002`: native value above `max_native_spend`.
- `POLICY-003`: simulated token outflow above `max_token_spend`.
- `POLICY-004`: target/method/spender outside allowlist.
- `POLICY-005`: no policy configured.
- `POLICY-006`: enforce mode cannot verify because simulation is unavailable.

Advisory is the default. Policy never signs or blocks by itself; enforcement still needs the signer.

## Rules

- `POLICY-001`: chain not allowed -> block.
- `POLICY-002`: native spend exceeds cap -> block.
- `POLICY-003`: token spend exceeds cap -> block.
- `POLICY-004`: target/method/spender outside allowlist -> block or warn by policy mode.
- `POLICY-005`: policy missing -> `not_configured`.
- `POLICY-006`: policy cannot be evaluated because effects are unknown -> block if mode is enforce;
  otherwise unknown.

## Modes

- `advisory` (default): findings only.
- `enforce`: return a policy decision that the caller must honor. Fluxion does not sign, so
  enforcement still depends on the signer integration.

## Failure and Unknown Behaviour

- Policy errors are capability errors, not findings.
- Missing coverage cannot silently pass an enforce policy.

## Tests

- pass/fail for each cap;
- missing policy;
- unknown effects under advisory and enforce;
- allowlist hit/miss.

## Acceptance

- policy source is visible in the report;
- no hidden default policy;
- enforce mode never treats unknown as pass.
