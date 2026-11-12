# wallet capabilities

Local wallet capabilities for **BSC by default**, with Ethereum available through `--chain
ethereum`. They cover balances, asset discovery, allowances, NFTs, WalletConnect sessions, message
signing and native/ERC20/BEP-20 transfers.

## `wallet.balance`

Exact balances for an explicit token set, queried locally at one pinned block.

```bash
fluxion wallet balance 0xabc... --tokens USDT,CAKE --json
fluxion wallet balance 0xabc... --tokens ETH,USDC --chain ethereum --json
```

- Default chain is BSC; use `--chain ethereum` for Ethereum.
- `tokens` may be built-in symbols or `0x` addresses; an empty list uses the chain registry.
- Native BNB/ETH is included unless `--no-native` is passed.
- Unknown token addresses get `decimals()` / `symbol()` from the token contract.
- Coverage is always `local`.

## `wallet.assets`

Discover a wallet's ERC20 positions, then verify every returned balance locally.

```bash
fluxion wallet assets 0xabc... --json
fluxion wallet assets 0xabc... --chain ethereum --include-unknown --json
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

- native BNB/ETH + built-in chain registry tokens
- unknown nonzero tokens are summarized in `unknown_count`
- `--include-unknown` expands unknown tokens (metadata is read from the token contract)
- if the discovery provider is unavailable, falls back to registry-only discovery and reports
  `coverage: registry+local` plus a warning

## `wallet.allowance`

Exact ERC20 allowances for one owner/spender pair at a pinned block.

```bash
fluxion wallet allowance 0xowner --spender 0xspender --tokens USDT,CAKE --json
fluxion wallet allowance 0xowner --spender 0xspender --tokens USDC --chain ethereum --json
```

- Default chain is BSC; use `--chain ethereum` for Ethereum.
- `tokens` may be symbols or addresses; an empty list uses the chain registry.
- Zero allowances are returned so approval checks are unambiguous.
- Coverage is `local`.

## `wallet.collectibles`

Read ERC721/ERC1155 holdings.

```bash
fluxion wallet collectibles 0xowner --json
fluxion wallet collectibles 0xowner --chain ethereum --json
fluxion wallet collectibles 0xowner --collections 0xcollection1,0xcollection2 --json
```

- Default chain is BSC; use `--chain ethereum` for Ethereum.
- With `ALCHEMY_API_KEY`, Alchemy NFT discovery uses `bnb-mainnet` or `eth-mainnet`.
- Without an indexer, pass `--collections`. The Ethereum curated registry is not used on BSC.
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
fluxion wallet sign --typed-data typed-data.json --policy policy.json --json
```

- Requires an active `wallet.connect` session.
- Supports `personal_sign` and `eth_signTypedData_v4`.
- Returns the signature, address, chain ID and method.
- The phone rejection is returned as `USER_REJECTED`.
- Message and typed data are limited to 64 KB.
- Typed data `domain.chainId` must match the session chain when present.
- With `--policy`, typed data is inspected before signing; enforce-mode high-risk findings return
  `POLICY_BLOCKED`. Message signing with a policy is rejected.
- Sessions created before the relay identity was persisted must be recreated with
  `wallet.disconnect` + `wallet.connect`.

## `wallet.send`

Send a native or ERC20/BEP-20 transfer through the connected WalletConnect session. The transfer
is simulated locally before the phone confirmation.

```bash
fluxion wallet send --to 0x... --token BNB --amount 0.01
fluxion wallet send --to 0x... --token USDT --amount 10 --json
fluxion wallet send --to 0x... --token USDT --amount 10 --policy policy.json
```

- Only native transfers and ERC20/BEP-20 `transfer` are supported.
- On Ethereum use `ETH`; on BSC use `BNB`.
- `from` defaults to the session account and must belong to the session.
- `token` accepts the native symbol, built-in symbols, or a token address; unknown addresses need `decimals`.
- BSC built-ins: `WBNB`, `USDT`, `USDC`, `CAKE`.
- Simulation failure returns `SIMULATION_FAILED` and does not reach the phone.
- With `--policy`, the generated transfer is inspected before phone confirmation. Enforce-mode
  high-risk findings return `POLICY_BLOCKED`; advisory mode returns findings but does not block.
- Wallet rejection returns `USER_REJECTED`.
- After a timeout, check the wallet/transaction history; do not blindly retry.
- Arbitrary calldata, approvals, NFT transfers and swap execution are out of scope.

