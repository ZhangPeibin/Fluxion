# inspect.contract - Contract Trust and Upgrade Risk

## Goal

Surface target-code risks that calldata alone cannot prove safe.

## Dependencies

`inspect.metadata`.

## Implemented Slice

- `CONTRACT-001` only when a verification provider explicitly reports `verified: false`; missing
  verification stays `null` and is not a finding.
- `CONTRACT-003` on EIP-1967 proxy targets: low by default, high for `upgradeTo`.

New-contract age, delegatecall and implementation-change checks remain pending.

## Rules

- `CONTRACT-001`: target is unverified -> medium, or high when calldata includes approval/value.
- `CONTRACT-002`: target is a newly deployed contract -> medium with age evidence.
- `CONTRACT-003`: upgradeable proxy -> warning; high if the call can change the implementation.
- `CONTRACT-004`: delegatecall or DELEGATECALL-like selector -> high.
- `CONTRACT-005`: implementation is unverified or changes during inspection -> high.
- `CONTRACT-006`: bytecode unavailable -> unknown, never low.

## Evidence

- address, kind, verified source, proxy pattern, implementation, admin, block;
- bytecode hash and deployment age source when available.

## Failure and Unknown Behaviour

- Missing verification provider -> `verified: unknown`.
- Unsupported proxy pattern -> `proxy: unknown`.
- A verified status is not a safety claim.

## Tests

- EOA;
- unverified contract;
- verified contract;
- EIP-1967 proxy with implementation;
- unsupported proxy;
- delegatecall selector fixture.

## Acceptance

- no “verified = safe” inference;
- all contract findings cite metadata evidence;
- unknown metadata remains unknown.
