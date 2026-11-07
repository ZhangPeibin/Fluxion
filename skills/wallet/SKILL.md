---
name: wallet
description: Use when the user asks for an Ethereum account's native ETH or ERC20 balances, wants to know which tokens an address holds, or uses the Fluxion wallet_balance / wallet_assets MCP tools. Read-only; never signs, approves, or executes transactions.
---

# wallet capabilities

Use these capabilities for read-only account balances on Ethereum. Never ask for or handle private
keys, and never treat a balance response as authorization to transact.

## `wallet_balance`

Exact balances for a known token set.

```json
{
  "name": "wallet_balance",
  "arguments": {
    "address": "0x...",
    "tokens": ["ETH", "USDC", "LDO"]
  }
}
```

- `tokens` are symbols or addresses; omit it to scan the built-in registry.
- Native ETH is included unless `include_native` is `false`.
- Coverage is `local`; every balance is read from one pinned block.

## `wallet_assets`

Discover what tokens an account holds, then verify locally.

```json
{
  "name": "wallet_assets",
  "arguments": {
    "address": "0x...",
    "include_unknown": false
  }
}
```

- Default result: native ETH + known-registry tokens with nonzero balances.
- `unknown_count` reports hidden unknown tokens.
- Set `include_unknown: true` to expand unknown tokens; metadata is read from the token contract.
- `coverage` is `provider+local` or `registry+local`; always read `warnings` before claiming the
  result is complete.

## Reading the result

- `block_number` / `block_hash`: the pinned block for every balance.
- `native` / `tokens[]`: `token.symbol`, `token.address`, `token.decimals`, `amount.raw`,
  `amount.decimal`, `known`.
- `sources`: discovery vs local verification.
- `coverage` and `warnings`: completeness boundaries.
- `unknown_count`: unknown nonzero tokens not expanded.

## Boundaries

- Ethereum only.
- Native ETH + ERC20 only.
- No NFT, USD valuation, signing, approvals, or execution.
- Re-check the block and balances before building any transaction.
