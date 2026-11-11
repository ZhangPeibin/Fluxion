# inspect.core - Capability Contract

## Goal

Add the `transaction.inspect` capability and the report shape without claiming safety.

## Dependencies

None.

## Inputs

- chain (default BSC)
- from (optional)
- to (required)
- value (default 0)
- data (required; `0x` for native transfers)
- block (optional)

## Output

- chain / chain_id
- block_number / block_hash
- from / to / value
- intent (selector plus optional signature/name/args)
- target (address plus kind = `unknown` in the first slice)
- effects (empty in the first slice)
- findings (empty in the first slice)
- risk = `unknown` in the first slice
- coverage and unknowns
- sources / warnings

## Rules

- `CORE-001`: empty data is a native-transfer intent.
- `CORE-002`: a non-empty payload shorter than four bytes is invalid calldata.
- `CORE-003`: a response without successful simulation must include an effect-related unknown.
- `CORE-004`: `risk` must never be `safe`.

## Failure and Unknown Behaviour

- RPC failure to pin the block is a capability error.
- Unknown selectors are valid input and produce an unknown coverage entry.
- Metadata, simulation and policy are `not_run` unless a later detector fills them.

## Tests

- native transfer (`0x`) decodes as native transfer;
- malformed short calldata is rejected;
- response is pinned to a block;
- default chain is BSC;
- schema round-trip and CLI/MCP smoke test.

## Acceptance

- capability appears in `fluxion capability list` and MCP `tools/list`;
- CLI and MCP accept the same fields;
- no signing or broadcast paths are touched;
- workspace fmt/clippy/test pass.

## Out of Scope

- ABI argument decoding beyond the basic slice.
- Contract metadata, trace effects, findings and policy.
