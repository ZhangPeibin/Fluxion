# Quote benchmarks

## Local-state V2/V3 migration checkpoints (#31)

[V3 evidence](2026-11-03-local-state-v3.md) records 30/30 exact QuoterV2 matches across four fee tiers,
independent checks of mixed V2/V3 splits and multi-hop routes, and a restricted four-pool HTTP
sample (20 requests, p50/p95 31.05/31.33 ms). [V2 history](2026-11-02-local-state-v2.md) retains the
first checkpoint. Neither replaces the full-venue 90-row comparison below. The default remains
RPC pricing until Curve/conversion and full-coverage migration acceptance is complete.

## Latest: single / two-pool / multi-pool / ParaSwap (2 Nov 2026)

Run without an HTTP server:

```bash
cargo run --bin split_bench -- --paraswap --max-branches 4 --max-quotes 256 \
  --out docs/benchmarks/multi-pool-run.md
```

- [90-row four-way report](2026-11-02-multi-pool-vs-paraswap.md) /
  [raw JSON with full routes, replay and costs](2026-11-02-multi-pool-vs-paraswap.json)
- [Chinese analysis](2026-11-02-multi-pool-analysis.md) / [HTTP and algorithm](../MULTI-POOL.md)
- [Earlier six-row pilot](2026-11-02-multi-shib-initial.md) / [JSON](2026-11-02-multi-shib-initial.json)
  (different time/source state; not a paired before/after control).

All modes succeeded on 90/90. Within ±50 bps of ParaSwap: single **85/90**, two-pool **87/90**,
multi-pool **88/90**. Multi-pool improved 13 outputs over two-pool and reduced none. All six SHIB
rows improved; the large buy gap fell from −1.8821% / −0.8294% to **−0.1697%**. Two DAI→WETH
rows remain more than 0.5% behind. Multi-pool p50/p95 was **4727/6496 ms** versus ParaSwap
**363/575 ms** (debug/public RPC; gross output excludes gas).

Each row pins local single/two/multi pricing to the available external reported block or head;
range 26325701–26325776, hashes verified at completion. This does not force the raw external API
to use consistent same-block state. Separate SHIB allocation replays supported 5/6 rows; one
`pancakeswapv3` case is explicitly unsupported. The allowlisted diagnostic filters then renormalizes
weights; it does not expand the production graph or represent an external re-optimization.

JSON records source/config/corpus, per-row block/hash, full plans, external routes, unsupported
cases, and separate context/optimizer/replay timing. Candidate p50/p95 includes its baseline and
extra search; it excludes universe preparation, external HTTP and diagnostic replay. Row RPC counts
include setup, both optimizers and replay, so are not the cost of one production multi-pool request.

## Fixed-block two-pool experiment (#30)

This is an internal single-route vs split comparison, distinct from the live aggregator runs below.
Run `cargo run --bin split_bench -- --config config.benchmark.yaml`; no HTTP server is required.
Use `--focus SHIB`, `--block NUMBER` and `--help` for targeted runs and search-budget options.

- [90-row fixed-block report](2026-11-02-split-mainnet.md) / [raw JSON](2026-11-02-split-mainnet.json)
- [Chinese analysis and parameter comparison](2026-11-02-split-analysis.md)
- [Default SHIB calibration](2026-11-02-split-shib-calibration.md) / [JSON](2026-11-02-split-shib-calibration.json)
- [Denser SHIB grid](2026-11-02-split-shib-dense.md) / [JSON](2026-11-02-split-shib-dense.json)

At block 26325207: 90 successful, 35 split wins, 55 baseline selections, no output decrease.
Large SHIB buy/sell improvements: 1.0723% / 0.4812% over the existing single route. Total p50/p95
3451/4697 ms; additional quote-phase RPC 3727. Gas/actual execution are excluded. That historical
phase did not change HTTP/Viewer; the latest multi-pool implementation does. [Two-pool details](../SPLIT-ROUTING.md).

## Live aggregator comparison

`cargo run --bin bench --` compares Fluxion's `POST /quote` against external aggregator quote
endpoints for a fixed corpus of token pairs and amount tiers.

## Run

```bash
# Terminal 1: real discovery requires GRAPH_API_KEY in the gitignored .env.
cargo run -- --config config.benchmark.yaml

# Terminal 2: same public profile, service on :8081.
cargo run --bin bench -- --service http://127.0.0.1:8081 --config config.benchmark.yaml --limit 3
cargo run --bin bench -- --service http://127.0.0.1:8081 --config config.benchmark.yaml
```

Options: `--service URL`, `--corpus PATH` (default `bench/corpus.yaml`), `--limit N`,
`--delay-ms MS` (pause after each external request, default 250), `--config PATH` (local RPC
configuration, default `config.yaml`). An output basename such as `--out run.md` also works.
The harness requires the running service's `/info` endpoint and refuses mock mode.

For a same-run feature-disabled control, start a second instance with
`cargo run -- --config config.benchmark-control.yaml` (:8082), then add
`--control-service http://127.0.0.1:8082` to the benchmark command. Candidate/control order
alternates by row. Reports separate stETH, SHIB and other cohorts, and raw JSON includes both
local paths plus ParaSwap's `bestRoute` (pool addresses and allocation percentages when returned).
Live snapshots and block skew still differ; this is not a pinned-state replay.

ParaSwap's price API is keyless and always runs. A real Fluxion universe is still required;
missing Graph credentials must not silently turn a comparison into mock-vs-mainnet.
1inch and 0x are opt-in via `.env`:

```bash
ONEINCH_API_KEY=...   # https://portal.1inch.dev — quote API v6.0
ZEROX_API_KEY=...     # https://dashboard.0x.org — allowance-holder v2 price API
```