## `wallet.connect`

Start a WalletConnect v2 pairing and return a URI plus an ASCII QR code. The wallet
approves on the user's phone; private keys never enter Fluxion.

```bash
fluxion wallet connect                 # BSC by default
fluxion wallet connect --chain ethereum
fluxion wallet connect --json
```

- Requires `WALLETCONNECT_PROJECT_ID`.
- `--chain bsc` (default) or `--chain ethereum` selects the session chain.
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
- Returns accounts, chain, expiry, and wallet metadata. It does not contact the wallet.

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
| `wallet_send` | local |
| `wallet_connect` | local |
| `wallet_status` | local |
| `wallet_disconnect` | local |

## Configuration

- `FLUXION_RPC_URL` (or `RPC_URL`): local Ethereum RPC; defaults to `https://eth.drpc.org`
- `FLUXION_BSC_RPC_URL`: local BSC RPC; defaults to `https://bsc-dataseed.bnbchain.org`
- `ONEINCH_API_KEY`: optional; enables full `wallet.assets` discovery
- `FLUXION_ONEINCH_BALANCE_URL`: optional provider override
- `ALCHEMY_API_KEY`: optional; enables full `wallet.collectibles` discovery
- `FLUXION_ALCHEMY_NFT_URL`: optional Alchemy NFT endpoint override (defaults to the selected chain network)
- `WALLETCONNECT_PROJECT_ID`: required for `wallet.connect`; create one at https://cloud.reown.com
- `WALLETCONNECT_RELAY_URL`: optional relay override; defaults to `wss://relay.walletconnect.com`
- `FLUXION_AGENT_ID_API_URL`: optional SPACE ID Agent ID API override

## Output

- Balances: [`schemas/wallet_balances.v1.json`](../../../schemas/wallet_balances.v1.json)
- Allowances: [`schemas/wallet_allowances.v1.json`](../../../schemas/wallet_allowances.v1.json)
- Collectibles: [`schemas/wallet_collectibles.v1.json`](../../../schemas/wallet_collectibles.v1.json)
- Connect: [`schemas/wallet_connect.v1.json`](../../../schemas/wallet_connect.v1.json)
- Status: [`schemas/wallet_status.v1.json`](../../../schemas/wallet_status.v1.json)
- Disconnect: [`schemas/wallet_disconnect.v1.json`](../../../schemas/wallet_disconnect.v1.json)
- Agent ID: [`schemas/wallet_agent_id.v1.json`](../../../schemas/wallet_agent_id.v1.json)
- Sign: [`schemas/wallet_sign.v1.json`](../../../schemas/wallet_sign.v1.json)
- Send: [`schemas/wallet_send.v1.json`](../../../schemas/wallet_send.v1.json)

```json
{
  "chain": "bsc",
  "chain_id": 56,
  "address": "0xabc...",
  "block_number": 132891764,
  "block_hash": "0x...",
  "native": {
    "token": {"symbol": "BNB", "address": "0xeeee...", "decimals": 18},
    "amount": {"raw": "5793818533768043911", "decimal": "5.793818533768043911"},
    "known": true
  },
  "tokens": [],
  "unknown_count": 0,
  "coverage": "provider+local",
  "sources": [
    {"name": "1inch-balance", "scope": "candidate-discovery"},
    {"name": "bsc-rpc", "scope": "verified-balances", "block": 132891764}
  ],
  "warnings": []
}
```

## Boundaries

- BSC and Ethereum.
- Native BNB/ETH, ERC20/BEP-20, ERC721 and ERC1155 are supported; NFT discovery is best-effort.
- WalletConnect sessions support `eip155:1` and `eip155:56`; create a separate session per chain.
- `wallet.agent-id` is BSC only; Binance Agentic Wallet login and execution are not implemented.
- All local wallet reads, connect, sign, send and simulation support Ethereum and BSC.
- Default local chain is BSC; use `--chain ethereum` for Ethereum.
- `wallet.sign` signs messages and EIP-712 typed data only; it does not sign transactions.
- `wallet.send` broadcasts native/ERC20/BEP-20 transfers only; no arbitrary contract calls, approvals or
  swap execution.
- No private keys.
- No USD valuation.
- `wallet.assets` discovery is best-effort; `coverage` and `warnings` make completeness explicit.
