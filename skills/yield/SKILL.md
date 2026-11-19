---
name: yield
description: Use when the user asks where to earn yield on ETH safely, wants the safest or highest-APY ETH staking or lending opportunity, wants to compare Lido/Rocket Pool/Aave/SparkLend yields, or uses the Fluxion yield_discover tool. Read-only; never deposits, signs or broadcasts.
---

# yield.discover

Use this capability when an agent needs to compare **ETH-denominated staking and lending**
opportunities on Ethereum. It returns a safety tier, APY components, TVL, withdrawal conditions,
evidence and one recommendation with tradeoffs.

Human-facing capability documentation is in
[docs/capabilities/yield/README.md](../../docs/capabilities/yield/README.md); the JSON contract is
[schemas/yield_discover.v1.json](../../schemas/yield_discover.v1.json).

## MCP

```json
{"name": "yield_discover", "arguments": {"asset": "ETH", "amount": "10", "risk_profile": "conservative"}}
```

## CLI

```bash
fluxion yield discover --asset ETH --amount 10 --json
fluxion yield discover --asset ETH --amount 10 --risk balanced --json
```

## Reading the result

- `recommended`: highest `ranking_apy_percent` among opportunities eligible for the profile.
  Always report its `tradeoffs`; do not present it as "safe".
- `opportunities[].safety_tier`: `core`, `established`, or `experimental`. A non-core tier is a
  real risk statement, not a formatting detail. `experimental` may include restaking/LRT,
  custodial/off-chain entry, or a curated vault with small liquidity.
- `opportunities[].yield`: `apy_percent`, `base_apy_percent`, `reward_apy_percent`,
  `apy_mean_30d_percent`, and `ranking_apy_percent`. Ranking uses the 30-day mean of base APY;
  reward APY is excluded unless `include_reward_apy` is true.
- `opportunities[].market`: `deposit_asset`, `receipt_asset`, `requires_wrap`, and withdrawal
  conditions. Aave/SparkLend require WETH, so ETH must be wrapped.
- `opportunities[].risk_factors` and `evidence`: cite these when explaining a choice.
- `excluded`: curated entries filtered by the profile or data quality.
- `stale`, `age_secs`, `sources`, `warnings`: data freshness and provenance.
- `disclaimer`: always include the no-guarantee caveat when giving financial information.

## Choosing a profile

- Default to `conservative` for "最安全" or capital-preservation language.
- Use `balanced` when the user explicitly wants more yield and accepts an `established` market.
- Use `aggressive` only when the user explicitly accepts experimental/restaking risk.
- Never silently raise the risk profile to make an APY look better.

## Boundaries

- Read-only; this skill has no deposit or execution path.
- Do not claim an opportunity is safe, guaranteed, or risk-free.
- Do not invent APY, TVL, audits, withdrawal times, or tiers when fields are missing.
- Do not use this capability for stablecoin yields, LP, leverage, or cross-chain opportunities;
  those are outside the first slice.
- Not financial advice.
