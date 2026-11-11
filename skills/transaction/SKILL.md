---
name: transaction
description: Use when the user wants to inspect or simulate BSC or Ethereum calldata before signing, or uses the Fluxion transaction_inspect or transaction_simulate MCP tool. Read-only; never signs, approves, or broadcasts.
---

# transaction capabilities

`transaction.inspect` is a deterministic pre-sign report. `transaction.simulate` executes only an
`eth_call`. Neither signs or broadcasts.

## transaction.inspect

```json
{
  "name": "transaction_inspect",
  "arguments": {
    "chain": "bsc",
    "from": "0x...",
    "to": "0x...",
    "data": "0x...",
    "value": "0",
    "block": 132893125
  }
}
```

- `chain` defaults to `bsc`; use `ethereum` for Ethereum.
- Empty `data` represents a native transfer.
- Returns `intent`, `target`, `effects`, `findings`, `risk`, `coverage` and `unknowns`.
- Unknown selectors stay unknown; never reinterpret them as safe.
- `risk` is `unknown` until the relevant detectors are implemented; never treat it as approval.
- Read [`transaction_inspect`](../../schemas/transaction_inspect.v1.json) for the full shape.
- Human docs: [`docs/capabilities/transaction`](../../docs/capabilities/transaction/README.md)

# transaction.simulate

Simulate an EVM call with `eth_call` at one pinned block and return success/revert, return data and
an estimated gas limit.

- Human docs: [`docs/capabilities/transaction`](../../docs/capabilities/transaction/README.md)
- Schema: [`transaction_simulate`](../../schemas/transaction_simulate.v1.json)

## Execution model

`transaction.simulate` is fixed to the local runtime. The agent does not start or manage a server.
It defaults to BSC (`FLUXION_BSC_RPC_URL`); pass `chain: "ethereum"` to use `FLUXION_RPC_URL`.

## MCP tool

```json
{
  "name": "transaction_simulate",
  "arguments": {
    "chain": "bsc",
    "from": "0x...",
    "to": "0x...",
    "data": "0x...",
    "value": "0.1",
    "block": 26356654
  }
}
```

- `chain` defaults to `bsc`; use `ethereum` for Ethereum.
- `to` and `data` are required.
- `data` must be `0x`-prefixed, even-length hex.
- `value` is human native units (BNB/ETH), not wei; omit it or use `"0"` for no value.
- `from` is optional; include it when the call depends on `msg.sender`.
- `block` pins the simulation state; otherwise the latest block is used.

## CLI

```bash
fluxion transaction inspect --to 0x... --data 0x... --json
fluxion transaction simulate \
  --to 0x... --data 0x... \
  --from 0x... --value 0.1 --block 26356654 --json
```

Options: `--chain bsc|ethereum`, `--from`, `--to`, `--data`, `--value`, `--block`, `--rpc-url`, `--json`.

## Reading the result

- `chain` / `chain_id`: selected chain.
- `success`: the call did not revert.
- `return_data`: raw bytes returned by the call (`0x...`).
- `gas_estimate`: `eth_estimateGas` result when available.
- `revert_data` / `error`: revert bytes and normalized message.
- `block_number` / `block_hash`: exact state used.
- `value`: raw + decimal native value supplied.

A `success: false` response is still a successful simulation; inspect `error` and `revert_data`.
Transport or RPC failures use stable capability error codes instead.

## Errors

| Code | Meaning | Agent action |
|---|---|---|
| `INVALID_ARGUMENT` | Bad address, data or value | Fix the request |
| `RPC_ERROR` | Local RPC unavailable, invalid params, or non-revert failure | Retry or check the chain RPC URL |
| `TIMEOUT` | RPC timed out | Retry later |

## Boundaries

- BSC and Ethereum.
- Read-only; no signing, approvals, or broadcasting.
- Does not construct calldata; a build capability is separate and not yet available.
- `eth_call` is not a full block-level simulation: it does not model state across multiple
  transactions, block gas accounting, or MEV.
- A successful simulation is not an execution guarantee.
