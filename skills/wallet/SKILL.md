---
name: wallet
description: Use when the user asks for an account's native or token balances, wants to know which tokens or NFTs an address holds, needs allowances, wants to resolve a BSC .ai Agent ID, wants to link a mobile wallet with WalletConnect, wants to sign a message or EIP-712 typed data, or wants to send native/ERC20 funds. Local wallet tools default to BSC and accept Ethereum via chain. Covers the Fluxion wallet_balance, wallet_assets, wallet_allowance, wallet_collectibles, wallet_agent_id, wallet_connect, wallet_status, wallet_sign, wallet_send and wallet_disconnect MCP tools. Account data is read-only; linking, signing and sending never ask for private keys.
---

# wallet capabilities

Read-only account capabilities, BSC Agent ID identity, local WalletConnect session management,
message/EIP-712 signing, and native/ERC20/BEP-20 transfers. Local wallet tools default to BSC;
pass `chain: "ethereum"` for Ethereum. These capabilities never ask for or handle
private keys, and a balance or allowance response is never authorization to transact.

- Human docs: [`docs/capabilities/wallet`](../../docs/capabilities/wallet/README.md)
- Schemas:
  [`wallet_balance v2`](../../schemas/wallet_balances.v2.json),
  [`wallet_assets v2`](../../schemas/wallet_assets.v2.json),
  [`wallet_allowances v2`](../../schemas/wallet_allowances.v2.json),
  [`wallet_collectibles v2`](../../schemas/wallet_collectibles.v2.json),
  [`wallet_connect`](../../schemas/wallet_connect.v2.json),
  [`wallet_status`](../../schemas/wallet_status.v2.json),
  [`wallet_disconnect`](../../schemas/wallet_disconnect.v2.json),
  [`wallet_agent_id`](../../schemas/wallet_agent_id.v2.json),
  [`wallet_sign`](../../schemas/wallet_sign.v2.json),
  [`wallet_send`](../../schemas/wallet_send.v2.json)

## Execution model

Do not choose local/remote. Execution is fixed per capability:

| Capability | Execution |
|---|---|
| `wallet.balance` | local |
| `wallet.assets` | hybrid (1inch candidate discovery + local verification) |
| `wallet.allowance` | local |
| `wallet.collectibles` | hybrid |
| `wallet.agent-id` | hybrid |
| `wallet.sign` | local |
| `wallet.send` | local |
| `wallet.connect` | local |
| `wallet.status` | local |
| `wallet.disconnect` | local |

The agent does not start or manage a server. `FLUXION_BSC_RPC_URL` selects the local BSC RPC
(default chain); `FLUXION_RPC_URL` selects Ethereum. `ONEINCH_API_KEY` enables full
`wallet.assets` discovery.

## `wallet_balance`

Exact balances for a known token set at one pinned block.

```json
{
  "name": "wallet_balance",
  "arguments": {
    "address": "0x...",
    "chain": "bsc",
    "tokens": ["BNB", "USDT", "CAKE"],
    "include_native": true
  }
}
```

- `address` is required.
- `chain` defaults to `bsc`; use `ethereum` for Ethereum.
- `tokens` are symbols or addresses; omit it to scan the chain registry.
- Native BNB/ETH is included unless `include_native` is `false`.
- Returns capability envelope v2: `meta`, `chain`, `block`, `provenance`, `coverage`, `warnings`,
  `unknowns`, and `data`.
- Read balances from `data.native` and `data.tokens[]`; the pinned block is in `block`.
- `coverage.dimensions.data` is `complete` when the local read succeeded.

## `wallet_assets`

Discover what ERC20 tokens an account holds, then verify every balance locally.

```json
{
  "name": "wallet_assets",
  "arguments": {
    "address": "0x...",
    "chain": "bsc",
    "include_unknown": false
  }
}
```

- Default result: native ETH + known-registry tokens with nonzero balances.
- `unknown_count` reports unknown nonzero tokens that were hidden.
- `include_unknown: true` expands unknown tokens; metadata is read from the token contract.
- Returns envelope v2; balances are under `data`, and discovery sources are under
  `provenance.sources`.
- Always inspect `warnings` and `unknowns` before claiming the result is complete.

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
- Returns envelope v2; `owner`, `spender`, and `allowances[]` are under `data`.

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
- Without an indexer, pass `collections` or use the curated registry.
- Non-enumerable ERC721 returns `enumerable: false` and a count only.
- ERC1155 enumeration requires the indexer.
- Returns envelope v2; `owner` and `collections[]` are under `data`.

## `wallet_agent_id`

Resolve a BSC `.ai` Agent ID from a handle or wallet address.

