# wallet capabilities

Read-only account capabilities for Ethereum, BSC Agent ID identity, and local WalletConnect session
management. `wallet.sign` signs messages and EIP-712 typed data only; none of these capabilities
approves or broadcasts transactions.

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

## `wallet.allowance`

Exact ERC20 allowances for one owner/spender pair at a pinned block.

```bash
fluxion wallet allowance 0xowner --spender 0xspender --tokens USDC,LDO --json
```

- `tokens` may be symbols or addresses; an empty list uses the built-in registry.
- Zero allowances are returned so approval checks are unambiguous.
- Coverage is `local`.

## `wallet.collectibles`

Read ERC721/ERC1155 holdings.

```bash
fluxion wallet collectibles 0xowner --json
fluxion wallet collectibles 0xowner --collections 0xcollection1,0xcollection2 --json
```

- If `ALCHEMY_API_KEY` is configured, Alchemy NFT discovery supplies candidate token IDs and
  local RPC verifies ownership.
- Without an indexer, `--collections` or the curated collection registry is searched on-chain.
- ERC721 collections that implement `ERC721Enumerable` return token IDs.
- Non-enumerable ERC721 collections return the count only; ERC1155 enumeration requires the
  indexer.
- `coverage` is `provider+local`, `registry+local`, or `local`.

## `wallet.agent-id`

Resolve a Binance/SPACE ID `.ai` Agent ID on BSC. Agent ID is a soulbound ERC-721
handle that points at a BSC wallet address.

```bash
fluxion wallet agent-id alice.ai
fluxion wallet agent-id 0xA506ac7069937571DC56C5f8FDa53c4A729456c6 --json
```

- Exactly one of a handle or address is required.
- Primary source: SPACE ID Agent ID API.
- Verification/fallback: BSC registry `0x60B9262456dE011F7d15a54CdD8c48bAaa8B022F`.
- `found: false` means no Agent ID is registered for the handle or address.
- `coverage` is `api+onchain`, `api`, or `onchain`.
- This is identity resolution only. It does not log in to Binance Agentic Wallet and does not
  check Four.meme launch eligibility.

## `wallet.sign`

Sign a message or EIP-712 typed data through the connected WalletConnect session. The user must
confirm on their phone.

```bash
fluxion wallet sign --message "hello"
fluxion wallet sign --typed-data typed-data.json --json
```

- Requires an active `wallet.connect` session.
- Supports `personal_sign` and `eth_signTypedData_v4`.
- Returns the signature, address, chain ID and method.
- The phone rejection is returned as `USER_REJECTED`.
- Message and typed data are limited to 64 KB.
- Typed data `domain.chainId` must match the session chain when present.
- Sessions created before the relay identity was persisted must be recreated with
  `wallet.disconnect` + `wallet.connect`.

## `wallet.connect`

Start a WalletConnect v2 pairing and return a URI plus an ASCII QR code. The wallet
approves on the user's phone; private keys never enter Fluxion.

```bash
fluxion wallet connect
fluxion wallet connect --json
```

- Requires `WALLETCONNECT_PROJECT_ID`.
- The CLI prints the QR code and waits up to 5 minutes.
- MCP returns the pending URI and a 53-column terminal QR (`qr_ascii`) immediately. The QR is the
  first text content block so the agent can reproduce it before polling. A small PNG is also
  written and returned as `qr_png_path` for fallback, but it is not opened automatically. The agent
  should show the QR and call `wallet_status` with `wait_seconds` (up to 110) instead of asking the
  user to check manually.
- If a valid session already exists, `wallet.connect` returns `connected` without a new pairing.
- The session is stored at `~/.fluxion/walletconnect/session.json` with mode `0600`.

## `wallet.status`

Read the stored WalletConnect session.

```bash
fluxion wallet status
fluxion wallet status --json
fluxion wallet status --wait 90
```

- MCP `wait_seconds` (`0..110`) long-polls a pending pairing; CLI `--wait N` waits for a session
  file written by another process.
- `disconnected`: no local session.
- `pending`: a pairing is waiting for the wallet.
- `connected`: session is active.
- `expired`: session exists but its expiry has passed.
- Returns accounts, `chain_id`, expiry, and wallet metadata. It does not contact the wallet.

## `wallet.disconnect`

Remove the local session and abort any pending pairing.

```bash
fluxion wallet disconnect --json
```

## MCP tools

| Tool | Execution |
|---|---|
| `wallet_balance` | local |
| `wallet_assets` | hybrid |
| `wallet_allowance` | local |
| `wallet_collectibles` | hybrid |
| `wallet_agent_id` | hybrid |
| `wallet_sign` | local |
| `wallet_connect` | local |
| `wallet_status` | local |
| `wallet_disconnect` | local |

## Configuration

- `FLUXION_RPC_URL` (or `RPC_URL`): local Ethereum RPC; defaults to `https://eth.drpc.org`
- `ONEINCH_API_KEY`: optional; enables full `wallet.assets` discovery
- `FLUXION_ONEINCH_BALANCE_URL`: optional provider override
- `ALCHEMY_API_KEY`: optional; enables full `wallet.collectibles` discovery
- `FLUXION_ALCHEMY_NFT_URL`: optional Alchemy NFT endpoint override
- `WALLETCONNECT_PROJECT_ID`: required for `wallet.connect`; create one at https://cloud.reown.com
- `WALLETCONNECT_RELAY_URL`: optional relay override; defaults to `wss://relay.walletconnect.com`
- `FLUXION_AGENT_ID_API_URL`: optional SPACE ID Agent ID API override
- `FLUXION_BSC_RPC_URL`: optional BSC RPC override for Agent ID verification/fallback

## Output

- Balances: [`schemas/wallet_balances.v1.json`](../../../schemas/wallet_balances.v1.json)
- Allowances: [`schemas/wallet_allowances.v1.json`](../../../schemas/wallet_allowances.v1.json)
- Collectibles: [`schemas/wallet_collectibles.v1.json`](../../../schemas/wallet_collectibles.v1.json)
- Connect: [`schemas/wallet_connect.v1.json`](../../../schemas/wallet_connect.v1.json)
- Status: [`schemas/wallet_status.v1.json`](../../../schemas/wallet_status.v1.json)
- Disconnect: [`schemas/wallet_disconnect.v1.json`](../../../schemas/wallet_disconnect.v1.json)
- Agent ID: [`schemas/wallet_agent_id.v1.json`](../../../schemas/wallet_agent_id.v1.json)
- Sign: [`schemas/wallet_sign.v1.json`](../../../schemas/wallet_sign.v1.json)

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
- Native ETH, ERC20, ERC721 and ERC1155 are supported; NFT discovery is best-effort.
- WalletConnect sessions currently support `eip155:1` only.
- `wallet.agent-id` is BSC only; Binance Agentic Wallet login and execution are not implemented.
- `wallet.sign` signs messages and EIP-712 typed data only; it does not sign transactions or
  broadcast.
- No approvals, private keys, or transaction execution.
- No USD valuation.
- `wallet.assets` discovery is best-effort; `coverage` and `warnings` make completeness explicit.
