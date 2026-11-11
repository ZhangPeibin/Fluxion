# inspect.effects - Simulation and State Diff

## Goal

Produce observable native, token, NFT and allowance deltas for the inspected transaction.

## Dependencies

`inspect.core`, `inspect.decode`.

## Preferred Backend

1. RPC simulation with state diff and logs (`eth_simulateV1`, `debug_traceCall`, or equivalent).
2. `eth_call` success/revert with reduced coverage.
3. No simulation: `coverage.simulation = unavailable`.

The backend name must appear in `sources`. Public BSC/Ethereum RPC support must be measured before
depending on a trace API.

## Output

```json
{
  "native": [{"account": "0x...", "delta": "-1000000000000000000"}],
  "tokens": [{"token": "0x...", "account": "0x...", "delta": "-100"}],
  "approvals": [{"token": "0x...", "owner": "0x...", "spender": "0x...", "after": "max"}],
  "nfts": [{"collection": "0x...", "token_id": "1", "from": "0x...", "to": "0x..."}]
}
```

## Rules

- `EFFECT-001`: pre/post balance reads must use the same pinned block.
- `EFFECT-002`: no trace backend -> simulation unknown, never an empty “no effects” claim.
- `EFFECT-003`: `eth_call` success alone is not evidence of asset effects.
- `EFFECT-004`: unknown token decimals must be exposed, not guessed.
- `EFFECT-005`: fee-on-transfer or rebasing differences must preserve the raw delta.

## Failure and Unknown Behaviour

- Trace failure degrades coverage and adds an unknown.
- Unsupported token standards are reported as unsupported.
- A simulation result is not an execution guarantee.

## Tests

- native transfer delta;
- ERC20 transfer delta;
- approval delta;
- NFT transfer fixture;
- no-trace fallback;
- RPC trace unavailable fixture.

## Acceptance

- effects are block-pinned;
- every effect has a source/backend;
- no effects are reported without simulation coverage.
