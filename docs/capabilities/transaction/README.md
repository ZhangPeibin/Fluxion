# transaction.simulate

Local read-only EVM transaction simulation at one pinned block.

```bash
fluxion transaction simulate \
  --to 0x... --data 0x... \
  --from 0x... --value 0.1 --block 26356654 --json
```

- `to` and `data` are required.
- `data` must be `0x`-prefixed, even-length hex.
- `value` is human ETH units; it is converted to wei without floats.
- `from` is optional but recommended for `msg.sender`-dependent calls.
- `block` pins the simulation block; otherwise the latest block is used.

## Output

See [`schemas/transaction_simulate.v1.json`](../../../schemas/transaction_simulate.v1.json).

- `block_number` / `block_hash`: pinned state
- `success`: `eth_call` completed without a JSON-RPC revert
- `return_data`: raw return bytes (`0x...`)
- `gas_estimate`: `eth_estimateGas` result when available
- `revert_data`: revert bytes when the RPC exposes them
- `error`: normalized RPC/revert message
- `value`: raw + decimal ETH value supplied

A revert is returned as a normal response with `success: false`; transport/RPC failures are returned
as capability errors.

## Boundaries

- Ethereum only.
- Read-only: no signing, approval, broadcast, or state change.
- `eth_call` is not a full transaction simulation: it does not model block-level gas accounting,
  state changes across multiple transactions, or MEV.
- This capability does not construct swap calldata; `swap.build` remains a separate local
  capability.
