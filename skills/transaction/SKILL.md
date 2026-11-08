---
name: transaction
description: Use when the user wants to simulate an Ethereum transaction, calldata or contract call before building/signing, or uses the Fluxion transaction_simulate MCP tool. Read-only; never signs, approves, or broadcasts.
---

# transaction.simulate

Simulate an EVM call with `eth_call` at one pinned block and return success/revert, return data and
an estimated gas limit.

- Human docs: [`docs/capabilities/transaction`](../../docs/capabilities/transaction/README.md)
- Schema: [`transaction_simulate`](../../schemas/transaction_simulate.v1.json)

## Execution model

`transaction.simulate` is fixed to the local runtime. The agent does not start or manage a server.
`FLUXION_RPC_URL` selects the local Ethereum RPC.

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
- `data` must be `0x`-prefixed, even-length hex.
- `value` is human ETH units, not wei; omit it or use `"0"` for no value.
- `from` is optional; include it when the call depends on `msg.sender`.
- `block` pins the simulation state; otherwise the latest block is used.

## CLI

```bash
fluxion transaction simulate \
  --to 0x... --data 0x... \
  --from 0x... --value 0.1 --block 26356654 --json
```

Options: `--from`, `--to`, `--data`, `--value`, `--block`, `--rpc-url`, `--json`.

## Reading the result

- `success`: the call did not revert.
- `return_data`: raw bytes returned by the call (`0x...`).
- `gas_estimate`: `eth_estimateGas` result when available.
- `revert_data` / `error`: revert bytes and normalized message.
- `block_number` / `block_hash`: exact state used.
- `value`: raw + decimal ETH value supplied.

A `success: false` response is still a successful simulation; inspect `error` and `revert_data`.
Transport or RPC failures use stable capability error codes instead.

## Errors

| Code | Meaning | Agent action |
|---|---|---|
| `INVALID_ARGUMENT` | Bad address, data or value | Fix the request |
| `RPC_ERROR` | Local RPC unavailable, invalid params, or non-revert failure | Retry or check `FLUXION_RPC_URL` |
| `TIMEOUT` | RPC timed out | Retry later |

## Boundaries

- Ethereum only.
- Read-only; no signing, approvals, or broadcasting.
- Does not construct calldata; a build capability is separate and not yet available.
- `eth_call` is not a full block-level simulation: it does not model state across multiple
  transactions, block gas accounting, or MEV.
- A successful simulation is not an execution guarantee.
