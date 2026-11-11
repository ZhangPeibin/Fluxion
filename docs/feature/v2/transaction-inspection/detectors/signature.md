# inspect.signature - Signature-Based Authorization

## Goal

Cover authorization that is not ordinary calldata: typed data, permits, EIP-7702 authorizations and
ERC-4337 user operations.

## Dependencies

`inspect.core`.

## Implemented Slice

EIP-2612 `permit` calldata only:

- `SIG-003`: value is `uint256::MAX` -> high.
- `SIG-002`: deadline is zero -> high.

Typed data, Permit2 signatures, EIP-7702 authorizations, ERC-4337 user operations and domain-chain
mismatch are tracked in #97.

## Rules

- `SIG-001`: EIP-712 domain chain ID differs from the session chain -> high.
- `SIG-002`: Permit2 approval has no expiration or an excessive expiration -> high.
- `SIG-003`: permit spender is unknown or unlimited -> high.
- `SIG-004`: EIP-7702 authorization changes code for an account -> high.
- `SIG-005`: 4337 user operation has unsupported fields or unknown paymaster -> unknown.
- `SIG-006`: signature payload cannot be decoded -> unknown, not low.

## Why Separate

Permit2, EIP-2612 and EIP-7702 are signatures or authorization lists, not normal ABI calldata.
Reusing the calldata decoder would hide the actual authorization semantics.

## Evidence

- domain, chain ID, verifying contract, owner, spender, nonce, deadline, value, authorization list.

## Failure and Unknown Behaviour

- Unsupported signature standard -> unknown.
- Missing session chain -> no domain comparison; coverage policy is not_configured.

## Tests

- mismatched typed-data chain;
- Permit2 unlimited approval;
- EIP-7702 authorization fixture;
- undecodable typed data.

## Acceptance

- signature checks are not mixed into `inspect.approval`;
- chain mismatch is deterministic;
- unknown signature types are explicit.
