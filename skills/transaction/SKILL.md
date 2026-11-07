---
name: transaction
description: Use when the user wants to simulate an Ethereum transaction or calldata before building/signing, or uses the Fluxion transaction_simulate MCP tool. Read-only; never signs, approves, or broadcasts.
---

# transaction.simulate

Simulate an EVM call with `eth_call` at one pinned block and return the result.

## MCP tool

```json
{
  "name": "transaction_simulate",
  "arguments": {
    "from": "0x...",
    "to": "0x...",
    "data": "0x...",
    "value": "0.1",
    "block": 26356654
  }
}
```

- `to` and `data` are required.
- `value` is human ETH units, not wei.
- `from` is optional; include it when the call depends on `msg.sender`.
- `block` pins the simulation state.

## Reading the result

- `success`: the call did not revert.
- `return_data`: raw bytes returned by the call.
- `gas_estimate`: `eth_estimateGas` result when available.
- `revert_data` / `error`: revert bytes and normalized message.
- `block_number` / `block_hash`: exact state used.

A `success: false` response is still a successful simulation; inspect `error` and `revert_data`.
Transport failures use stable capability error codes instead.

## Boundaries

- Ethereum only.
- Read-only; no signing, approvals, or broadcasting.
- Does not construct calldata; use a build capability separately when available.
- `eth_call` is not a full block-level simulation.
