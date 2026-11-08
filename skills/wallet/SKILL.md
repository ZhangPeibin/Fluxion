---
name: wallet
description: Use when the user asks for an Ethereum account's native ETH or ERC20 balances, wants to know which tokens an address holds, or needs ERC20 allowances for an owner/spender pair. Covers the Fluxion wallet_balance, wallet_assets and wallet_allowance MCP tools. Read-only; never signs, approves, or executes transactions.
---

# wallet capabilities

Read-only Local Runtime capabilities for Ethereum accounts. These capabilities never ask for or
handle private keys, and a balance or allowance response is never authorization to transact.

- Human docs: [`docs/capabilities/wallet`](../../docs/capabilities/wallet/README.md)
- Schemas:
  [`wallet_balances`](../../schemas/wallet_balances.v1.json),
  [`wallet_allowances`](../../schemas/wallet_allowances.v1.json)

## Execution model

Do not choose local/remote. Execution is fixed per capability:

| Capability | Execution |
|---|---|
| `wallet.balance` | local |
| `wallet.assets` | hybrid (1inch candidate discovery + local verification) |
| `wallet.allowance` | local |

The agent does not start or manage a server. `FLUXION_RPC_URL` selects the local Ethereum RPC;
`ONEINCH_API_KEY` enables full `wallet.assets` discovery.

## `wallet_balance`

Exact balances for a known token set at one pinned block.

```json
{
  "name": "wallet_balance",
  "arguments": {
    "address": "0x...",
    "tokens": ["ETH", "USDC", "LDO"],
    "include_native": true
  }
}
```

- `address` is required.
- `tokens` are symbols or addresses; omit it to scan the built-in registry.
- Native ETH is included unless `include_native` is `false`.
- Coverage is `local`.

## `wallet_assets`

Discover what ERC20 tokens an account holds, then verify every balance locally.

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
- `unknown_count` reports unknown nonzero tokens that were hidden.
- `include_unknown: true` expands unknown tokens; metadata is read from the token contract.
- `coverage` is `provider+local` or `registry+local`.
- Always inspect `warnings` before claiming the result is complete.

## `wallet_allowance`

ERC20 allowances for one owner/spender pair at one pinned block.

```json
{
  "name": "wallet_allowance",
  "arguments": {
    "address": "0xowner",
    "spender": "0xspender",
    "tokens": ["USDC", "LDO"]
  }
}
```

- `address` (owner) and `spender` are required.
- `tokens` defaults to the built-in registry.
- Zero allowances are included; use `amount.raw` for approval checks.
- Coverage is `local`.

## CLI

```bash
fluxion wallet balance 0xabc... --tokens ETH,USDC,LDO --json
fluxion wallet assets 0xabc... --json
fluxion wallet assets 0xabc... --include-unknown --json
fluxion wallet allowance 0xowner --spender 0xspender --tokens USDC,LDO --json
```

Common options: `--rpc-url URL`, `--block N`, `--no-native` (balance/assets),
`--include-unknown` (assets), `--spender ADDRESS` (allowance).

## Reading the result

Balances and assets:

- `block_number` / `block_hash`: the pinned block for every read.
- `native` / `tokens[]`: `token.symbol`, `token.address`, `token.decimals`, `amount.raw`,
  `amount.decimal`, `known`.
- `unknown_count`: unknown nonzero tokens not expanded.
- `coverage` / `sources` / `warnings`: completeness and provenance.

Allowances:

- `owner`, `spender`: the queried pair.
- `allowances[]`: `token`, `spender`, `amount.raw`, `amount.decimal`, `known`.
- A zero `amount.raw` means no ERC20 allowance; it does not mean an approval is safe or required.

## Errors

| Code | Meaning | Agent action |
|---|---|---|
| `INVALID_ARGUMENT` | Bad address, token or block | Fix the request |
| `RPC_ERROR` | Local RPC unavailable or returned bad data | Retry or check `FLUXION_RPC_URL` |
| `UNSUPPORTED` | Asset discovery unavailable (for example missing API key) | Use registry-only results or configure a provider |
| `TIMEOUT` | RPC or discovery timed out | Retry later |

## Boundaries

- Ethereum only.
- Native ETH and ERC20 only; NFT/collectibles are separate.
- No USD valuation.
- Read-only: no approval, signing, or transaction execution.
- Re-check the pinned block, balances and allowances before building a transaction.
