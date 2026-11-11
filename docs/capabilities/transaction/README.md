# transaction.inspect

Deterministic pre-sign inspection of unsigned calldata. It returns intent, target metadata,
effects, findings, coverage and explicit unknowns. It is advisory only and never claims safety.

```bash
fluxion transaction inspect --to 0x... --data 0x... --json
fluxion transaction inspect --to 0x... --data 0x... --chain ethereum --json
```

- Default chain is BSC; use `--chain ethereum` for Ethereum.
- Empty `data` is a native transfer.
- `multicall(bytes[])` is expanded into `call[i]` arguments; nested approvals are checked.
- Built-in selectors decode signature, name and static arguments.
- Unknown selectors return `signature: null`, `name: null` and an unknown entry.
- The block is pinned and returned as `block_number` / `block_hash`.
- `target` distinguishes EOA/contract and detects the EIP-1967 implementation slot.
- Source verification is `null` unless a verification provider is configured.
- Effects come from `eth_simulateV1` logs when the RPC supports it. They are `partial`, not a full
  state diff, and arbitrary contracts can forge logs.
- `risk` reflects known deterministic findings, otherwise `unknown`. It is never `safe`.

## Output

See [`schemas/transaction_inspect.v1.json`](../../../schemas/transaction_inspect.v1.json).

- `intent`: selector, signature, name and decoded arguments.
- `target`: address, `kind`, `verified`, `proxy`, `implementation`. `verified: null` means not
  checked, not safe.
- `effects`: native value plus token/NFT/approval effects derived from simulated logs. Native gas
  fees and full state changes are not included.
- `findings`: rule-ID findings with evidence. Current checks include unlimited ERC20 approval
  (`AUTH-001`), NFT operator approval (`AUTH-004`), asset-flow findings (`FLOW-001`–`FLOW-006`),
  contract findings (`CONTRACT-001`, `CONTRACT-003`), address findings (`ADDR-001`, `ADDR-002`,
  `ADDR-005`), and swap findings (`SWAP-001`, `SWAP-004`, `SWAP-007`). Nested approvals from decoded
  multicalls carry a `nested_call` evidence field.
- `risk`: `critical`, `high`, `medium`, `low` or `unknown`; never `safe`.
- `coverage`: decode, metadata, simulation and policy status. Contract metadata is `partial` until
  source verification is configured; simulation is `partial` because it is log-derived.
- `unknowns`: what was not inspected and could change the conclusion.

## Boundaries

- Advisory only; no signing, approval or broadcast.
- Unknown or unsupported calldata stays unknown.
- A successful inspection is not an execution guarantee.
- LLM explanation is not part of the deterministic risk decision.

# transaction.simulate

Local read-only EVM transaction simulation on BSC (default) or Ethereum.

```bash
fluxion transaction simulate \
  --to 0x... --data 0x... \
  --from 0x... --value 0.1 --block 26356654 --json

fluxion transaction simulate --to 0x... --data 0x... --chain ethereum --json
```

- Default chain is BSC; use `--chain ethereum` for Ethereum.
- `to` and `data` are required.
- `data` must be `0x`-prefixed, even-length hex.
- `value` is human native units (BNB/ETH); it is converted to wei without floats.
- `from` is optional but recommended for `msg.sender`-dependent calls.
- `block` pins the simulation block; otherwise the latest block is used.

## Output

See [`schemas/transaction_simulate.v1.json`](../../../schemas/transaction_simulate.v1.json).

- `chain` / `chain_id`: selected chain
- `block_number` / `block_hash`: pinned state
- `success`: `eth_call` completed without a JSON-RPC revert
- `return_data`: raw return bytes (`0x...`)
- `gas_estimate`: `eth_estimateGas` result when available
- `revert_data`: revert bytes when the RPC exposes them
- `error`: normalized RPC/revert message
- `value`: raw + decimal ETH value supplied

A revert or EVM execution error from `eth_call` is returned as a normal response with
`success: false`; transport/RPC failures before the call are returned as capability errors.

## Boundaries

- BSC and Ethereum.
- Read-only: no signing, approval, broadcast, or state change.
- `eth_call` is not a full transaction simulation: it does not model block-level gas accounting,
  state changes across multiple transactions, or MEV.
- This capability does not construct swap calldata; `swap.build` remains a separate local
  capability.
