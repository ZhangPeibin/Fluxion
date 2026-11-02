# Fluxion

English | [中文](README.md)

Ethereum mainnet multi-hop routing quote (RFQ) HTTP service. Given `token_in` / `token_out` /
`amount_in`, it builds a **directed multigraph** of Uniswap V2 / V3, Curve, ShibaSwap V2 pools,
and configured protocol conversions. Bounded multi-hop search and exact refinement return the
highest-output single candidate path (exact-in). The first conversion adapter supports stETH↔wstETH.

> Open-source, self-hosted: download it, supply a YAML config plus your own RPC / Graph key, run it.

## Benchmark: mainnet quote comparison (2 Nov 2026)

**90/90 successful quotes; 84/90 within ±0.5% of ParaSwap's output.**

The same run compares two local instances and ParaSwap across 30 directed pairs × 3 fixed input
sizes. The control disables conversions and ShibaSwap; the candidate enables stETH↔wstETH,
ShibaSwap V2, and capability-diverse candidates. Output delta is `(local − ParaSwap) / ParaSwap`;
negative means fewer output tokens.

| Metric | Feature-disabled control | Current implementation |
|---|---:|---:|
| Successful quotes | 90/90 | **90/90** |
| Median output delta | −0.0148% | **−0.0070%** |
| Mean output delta | −1.3372% | **−0.1018%** |
| Within ±0.5% output delta | 72/90 (80.0%) | **84/90 (93.3%)** |
| Latency p50 | 1085 ms | 1246 ms |
| Latency p95 | 1586 ms | 1871 ms |

### Representative stETH / SHIB samples

| Input and direction | Control output | Current output | ParaSwap output | Delta change |
|---|---:|---:|---:|---:|
| 100000 USDC → stETH | 25.65966 stETH | **43.28001 stETH** | 43.28921 stETH | **−40.725% → −0.0213%** |
| 33.333… WETH → SHIB | 12.693 billion SHIB | **15.376 billion SHIB** | 15.672 billion SHIB | **−19.010% → −1.887%** |
| 8 billion SHIB → WETH | 14.30898 WETH | **15.82713 WETH** | 15.96768 WETH | **−10.388% → −0.880%** |

All 15 stETH-related and 6 SHIB-related outputs exceed their paired controls. Remaining SHIB gaps
involve external pool splitting and BONE intermediates; Fluxion still returns one path. Additional
candidates increase cost: p50 latency rises about 15%.

Ethereum block range **26324635–26324658**, local debug builds, public RPC. ParaSwap p50/p95
is 351/436 ms in this run. Quotes are not same-block; gas and actual execution are excluded.
1inch/0x were skipped because keys were absent.

**Evidence:** [90-row quote/control tables](docs/benchmarks/2026-11-02-quote-quality.md) ·
[raw JSON with routes and allocations](docs/benchmarks/2026-11-02-quote-quality.json) ·
[analysis (Chinese)](docs/benchmarks/2026-11-02-quote-quality-analysis.md) ·
[methodology and historical runs](docs/benchmarks/README.md)

## Quick start

```bash
# 1) Secrets (gitignored) — the service needs GRAPH_API_KEY
cp .env.example .env
#   edit .env and set GRAPH_API_KEY (create one for free at The Graph Studio)

# 2) Config (optional; defaults are used if absent) — RPC/thresholds/platforms/host live here
cp config.example.yaml config.yaml

# 3) Run (listens on 127.0.0.1:8080 by default)
cargo run
```

> Split: **`.env` holds the secret only** (`GRAPH_API_KEY`); **everything else lives in `config.yaml`**.

Startup loads configuration, discovers pools, validates conversion deployments, builds the graph,
and warms a price snapshot before serving. The default configuration enables Uniswap subgraphs;
without `GRAPH_API_KEY` it uses a built-in mock universe for offline development.
Use `cargo run -- --config PATH` to select another configuration file.

```bash
curl -s localhost:8080/health   # {"status":"ok"}
curl -s localhost:8080/ready    # probes upstream RPC; 503 if unreachable

curl -s -X POST localhost:8080/quote \
  -H 'content-type: application/json' \
  -d '{"token_in":"0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
       "token_out":"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
       "amount_in":"1000000000000000000"}'
```

