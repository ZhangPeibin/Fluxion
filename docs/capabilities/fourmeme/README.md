# Four.meme capabilities

Read-only discovery and detail for Four.meme **Binance Agent ID Exclusive** tokens on BSC.

Both capabilities return capability envelope v2: `meta`, `chain`, `as_of`, `provenance`,
`coverage`, `warnings`, `unknowns`, and `data`. Discovery and detail fields are under `data`.

This is not the same as Agent Creator or the `.ai` tag. Agent ID Exclusive restricts bonding-curve
trading to Binance Agentic Wallets with an Agent ID; after graduation trading is open to everyone.

## `fourmeme.agent-tokens`

### Usage

```bash
fluxion fourmeme agent-tokens --json
fluxion fourmeme agent-tokens --wallet 0x... --limit 20 --json
fluxion fourmeme agent-tokens --lookback-blocks 100000 --json
```

- `limit`: maximum returned tokens (1–50, default 20).
- `lookback-blocks`: BSC block range to scan (1–5,000,000, default 100,000; roughly 3.5 days at
  3-second blocks).
- `wallet`: optional; checks Four.meme `AgentIdentifier.isAgent(wallet)`.

The `data` payload includes address, name, symbol, image, template id, phase, progress/raised when
available, `agent_id_exclusive`, optional `wallet_eligible` / `eligibility_reason`, and
`scanned` / `lookback_blocks` / `from_block` / `to_block` / `tokens`. Coverage is in
`coverage.dimensions.data`; sources and warnings are in `provenance.sources` and `warnings`.

## `fourmeme.token`

```bash
fluxion fourmeme token 0x... --wallet 0x... --json
```

Returns envelope v2 with one token's image, description, phase, template, raised, market cap,
price, supply, creator, timestamps, and optional wallet eligibility under `data`; warnings and
sources are in the envelope. If the Four.meme detail API is
unavailable (rate limit, region restriction, outage), Fluxion reads OpenFour Core
`tokens(address)`, the vault phase and ERC20 `name` / `symbol` / `totalSupply` on BSC. It then
reports the on-chain phase/template/creator/supply with an explicit warning. If Core state is also
unavailable, only ERC20 basics are returned with `phase: unknown`.

### Socials and holder count

`fourmeme.token` also returns:

- `socials`: optional `twitter`, `telegram`, and `website` links from the detail API. The local UI
  only renders `http`/`https` links.
- `holder_count`: the API holder count when present. This is bonding-phase platform data; it may
  be 0 or unavailable after graduation. Missing values are omitted, never reported as zero.

### Market and momentum

`fourmeme.token` returns a `market` object when the detail API supplies it:

- `price`, `max_price`, `market_cap`, `liquidity`, `raised`
- `volume_24h`: API `dayTrading`
- `change_24h`: raw API `dayIncrease`; `change_24h_percent` is filled only when the raw value
  is unambiguous (explicit `%`, or an absolute value greater than 1)
- `progress_percent`: curve completion normalized to 0–100

Missing or null values are omitted. Prefer the raw `change_24h` when `change_24h_percent` is
absent rather than guessing a unit.

### Tradability and 1% cap

`fourmeme.token` also returns:

- `tradability.transfer_locked`: `true` while bonding, `false` after graduation, omitted when the
  phase is unknown. Inner-market transfers are locked during bonding.
- `tradability.progress_percent`: curve completion normalized to 0–100 when the API exposes it.
- `tradability.max_supply`: creation-time max supply as `{raw, decimal}` from OpenFour Core
  `tokens(address)`; this is the 1% hold-cap denominator, not live ERC20 `totalSupply()`.
- `wallet_cap`: only when a wallet is supplied, the token is Agent ID Exclusive, and the phase is
  bonding. It contains `balance`, `cap` (1% of max supply), `remaining`, and `reached`.

If the supply or balance is unavailable, the field is omitted and a warning is added; it is never
reported as zero.

## Local web page

```bash
fluxion fourmeme ui --open
fluxion fourmeme ui --port 8787 --lookback-blocks 100000 --wallet 0x...
```

- Binds only `127.0.0.1`; GET-only read routes.
- List cards link to `#/token/<address>` detail pages.
- Phase tabs, search, lookback and optional wallet eligibility.
- Four.meme and BscScan external links.
- No wallet connection, signing, buying or selling.
- List cache 30 s; detail cache 60 s. API failures are shown; they never become invented phases.

Routes:

```text
GET /                       HTML
GET /app.js /styles.css     embedded static assets
GET /api/config             UI defaults
GET /api/tokens             list + optional wallet eligibility
GET /api/token/<address>    detail + optional wallet eligibility
```

## Semantics

- Discovery scans OpenFour Core `TokenCreated` logs and filters on `presetId=1778027615735`.
  Template id / preset is the source of truth; `aiCreator` and `.ai` tag are **not** sufficient.
- `PUBLISH` / `PUBLISHED` is treated as bonding; `TRADE` as graduated.
- On-chain fallback maps vault phase `Created/Trading/SoldOut` to `bonding`, `MigratePending/Migrated`
  to `graduated`, and `Terminal` or unreadable state to `unknown`.
- `wallet_eligible` is `true`, `false`, or omitted when on-chain verification is unavailable.
- A graduated token is open to any wallet; a bonding token requires the AgentIdentifier gate.
- `wallet_cap` applies only while bonding and only to Agent ID Exclusive tokens; a graduated token
  has no 1% cap, and a creator create-block presale is exempt.
- `wallet_cap.reached` means further buys would revert or be clamped; do not treat a small
  `remaining` as a guarantee that a full buy will fill.
- The scan is bounded by `lookback-blocks`; it is not complete history.
- API detail is optional enrichment. If the API is unavailable or rate-limited, token discovery
  still works from logs, with `phase: unknown` and one warning; detail calls stop after an
  unavailable response.
- A non-Agent template returns `agent_id_exclusive=false`, not a false positive.

## Data sources

- BSC `eth_getLogs` on OpenFour Core `0xebe7b6C1089D9F72aD07f34E36d898e44E5e27f3`
- BSC `AgentIdentifier` `0x09B44A633de9F9EBF6FB9Bdd5b5629d3DD2cef13`
- BSC OpenFour Core `tokens(address)` for creation-time max supply and phase
- ERC20 `balanceOf(wallet)` on the token for the 1% cap
- Four.meme REST API (`FOURMEME_API_URL`, default `https://four.meme/meme-api/v1`)
- BSC RPC (`FOURMEME_BSC_RPC_URL`, default `https://bsc-rpc.publicnode.com`) must support
  `eth_getLogs`; public dataseed endpoints may not.

## Boundaries

- Read-only; no buy/sell calldata or transaction execution.
- Does not log in to Binance Agentic Wallet or enable Developer Mode.
- The 1% cap is a snapshot of balance vs creation max supply; it is not a buy quote and does not
  pre-compute how much a purchase will fill.
- Partial log coverage is explicit in `warnings`; missing ranges are not treated as zero tokens.
- The local page is not a public deployment and has no authentication beyond loopback binding.
- Not financial advice; thin bonding curves are high risk.
