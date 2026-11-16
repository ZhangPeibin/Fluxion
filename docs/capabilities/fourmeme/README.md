# fourmeme.agent-tokens

Read-only discovery of recent Four.meme tokens using the **Binance Agent ID Exclusive** launch
mode on BSC.

This is not the same as Agent Creator or the `.ai` tag. Agent ID Exclusive restricts bonding-curve
trading to Binance Agentic Wallets with an Agent ID; after graduation trading is open to everyone.

## Usage

```bash
fluxion fourmeme agent-tokens --json
fluxion fourmeme agent-tokens --wallet 0x... --limit 20 --json
fluxion fourmeme agent-tokens --lookback-blocks 100000 --json
```

- `limit`: maximum returned tokens (1–50, default 20).
- `lookback-blocks`: BSC block range to scan (1–5,000,000, default 100,000; roughly 3.5 days at
  3-second blocks).
- `wallet`: optional; checks Four.meme `AgentIdentifier.isAgent(wallet)`.

Output includes:

- token address, name, symbol
- template id (`1778027615735` for Agent ID Exclusive)
- phase: `bonding`, `graduated`, or `unknown`
- progress and raised amount when the Four.meme detail API is available
- `agent_id_exclusive`
- optional `wallet_eligible` / `eligibility_reason`
- `scanned`: number of matching `TokenCreated` events found (before `limit`)
- `lookback_blocks`, `from_block`, `to_block`, `coverage`, `sources`, `warnings`

## Semantics

- Discovery scans OpenFour Core `TokenCreated` logs and filters on `presetId=1778027615735`.
  Template id / preset is the source of truth; `aiCreator` and `.ai` tag are **not** sufficient.
- `PUBLISH` is treated as bonding; `TRADE` as graduated.
- `wallet_eligible` is `true`, `false`, or omitted when on-chain verification is unavailable.
- A graduated token is open to any wallet; a bonding token requires the AgentIdentifier gate.
- The scan is bounded by `lookback-blocks`; it is not complete history.
- API detail is optional enrichment. If the API is unavailable or rate-limited, token discovery
  still works from logs, with `phase: unknown` and one warning; detail calls stop after a rate-limit
  response.

## Data sources

- BSC `eth_getLogs` on OpenFour Core `0xebe7b6C1089D9F72aD07f34E36d898e44E5e27f3`
- BSC `AgentIdentifier` `0x09B44A633de9F9EBF6FB9Bdd5b5629d3DD2cef13`
- Four.meme REST API (`FOURMEME_API_URL`, default `https://four.meme/meme-api/v1`)
- BSC RPC (`FOURMEME_BSC_RPC_URL`, default `https://bsc-rpc.publicnode.com`) must support
  `eth_getLogs`; public dataseed endpoints may not.

## Boundaries

- Read-only; no buy/sell calldata or transaction execution.
- Does not log in to Binance Agentic Wallet or enable Developer Mode.
- Does not verify the 1% wallet cap from on-chain token balance yet.
- Partial log coverage is explicit in `warnings`; missing ranges are not treated as zero tokens.
- Not financial advice; thin bonding curves are high risk.
