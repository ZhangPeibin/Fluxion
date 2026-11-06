# Split search budget tuning

Date: 6 Nov 2026
Profile: `configs/runtime.yaml` (SushiSwap V3 + PancakeSwap V3, `min_liquidity: 10000`, ~309 pools)

## Method

- Same 90-row `bench/corpus.yaml` corpus.
- Same split settings: `allocation_segments: 20`, `refinement_rounds: 2`,
  `refinement_segments: 4`, `max_branches: 8`.
- Only `split.max_quote_calls` changes: `2048`, `4096`, `8192`.
- Runs are sequential, so each observes a slightly different block range. Aggregator
  deltas are therefore indicative; budget stops, latency and calls are the primary
  decision metrics.
- `local_snapshot` mode: quote calls are local CPU work, not RPC.

| budget | run blocks | success | p50 / p95 latency | quote_budget / completed / no_pairs | calls median / max | ParaSwap ±50 bps | 1inch ±50 bps |
|---:|---|---:|---:|---|---:|---:|---:|
| 2048 | 26356587–26356600 | 90/90 | 28.0 / 86.6 ms | 68 / 18 / 4 | 2048 / 2104 | 76/90 | 75/90 |
| **4096** | 26356603–26356615 | 90/90 | **34.0 / 129.4 ms** | **36 / 45 / 9** | **3366 / 4125** | **78/90** | **75/90** |
| 8192 | 26356618–26356631 | 90/90 | 44.5 / 177.6 ms | 5 / 70 / 15 | 3241 / 8237 | 78/90 | 75/90 |

Baseline gain is measured as `(selected - baseline) / baseline` in bps:

| budget | median | mean | max |
|---:|---:|---:|---:|
| 2048 | 0.03 | 178.62 | 15206.75 |
| 4096 | 0.29 | 109.54 | 7434.55 |
| 8192 | 0.33 | 110.63 | 7449.38 |

The means are skewed by coverage-limited rows; the medians are the stable comparison.

## Decision

Keep `max_quote_calls: 4096`.

- `2048` is 43 ms faster at p95 but leaves 68/90 rows at the quote budget and has the
  weakest aggregator agreement (76/90 and 75/90).
- `8192` reduces budget stops to 5/90 but adds ~48 ms at p95 and does not materially
  improve median output or aggregator agreement over `4096`.
- `4096` is the balance point: acceptable p95, materially fewer budget stops than 2048,
  and nearly all of the quality of 8192.

Raw full-row JSON is intentionally not retained for these three runs; this report and
`split-budget-2026-11-06.json` contain the decision data. The 4096 runtime profile's
full 90-row report remains at
[`split-quality-2026-11-06-runtime.md`](split-quality-2026-11-06-runtime.md).
