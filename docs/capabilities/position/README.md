# position.analyze

Read-only analysis of a wallet's curated ETH yield positions on Ethereum mainnet. It reads receipt
token balances at a pinned block, estimates ETH value, attaches the current `yield.discover` APY and
risk context, and optionally computes PnL from a supplied cost basis.

`position.analyze` runs on the remote Eollo service; agents and users do not choose local or
remote. It never signs, approves or broadcasts.

Returns capability envelope v2: `meta`, `chain`, `block`, `as_of`, `provenance`, `coverage`,
`warnings`, `unknowns`, and `data`.

Schema: [`schemas/position_analyze.v2.json`](../../../schemas/position_analyze.v2.json).

## Usage

```bash
eollo position analyze --owner 0x... --json
eollo position analyze --owner 0x... --opportunities lido-steth,aave-v3-weth --json
eollo position analyze --owner 0x... --include-zero --json
eollo position analyze --owner 0x... --cost-basis lido-steth=10,aave-v3-weth=5 --json
```

MCP tool: `position_analyze`.

## Request

| Field | Required | Description |
|---|---|---|
| `owner` | yes | Wallet address to analyze |
| `chain` | no | Must be `ethereum`; default `ethereum` |
| `opportunity_ids` | no | Subset of curated opportunity IDs; default all |
| `include_zero` | no | Include positions with zero receipt balance; default `false` |
| `cost_basis` | no | Per-opportunity ETH cost basis for PnL |

Cost basis example:

```json
{
  "owner": "0x...",
  "cost_basis": [
    {"opportunity_id": "lido-steth", "amount": "10"},
    {"opportunity_id": "aave-v3-weth", "amount": "5"}
  ]
}
```

## Response

- `block` / `as_of`: the pinned Ethereum block and observation time on the envelope.
- `data.yield_data_as_of`: the `yield.discover` snapshot used for APY and risk context.
- `positions`: receipt balance, underlying asset, estimated ETH value, optional cost basis/PnL,
  APY, estimated annual yield, withdrawal conditions, risk factors, data confidence and status.
- `totals`: position count, active count, value, estimated annual yield, and cost basis/PnL when
  every active position has a cost basis.
- `data.disclaimer`; coverage, sources and warnings are on the envelope.

## Value conversion

| Market | Conversion |
|---|---|
| Lido stETH | 1:1 to ETH |
| Aave aEthWETH / SparkLend spWETH | 1:1 to WETH/ETH |
| Rocket Pool rETH | `getExchangeRate()` |
| cbETH | `exchangeRate()` |
| ether.fi weETH | `getRate()` |
| Morpho vaults | ERC4626 `convertToAssets(shares)` |
| Kelp rsETH / Renzo ezETH | not implemented; receipt balance is returned without an ETH value |

A missing value is explicit (`value_eth` omitted + warning), never silently zero.

## PnL and principal

On-chain balances do not contain a cost basis. `position.analyze` therefore reports current value
and estimated annual yield by default. Supply `cost_basis` to get `cost_basis_eth`, signed
`pnl_eth` and `pnl_percent`. Without it, principal and realized/unrealized PnL are unknown.

## Errors

| Code | HTTP | Meaning |
|---|---|---|
| `INVALID_ARGUMENT` | 400 | Unsupported chain, unknown opportunity id, invalid cost basis |
| `RATE_LIMITED` | 429 | Too many requests |
| `RPC_ERROR` (`PositionUnavailable`) | 502 | Ethereum RPC or yield snapshot unavailable |
| `STALE_STATE` | 503 | Yield snapshot is not ready |
| `INTERNAL` | 500 | Internal error |

## Boundaries

- Read-only; no approvals, signatures, deposits or withdrawals.
- Ethereum mainnet only in this slice.
- Values are estimates at one pinned block; they are not a portfolio valuation for tax or
  accounting purposes.
- APY comes from the `yield.discover` snapshot and may be older than the position block.
- A position in an unsupported or missing market is reported with a warning, not fabricated.
- Not financial advice.
