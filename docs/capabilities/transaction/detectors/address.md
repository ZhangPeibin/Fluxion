# inspect.address - Address Checks

## Goal

Detect addresses that are invalid, unexpected, or outside the caller's policy.

## Dependencies

`inspect.core`, `inspect.decode`.

## Implemented Slice

- zero target or decoded recipient -> `ADDR-001` high;
- known burn address (`0x…dEaD`) target or recipient -> `ADDR-002` high;
- target equals sender -> `ADDR-005` low.

Session mismatch and allowlist checks remain pending because `transaction.inspect` does not yet take
session/policy context.

## Rules

- `ADDR-001`: recipient is the zero address -> high; native transfers are unrecoverable.
- `ADDR-002`: recipient is the burn address -> high.
- `ADDR-003`: `from` differs from the connected session account -> high for sign/send paths.
- `ADDR-004`: recipient is outside the configured allowlist -> medium.
- `ADDR-005`: target equals the sender -> warning unless the decoded intent requires it.
- `ADDR-006`: address is not checksummed/valid -> capability error before inspection.

## Evidence

- role (`from`, `to`, recipient, spender), address, rule, policy source.

## Failure and Unknown Behaviour

- No allowlist configured -> allowlist rules do not run; coverage policy is `not_configured`.
- No session -> session mismatch rules do not run.
- Address reputation/sanctions are out of scope unless a provider is added.

## Tests

- zero recipient;
- burn address;
- session mismatch;
- allowlist hit/miss;
- self target.

## Acceptance

- address validation happens before findings;
- policy-dependent findings disclose the policy source;
- no implicit reputation claims.
