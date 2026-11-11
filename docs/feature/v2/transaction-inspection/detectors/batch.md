# inspect.batch - Batch and Nested Calls

## Goal

Prevent hidden actions inside multicall, Safe batches and aggregator wrappers from being missed.

## Dependencies

`inspect.decode`, `inspect.effects`.

## Implemented Slice

- `multicall(bytes[])` is ABI-decoded into `call[i]` arguments.
- Each nested call is decoded and nested approvals are re-checked.
- Malformed offsets/lengths or partial nested decoding reduce `coverage.decode` to `partial`.
- `call[i]` values are capped by the total calldata size; external assembly wrappers and Safe
  batches remain unsupported.

## Rules

- `BATCH-001`: multicall payload -> decode every nested call.
- `BATCH-002`: nested call fails decoding -> overall batch coverage is partial.
- `BATCH-003`: batch contains an approval -> run `inspect.approval` on the nested call.
- `BATCH-004`: batch contains a transfer to a different recipient than the outer intent -> high.
- `BATCH-005`: recursive batch depth above the configured limit -> unknown, not truncated silently.

## Evidence

- batch selector, call count, decoded/undecoded count, nested call index, nested selector.

## Failure and Unknown Behaviour

- Unsupported wrapper -> batch unknown; do not summarize as a single call.
- A batch with any undecoded member cannot claim `decode: complete`.

## Tests

- multicall of two transfers;
- multicall with a nested approval;
- multicall with an undecodable member;
- recursive batch depth limit.

## Acceptance

- every nested call is represented or explicitly counted as unknown;
- no silent truncation;
- detached nested findings keep their nested index.
