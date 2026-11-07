# wallet capabilities

Read-only Local Runtime capabilities for Ethereum accounts. Neither capability signs, approves,
or broadcasts transactions.

## `wallet.balance`

Exact balances for an explicit token set, queried locally at one pinned block.

```bash
fluxion wallet balance 0xabc... --tokens ETH,USDC,LDO --json
```

- `tokens` may be built-in symbols or `0x` addresses; an empty list uses the built-in registry.
- Native ETH is included unless `--no-native` is passed.
- Unknown token addresses get `decimals()` / `symbol()` from the token contract.
- Coverage is always `local`.

## `wallet.assets`

Discover a wallet's ERC20 positions, then verify every returned balance locally.

```bash
fluxion wallet assets 0xabc... --json
fluxion wallet assets 0xabc... --include-unknown --json
```

Flow:

```text
address
  -> 1inch Balance API candidate discovery
  -> known-registry filter / unknown summary
  -> token metadata (known registry or ERC20 calls)
  -> local fixed-block multicall: balanceOf + eth_getBalance
  -> nonzero token list + coverage/sources/warnings
```

Defaults:

- native ETH + built-in registry tokens
- unknown nonzero tokens are summarized in `unknown_count`
- `--include-unknown` expands unknown tokens (metadata is read from the token contract)
- if the discovery provider is unavailable, falls back to registry-only discovery and reports
  `coverage: registry+local` plus a warning

## MCP tools

| Tool | Execution |
|---|---|
| `wallet_balance` | local |
| `wallet_assets` | hybrid |

## Configuration

- `FLUXION_RPC_URL` (or `RPC_URL`): local Ethereum RPC; defaults to `https://eth.drpc.org`
- `ONEINCH_API_KEY`: optional; enables full `wallet.assets` discovery
- `FLUXION_ONEINCH_BALANCE_URL`: optional provider override

## Output

See [`schemas/wallet_balances.v1.json`](../../../schemas/wallet_balances.v1.json).

```json
{
  "address": "0xabc...",
  "block_number": 26356654,
  "block_hash": "0x...",
  "native": {
    "token": {"symbol": "ETH", "address": "0xeeee...", "decimals": 18},
    "amount": {"raw": "5793818533768043911", "decimal": "5.793818533768043911"},
    "known": true
  },
  "tokens": [],
  "unknown_count": 0,
  "coverage": "provider+local",
  "sources": [
    {"name": "1inch-balance", "scope": "candidate-discovery"},
    {"name": "ethereum-rpc", "scope": "verified-balances", "block": 26356654}
  ],
  "warnings": []
}
```

## Boundaries

- Ethereum only.
- Native ETH and ERC20 only; ERC721/ERC1155 belong to a future `wallet.collectibles`.
- No signing, approvals, private keys, or transaction execution.
- No USD valuation.
- `wallet.assets` discovery is best-effort; `coverage` and `warnings` make completeness explicit.