```json
{
  "name": "wallet_agent_id",
  "arguments": {
    "handle": "alice.ai"
  }
}
```

- Pass exactly one of `handle` or `address`.
- `alice`, `alice.ai`, and mixed-case input are normalized to `alice`.
- Returns envelope v2; `data` contains `query`, `found`, optional `handle`, `agent_id`, `owner`,
  `token_id`, `tx_hash`, `as_of_block`. The pinned block is on the envelope.
- `coverage.dimensions` reports data and eligibility coverage.
- `found: false` means not registered; it is not an error.
- This is identity resolution only. It does not log in to Binance Agentic Wallet and does not
  check Four.meme launch eligibility.

```json
{
  "name": "wallet_agent_id",
  "arguments": {
    "address": "0xA506ac7069937571DC56C5f8FDa53c4A729456c6"
  }
}
```

## `wallet_sign`

Sign a message or EIP-712 typed data with the connected WalletConnect session. The user confirms
on their phone.

```json
{
  "name": "wallet_sign",
  "arguments": {
    "message": "hello"
  }
}
```

```json
{
  "name": "wallet_sign",
  "arguments": {
    "typed_data": {
      "types": {},
      "primaryType": "Person",
      "domain": {"chainId": 1},
      "message": {}
    }
  }
}
```

- Pass exactly one of `message` or `typed_data`.
- `timeout_seconds` defaults to 90 and is capped at 110.
- `address` is optional; it must match the session account.
- The user must confirm on their phone. A rejection returns `USER_REJECTED`.
- Do not call this on an opaque payload. Show the user what will be signed first.
- This signs messages/typed data only; it does not sign or send transactions.

## `wallet_send`

Send ETH or an ERC20 transfer through the connected WalletConnect session.

```json
{
  "name": "wallet_send",
  "arguments": {
    "to": "0xrecipient",
    "token": "ETH",
    "amount": "0.01"
  }
}
```

- Pass `to`, `token`, and `amount`.
- Use `ETH` on Ethereum and `BNB` on BSC.
- `token` may also be a built-in symbol or token address.
- BSC built-ins: `WBNB`, `USDT`, `USDC`, `CAKE`.
- `from` is optional and must match the session account.
- Always simulate first. A revert returns `SIMULATION_FAILED`; no phone prompt.
- The user confirms the transfer on their phone. Rejection returns `USER_REJECTED`.
- On timeout, do not retry blindly; tell the user to check the wallet and transaction history.
- Only native/ERC20/BEP-20 transfers are supported. No arbitrary calldata, approve, NFT, or swap execution.

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
- Pass `chain: "bsc"` (default) or `chain: "ethereum"`.
- Returns `status: "pending"`, `uri`, `qr_ascii`, and `expires_at` immediately.
- The first text content block of the MCP result contains the 53-column fenced QR. Reproduce that
  block verbatim in your own reply **before** calling `wallet_status`. Do not summarize it.
- Only after the QR block is written, call `wallet_status` with `wait_seconds: 90` and repeat while
  it returns `pending`.
- `qr_png_path` and the `image/png` content block are fallbacks if the terminal QR cannot be
  scanned.
- Do not ask for a private key or seed phrase.
- After showing the QR, call `wallet_status` with `wait_seconds: 90`. If it is still `pending`,
  call it again. Do not ask the user to say "check status".
- Continue until `connected`, `expired`, or `error`.
- If a valid session already exists, returns `connected` with the stored accounts.

## `wallet_status`

Read the local session. No wallet interaction.

```json
{"name": "wallet_status", "arguments": {"wait_seconds": 90}}
```

- `wait_seconds` is optional, `0..110`. With a value above zero it long-polls a pending pairing
  and returns as soon as the status changes or the wait expires.
