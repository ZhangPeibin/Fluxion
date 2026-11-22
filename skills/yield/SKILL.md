---
name: yield
description: Use when the user asks where to earn yield on ETH safely, wants the safest or highest-APY ETH staking or lending opportunity, wants to compare Lido/Rocket Pool/Aave/SparkLend yields, or uses the Fluxion yield_discover, yield_build, yield_history or yield_withdraw tools. Never deposits, signs or broadcasts.
---

# yield.discover

Use this capability when an agent needs to compare **ETH-denominated staking and lending**
opportunities on Ethereum. It returns a safety tier, APY components, TVL, withdrawal conditions,
evidence and one recommendation with tradeoffs.

Human-facing capability documentation is in
[docs/capabilities/yield/README.md](../../docs/capabilities/yield/README.md); the JSON contract is
[schemas/yield_discover.v2.json](../../schemas/yield_discover.v2.json).

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

`yield.discover` uses envelope v2: `meta`, `chain`, `as_of`, `provenance`, `coverage`,
`warnings`, `unknowns`, and `data`.

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
- `stale`, `age_secs`: in `data`; freshness, sources and warnings are on the envelope.
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

## yield.build

Build local calldata for a chosen opportunity:

```json
{"name": "yield_build", "arguments": {"opportunity_id": "lido-steth", "amount": "10", "recipient": "0x..."}}
```

- Returns `steps` (`wrap`, `approve`, `stake`, `supply`, `deposit`) and `approvals`.
- `funding_asset` defaults to `ETH`; pass `WETH` only when the sender already holds WETH.
- The recipient must be the transaction sender for Lido and Rocket Pool.
- Supported: Lido, Rocket Pool, Aave V3, SparkLend and curated Morpho vaults.
- LRT/cbETH markets return `UNSUPPORTED` from build until their deposit contracts are implemented.
- Never sign or broadcast from this skill. Simulate and inspect each step, then let the user approve.

## yield.history

```json
{"name": "yield_history", "arguments": {"opportunity_id": "lido-steth", "days": 30}}
```

- `series` is downsampled; `summary` contains mean/min/max/latest, change bps and base/reward means.
- Historical APY is not current APY and does not predict future yield.
- Use it to explain stability, not to claim safety or guaranteed returns.

## yield.withdraw

Build local exit calldata:

```json
{"name": "yield_withdraw", "arguments": {"opportunity_id": "aave-v3-weth", "amount": "10", "recipient": "0x..."}}
```

- One `steps` entry: `withdraw` for Aave/Spark, `redeem` for Morpho, `burn` for Rocket Pool.
- `amount=max` is supported for Aave/Spark only; Morpho needs an exact vault-share amount and Rocket Pool an exact rETH amount.
- `owner` defaults to `recipient`; Rocket Pool burn uses the transaction sender and may require a delay.
- Lido stETH and LRT/cbETH exits return `UNSUPPORTED` for now.
- Never sign or broadcast from this skill. Simulate and inspect the step, then let the user approve.

## Workflow recipe: yield-deposit

Use this when the user wants to discover, deposit into, and confirm a curated ETH yield market. The
workflow is `yield.discover → yield.build → wallet.allowance → transaction.inspect →
transaction.simulate → transaction.send → transaction.verify → position.analyze`.

Required inputs: asset (`ETH`), amount, opportunity id (or a profile for discovery), recipient, and
an optional policy. The chain is Ethereum.

Checkpoints:

1. `yield.discover`: stop on `STALE_STATE`; keep the user's risk profile.
2. `yield.build`: stop on `UNSUPPORTED` or `INVALID_ARGUMENT`; show every step and approval.
3. `wallet.allowance`: approval sequencing is not a shared helper yet; request each approval
   explicitly and do not hide it.
4. `transaction.inspect`: stop on a policy block; surface high findings.
5. `transaction.simulate`: stop on revert before the wallet request.
6. `transaction.send`: requires a connected WalletConnect session; the user confirms on the phone.
7. `transaction.verify`: require `status: success`.
8. `position.analyze`: read the position at a block at or after the deposit. The post-execution
   block is not pinned automatically yet; state it explicitly.

Failure handling: stale data, unsupported market, approval failure, simulation failure, policy
block, or user rejection stops the workflow. Never deposit from a failed step.
