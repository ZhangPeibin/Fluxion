# Fluxion

English | [中文](README.md)

**Runtime for the Web3 Agent Capability Infrastructure.** Fluxion exposes Web3 actions as stable
capabilities; the first delivered capability is `swap.quote`, callable by a local CLI or an MCP
agent.

Underneath is an Ethereum mainnet exact-in routing engine: it builds a **directed multigraph** of
Uniswap V2 / V3, Curve, ShibaSwap V2 pools, and configured protocol conversions. Bounded multi-hop
search and exact refinement return the highest-output single candidate path; `split=true` allocates
across independent pools and disjoint routes. The first conversion adapter supports stETH↔wstETH.

> Open-source, self-hosted: download it, supply a YAML config plus your own RPC / Graph key, run it.

> Product vision: this repository implements the `swap.quote` engine of a broader **Web3 Agent
> Capability Infrastructure**. See the full [whitepaper](docs/WHITEPAPER.md).

## Capabilities

| Capability | Status | Providers | Docs |
|---|---|---|---|
| `swap.quote` | available | `local-cli`, `local-mcp`; `remote-mcp` planned | [docs/capabilities/swap/README.md](docs/capabilities/swap/README.md) |

Capability defines *what* the agent wants; provider defines *where and how* it runs. Only local
providers are implemented; `--provider remote` returns `UNSUPPORTED` without silent fallback.
See the [capability index](docs/capabilities/README.md).

## CLI / MCP

Start a warm runtime once; subsequent quotes reuse state:

```bash
cargo install --path .
fluxion start               # background warmup; first start takes about a minute

fluxion swap ldo-eth 100
fluxion swap ldo-eth 100 --json
fluxion ldo-eth 100

fluxion status              # readiness / lag
fluxion stop
fluxion mcp                 # local MCP stdio server

fluxion capability list
fluxion capability describe swap.quote --json
```

The machine interface is:

```bash
fluxion swap quote --in LDO --out ETH --amount 100 --json
```

Local providers need `GRAPH_API_KEY` (env or local `.env`). The default runtime config is
`config.runtime.yaml` (local snapshot + incremental); the default endpoint is
`http://127.0.0.1:8080`. Warm quotes are typically 5–30 ms. Responses include `schema_version`,
`provider`, `quote_block`, `state_age_secs` and `lag_blocks`.

## Integrated liquidity

Live `/info` and `/graph` inventory on 3 Nov 2026 with `config.benchmark-local.yaml`:

| Metric | Count |
|---|---:|
| Tokens | **48** |
| Actual AMM pools | **208** |
| Unique direct swap pairs (unordered) | **106** |
| Unique direct swap directions | **212** |
| Protocol conversion links | **1** (stETH↔wstETH) |
| Total pool/conversion records | **209** |
| Directed graph edges, including parallel pools | **426** |

| Source | Pools / conversion links | Unique direct pairs within source |
|---|---:|---:|
| Uniswap V2 | 29 | 29 |
| Uniswap V3 | 169 | 103 |
| Curve | 4 | 7 |
| ShibaSwap V2 | 6 | 6 |
| Lido wstETH conversion | 1 link | 1 |

Sources overlap, so their pair counts are not additive. Including conversions gives **107 unique
direct connections / 214 directions**. Multi-hop combinations are not counted as integrated pairs;
the benchmark below tests a 30-direction subset. Counts depend on discovery and configuration.
This local profile explicitly excludes legacy Tricrypto and retains Tricrypto2.
[Inventory definitions and snapshot](docs/benchmarks/2026-11-03-graph-inventory.json).

## Latest benchmark: disjoint multi-route split (4 Nov 2026)

All 90 rows succeed under the incremental local snapshot. Snapshot lag is **0/1/1 blocks**
(p50/p95/max) or **0/12/12 seconds**, and Fluxion HTTP latency is **49/133 ms** p50/p95.
Disjoint multi-hop route allocation improves large-order accuracy: ParaSwap is within ±50 bps on
**89/90** rows (mean −2.6 bps) and 1inch on **82/90** (mean −8.7 bps), up from 86/90 and 80/90
in the previous incremental run.

