---
name: position
description: Use when the user asks what yield positions a wallet holds, wants ETH value, APY, estimated annual yield, withdrawal conditions or PnL for Lido/Rocket Pool/Aave/SparkLend/Morpho positions, or uses the Fluxion position_analyze tool. Read-only; never deposits, signs or broadcasts.
---

# position.analyze

Use this capability to inspect a wallet's curated ETH yield positions at a pinned Ethereum block.
It combines on-chain receipt-token balances with the current `yield.discover` APY and risk context.

Human-facing documentation is in
[docs/capabilities/position/README.md](../../docs/capabilities/position/README.md); the JSON contract
is [schemas/position_analyze.v1.json](../../schemas/position_analyze.v1.json).

## MCP

```json
{"name": "position_analyze", "arguments": {"owner": "0x..."}}
{"name": "position_analyze", "arguments": {"owner": "0x...", "opportunity_ids": ["lido-steth"]}}
{"name": "position_analyze", "arguments": {"owner": "0x...", "cost_basis": [{"opportunity_id": "lido-steth", "amount": "10"}]}}
```

## CLI

```bash
fluxion position analyze --owner 0x... --json
fluxion position analyze --owner 0x... --cost-basis lido-steth=10 --json
```

## Reading the result

- `block_number` / `block_hash`: all balances are pinned to this block.
- `yield_data_as_of`: APY/risk snapshot time; check `warnings` if it is old.
- `positions[]`: receipt balance, estimated `value_eth`, APY, estimated annual yield, withdrawal
  conditions and risk factors.
- `value_eth` may be omitted for rsETH/ezETH because exchange-rate conversion is not implemented.
  Do not treat a missing value as zero.
- `cost_basis_eth`, `pnl_eth` and `pnl_percent` are present only when the caller supplies cost
  basis. Never invent a principal or PnL.
- `totals` sums known values; if some values are missing, report the gap.

## Boundaries

- Read-only; this skill has no approval, deposit, withdraw or execution path.
- Do not claim a position is safe or that APY is guaranteed.
- Do not infer cost basis from current value.
- Not financial advice.
