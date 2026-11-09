---
name: wallet
description: Use when the user asks for an Ethereum account's native ETH or ERC20 balances, wants to know which tokens or NFTs an address holds, needs ERC20 allowances for an owner/spender pair, or wants to link a mobile wallet with WalletConnect. Covers the Fluxion wallet_balance, wallet_assets, wallet_allowance, wallet_collectibles, wallet_connect, wallet_status and wallet_disconnect MCP tools. Read-only for account data; wallet linking never asks for private keys and does not sign or execute transactions.
---

# wallet capabilities

Read-only account capabilities and local WalletConnect session management. These capabilities
never ask for or handle private keys, and a balance or allowance response is never authorization
to transact.

- Human docs: [`docs/capabilities/wallet`](../../docs/capabilities/wallet/README.md)
- Schemas:
  [`wallet_balances`](../../schemas/wallet_balances.v1.json),
  [`wallet_allowances`](../../schemas/wallet_allowances.v1.json),
  [`wallet_connect`](../../schemas/wallet_connect.v1.json),
  [`wallet_status`](../../schemas/wallet_status.v1.json),
  [`wallet_disconnect`](../../schemas/wallet_disconnect.v1.json)

## Execution model

Do not choose local/remote. Execution is fixed per capability:

| Capability | Execution |
|---|---|
| `wallet.balance` | local |
| `wallet.assets` | hybrid (1inch candidate discovery + local verification) |
| `wallet.allowance` | local |
| `wallet.collectibles` | hybrid |
| `wallet.connect` | local |
| `wallet.status` | local |
| `wallet.disconnect` | local |

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

## `wallet_collectibles`

Read ERC721/ERC1155 holdings.

```json
{
  "name": "wallet_collectibles",
  "arguments": {
    "address": "0xowner",
    "collections": ["0xcollection"]
  }
}
```

- With `ALCHEMY_API_KEY`, Alchemy discovers candidate NFTs and local RPC verifies ownership.
- Without an indexer, pass `collections` or use the curated registry; coverage is
  `registry+local` / `local`.
- Non-enumerable ERC721 returns `enumerable: false` and a count only.
- ERC1155 enumeration requires the indexer.

## `wallet_connect`

Start a WalletConnect v2 pairing and return a QR code for the user to scan with MetaMask,
OKX, or another WalletConnect wallet.

```json
{
  "name": "wallet_connect",
  "arguments": {}
}
```

- Requires `WALLETCONNECT_PROJECT_ID` in the environment that starts `fluxion mcp`.
- Returns `status: "pending"`, `uri`, `qr_ascii`, and `expires_at` immediately.
- Show the QR text to the user. Do not ask for a private key or seed phrase.
- Poll `wallet_status` until it returns `connected`, `expired`, or `error`.
- If a valid session already exists, returns `connected` with the stored accounts.

## `wallet_status`

Read the local session. No wallet interaction.

```json
{"name": "wallet_status", "arguments": {}}
```

- `disconnected`: no session.
- `pending`: waiting for the wallet to scan.
- `connected`: accounts and chain are available.
- `expired`: session must be recreated.
- `accounts` are public addresses; they are safe to show.

## `wallet_disconnect`

Remove the local session and abort a pending pairing.

```json
{"name": "wallet_disconnect", "arguments": {}}
```

## CLI

```bash
fluxion wallet balance 0xabc... --tokens ETH,USDC,LDO --json
fluxion wallet assets 0xabc... --json
fluxion wallet assets 0xabc... --include-unknown --json
fluxion wallet allowance 0xowner --spender 0xspender --tokens USDC,LDO --json
fluxion wallet collectibles 0xowner --json
fluxion wallet collectibles 0xowner --collections 0xcollection --json
fluxion wallet connect
fluxion wallet status
fluxion wallet disconnect
```

Common options: `--rpc-url URL`, `--block N`, `--no-native` (balance/assets),
`--include-unknown` (assets), `--spender ADDRESS` (allowance),
`--collections LIST` / `--include-empty` (collectibles).

## Reading the result

Balances and assets:

- `block_number` / `block_hash`: the pinned block for every read.
- `native` / `tokens[]`: `token.symbol`, `token.address`, `token.decimals`, `amount.raw`,
  `amount.decimal`, `known`.
- `unknown_count`: unknown nonzero tokens not expanded.
- `coverage` / `sources` / `warnings`: completeness and provenance.

Collectibles:

- `collections[]`: `collection`, `name`, `symbol`, `standard`, `enumerable`, `item_count`,
  `items[]` (`token_id`, `balance`).
- `coverage`: `provider+local`, `registry+local`, or `local`.
- Non-enumerable collections have no token IDs; do not infer holdings from `item_count` alone.

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
- Native ETH, ERC20, ERC721 and ERC1155 are supported; NFT discovery is best-effort.
- No USD valuation.
- `wallet.connect` links a wallet but does not sign or execute. Signing is a later capability.
- Read-only: no approval, signing, or transaction execution.
- Re-check the pinned block, balances and allowances before building a transaction.