| Metric | Previous incremental | Multi-route split |
|---|---:|---:|
| Snapshot lag blocks p50 / p95 / max | 0 / 1 / 1 | **0 / 1 / 1** |
| Snapshot lag seconds p50 / p95 / max | 0 / 12 / 12 | **0 / 12 / 12** |
| Fluxion HTTP p50 / p95 | 40 / 101 ms | **49 / 133 ms** |
| Success | 90 / 90 | **90 / 90** |
| ParaSwap mean delta / within ±50 bps | −10.5 bps / 86 | **−2.6 bps / 89** |
| 1inch mean delta / within ±50 bps | −17.3 bps / 80 | **−8.7 bps / 82** |

Remaining large-order outliers are mostly venue coverage limits (for example LDO and SHIB) rather
than the allocation algorithm. Freshness and route allocation are measured separately from
execution, gas and MEV.

[90-row report](docs/benchmarks/quote-freshness-2026-11-04-route-split.md) ·
[Raw JSON](docs/benchmarks/quote-freshness-2026-11-04-route-split.json) ·
[Previous incremental run](docs/benchmarks/quote-freshness-2026-11-04-incremental.md)

## Data/compute separation (#31–#35: V2/V3, wstETH, three stable pools and Tricrypto2)

Opt-in `pricing_mode: local_snapshot` synchronizes V2/ShibaSwap, V3, Lido v4, three Curve stable pools and Tricrypto2, and
publishes immutable block snapshots. Single-route, multi-hop and split calculations use the same
snapshot for coarse and exact pricing, with **zero RPC during request computation**. Offline
operation, reorg/staleness/failure handling and integer parity with deployed Routers are verified.
V3, wstETH and each Curve deployment use protocol-specific integer models checked against
independent on-chain views. #36 isolates writer synchronization/build/validation from HTTP execution;
short-lock publication and writer-owned retirement preserve immutable reader snapshots.
wstETH uses Lido internal ETH/shares; two-coin stable pools retain their
distinct balance sources and rounding orders; Tricrypto2 uses stored D and dynamic fees. The latest
local-snapshot comparison appears above; local model details are linked below.

```bash
# Restricted real ShibaSwap example on :8083, not the full-venue benchmark profile
cargo run -- --config config.local-v2.example.yaml
curl -s localhost:8083/state
# Mixed V2/V3/ShibaSwap profile on :8084 (requires a Graph key)
cargo run -- --config config.local-v3.example.yaml
# stETH/wstETH + V3 multi-hop on :8085 (requires a Graph key)
cargo run -- --config config.local-wsteth.example.yaml
# Explicit single-deployment 3pool HTTP demo on :8086 (no Graph key/discovery)
cargo run --example local_curve_server
# Explicit Tricrypto2 graph on :8088 (no Graph key/registry discovery)
cargo run --example local_curve_server -- --tricrypto2 --config config.local-tricrypto2.example.yaml
# Explicit three-stable-pool graph on :8087 (not registry discovery)
cargo run --example local_curve_server -- --all-stable --config config.local-curve-stable.example.yaml
```

The default remains `rpc`. The local experiment explicitly excludes legacy Tricrypto;
continuous service under the default freshness target remains to be accepted.
Unsupported pools/protocol versions or missing V3 coverage are explicit rather
than disappearing from the graph or falling back to request-time RPC. [Design/run guide](docs/LOCAL-STATE.md) ·
[V3 measurements](docs/benchmarks/2026-11-03-local-state-v3.md) ·
[wstETH measurements](docs/benchmarks/2026-11-03-local-state-wsteth.md) ·
[Curve support matrix](docs/CURVE-SUPPORT.md) · [3pool measurements](docs/benchmarks/2026-11-03-local-state-curve.md) ·
[Tricrypto2 model](docs/TRICRYPTO2.md) · [Tricrypto2 measurements](docs/benchmarks/2026-11-03-local-state-tricrypto2.md) ·
[two-coin measurements](docs/benchmarks/2026-11-03-local-state-curve-plain.md) (Chinese).

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
- `amount_out_min` optional; selected total output below it returns `AmountOutBelowMin`.
- `split` optional, default `false`; existing requests keep the single-path response below.
- With `split=true`, optional `split_max_branches` must be at least 2 and cannot exceed the server
  cap (default 4). Supplying a cap without opting in returns 400.

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

