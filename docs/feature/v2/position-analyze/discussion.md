# position.analyze — pre-PRD discussion

Status: ready-for-prd
Release: v2
Capability: `position.analyze` (remote, read-only)

## Problem

`yield.discover` answers where to earn yield and `yield.build` answers how to enter. Users then
need to see what they actually hold, what it is worth, what APY context applies, and how they can
exit. That data is split across receipt-token contracts and the yield registry.

## Decisions

| # | Decision | Rationale |
|---|---|---|
| 1 | Analyze the same curated universe as `yield.discover` | One registry and one safety/risk vocabulary across discover, build and hold |
| 2 | Pin every balance to one Ethereum block | Reproducible value totals and block provenance |
| 3 | Run remotely | Needs Ethereum RPC plus the warm yield snapshot; address data is public |
| 4 | Report receipt balance and estimated ETH value separately | The receipt token and its underlying are different assets; a missing rate is explicit |
| 5 | Support 1:1, protocol exchange-rate and ERC4626 conversions; leave unsupported LRTs without a value | Do not fabricate an ETH value for rsETH/ezETH before their rate logic exists |
| 6 | Cost basis is optional input, not inferred | On-chain balances do not contain principal; PnL must not be invented |
| 7 | Include withdrawal conditions and risk factors | A position is not just a balance; exit constraints are part of the holding state |

## Contract sketch

Request: `owner`, optional `chain`, `opportunity_ids`, `include_zero`, and `cost_basis`.

Response: pinned `block_number`/`block_hash`, `yield_data_as_of`, per-position receipt balance,
`value_eth`, APY, estimated annual yield, withdrawal, risk factors, confidence and status; plus
totals, sources, disclaimer and warnings. PnL fields appear only when cost basis is supplied.

## Non-goals

- No approvals, deposits, withdrawals or signing.
- No tax/accounting valuation.
- No positions outside the curated registry.
- No LRT exchange-rate conversion for rsETH/ezETH in the first slice.
- No stablecoin, LP or cross-chain positions.

## Follow-ups

- Add rsETH/ezETH rate conversion.
- Add position history and realized PnL when a cost-basis ledger exists.
- Reuse the same position model for `lending.*` and `liquidity.*`.