- `disconnected`: no session.
- `pending`: waiting for the wallet to scan; call again if the wait expires.
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
fluxion wallet balance 0xabc... --tokens BNB,USDT,CAKE --json
fluxion wallet balance 0xabc... --chain ethereum --tokens ETH,USDC --json
fluxion wallet assets 0xabc... --json
fluxion wallet assets 0xabc... --chain ethereum --include-unknown --json
fluxion wallet allowance 0xowner --spender 0xspender --tokens USDT,CAKE --json
fluxion wallet collectibles 0xowner --json
fluxion wallet collectibles 0xowner --collections 0xcollection --json
fluxion wallet agent-id alice.ai --json
fluxion wallet sign --message "hello" --json
fluxion wallet sign --typed-data typed-data.json --policy policy.json --json
fluxion wallet send --to 0x... --token BNB --amount 0.01 --json
fluxion wallet send --to 0x... --token USDT --amount 10 --policy policy.json
fluxion wallet connect --chain bsc
fluxion wallet status
fluxion wallet disconnect
```

Common options: `--chain bsc|ethereum`, `--rpc-url URL`, `--block N`, `--no-native` (balance/assets),
`--include-unknown` (assets), `--spender ADDRESS` (allowance),
`--collections LIST` / `--include-empty` (collectibles).

## Reading the result

Read capabilities (`wallet.balance`, `wallet.assets`, `wallet.allowance`,
`wallet.collectibles`) use envelope v2:

- `meta`: `schema_version`, `capability`, `request_id`.
- `chain`: `{name, id}` for the selected chain.
- `block`: pinned block number and hash.
- `provenance` / `coverage` / `warnings` / `unknowns`: completeness and provenance.
- `data`: capability payload.

`wallet.balance` / `wallet.assets` data:

- `native` / `tokens[]`: `token.symbol`, `token.address`, `token.decimals`, `amount.raw`,
  `amount.decimal`, `known`.
- `unknown_count`: unknown nonzero tokens not expanded.

`wallet.allowance` data:

- `owner`, `spender`: the queried pair.
- `allowances[]`: `token`, `spender`, `amount.raw`, `amount.decimal`, `known`.
- A zero `amount.raw` means no ERC20 allowance; it does not mean an approval is safe or required.

`wallet.collectibles` data:

- `owner`: the queried wallet.
- `collections[]`: `collection`, `name`, `symbol`, `standard`, `enumerable`, `item_count`,
  `items[]` (`token_id`, `balance`).
- Non-enumerable collections have no token IDs; do not infer holdings from `item_count` alone.

Session, signing, and send capabilities use the same envelope:

- `wallet.connect` data: MCP connect response (`status`, `uri`, `qr_ascii`, `accounts`, `chain_id`)
  or the settled CLI status response.
- `wallet.status` data: `status`, `accounts`, optional `chain_id`, `expires_at`, `connection_id`,
  `wallet`, `error`.
- `wallet.disconnect` data: `disconnected`.
- `wallet.sign` data: `address`, `method`, `signature`.
- `wallet.send` data: `from`, `recipient`, `token`, `amount`, `contract`, `tx_hash`, `simulation`.
- `wallet.agent-id` data: `query`, `found`, and optional `handle`, `agent_id`, `owner`, `token_id`,
  `tx_hash`, `as_of_block`.

## Workflow recipe: portfolio

Use this when the user wants a read-only summary of holdings and curated yield positions. The
workflow is `wallet.balance → wallet.assets → wallet.collectibles → position.analyze`.

Checkpoints:

1. `wallet.balance` and `wallet.assets`: state the pinned block and whether discovery used a
   provider or the fallback registry.
2. `wallet.collectibles`: NFT discovery is best-effort; non-enumerable collections may report only
   a count.
3. `position.analyze`: Ethereum only. BSC portfolios stop before this step and say so.

Failure handling: provider unavailable → return registry results with warnings; unknown tokens stay
unknown; a position failure is reported separately and does not invalidate wallet balances. Never
invent USD values or infer positions from token balances.

## Errors

| Code | Meaning | Agent action |
|---|---|---|
| `INVALID_ARGUMENT` | Bad address, token or block | Fix the request |
| `RPC_ERROR` | Local RPC/relay unavailable or returned bad data | Retry or check the endpoint |
| `USER_REJECTED` | User rejected the signing/transfer request on their phone | Do not retry automatically |
| `SIMULATION_FAILED` | Transfer simulation reverted | Fix amount/recipient/token; do not send |
| `POLICY_BLOCKED` | Enforce policy rejected the transfer | Do not send; show policy reasons to the user |
| `UNSUPPORTED` | Asset discovery unavailable (for example missing API key) | Use registry-only results or configure a provider |
| `TIMEOUT` | RPC or discovery timed out | Retry later |

## Boundaries

- BSC and Ethereum.
- Native BNB/ETH, ERC20/BEP-20, ERC721 and ERC1155 are supported; NFT discovery is best-effort.
- No USD valuation.
- `wallet.agent-id` resolves BSC identities only; it does not log in to Binance Agentic Wallet.
- `wallet.sign` signs messages and EIP-712 typed data only; no transaction signing or execution.
- `wallet.send` sends native/ERC20/BEP-20 transfers only; no arbitrary contract calls, approvals,
  NFT transfers, or swap execution.
- `wallet.send --policy` / `wallet.sign --policy` enforce the same policy engine; a blocked request
  returns `POLICY_BLOCKED` before reaching the phone. Sign policy applies to typed data only.
- Local wallet tools default to BSC; use `chain: "ethereum"` for Ethereum.
- Read-only account data: no private keys.
- Re-check the pinned block, balances and allowances before building a transaction.
