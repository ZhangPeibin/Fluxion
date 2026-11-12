# inspect.effects - Simulation and State Diff

## Goal

Produce observable native, token, NFT and allowance deltas for the inspected transaction.

## Dependencies

`inspect.core`, `inspect.decode`.

## Implemented Verification

After decoding logs, `transaction.inspect` builds a second `eth_simulateV1` batch:

1. target call;
2. `balanceOf` reads for log-derived token/account pairs;
3. `allowance` reads for approval pairs;
4. `ownerOf` reads for ERC721 transfer pairs.

It compares the post-state with pre-state reads at the same pinned block.

- `EFFECT-007`: Transfer log amount differs from balance delta.
- `EFFECT-009`: Approval log differs from allowance.
- `EFFECT-010`: NFT Transfer log differs from owner.

Address zero is skipped for balance checks because burn events conventionally do not credit the
zero address. Unsupported/non-token targets degrade to explicit unknowns.

## Implemented Backend

BSC public RPC supports `eth_simulateV1`. The current implementation uses its `calls[].logs` to
derive token/NFT/approval effects and adds native value movement from the transaction itself.

This is **not** a full state diff. `debug_traceCall` is not available on the tested BSC public RPC.
Effects are marked `partial`, and every report warns that logs can be forged by arbitrary contracts.
A future trace/state-diff adapter can upgrade coverage without changing the response shape.

Fallback order:

1. `eth_simulateV1` logs and call status.
2. No simulation: `coverage.simulation = unavailable` with an explicit unknown.
3. Future trace/state-diff provider when configured.

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
- `EFFECT-002`: no simulation backend -> simulation unknown, never an empty “no effects” claim.
- `EFFECT-003`: `eth_call` success alone is not evidence of asset effects.
- `EFFECT-004`: unknown token decimals must be exposed, not guessed.
- `EFFECT-005`: fee-on-transfer or rebasing differences must preserve the raw delta.
- `EFFECT-006`: log-derived effects must be labeled log-derived, never a full state diff.

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
