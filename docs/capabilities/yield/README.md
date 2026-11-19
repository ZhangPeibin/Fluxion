# Yield capabilities

ETH yield discovery, local calldata building and historical APY on Ethereum mainnet.
`yield.discover` ranks curated staking and lending markets by a safety tier and a normalized APY;
`yield.build` returns deposit calldata; `yield.history` returns a historical APY series.

`yield.discover` and `yield.history` run on the remote Fluxion service; `yield.build` runs locally.
Agents and users do not choose local or remote. None of them deposits, approves, signs or
broadcasts.

Schemas: [`discover`](../../../schemas/yield_discover.v1.json) · [`build`](../../../schemas/yield_build.v1.json) · [`history`](../../../schemas/yield_history.v1.json).

## Usage

```bash
fluxion yield discover --asset ETH --amount 10 --json
fluxion yield discover --asset ETH --amount 10 --risk balanced --json
fluxion yield discover --asset ETH --amount 10 --min-tvl-usd 1000000000 --json
fluxion yield discover --asset ETH --amount 10 --protocols lido,aave-v3 --json
```

MCP tool: `yield_discover`.

The amount is only used to estimate annual yield. `yield.discover` does not read a wallet balance;
compose `wallet.balance` locally first if the amount should come from the wallet.

## Request

| Field | Required | Description |
|---|---|---|
| `chain` | no | Must be `ethereum`; default `ethereum` |
| `asset` | yes | Must be `ETH` in the first slice |
| `amount` | no | Human amount, e.g. `10`; used for estimated annual yield |
| `risk_profile` | no | `conservative` (default), `balanced`, or `aggressive` |
| `min_tvl_usd` | no | Override the profile's market-TVL floor |
| `max_lockup_days` | no | Override the profile's withdrawal lockup limit |
| `include_reward_apy` | no | Include reward APY in ranking; default `false` |
| `protocols` | no | Optional DefiLlama protocol slugs |
| `limit` | no | 1–20, default 10 |

Profile defaults:

| Profile | Eligible tiers | Min market TVL | Max lockup |
|---|---|---|---|
| `conservative` | `core` | $250M | 0 days |
| `balanced` | `core`, `established` | $100M | 7 days |
| `aggressive` | all tiers | $10M | 30 days |

## Response

The response includes:

- `recommended`: highest ranking APY among eligible opportunities, plus `reason` and `tradeoffs`.
- `opportunities`: ranked opportunities, each labeled with a safety tier, protocol, market, APY
  components, TVL, withdrawal conditions, risk factors, evidence and data confidence.
- `excluded`: curated entries removed by the request or by data quality, with a stable reason.
- `as_of`, `age_secs`, `stale`, `sources`, `coverage`, `disclaimer`, `warnings`.

Each opportunity's `yield` object separates:

- `apy_percent`: current total APY from DefiLlama
- `base_apy_percent`: base staking/supply APY
- `reward_apy_percent`: reward-token APY
- `apy_mean_30d_percent`: 30-day mean
- `ranking_apy_percent`: the value actually used for ranking

Ranking uses the 30-day mean of base APY, falling back to current base APY. Reward APY only
participates when `include_reward_apy=true`.

## Safety tiers

Tiers are curated, not inferred from APY or TVL alone.

| Tier | Rules |
|---|---|
| `core` | >=3 years in production, >=2 audits, no unresolved loss-of-funds incident in the last 2 years, protocol TVL >= $1B, direct ETH exposure, no reward-token dependency, withdrawal <=7 days, reviewed admin/upgrade path |
| `established` | >=1 year, >=2 audits, TVL >= $250M, documented withdrawal path; may include LST/restaking or reward components |
| `experimental` | Newer, smaller, reward-heavy, restaking/LRT, or custodial/off-chain entry |
| excluded | No audits, unverified contract, unresolved incident, or conflicting provider data |

The registry records `reviewed_at`, `reviewer` and risk notes. DefiLlama and on-chain facts are
returned as evidence; the tier itself is not a provider field.

