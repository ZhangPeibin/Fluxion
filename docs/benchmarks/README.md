# Quote benchmarks

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

## Latest run: 2 Nov 2026

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
