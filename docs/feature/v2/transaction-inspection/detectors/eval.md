# inspect.eval - Evaluation and Red Team

## Goal

Make detector quality measurable before any risk claim ships.

## Dependencies

All detectors.

## Corpus

Maintain fixtures with:

- benign native transfer, ERC20 transfer, approval, NFT transfer;
- unlimited approval;
- fake token / fee-on-transfer;
- unverified contract;
- EIP-1967 proxy and upgrade;
- multicall with hidden approval;
- Permit2 and EIP-7702;
- unknown selector;
- malformed calldata;
- prompt-injection strings inside decoded values.

Each fixture records chain, block, request, expected findings, and expected coverage.

## Metrics

- false negatives per rule;
- false positives per rule;
- unknown rate;
- decode coverage;
- simulation coverage;
- latency p50/p95 without and with explanation;
- report stability across repeated runs.

## Rules

- `EVAL-001`: no detector ships without positive and negative fixtures.
- `EVAL-002`: any “safe/low” claim requires a measured false-negative rate.
- `EVAL-003`: adversarial fixtures must include nested/multicall hiding.
- `EVAL-004`: LLM explanation is evaluated separately from deterministic detection.
- `EVAL-005`: real user transactions must be anonymized before becoming fixtures.

## Acceptance

- CI runs deterministic fixtures offline;
- optional network tests are marked ignored and document the RPC used;
- report includes detector/rule version;
- known gaps are published in capability docs.

## Out of Scope

- A certification or audit claim.
- Proving absence of all vulnerabilities.
