# yield.discover — pre-PRD discussion

Status: ready-for-prd
Release: v2
Capability: `yield.discover` (remote, read-only)

## Problem

A user with ETH wants to answer a practical question: "where do I earn yield without taking
unreasonable risk?" Existing data sources expose APY and TVL, but they do not normalize risk,
withdrawal conditions, reward dependency, or provenance. A raw APY ranking is not an answer and
can promote unsafe opportunities.

## Decisions

| # | Decision | Rationale |
|---|---|---|
| 1 | First slice covers **ETH-denominated staking and lending on Ethereum mainnet** | Matches the "10 ETH" intent and keeps exposure comparable; excludes stablecoin/LP/bridge risk |
| 2 | Safety is a **curated whitelist + safety tier + evidence**, not an automatic composite score | Automated scores imply false precision; a reviewed universe is defensible and auditable |
| 3 | Output is a **tiered list + one recommendation + explicit tradeoffs** | Users need a direct answer without hiding the safety/yield tradeoff |
| 4 | Data sources are **DefiLlama + on-chain verification + a versioned static risk registry** | Provider data gives live APY/TVL; RPC verifies contracts; the registry owns safety and withdrawal context |
| 5 | First slice is **Ethereum only**, read-only discovery; no deposit calldata | Discovery is the risky, reusable intelligence layer; execution needs protocol adapters and a separate security review |
| 6 | Capability runs on the **remote** service and is cached, not fetched per request | Aggregation and scoring are shared across users; caching avoids provider rate limits and makes freshness explicit |

## Safety tiers

Tiers are assigned in the registry and reviewed manually. DefiLlama/RPC facts are attached as
evidence; the tier itself is not an on-chain or provider field.

| Tier | Rules |
|---|---|
| `core` | >=3 years in production, >=2 audits, no unresolved loss-of-funds incident in the last 2 years, protocol TVL >= $1B, direct ETH exposure, no reward-token dependency, withdrawal <=7 days, reviewed admin/upgrade path |
| `established` | >=1 year, >=2 audits, TVL >= $250M, documented withdrawal path; may include LST/restaking or reward components |
| `experimental` | Newer, smaller, reward-heavy, restaking/LRT, or custodial/off-chain entry |
| excluded | No audits, unverified contract, unresolved incident, or conflicting provider data |

## Ranking and recommendation

- Rank on `apyMean30d` of **base APY**; fall back to current base APY. Reward APY is reported
  separately and only participates when `include_reward_apy=true`.
- `conservative` considers `core` only.
- `balanced` considers `core + established`.
- `aggressive` considers all three tiers.
- `recommended` is the highest ranked opportunity among the eligible tiers, with explicit
  tradeoffs against the next-safest alternative.
- `outlier=true`, low-confidence, stale, or conflicting entries are excluded with a reason.

## Contract sketch

Request:

```json
{
  "chain": "ethereum",
  "asset": "ETH",
  "amount": "10",
  "risk_profile": "conservative",
  "min_tvl_usd": 1000000000,
  "max_lockup_days": 0,
  "include_reward_apy": false,
  "protocols": [],
  "limit": 10
}
```

Response includes `recommended`, `opportunities`, `excluded`, `as_of`, `age_secs`, `stale`,
`sources`, `coverage`, `disclaimer`, and `warnings`. Each opportunity carries the safety tier,
protocol/market metadata, APY components, TVL, withdrawal conditions, risk factors, evidence, and
an optional estimated annual yield for the requested amount.

## Data and freshness

- DefiLlama `/pools`, `/protocols`, `/hacks` fetched by a background refresher.
- Ethereum RPC verifies contract code and receipt-token `symbol`/`decimals`.
- Registry entries pin a DefiLlama pool id; fallback matching uses
  `(project, symbol, poolMeta)`. Unmatched entries are excluded with a warning.
- Snapshots are immutable in the request path. `max_age_secs` marks stale data; a hard limit
  returns `STALE_STATE` rather than inventing values.

## Non-goals

- No transaction construction, signing, or broadcasting.
- No stablecoin, LP, leverage, or cross-chain opportunities in the first slice.
- No claim that any protocol is safe or any APY is guaranteed.
- No wallet balance lookup; the caller passes the amount. An agent can compose
  `wallet.balance` locally first if it wants to use a real balance.

## Open questions for the PRD

- Whether to add protocol-native APY providers in the second slice.
- Whether to add `yield.history` and `yield.build` after the discovery contract stabilizes.

## Expansion pass (2026-11-19)

- Added cbETH, ether.fi weETH, Kelp rsETH, Renzo ezETH, Steakhouse ETH and Gauntlet WETH Prime.
- cbETH is `experimental`: primary mint/redemption is custodial and DefiLlama lists no protocol audits.
- weETH and rsETH are `established` because they meet the age/audit/TVL rules, with restaking and
  withdrawal-queue risk factors attached.
- Renzo and the two curated Morpho vaults are `experimental` because of withdrawal, curator or
  small-vault liquidity risk.
- The `aggressive` market-TVL floor was lowered to $10M so the curated Morpho vaults can appear;
  `conservative` and `balanced` floors are unchanged.

## Follow-up: build and history (2026-11-19)

- `yield.build` is local and returns calldata only. It supports Lido, Rocket Pool, Aave V3,
  SparkLend and the two curated Morpho vaults. LRT/cbETH entries remain discoverable but return
  `UNSUPPORTED` until protocol-specific deposit contracts are implemented.
- Aave/Spark/Morpho builds return `wrap -> approve -> supply/deposit` steps when funding with ETH;
  `funding_asset=WETH` skips the wrap step.
- `yield.history` is remote, reads the DefiLlama chart endpoint by registry pool id, caches the
  series in-process, and returns a downsampled series plus summary. Historical APY is explicitly
  not current APY or a guarantee.
- The curated registry moved into `fluxion-protocol` so local build and remote intelligence share
  one source of truth.
