# Quote gap attribution — worst 10 rows

Date: 6 Nov 2026
Source run: [`split-quality-2026-11-06-runtime.json`](split-quality-2026-11-06-runtime.json)
Profile: 309-pool runtime profile (Uniswap V2/V3, Sushi V3, Pancake V3, Curve, ShibaSwap, wstETH)

## Method

For each of the 90 rows, compare Fluxion `amount_out` with the better of ParaSwap and 1inch.
Take the 10 worst rows, then inspect:

- Fluxion baseline / selected route and `stop_reason`
- ParaSwap pool addresses and whether each pool exists in the current graph
- 1inch protocol names
- Token path / missing conversion steps

Classes:

- **A — missing venue/liquidity/conversion**: the winning route needs a protocol or conversion the
  current graph does not have.
- **B — path exists but candidate missing**: could not be proven from benchmark output alone; no row
  is assigned B without candidate-level tracing.
- **C — present venues but candidate/allocation/budget insufficient**: winning ParaSwap route is
  mostly or entirely in the current graph, but Fluxion did not find or fully allocate it.
- **D — benchmark/state anomaly**: none found.

## Results

| # | pair | amount | delta bps | best | Fluxion stop | ParaSwap pools present | primary class |
|---:|---|---:|---:|---|---:|---:|---|
| 1 | USDC→stETH | 100,000 | -2307.6 | ParaSwap | no_pairs | 2/3 | **A** — missing WETH→ETH→stETH (Lido) conversion |
| 2 | AAVE→USDC | 400 | -1872.9 | 1inch | no_pairs | 7/9 | **A** — Balancer / Curve factory / PMM / V4 missing; some present pools underused |
| 3 | LINK→USDC | 6,666 | -497.0 | 1inch | quote_budget | 6/7 | **C** — Uniswap/Curve pools present; budget stopped search |
| 4 | LDO→WETH | 160,000 | -458.0 | 1inch | quote_budget | 6/7 | **C** — Uniswap V3 pools present; Sushi V2/PMM missing |
| 5 | AAVE→USDC | 40 | -456.6 | ParaSwap | no_pairs | 2/3 | **A** — Balancer / PMM / Ekubo missing; Uniswap present |
| 6 | WETH→SHIB | 33.33 | -342.6 | 1inch | completed | 4/6 | **C** — Uniswap/Shiba present; two Shiba pools + V4/PMM missing |
| 7 | stETH→LDO | 33.33 | -318.6 | 1inch | quote_budget | 6/10 | **A** — Curve factory / Sushi V2 / Balancer / PMM / Origin |
| 8 | stETH→USDC | 33.33 | -226.1 | 1inch | no_pairs | 8/9 | **A** — Curve factory / PMM / Origin missing; present Uniswap/Sushi path underused |
| 9 | SHIB→WETH | 8e9 | -202.0 | 1inch | completed | 4/6 | **C** — present Uniswap/Shiba pools; two Shiba pools + V4/PMM missing |
| 10 | AAVE→USDC | 4 | -107.6 | ParaSwap | no_pairs | 2/2 | **C** — both ParaSwap pools present; direct candidate not improved |

The `pools present` column counts ParaSwap pool addresses found in the current graph. It is a
lower bound on coverage because 1inch routes often use venues without public pool addresses.

## Takeaways

- **5/10 primarily horizontal**: missing venues/conversions dominate the largest losses
  (USDC→stETH, AAVE→USDC, stETH→LDO, stETH→USDC).
- **5/10 primarily vertical**: the winning ParaSwap route uses pools already in our graph, but
  Fluxion either stopped at the quote budget or completed without finding the better allocation.
- The single largest gap, USDC→stETH, is not an allocator problem: it needs a WETH→ETH→stETH
  conversion path.
- No `D` anomalies were found.
- B vs C cannot be separated without candidate-level instrumentation. The benchmark `route_tree`
  and stop reasons are not enough to prove a path was never generated.

## Suggested next steps

1. If staying within current venues: add candidate/trace instrumentation for the C rows
   (LINK→USDC, LDO→WETH, WETH→SHIB, SHIB→WETH, small AAVE→USDC) and tighten the allocator for
   `quote_budget` rows.
2. If lifting the platform freeze: prioritize the missing pieces with the highest measured impact:
   WETH/ETH/stETH Lido conversion, Balancer, Curve factory pools, Sushi V2, then PMM/V4/RFQ.
3. Do not raise `max_quote_calls` globally: the 4096→8192 experiment did not materially improve
   output, and several worst rows are `completed` rather than budget-limited.
