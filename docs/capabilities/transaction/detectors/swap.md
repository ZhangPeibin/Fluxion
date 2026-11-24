# inspect.swap - Swap-Specific Checks

## Goal

Check swap calldata for parameters that commonly cause loss, after a swap-building capability exists.

## Dependencies

`inspect.decode`, `inspect.effects`.

## Implemented Slice

- Decodes common Uniswap/Pancake V2 `swapExact*` selectors, including `address[]` paths.
- `SWAP-001`: `amountOutMin == 0` -> high.
- `SWAP-004`: decoded recipient differs from sender -> high.
- `SWAP-007`: target outside the built-in router registry -> medium.
- Deadline, price impact, unknown path tokens and V3/aggregator routers remain pending.

## Rules

- `SWAP-001`: `amountOutMin` is zero or below policy -> high.
- `SWAP-002`: deadline is in the past -> high.
- `SWAP-003`: deadline is unreasonably far in the future -> medium.
- `SWAP-004`: recipient differs from the sender -> high unless explicitly allowed.
- `SWAP-005`: path contains an unknown token or pool -> medium/unknown.
- `SWAP-006`: quoted price impact exceeds policy -> high.
- `SWAP-007`: calldata targets an unknown router -> medium, or high with approval attached.

## Evidence

- router, token path, amounts, minOut, deadline, recipient, price impact source.

## Failure and Unknown Behaviour

- No decoded swap intent -> rules do not run.
- Unknown router -> report unknown coverage rather than infer a path.
- This capability does not build swap calldata; `swap.build` remains separate.

## Tests

- zero minOut;
- expired deadline;
- recipient mismatch;
- unknown router;
- valid swap fixture.

## Acceptance

- swap findings only run on recognized swap intents;
- no quote or execution is performed;
- price-impact data is source-attributed.