## API

### `POST /quote`

Request:

```json
{
  "token_in": "0xC02a...",
  "token_out": "0xA0b8...",
  "amount_in": "1000000000000000000",
  "max_hops": 3,
  "amount_out_min": "0"
}
```

- Amounts are **decimal strings** (`U256`).
- `max_hops` optional, counts **edges** (defaults to config); each swap, wrap or unwrap counts as one hop.
- `amount_out_min` optional; below it returns `AmountOutBelowMin`.

Response (`200`):

```json
{
  "amount_out": "2318785201",
  "price_impact_bps": 32,
  "path": [
    {
      "operation": "swap",
      "platform": "uniswap-v3",
      "pool": "0xE0554a476A092703abdB3Ef35c80e0D76d32939F",
      "i": 1, "j": 0,
      "token_in": "0xC02a...",
      "token_out": "0xA0b8...",
      "amount_in": "1000000000000000000",
      "amount_out": "2318785201"
    }
  ]
}
```

- `operation` is `swap` / `wrap` / `unwrap`; a conversion's `pool` identifies the wrapper contract.
- Responses contain quotes and route information, not transaction calldata. No transaction is sent.

### `GET /graph`

Read-only export of the current routing multigraph for the web viewer and other tools:

```json
{
  "tokens": ["0xA0b8...", "0xC02a..."],
  "edges": [
    {
      "operation": "swap",
      "from": "0xC02a...",
      "to": "0xA0b8...",
      "platform": "uniswap-v3",
      "pool": "0xE055...",
      "i": 1,
      "j": 0,
      "mid_price": 2.3187e-9,
      "reserve": "1234567890123456789"
    }
  ]
}
```

- Each edge is a directed swap or conversion; `i` / `j` index `Pool.coins`.
- `mid_price` is output raw units / input raw units; use token decimals to derive whole-token prices.
- `mid_price` / `reserve` come from the current price snapshot; when no snapshot (or no value for
  that edge) is available those fields are **omitted** rather than erroring.
- Conversion edges omit `reserve` rather than exposing an internal sentinel as real liquidity.
- Read-only: it never rebuilds a snapshot, never consumes the rate limit, and does not affect `/quote`.

### `GET /info`

Returns `mode` (`mainnet` / `mock`), effective configuration, graph size and `platform_edges`.
The configuration excludes the RPC URL. `pools` counts graph records, including wrapper links.
The benchmark uses this endpoint to verify mode and actually loaded venues.

### `GET /health` / `GET /ready`

- `/health`: process liveness.
- `/ready`: does an `eth_blockNumber` against upstream RPC; returns `503` if unreachable.

### Errors

`QuoteError` → HTTP status: `NoPathFound`(404), `AmountOutBelowMin`/`InsufficientLiquidity`/
`Overflow`(422), `ZeroAmount`(400), `PoolNotFound`(404), `RpcError`(502), rate-limited(429).

## Graph viewer (web)

`ui/` is a standalone React + Vite + vis-network frontend that polls `GET /graph` read-only to
render the whole multigraph and preview routes.

```bash
# terminal 1: service (default :8080)
cargo run

# terminal 2: frontend (default :5173); install deps once
cd ui
npm install
npm run dev
```

Open <http://localhost:5173>. The Vite dev server proxies `/api` to `http://localhost:8080` and
strips the `/api` prefix, so the browser stays same-origin and the service needs no CORS.

- Renders every token (node) and directed pool hop (edge); nodes prefer known token symbols
  (USDC/WETH/…, unknown tokens fall back to short addresses). Edge labels (platform + pool) are off
  by default and can be enabled in the sidebar; parallel edges are separated by direction and index.
- The platform legend doubles as a filter and shows each platform's edge count; there is also a
  token search (dims unrelated nodes/edges) and a minimum-liquidity (raw reserve) filter.
- ShibaSwap and wstETH conversions have separate legend entries; conversion links are dashed and
  exempt from the pool-reserve threshold filter.
- Hover shows full address, pool, platform, `mid_price`, `reserve`; click a node/edge for a detail
  panel with an Etherscan link.