**Optional multi-pool mode:** add `"split": true` to the request. Responses also contain
`plan_kind`, `allocations`, `baseline_amount_out`, `search`, and, for real mainnet pricing,
`quote_block`. All exact quotes use that block. Each allocation has `amount_in`, `amount_out`,
`input_share_percent`, and its own sequential `path`. Parallel branches are not concatenated: the top-level
`path` is empty for multi-branch plans and `amount_out` sums branch outputs. Baseline selections
still have one 100% allocation. Budget exhaustion retains the best complete evaluated plan.
New split/direct selections return `price_impact_bps: null` because aggregate impact is not
calculated. See the [full contract and budgets](docs/MULTI-POOL.md).

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
- `/ready`: normally probes upstream `eth_blockNumber`; returns `503` if unreachable. Local mode
  checks published snapshot availability without an RPC probe.

### `GET /state`

Local mode reports block/hash, age, lag relative to the last observed head, pool coverage, sync
RPC/time, errors and reorg count. Successful local quotes add `state` metadata and `quote_block`;
the Viewer displays mode/block/age/lag. Cold, stale, incomplete or unsupported local state returns
an explicit 503 instead of falling back to request-time RPC. See [LOCAL-STATE](docs/LOCAL-STATE.md).

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
- Multi-pool routing is enabled by default with a toggle back to single-route mode. Branch
  percentages, inputs/outputs, quote block and all selected edges are displayed; unavailable
  aggregate price impact is shown as “—”.
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
    split=true -> pinned exact pricing -> two-pool refinement / allocation DP -> best complete plan
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
5. **Optional splitting** retains the baseline and two-pool refinement before discrete allocation
   DP across up to four independent direct pools. Remainders are re-quoted exactly and only a
   strictly higher total replaces the winner. Default extra budget: 256 quotes and 8 seconds.

This is bounded heuristic search: families prevent one class from crowding out all others, but do
not guarantee a globally optimal route across the market.

### Shared interface, different pricing semantics

`QuoteSource` exposes `amount_out`, `mid_price` and `reserve_in`; `Dispatcher` selects by `PoolKind`:

| Type | Exact refinement | Semantics |
|---|---|---|
| Uniswap V2 / ShibaSwap V2 | Short-TTL reserves + local constant-product math | Shared math, distinct venue labels, 0.3% fee |
| Uniswap V3 | QuoterV2 `eth_call` | Quotes the input amount using the token pair and fee tier |
| Curve | `get_dy`, dispatched by interface | Stableswap / crypto index types |
| wstETH conversion | RPC views / local Lido v4 integer state | Actual-amount rounding; no fixed 1:1 assumption or additional AMM fee |

For example, `stETH -> wrap -> wstETH -> V3 -> WETH -> V3 -> USDC` is a three-hop route.
**Code implements an interface type; configuration declares deployments.** Another compatible
wrapper can be configured; a different protocol interface requires an adapter.

### Cache responsibilities

| Component | Data | Refresh policy |
|---|---|---|
| `StateCache` | V2 reserves used for refinement | Lazy cold loads; batched refresh for pools accessed in the last 5 minutes; default TTL 30 s |
| `CachingSource` | Coarse mid-prices and reserves | Batch prefetch or on-demand reads; TTL is the larger refresh interval |
| `PriceSnapshot` | Read-only price vector indexed by graph edge | Default wait of 120 s between rebuilds; replace the shared snapshot when ready |

Default single-route refinement may use short-TTL V2 reserves and live calls. Mainnet split
requests build fresh pricing caches pinned to `quote_block`; coarse search may still use an older
snapshot. Neither mode simulates execution of the complete transaction.

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
  split.rs       bounded two-/multi-pool allocation, baseline fallback and integer conservation
  reference.rs   ParaSwap quotes and pinned allocation replay; diagnostics without graph expansion
  universe.rs    shared discovery, pricing assembly and batch prefetch
  api.rs         /quote, /graph, /health, /ready; DTOs, error mapping and rate limits
  capability/    capability contract, provider descriptors, shared types and stable errors
  swap/          swap.quote DTOs and provider matrix
  client.rs      thin client for the warm runtime
  cli.rs         CLI dispatch, runtime lifecycle and provider selection
  mcp.rs         MCP stdio server (swap_quote)
  token.rs       built-in token registry and decimal amount parsing
  error.rs       shared quote errors
  config.rs      YAML config, typed conversion instances and validation
  util.rs        ABI encoding, hex, numeric conversion
  main.rs        CLI dispatch plus server startup/assembly
  bin/
    bench.rs     paired benchmarks, aggregator comparison, routes, JSON / Markdown reports
    split_bench.rs pinned single/two/multi/ParaSwap comparison and cost reporting
    graph.rs     Mermaid / HTML export from a running service