Without those keys their columns are reported as skipped; Fluxion vs ParaSwap still runs.

## Corpus

`bench/corpus.yaml` declares:

- `symbols`: address → label used in the report;
- `decimals`: address → token decimals (required by keyless aggregators such as ParaSwap);
- `amounts`: input-token address → raw-unit amounts (three tiers, roughly $1k / $10k / $100k at
  authoring time);
- `pairs`: directed `[token_in, token_out]` pairs.

Add or edit pairs there; both sides always receive the same raw amount.
The harness validates addresses, positive integer amounts and explicit decimals before sending requests.

## Output

Each run writes a Markdown table and a sibling JSON file with:

- rows, summary stats, start/end timestamps for each row, hop counts and used platforms;
- both-success / Fluxion-only / aggregator-only / both-failed counts;
- timestamp, git SHA, dirty-worktree flag and a content fingerprint of Cargo files + Rust sources;
- service URL, `/info` mode, graph size and effective public config (RPC URL omitted);
- the full input corpus, selected pair count and request pacing;
- mainnet RPC block numbers before/after the run.

The comparison metric is:

```
delta_bps = (fluxion_out - aggregator_out) / aggregator_out * 10000
```

Negative means Fluxion returned less. Output deltas use only successful pairs; latency p50/p95
includes failures and measures request start through complete response-body receipt, without the
pacing sleep. ParaSwap/1inch hop counts describe their first returned route; 0x fill counts are
not directly comparable with sequential hop counts. Platform names are unique venue labels for
external services and ordered per-hop labels for Fluxion.

This is a reproducible **procedure**, not deterministic replay: aggregator APIs do not pin a
block; discovery, cached reserves, and live calls can observe different states. The before/after
block range and timestamps disclose skew, not same-block equivalence. Amount tiers are fixed
raw inputs that were approximately USD-sized when authored. No gas, execution or MEV is modelled.
The competitors can support additional venues, conversions and split routes. 1inch/0x are skipped,
not failed, when their keys are absent. Their live integrations have not been verified without keys.

## Historical paired run: protocol conversions + ShibaSwap (2 Nov 2026)

See [Chinese analysis](2026-11-02-quote-quality-analysis.md), [full tables](2026-11-02-quote-quality.md),
and [raw JSON](2026-11-02-quote-quality.json).

| Metric | Feature-disabled control | Candidate |
|---|---:|---:|
| Success | 90/90 | 90/90 |
| Median output delta vs ParaSwap | −1.479 bps | −0.699 bps |
| Mean output delta vs ParaSwap | −133.722 bps | −10.179 bps |
| Within ±50 bps | 72/90 | 84/90 |
| Latency p50/p95 | 1085/1586 ms | 1246/1871 ms |

- All 15 stETH-related and all 6 SHIB-related rows improve over their paired control outputs.
- stETH: median delta −160.649→0 bps; 4/15→14/15 within ±50 bps. All six stETH↔WETH
  outputs equal ParaSwap. The one stETH outlier outside ±50 bps is positive, not a large loss.
- The 100000 USDC→stETH sample moves from about −40.725% to **−0.0213%** vs ParaSwap.
- SHIB: median delta −151.087→−13.863 bps. The large WETH→SHIB sample moves from
  about −19.010% to **−1.887%**, not complete parity. External raw routes show actual splitting
  between Uniswap V2/V3 and ShibaSwap, including a BONE intermediate not on our allowlist.
- Other rows: 68/69 outputs exactly equal the control. The remaining UNI→USDC row is 1.7 bps
  lower on an identical route with the same first-hop output but a different second-hop RPC result.
- The quality improvement costs additional candidates/RPCs: p50 rises about 15% in this run.
- An [initial integration run](2026-11-02-quote-quality-initial.md) exposed conversion routes being
  pruned before refinement. It is retained with [raw data](2026-11-02-quote-quality-initial.json);
  the final version preserves bounded conversion/ShibaSwap candidate families. See
  [implementation and regression evidence](../QUOTE-QUALITY.md).

## Earlier baseline: 2 Nov 2026

See [full table](2026-11-02-mainnet.md), [raw data](2026-11-02-mainnet.json), and
[Chinese analysis](2026-11-02-analysis.md).

- 30 directed pairs × 3 input sizes = **90 samples**, block **26324182–26324194**.
- Fluxion / ParaSwap success **90/90 each**; 1inch/0x skipped (keys absent).
- Output delta median **−2.2 bps**, mean **−132.7 bps**, **73/90 (81.1%)** within ±50 bps.
- Latency p50/p95: Fluxion **944/1629 ms**, ParaSwap **381/827 ms**.
- Mainnet service: **203 pools, 48 tokens, 418 edges**, built-in both/all-token allowlist;
  public profile in `config.benchmark.yaml`; debug build, not a release-performance claim.
- The large negative tail remains significant: the $100k raw USDC→stETH sample returns about
  **41.14% less**. ParaSwap uses `wsteth` conversion + `uniswapv3` in the stETH samples;
  Fluxion does not model that conversion edge. Large SHIB orders also lag materially.
- The median supports a useful baseline, not an assertion of parity or global optimality.

## First run

`bench-1793355326-26304897.md` (28 Oct 2026, block 26304897–26304909, ParaSwap only; 1inch/0x keys
absent):

- rows 90; success Fluxion 86, ParaSwap 90.
- ParaSwap deltas: median **-4.3 bps**, mean -228.2 bps, 62/86 within ±50 bps.
- latency p50/p95: Fluxion 981/1814 ms, ParaSwap 379/489 ms.
- Failures/huge losses concentrate in large stETH trades (Fluxion `NoPathFound` at $100k) and
  large SHIB orders (>1000 bps behind). That older run has less complete provenance and is
  retained as historical evidence, not a controlled before/after experiment.
