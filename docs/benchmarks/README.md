# Quote benchmarks

## Latest: disjoint multi-route split for large inputs (4 Nov 2026)

[90-row report](quote-freshness-2026-11-04-route-split.md) /
[raw JSON](quote-freshness-2026-11-04-route-split.json).

The allocator now also evaluates disjoint multi-hop routes as independent branches, not only
direct pools. Routes are exact-quoted at sampled fractions, greedily filtered to share no pool,
and allocated with the same sampled DP. This is a gross-output optimization; shared-pool
allocations remain unsupported because their independent quotes would double-count reserves.

| timing | previous incremental | multi-route split |
|---|---:|---:|
| snapshot lag blocks p50 / p95 / max | 0 / 1 / 1 | **0 / 1 / 1** |
| snapshot lag seconds p50 / p95 / max | 0 / 12 / 12 | **0 / 12 / 12** |
| Fluxion HTTP latency p50 / p95 | 40 / 101 ms | **49 / 133 ms** |
| success | 90 / 90 | **90 / 90** |
| ParaSwap mean delta / within ±50 bps | -10.5 bps / 86 | **-2.6 bps / 89** |
| 1inch mean delta / within ±50 bps | -17.3 bps / 80 | **-8.7 bps / 82** |

The remaining large outliers are coverage-limited rather than algorithm-limited: LDO and SHIB
large orders still lack enough disjoint venues in the discovered 209-pool universe to match the
aggregators' deeper inventory. Freshness is unchanged at 0/1/1 blocks.

## Previous: incremental local snapshot (#38–#40, 4 Nov 2026)

[90-row report](quote-freshness-2026-11-04-incremental.md) /
[raw JSON](quote-freshness-2026-11-04-incremental.json).

With `local_state.sync_mode: incremental` on the same 30-pair × three-amount corpus and the same
209-pool full-range V3 universe, all 90 quotes and all 90 head observations succeeded. Snapshot
lag fell from the earlier 5/7-block (60/84 s) result to **0 blocks at p50 and 1 block at p95 and
maximum (0/12/12 s)**. Fluxion HTTP latency was **40/101 ms p50/p95**; ParaSwap and 1inch stayed
at 312/521 ms and 455/741 ms. Output deltas were within ±50 bps for 86/90 (ParaSwap) and 80/90
(1inch) rows, and Fluxion computed with zero request-time RPC.

| timing | full rebuild (previous) | incremental (this run) |
|---|---:|---:|
| snapshot lag blocks p50 / p95 / max | 5 / 7 / 7 | **0 / 1 / 1** |
| snapshot lag seconds p50 / p95 / max | 60 / 84 / 84 s | **0 / 12 / 12 s** |
| Fluxion HTTP latency p50 / p95 | 24 / 107 ms | **40 / 101 ms** |
| success | 90 / 90 | **90 / 90** |
| ParaSwap within ±50 bps | 86 / 90 | 86 / 90 |
| 1inch within ±50 bps | 82 / 90 | 80 / 90 |

The service cold-starts with one full load, then publishes a snapshot per block: V2 reserves, V3
`slot0`/`liquidity`, Curve and wstETH state are re-read; V3 bitmap/tick structure changes only on
`Mint`/`Burn` logs. Reorgs, large gaps and a periodic reconciliation interval fall back to the
full loader. The benchmark universe was pinned with `universe_refresh_secs: 0`.

```bash
cargo run -p fluxion-remote -- serve --config bench/configs/local.yaml
cargo run -p fluxion-remote --bin bench -- --service http://127.0.0.1:8089 --config bench/configs/local.yaml \
  --split --delay-ms 600 --freshness-rpc-url https://ethereum-rpc.publicnode.com \
  --out docs/benchmarks/quote-freshness-2026-11-04-incremental.md
```


## Latest per-quote freshness run (#37, 4 Nov 2026)

[90-row report](quote-freshness-2026-11-04-publicnode.md) / [raw JSON](quote-freshness-2026-11-04-publicnode.json).

Using the existing 30-pair × three-amount corpus against the 209-pool Ethereum local-snapshot
service, all 90 quotes succeeded and all 90 head observations were available. Snapshot lag was
5 blocks / 60 seconds at p50 and 7 blocks / 84 seconds at p95 and maximum. Fluxion quote HTTP
latency was 24/107 ms p50/p95. The service used its configured dRPC endpoint; an independent
PublicNode endpoint observed chain heads to avoid consuming the same free RPC quota twice.
This measures snapshot block age, not a same-path quote recomputed against latest pool state.
An initial same-dRPC observer attempt hit seven free-tier timeouts; the successful run uses an
independent head-observation endpoint. The initial outcome is also noted in GitHub issue #37.

## Latest: writer-isolation fix (#36, 3 Nov 2026)