config.example.yaml           complete configuration example
config.runtime.yaml           default CLI/MCP warm runtime (local snapshot)
config.benchmark.yaml         candidate with new capabilities (:8081)
config.benchmark-control.yaml control without conversions or ShibaSwap (:8082)
bench/corpus.yaml             fixed pairs and raw input amounts
schemas/                      capability JSON schemas
skills/                       agent skills (currently swap)
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
| `request_timeout_ms` | per-request timeout (ms) | 10000 |
| `pricing_mode` / `local_state` | exact pricing mode / background synchronization and freshness policy | `rpc` / see example |
| `split` | server-side opt-in allocation budgets; requests still need `split=true` | 4 branches / 4 pools / 20 segments / 256 calls / 8000 ms |
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

Use `--split` for the multi-pool three-way comparison. Latest results appear above and in the
[writer-isolation measurements](docs/benchmarks/2026-11-03-writer-isolation-fix.md).

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
- Multi-pool: four-pool gains, integer rounding/overflow/ties, budget fallback, HTTP compatibility,
  block pinning/deadline/min-output handling, offline mock mode, and independent mainnet branch totals.

## Limitations

- Ethereum mainnet only; exact-in; single-route by default, optional splitting across independent
  direct pools or disjoint multi-hop routes. No market-wide global-optimality guarantee.
- Allocation has candidate/grid/call budgets; disjoint multi-hop routes are supported, while
  shared-pool state interactions are not supported.
- Native ETH unsupported; callers handle ETH↔WETH wrapping. stETH↔wstETH is a separate supported conversion.
- The Curve adapter supports both stableswap (`get_dy(int128,…)`) and crypto (`get_dy(uint256,…)`);
  **discovery** covers the main stableswap + crypto registries by default (`curve_registry_ids: [0, 5]`);
  factory/metapool can be included by adding `7` (MetaRegistry) to `curve_registry_ids`, at a
  significant startup cost.
- Public RPC may rate-limit. Normal coarse search is in-memory; refinement dominates RPC waits.
  Additional candidate families increase RPC cost; measured latency is above, without a fixed SLA.
- No generic support for fee-on-transfer or arbitrary rebasing tokens. The dedicated stETH
  conversion adapter does not imply automatic support for other special-token semantics.
- Default single-route snapshots, cached V2 reserves and live calls may reflect different blocks.
  Split exact pricing is pinned; quotes do not construct or simulate transactions.
- Public HTTP has no auth, only simple rate limiting.

## Docs

- [`docs/WHITEPAPER.md`](docs/WHITEPAPER.md): Web3 Agent Capability Infrastructure whitepaper.
- [`docs/capabilities/`](docs/capabilities/README.md): capability index and provider matrix.
- [`docs/capabilities/swap/README.md`](docs/capabilities/swap/README.md): `swap.quote` contract, CLI/MCP usage and errors.
- [`skills/swap/SKILL.md`](skills/swap/SKILL.md): agent-facing `swap.quote` instructions.
- [`docs/MULTI-POOL.md`](docs/MULTI-POOL.md): allocation DP, pinned HTTP pricing, Viewer and reference-replay boundaries.
- [`docs/SPLIT-ROUTING.md`](docs/SPLIT-ROUTING.md): allocation, budgets, memoization and fixed-state experiment.
- [`docs/QUOTE-QUALITY.md`](docs/QUOTE-QUALITY.md): current conversion, ShibaSwap and candidate-family design.
- [`docs/benchmarks/README.md`](docs/benchmarks/README.md): methodology, paired results and raw data index.
- [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md): **detailed architecture** (data flow, modules,
  the three cache layers, routing, RPC, API, concurrency, limitations).
- [`docs/REQUEST-TRACE.en.md`](docs/REQUEST-TRACE.en.md): a function-by-function trace of one `/quote` (Chinese: `docs/REQUEST-TRACE.md`).
- `docs/PLAN.md`: learning milestones; `docs/0X-*.md`: per-stage notes.
- `AGENTS.md`: repository engineering conventions.
