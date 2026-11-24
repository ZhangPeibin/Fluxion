# inspect.metadata - Target Metadata

## Goal

Establish what the target is before evaluating calldata semantics.

## Dependencies

`inspect.core`; uses `inspect.decode` target address.

## Output

- kind: `eoa`, `contract`, or `unknown`
- verified: true / false / unknown
- proxy: true / false / unknown
- implementation: address or null
- verification source and block

## Rules

- `META-001`: no bytecode -> `kind: eoa`.
- `META-002`: bytecode present -> `kind: contract`.
- `META-003`: RPC bytecode unavailable -> `kind: unknown`.
- `META-004`: proxy detected without implementation -> `proxy: true`, `implementation: null`.
- `META-005`: newly deployed contract -> warning, not automatically critical.

## Detection Guidance

- Detect proxies only for known, evidenced patterns: EIP-1967 implementation/beacon slots, EIP-1822
  (UUPS) slot, and EIP-1167 minimal-proxy bytecode. `proxy_kind` records the detected pattern.
- Verification requires a source provider such as Etherscan/BscScan or an equivalent. Missing API
  key yields `verified: unknown`, not `false`.
- Do not infer safety from verification. Verification means source is available, not that the code
  is benign.

## Failure and Unknown Behaviour

- RPC failure yields a capability error at core level.
- Missing source provider yields `verified: unknown`.
- Unsupported proxy pattern yields `proxy: unknown`, not false.

## Tests

- EOA fixture;
- verified contract fixture;
- unverified contract fixture;
- EIP-1967 proxy fixture;
- unknown metadata fixture.

## Acceptance

- metadata fields are block-pinned and source-attributed;
- no `verified: true` without a real source result;
- unsupported cases remain unknown.