[90-row three-way report](2026-11-03-writer-isolation-threeway.md) /
[raw JSON with request IDs and state](2026-11-03-writer-isolation-threeway.json) /
[acceptance, health probes and analysis](2026-11-03-writer-isolation-fix.md).

All three succeed on 90/90 under continuous background updates. Fluxion computes with zero RPC
on every row: HTTP p50/p95 **27/124 ms**, maximum 238 ms. ParaSwap is 438/883 ms and 1inch
530/1059 ms. Output is within ±0.5% of ParaSwap on 89/90 and of 1inch on 84/90.
Concurrent health probes succeed 3832/3832, p95 3.143 ms, maximum 79.653 ms; versions advance 1→4.

This explicitly excludes only legacy Tricrypto and retains all other 209 records with full-range V3.
Actual state age remains 59–129 seconds under the unchanged 180-second / 15-block experiment policy;
default-freshness acceptance is separate from the fixed HTTP scheduling defect.

```bash
cargo run -p fluxion-remote -- serve --config bench/configs/local.yaml
# After /ready, with ONEINCH_API_KEY in .env:
cargo run -p fluxion-remote --bin bench -- --service http://127.0.0.1:8089 --config bench/configs/local.yaml \
  --split --delay-ms 600 --out docs/benchmarks/writer-isolation-run.md
```

## Pre-fix local snapshot reference (3 Nov 2026)

[90-row report](2026-11-03-local-threeway.md) / [raw JSON](2026-11-03-local-threeway.json) /
[analysis and reproduction](2026-11-03-local-threeway-analysis.md).

All three succeed on 90/90. Fluxion HTTP p50/p95 is **31/3501 ms**, with **zero computation RPC
on every row**. Output is within ±0.5% of ParaSwap on 88/90 and of 1inch on 84/90. This explicitly
excludes only legacy Tricrypto, retaining the other 209 records with full-range V3 coverage.
Actual state age is **50–112 seconds**, under an experiment policy of 180 seconds / 15 blocks;
default-freshness continuous operation and tail latency remain unresolved.

```bash
cargo run -p fluxion-remote -- serve --config bench/configs/local.yaml
cargo run -p fluxion-remote --bin bench -- --service http://127.0.0.1:8089 --config bench/configs/local.yaml \
  --split --delay-ms 600 --out docs/benchmarks/local-threeway-run.md
```

## RPC reference: Fluxion / ParaSwap / 1inch (3 Nov 2026)

- [90-row three-way report](2026-11-03-threeway-aggregators.md)
- [Raw JSON, full responses/routes and per-row timestamps](2026-11-03-threeway-aggregators.json)
- [Results and comparison scope](2026-11-03-threeway-analysis.md)

The full discovered graph has 210 records, 48 tokens and 432 directed edges. This run uses
`pricing_mode: rpc` with `split=true`, up to four independent direct-pool branches, the original
30 directions × three raw-input tiers, and both external APIs enabled. It is not a full-universe
local-snapshot migration benchmark; legacy Tricrypto is still unsupported in local mode.

```bash
cargo run -p fluxion-remote -- serve --config bench/configs/threeway.yaml
# In another terminal, with ONEINCH_API_KEY in the gitignored .env:
cargo run -p fluxion-remote --bin bench -- --service http://127.0.0.1:8081 --config bench/configs/threeway.yaml \
  --split --delay-ms 600 --out docs/benchmarks/threeway-run.md
```

## Tricrypto2 checkpoint (#35)

[Tricrypto2 evidence](2026-11-03-local-state-tricrypto2.md): 104 independent historical success/revert
scenarios, 52 live cases (44 successful raw outputs and 8 expected reverts), classic/V3/wstETH
composition, and a fixed-deployment HTTP sample (30/30, per-direction p50 0.74–0.84 ms, maximum
sample 12.18 ms). #34/#35 are now jointly integrated; these restricted measurements do not replace
full-universe 90-row migration acceptance.

## Two-coin Curve stable pools (#34)

[Two-coin evidence](2026-11-03-local-state-curve-plain.md): 58 independent historical vectors,
29/29 live raw outputs (including zeros), forced V3 composition and a fixed three-stable-pool HTTP
sample (25/25, per-direction p50 1.50–1.66 ms). At this checkpoint only the three stable pools were
supported. Together with #35, local coverage is now four of five default Curve deployments;
legacy Tricrypto remains explicitly unsupported.

## Classic Curve 3pool checkpoint (#33)

[3pool evidence](2026-11-03-local-state-curve.md): 64 historical raw vectors, 18/18 mainnet
six-direction quotes, independent mixed routes and a fixed-deployment HTTP sample (30/30,
per-direction p50 1.51–1.73 ms). [Support matrix](../CURVE-SUPPORT.md) and
[raw inventory](2026-11-03-curve-inventory.json) retain all five discovered deployments; at that checkpoint four remained
explicitly unsupported locally. This is not the full-universe 90-row migration acceptance.