## Data sources

- DefiLlama `/pools` for APY, base/reward split, TVL, 30-day mean and outlier flags.
- DefiLlama `/protocols` for protocol TVL, audits, category and listing age.
- DefiLlama `/hacks` for historical incident counts.
- Ethereum RPC for `eth_getCode` and receipt-token `symbol`/`decimals` verification.
- A versioned curated registry for the whitelist, tier, market addresses, withdrawal conditions
  and risk notes.

Environment overrides:

- `FLUXION_YIELD_POOLS_URL`
- `FLUXION_YIELD_PROTOCOLS_URL`
- `FLUXION_YIELD_HACKS_URL`

## Freshness

The remote service refreshes a background snapshot. Requests never call DefiLlama inline.

- `age_secs <= max_age_secs`: normal.
- `max_age_secs < age_secs <= hard_max_age_secs`: last snapshot is returned with `stale=true`.
- Beyond the hard limit, or before the first snapshot: `STALE_STATE`.

Provider failures keep the last successful snapshot and add a warning. Fluxion never invents an
APY or a tier when data is missing.

## Errors

| Code | HTTP | Meaning |
|---|---|---|
| `INVALID_ARGUMENT` | 400 | Unsupported chain/asset, invalid profile or limit |
| `RATE_LIMITED` | 429 | Too many requests |
| `RPC_ERROR` | 502 | DefiLlama or RPC unavailable and no cached snapshot |
| `STALE_STATE` | 503 | No snapshot or beyond the hard age limit |
| `INTERNAL` | 500 | Internal error |

## Boundaries

- Read-only discovery; no deposit calldata, approvals, signatures or broadcasting.
- Ethereum ETH only in the first slice; no stablecoins, LP, leverage or cross-chain.
- The curated universe is intentionally small; a missing protocol is not a safety judgment.
- `safety_tier` is a curated confidence label, not a guarantee. APY is variable.
- Not financial advice.

## `yield.build`

Local calldata construction for a curated opportunity returned by `yield.discover`.

```bash
fluxion yield build --opportunity lido-steth --amount 10 --recipient 0x... --json
fluxion yield build --opportunity aave-v3-weth --amount 10 --recipient 0x... --funding-asset WETH --json
```

- Returns `steps`: `wrap`, `approve`, `stake`, `supply`, or `deposit`.
- Returns `approvals` for ERC20 approvals; the caller must still sign each step.
- `funding_asset` defaults to `ETH`; pass `WETH` only if the sender already holds WETH.
- `recipient` is the receipt/position owner. For Lido and Rocket Pool the receipt is minted to
  the transaction sender, so the recipient must be the sender.
- Supported in this slice: Lido, Rocket Pool, Aave V3 WETH, SparkLend WETH and the two curated
  Morpho vaults.
- LRT and cbETH entries are discoverable but return `UNSUPPORTED` from `yield.build` until their
  protocol-specific deposit contracts are implemented.
- Never signs, approves or broadcasts. Simulate and inspect every step before signing.

Schema: [`schemas/yield_build.v1.json`](../../../schemas/yield_build.v1.json).

## `yield.history`

Remote historical APY series for a curated opportunity.

```bash
fluxion yield history --opportunity lido-steth --days 30 --json
fluxion yield history --opportunity aave-v3-weth --days 90 --max-points 200 --json
```

- `days`: 1–365, default 30.
- `max_points`: 1–2000, default 500. The series is downsampled, always keeping the latest point.
- Returns `series` with `apy_percent`, `base_apy_percent`, `reward_apy_percent` and `tvl_usd`.
- Returns a `summary` with mean/min/max/latest, change in bps and base/reward means.
- Historical APY is not current APY and is not a guarantee.
- Data source: DefiLlama chart endpoint for the registry's pool id.

Schema: [`schemas/yield_history.v1.json`](../../../schemas/yield_history.v1.json).
