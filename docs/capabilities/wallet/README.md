# wallet capabilities

Read-only account capabilities and local WalletConnect session management for Ethereum accounts.
None of these capabilities signs, approves, or broadcasts transactions.

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

## `wallet.connect`

Start a WalletConnect v2 pairing and return a URI plus an ASCII QR code. The wallet
approves on the user's phone; private keys never enter Fluxion.

```bash
fluxion wallet connect
fluxion wallet connect --json
```

- Requires `WALLETCONNECT_PROJECT_ID`.
- The CLI prints the QR code and waits up to 5 minutes.
- MCP returns the pending URI and a 51-column terminal QR (`qr_ascii`) immediately. A small PNG
  is also written and returned as `qr_png_path` for fallback, but it is not opened automatically.
  The agent should show `qr_ascii` and call `wallet_status` with `wait_seconds` (up to 110) instead
  of asking the user to check manually.
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

## Output

- Balances: [`schemas/wallet_balances.v1.json`](../../../schemas/wallet_balances.v1.json)
- Allowances: [`schemas/wallet_allowances.v1.json`](../../../schemas/wallet_allowances.v1.json)
- Collectibles: [`schemas/wallet_collectibles.v1.json`](../../../schemas/wallet_collectibles.v1.json)
- Connect: [`schemas/wallet_connect.v1.json`](../../../schemas/wallet_connect.v1.json)
- Status: [`schemas/wallet_status.v1.json`](../../../schemas/wallet_status.v1.json)
- Disconnect: [`schemas/wallet_disconnect.v1.json`](../../../schemas/wallet_disconnect.v1.json)

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
- `wallet.connect` links a wallet for future signing; it does not sign or send transactions yet.
- No signing, approvals, private keys, or transaction execution.
- No USD valuation.
- `wallet.assets` discovery is best-effort; `coverage` and `warnings` make completeness explicit.