## Local wstETH checkpoint (#32)

[wstETH evidence](2026-11-03-local-state-wsteth.md) verifies Lido v4 internal-ratio arithmetic,
18 bidirectional integer scenarios including zero-output rounding, input caps and four conversion+V3
routes. A restricted six-record HTTP profile succeeded on 20/20 requests, per-direction p50
0.54–1.22 ms. These samples exclude Curve and do not replace the full 90-row migration acceptance.

## Local-state V2/V3 migration checkpoints (#31)

[V3 evidence](2026-11-03-local-state-v3.md) records 30/30 exact QuoterV2 matches across four fee tiers,
independent checks of mixed V2/V3 splits and multi-hop routes, and a restricted four-pool HTTP
sample (20 requests, p50/p95 31.05/31.33 ms). [V2 history](2026-11-02-local-state-v2.md) retains the
first checkpoint. Neither replaces the full-venue 90-row comparison below. The default remains
RPC pricing until Curve and full-coverage migration acceptance is complete.

## Historical: single / two-pool / multi-pool / ParaSwap (2 Nov 2026)

Run without an HTTP server:

```bash
cargo run -p fluxion-remote --bin split_bench -- --paraswap --max-branches 4 --max-quotes 256 \
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
Run `cargo run -p fluxion-remote --bin split_bench -- --config bench/configs/threeway.yaml`; no HTTP server is required.
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

`cargo run -p fluxion-remote --bin bench --` compares Fluxion's `POST /quote` against external aggregator quote
endpoints for a fixed corpus of token pairs and amount tiers.

## Run

```bash
# Terminal 1: real discovery requires GRAPH_API_KEY in the gitignored .env.
cargo run -p fluxion-remote -- serve --config bench/configs/threeway.yaml

# Terminal 2: same public profile, service on :8081.
cargo run -p fluxion-remote --bin bench -- --service http://127.0.0.1:8081 --config bench/configs/threeway.yaml --limit 3
cargo run -p fluxion-remote --bin bench -- --service http://127.0.0.1:8081 --config bench/configs/threeway.yaml
```

Options: `--service URL`, `--corpus PATH` (default `bench/corpus.yaml`), `--limit N`, `--split`
(enable multi-pool Fluxion requests; omitted means single-route),
`--delay-ms MS` (pause after each external request, default 250), `--config PATH` (local RPC
configuration, default `config.yaml`), and `--freshness-rpc-url URL` (optional independent
Ethereum RPC used only for before/after head observations; the report records its hostname).
An output basename such as `--out run.md` also works.
The harness requires the running service's `/info` endpoint and refuses mock mode.

Each Fluxion request is bracketed by Ethereum `latest` header reads. The freshness section
reports the quote's `quote_block` and the observed before/after head lag in both blocks and
block-timestamp seconds. Only responses marked `local_snapshot` with complete quote metadata and
both head reads contribute to p50/p95/max; missing data and RPC errors remain visible per sample.
This measures snapshot age, not the amount-out difference from recomputing on latest pool state.

For a same-run feature-disabled control, start a second instance with
`cargo run -p fluxion-remote -- serve --config bench/configs/control.yaml` (:8082), then add
`--control-service http://127.0.0.1:8082` to the benchmark command. Candidate/control order
alternates by row. Reports separate stETH, SHIB and other cohorts, and raw JSON includes both
local paths plus ParaSwap's `bestRoute` (pool addresses and allocation percentages when returned).
Live snapshots and block skew still differ; this is not a pinned-state replay.

ParaSwap's price API is keyless and always runs. A real Fluxion universe is still required;
missing Graph credentials must not silently turn a comparison into mock-vs-mainnet.
1inch and 0x are opt-in via `.env`:

```bash
ONEINCH_API_KEY=...   # https://business.1inch.com/portal — api.1inch.com quote API v6.0
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
external services and ordered per-hop labels for Fluxion. In split mode Fluxion hops is the maximum
branch length; JSON retains the full response, allocations, quote block and search metrics. 1inch
responses include `protocols`; its public API does not provide a pinned block in these responses.

This is a reproducible **procedure**, not deterministic replay: aggregator APIs do not pin a
block; discovery, cached reserves, and live calls can observe different states. The before/after
block range and timestamps disclose skew, not same-block equivalence. Amount tiers are fixed
raw inputs that were approximately USD-sized when authored. No gas, execution or MEV is modelled.
The competitors can support additional venues, conversions and split routes. 1inch/0x are skipped,
not failed, when their keys are absent. 1inch is verified in the latest three-way run; 0x remains unverified.

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
  public profile in `bench/configs/threeway.yaml`; debug build, not a release-performance claim.
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