- Pick From/To by clicking nodes or typing addresses; the amount defaults to **one whole token**
  (raw units derived from the token's decimals) and can be edited to any raw value. `POST /quote`
  highlights the best route, dims the rest, and shows `amount_out`, `price_impact_bps` and per-hop
  details.
- Periodic refresh (default 5s, configurable) updates data in place, preserving node positions,
  selection and route highlight.

> Node symbols come from the built-in mainnet token table in `ui/src/graph.js` (covering the default
> allowlist); add entries there for custom tokens. Production packaging is deferred: v1 supports the
> two-process dev setup above only. `npm run build` emits static assets, but the service does not
> serve or deploy them yet.

## Architecture

### Startup and runtime

```text
config.yaml + .env
        |
        +-- Pool discovery: Uniswap subgraphs / Curve registries / ShibaSwap factory
        +-- Conversion instances: config -> validate underlying token and enabled directions
        |
        v
Graph: token nodes, directed AMM swaps and protocol conversions
        |
        v
Batch-prefetch V2 reserves and V3 slot0 -> CachingSource -> PriceSnapshot
        |
        v
axum HTTP service
  POST /quote -> Router -> snapshot weights -> diverse K-best DP -> refine -> best single path
  GET /graph  -> topology and installed snapshot
  GET /info   -> mode, effective config and graph size

Independent background loops:
  refresh_interval_secs  -> StateCache refreshes recently used V2 reserves in batches
  snapshot_refresh_secs -> prefetch full coarse data, rebuild and replace PriceSnapshot
```

Topology is fixed at startup. Runtime loops update pricing state and snapshots, not pool discovery.

### How a quote chooses its route

1. **Coarse search** reads installed mid-prices plus fee and optional liquidity penalties. Normal
   HTTP quotes perform no RPC during this stage.
2. **Candidate diversity** retains top-K per `(token, hop, capability family)`, with no repeated
   tokens. Four families distinguish original AMMs, conversion use, ShibaSwap use, and both.
   Candidates from every hop length survive to refinement.
3. **Refinement** runs at most 16 candidate paths concurrently. Each path quotes its hops in order,
   passing one hop's output into the next. Failure of one candidate does not fail the others.
4. **Selection** takes the highest exact `amount_out`, calculates price impact, and checks `amount_out_min`.

This is bounded heuristic search: families prevent one class from crowding out all others, but do
not guarantee a globally optimal route across the market.

### Shared interface, different pricing semantics

`QuoteSource` exposes `amount_out`, `mid_price` and `reserve_in`; `Dispatcher` selects by `PoolKind`:

| Type | Exact refinement | Semantics |
|---|---|---|
| Uniswap V2 / ShibaSwap V2 | Short-TTL reserves + local constant-product math | Shared math, distinct venue labels, 0.3% fee |
| Uniswap V3 | QuoterV2 `eth_call` | Quotes the input amount using the token pair and fee tier |
| Curve | `get_dy`, dispatched by interface | Stableswap / crypto index types |
| wstETH conversion | Wrapper's integer conversion views | Actual-amount conversion; no fixed 1:1 assumption or additional AMM fee |

For example, `stETH -> wrap -> wstETH -> V3 -> WETH -> V3 -> USDC` is a three-hop route.
**Code implements an interface type; configuration declares deployments.** Another compatible
wrapper can be configured; a different protocol interface requires an adapter.

### Cache responsibilities

| Component | Data | Refresh policy |
|---|---|---|
| `StateCache` | V2 reserves used for refinement | Lazy cold loads; batched refresh for pools accessed in the last 5 minutes; default TTL 30 s |
| `CachingSource` | Coarse mid-prices and reserves | Batch prefetch or on-demand reads; TTL is the larger refresh interval |
| `PriceSnapshot` | Read-only price vector indexed by graph edge | Default wait of 120 s between rebuilds; replace the shared snapshot when ready |

V2 refinement may read short-TTL cached reserves; other refinement reads live on-chain state.
The complete route is not a same-block execution simulation.

Stack:

- Rust (edition 2024), `tokio`, `axum`, `serde`, `thiserror`, `tracing`
- `alloy-primitives` for `U256` / `Address`
- Chain interaction: **raw JSON-RPC (`reqwest`)** with hand-written ABI encode/decode
  (`eth_call getReserves` / QuoterV2 / Curve `get_dy` / Multicall3 `aggregate3`)

## Module layout

```
src/
  lib.rs         library module exports
  model.rs       Token, PoolKind, ConversionKind, Edge, Hop, Route and request models
  graph.rs       adjacency multigraph; multi-coin expansion; enabled conversion directions
  discovery.rs   Uniswap subgraphs / Curve registries and allowlist filtering
  discovery/
    shibaswap.rs factory queries, reserve validation, liquidity filtering and ranking
  source.rs      QuoteSource trait, V2 math and mock implementations
  adapters.rs    V2/V3/Curve adapters, Dispatcher and CachingSource
  conversion.rs protocol conversion adapters; currently the wstETH interface
  state.rs       StateCache: V2 reserve TTL cache + Multicall3 hot-pool refresh
  rpc.rs         raw JSON-RPC client (429/transient retry with backoff)
  router.rs      snapshots, four-family K-best DP, bounded refinement and price impact
  api.rs         /quote, /graph, /health, /ready; DTOs, error mapping and rate limits
  error.rs       shared quote errors
  config.rs      YAML config, typed conversion instances and validation
  util.rs        ABI encoding, hex, numeric conversion
  main.rs        startup assembly, /info, background refresh and HTTP service
  bin/
    bench.rs     paired benchmarks, aggregator comparison, routes, JSON / Markdown reports
    graph.rs     Mermaid / HTML export from a running service

config.example.yaml           complete configuration example
config.benchmark.yaml         candidate with new capabilities (:8081)
config.benchmark-control.yaml control without conversions or ShibaSwap (:8082)
bench/corpus.yaml             fixed pairs and raw input amounts
tests/                        routing/conversion behavior, mainnet tests, pinned-block fixture
ui/                           standalone React + Vite + vis-network viewer
docs/QUOTE-QUALITY.md          conversion, ShibaSwap and candidate-diversity design
docs/benchmarks/               methodology, raw routes and result analysis
```

## Data model

- `Token { address, symbol, decimals }`
- `Pool { kind: PoolKind, address, coins: Vec<Address> }`
  - `PoolKind = UniswapV2 | ShibaSwapV2 | UniswapV3 { fee } | Curve | Conversion { kind, wrap, unwrap }`
  - `ConversionKind` currently supports `WstEth`. A conversion reuses the `Pool` container:
    `address` is the wrapper, `coins` is `[underlying, wrapped]`.
- `Edge { pool_idx, i, j }` (directed: sell coin index `i` → buy coin index `j`)
- The same token pair across platforms = **parallel edges**; multi-coin pools (e.g. Curve 3pool)
  expand to all ordered pairs: 3 coins → 6 edges.
- Conversions add only enabled directions; API hops expose `operation` as swap, wrap or unwrap.

## Pool discovery & filtering

- **Protocol conversions**: stETH↔wstETH is enabled by default through typed `protocol_conversions`
  instances. Directions are independently configurable; exact quotes call the protocol's integer
  conversion views. No fictional AMM fee, reserve or 1:1 rate is assumed. Each conversion counts
  as one hop. `protocol_conversions: []` disables them; new interface types require an adapter.
- **ShibaSwap V2**: batched factory `getPair` queries over both-allowlisted token pairs, checked
  token ordering and nonzero reserves, 30 bps V2 math, and a distinct `shibaswap-v2` label.
  `min_liquidity` uses stablecoin/WETH-anchored reserve-ratio estimates (unpriced pools are
  excluded with a positive floor). Volume/age filters cannot be evaluated; nonzero thresholds
  skip this discovery source with a warning.

- **Uniswap V2**: V2 subgraph, ordered by `reserveUSD`, paginated.
- **Uniswap V3**: V3 subgraph, ordered by `volumeUSD`, paginated (ordering by TVL surfaces
  inflated junk pools first).
- **Curve**: enumerate multiple registries via `AddressProvider.get_address(id)`
  (`curve_registry_ids`, default `[0, 5]` = main stableswap + crypto; `7` = MetaRegistry covering
  all incl. factory/metapool, slower), each `pool_count() → pool_list(i) → coins(i)`, validating
  each pool with one `get_dy` probe.
- **Filtering**: apply liquidity, activity-proxy and age thresholds supported by each source,
  plus a built-in major-token allowlist (~50 tokens). Empty `token_allowlist` uses that list;
  a nonempty list overrides it. V2/V3, ShibaSwap and conversions require both coins, Curve all coins.
- V2/V3 are ordered by liquidity/volume respectively and keep up to `max_pools` qualifying pools
  (default 10000).
- ShibaSwap truncates after estimated-liquidity ranking. Curve applies `max_pools` to enumeration
  within each registry; conversion count is controlled by the configured instance list.

## Routing algorithm

- Coarse search turns the rate product into an additive `-ln(rate)` cost, then adds fee and
  liquidity penalties. Exact output determines the final winner.
- Coarse edge weight: `-ln(mid_price) + fee_penalty(kind) + λ·(amount_in/reserve_in)`; when
  `reserve_ratio_k > 0`, skip edges with `reserve_in < k·amount_in`.
- **K-best DP**: keep top-K paths per `(token, hop, capability family)`, with up to four families
  distinguishing conversion/ShibaSwap use, so one family cannot crowd every other family out
  before refinement. Bounded by `max_hops`, no repeated token.
- **Direct fallback**: direct edges satisfying the configured reserve threshold are added to
  the candidates, so a coarse misrank cannot push an obvious direct route out.
- **Parallelism**: snapshot construction and exact refinement each have concurrency 16;
  snapshot weighting and DP are synchronous in-memory work.
- Slippage is absorbed by refine; the response also returns `price_impact_bps`.

## Configuration (`config.yaml`)

| Key | Description | Default |
|---|---|---|
| `host` / `port` | bind address | `127.0.0.1` / `8080` |
| `rpc_url` | third-party RPC | `https://eth.drpc.org` |
| `uniswap_v2_subgraph_id` / `uniswap_v3_subgraph_id` | The Graph decentralized-network IDs | see example |
| `curve_address_provider` | Curve AddressProvider | `0x0000…4383` |
| `curve_registry_ids` | Curve registries to enumerate (0=main stableswap, 5=crypto, 7=MetaRegistry all) | `[0, 5]` |
| `enabled_platforms` | AMM venues, independent of conversion instances | `uniswap-v2` / `uniswap-v3` / `curve` / `shibaswap-v2` |
| `shibaswap_v2_factory` | ShibaSwap V2 factory | see example |
| `protocol_conversions` | deployments of supported interfaces; `[]` disables conversions | Lido stETH↔wstETH, both directions |
| `max_hops` / `top_k` / `max_pools` | max edges / candidates per state and capability family / discovery limit | 3 / 8 / 10000 |
| `min_liquidity` | liquidity floor (USD-ish) | 50000 |
| `min_volume_24h` | volume floor (uses subgraph `volumeUSD` as proxy) | 0 |
| `min_pool_age_days` | minimum pool age (days) | 0 |
| `reserve_ratio_k` | shallow-pool threshold `reserve_in ≥ k·amount_in` (0 = off) | 0 |
| `slippage_penalty_lambda` | liquidity penalty coefficient | 0.0 |
| `refresh_interval_secs` | V2 reserve TTL / hot refresh interval; hot = accessed in the last 5 minutes | 30 |
| `snapshot_refresh_secs` | full price-snapshot rebuild interval | 120 |
| `request_timeout_ms` | per-request timeout (ms) | 5000 |
| `token_allowlist` | override built-in major-token allowlist | empty (built-in) |

`GRAPH_API_KEY` lives **only in the gitignored `.env`** — never committed.
Conversion instance fields are `kind`, `contract`, `underlying`, `wrapped`, `enabled`, `wrap`,
and `unwrap`; see [config.example.yaml](config.example.yaml). Rates come from the protocol,
not a fixed price entered in YAML.

Coarse cache TTL is `max(refresh_interval_secs, snapshot_refresh_secs)`. Multicall3 prefetches
V2 reserves and V3 `slot0` in batches without promoting the entire graph to the hot set.

## Benchmarks and graph export

```bash
cargo run -- --config config.benchmark.yaml # separate mainnet service on :8081
cargo run --bin bench -- --service http://127.0.0.1:8081 --config config.benchmark.yaml
cargo run --bin graph -- --service http://127.0.0.1:8081 --out graph
```

`GET /info` exposes mode, graph size and effective configuration, excluding the RPC URL.
Quote hops and graph edges expose `operation: swap | wrap | unwrap`. Conversion `pool` fields
identify the wrapper contract, and graph conversion edges omit reserves. The viewer uses dashed
conversion links. See [quote-quality design](docs/QUOTE-QUALITY.md).
The harness rejects mock services. ParaSwap is keyless; 1inch/0x are opt-in via
`ONEINCH_API_KEY` / `ZEROX_API_KEY` in `.env`. See [benchmark methodology and results](docs/benchmarks/README.md).

For a paired control, run a second service with `config.benchmark-control.yaml` (:8082), then add
`--control-service http://127.0.0.1:8082` to the benchmark invocation. It alternates quote order
and reports stETH, SHIB, and other cohorts, retaining full routes in JSON.

Latest results are displayed in the **Benchmark** section near the top of this README. Full
per-row outputs are in the [90-row paired report](docs/benchmarks/2026-11-02-quote-quality.md).

## Testing

```bash
cargo test                       # unit tests (offline)
cargo test -- --ignored          # real mainnet integration tests (read-only eth_call)

cd ui && npm install && npm run build   # frontend build
```

- Unit: mock `QuoteSource` multigraph, routing optimality, negative-weight/simple-path, encoding stability.
- Conversion/regression behavior: integer rounding, direction flags, config/underlying checks,
  conversion+swap composition, public operation fields, and `top_k=1` counterexamples protecting
  conversion and original deep-pool routes from another family's thin pools.
- Mainnet: V2/V3/Curve real quotes, V2 vs V3 best venue, Curve 3pool, tricrypto2 crypto pool,
  Multicall3 hot refresh, end-to-end multi-hop (V2/V3 + Curve), subgraph discovery. The V2 adapter
  also has a **fixed-block `getReserves` fixture** for deterministic offline tests.
- Added pinned-block mainnet checks: wstETH round-trip rounding, ShibaSwap discovery, and exact
  agreement with the deployed ShibaSwap Router.

## Limitations

- Ethereum mainnet only; exact-in; one highest-output refined candidate, without splitting or
  a market-wide global-optimality guarantee.
- Native ETH unsupported; callers handle ETH↔WETH wrapping. stETH↔wstETH is a separate supported conversion.
- The Curve adapter supports both stableswap (`get_dy(int128,…)`) and crypto (`get_dy(uint256,…)`);
  **discovery** covers the main stableswap + crypto registries by default (`curve_registry_ids: [0, 5]`);
  factory/metapool can be included by adding `7` (MetaRegistry) to `curve_registry_ids`, at a
  significant startup cost.
- Public RPC may rate-limit. Normal coarse search is in-memory; refinement dominates RPC waits.
  Additional candidate families increase RPC cost; measured latency is above, without a fixed SLA.
- No generic support for fee-on-transfer or arbitrary rebasing tokens. The dedicated stETH
  conversion adapter does not imply automatic support for other special-token semantics.
- Snapshots, short-TTL V2 reserves and live calls may reflect different blocks; a quote is not a
  simulation of executing the complete route.
- Public HTTP has no auth, only simple rate limiting.

## Docs

- [`docs/QUOTE-QUALITY.md`](docs/QUOTE-QUALITY.md): current conversion, ShibaSwap and candidate-family design.
- [`docs/benchmarks/README.md`](docs/benchmarks/README.md): methodology, paired results and raw data index.
- [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md): **detailed architecture** (data flow, modules,
  the three cache layers, routing, RPC, API, concurrency, limitations).
- [`docs/REQUEST-TRACE.en.md`](docs/REQUEST-TRACE.en.md): a function-by-function trace of one `/quote` (Chinese: `docs/REQUEST-TRACE.md`).
- `docs/PLAN.md`: learning milestones; `docs/0X-*.md`: per-stage notes.
- `AGENTS.md`: repository engineering conventions.
