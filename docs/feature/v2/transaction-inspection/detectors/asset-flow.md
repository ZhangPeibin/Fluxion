# inspect.asset-flow - Asset Flow

## Goal

Detect asset movement that the user or host agent would not expect from the decoded intent.

## Dependencies

`inspect.effects`, `inspect.decode`.

## Rules

- `FLOW-001`: native value leaves the sender without a matching decoded transfer -> high.
- `FLOW-002`: token outflow exceeds the decoded amount -> high.
- `FLOW-003`: token net outflow occurs when the intent is not a transfer/swap/approval -> high.
- `FLOW-004`: receiver differs from the decoded recipient -> high.
- `FLOW-005`: fee-on-transfer produces a recipient delta lower than the decoded amount -> warning.
- `FLOW-006`: NFT transfer not present in the decoded intent -> high.

## Evidence

- account, token/asset, raw delta, decoded expected amount, recipient;
- block and simulation backend.

## Failure and Unknown Behaviour

- Without state diff, `FLOW-*` rules do not run; coverage remains unknown.
- Unknown token decimals do not block raw amount comparison.
- Swap-specific price impact is handled by `inspect.swap`, not here.

## Tests

- native transfer to expected recipient;
- token transfer with unexpected extra outflow;
- recipient mismatch;
- fee-on-transfer;
- missing effects coverage.

## Acceptance

- no flow finding without an effect delta;
- raw deltas are authoritative when decimals are unknown;
- deterministic fixtures for each rule.
