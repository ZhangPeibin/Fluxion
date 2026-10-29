# Fluxion

English | [中文](README.md)

Ethereum mainnet multi-hop routing quote (RFQ) HTTP service. Given `token_in` / `token_out` /
`amount_in`, it searches a **directed multigraph** of Uniswap V2 / V3 / Curve pools for the best
route (multi-hop supported) and returns the best `amount_out` with the full path (exact-in).

> Open-source, self-hosted: download it, supply a YAML config plus your own RPC / Graph key, run it.

## Quick start

```bash
# 1) Secret (gitignored) — .env only holds GRAPH_API_KEY
cp .env.example .env
#   edit .env and set GRAPH_API_KEY (create one for free at The Graph Studio)

# 2) Config (optional; defaults are used if absent) — RPC/thresholds/platforms/host live here
cp config.example.yaml config.yaml

# 3) Run (listens on 127.0.0.1:8080 by default)
cargo run
```

> Split: **`.env` holds the secret only** (`GRAPH_API_KEY`); **everything else lives in `config.yaml`**.

On startup it discovers pools from subgraphs / the on-chain Curve registry, warms the mid-price
cache, then starts serving. (Without `GRAPH_API_KEY` it falls back to a built-in mock universe for
offline development.)

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
- `max_hops` optional, counts **edges** (defaults to config).
- `amount_out_min` optional; below it returns `AmountOutBelowMin`.

Response (`200`):

```json
{
  "amount_out": "2318785201",
  "price_impact_bps": 32,
  "path": [
    {
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

### `GET /graph`

Read-only export of the current routing multigraph for the web viewer and other tools:

```json
{
  "tokens": ["0xA0b8...", "0xC02a..."],
  "edges": [
    {
      "from": "0xC02a...",
      "to": "0xA0b8...",
      "platform": "uniswap-v3",
      "pool": "0xE055...",
      "i": 1,
      "j": 0,
      "mid_price": 2318.7,
      "reserve": "1234567890123456789"
    }
  ]
}
```

- Each edge is a **directed pool hop**; `i` / `j` are the coin indices inside the pool.
- `mid_price` / `reserve` come from the current price snapshot; when no snapshot (or no value for
  that edge) is available those fields are **omitted** rather than erroring.
- Read-only: it never rebuilds a snapshot, never consumes the rate limit, and does not affect `/quote`.

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

- Renders every token (node) and directed pool hop (edge); edges are labelled with platform + pool,
  and parallel edges are separated by direction and index.
- The platform legend doubles as a filter and shows each platform's edge count.
- Hover shows full address, pool, platform, `mid_price`, `reserve`; click a node/edge for a detail
  panel with an Etherscan link.
- Pick From/To by clicking nodes or typing addresses; amount defaults to `1` raw unit. `POST /quote`
  highlights the best route, dims the rest, and shows `amount_out`, `price_impact_bps` and per-hop
  details.
- Periodic refresh (default 5s, configurable) updates data in place, preserving node positions,
  selection and route highlight.

> Production packaging is deferred: v1 supports the two-process dev setup above only. `npm run build`
> emits static assets, but the service does not serve or deploy them yet.

## Architecture

```
request → axum /quote → Router
                        ├─ coarse: weights from the cached mid-price snapshot → K-best DP
                        │          (bounded hops, simple path, fee/liquidity penalty) → top-K candidates
                        └─ refine: concurrently chain real on-chain quotes per candidate, take max amount_out
background: discover pools + warm prices at startup; periodically refresh hot V2 reserves (Multicall3)
```

Two-layer pricing: **coarse** uses cached mid-prices for fast path search; **refine** uses the real
on-chain `amount_out` (slippage + fees) only for a few candidates to decide the winner.

Stack:

- Rust (edition 2024), `tokio`, `axum`, `serde`, `thiserror`, `tracing`
- `alloy-primitives` for `U256` / `Address`
- Chain interaction: **raw JSON-RPC (`reqwest`)** with hand-written ABI encode/decode
  (`eth_call getReserves` / QuoterV2 / Curve `get_dy` / Multicall3 `aggregate3`)

## Module layout

```
src/
  model.rs       Token / Pool / PoolKind / Edge / Hop / Route / QuoteRequest
  graph.rs       custom-adjacency multigraph (parallel edges; multi-coin pools expand to all ordered pairs)
  discovery.rs   V2/V3 subgraph discovery + Curve main-registry discovery + filtering
  source.rs      QuoteSource trait + mock impls
  adapters.rs    real adapters (V2/V3/Curve) + Dispatcher + CachingSource
  state.rs       StateCache: V2 reserve TTL cache + Multicall3 hot-pool refresh
  rpc.rs         raw JSON-RPC client (429/transient retry with backoff)
  router.rs      K-best DP coarse + parallel refine + price impact
  api.rs         axum (/quote, /graph, /health, /ready), error mapping, rate limiting
  config.rs      YAML config
  util.rs        ABI encoding, hex, numeric conversion
  main.rs        entry: discover → build graph → warm snapshot → serve

ui/              standalone web viewer (React + Vite + vis-network; read-only /graph + /quote)
```

## Data model

- `Token { address, symbol, decimals }`
- `Pool { kind: PoolKind, address, coins: Vec<Address> }`
  - `PoolKind = UniswapV2 | UniswapV3 { fee } | Curve`
- `Edge { pool_idx, i, j }` (directed: sell coin index `i` → buy coin index `j`)
- The same token pair across platforms = **parallel edges**; multi-coin pools (e.g. Curve 3pool)
  expand to all ordered pairs: 3 coins → 6 edges.

## Pool discovery & filtering

- **Uniswap V2**: V2 subgraph, ordered by `reserveUSD`, paginated.
- **Uniswap V3**: V3 subgraph, ordered by `volumeUSD`, paginated (ordering by TVL surfaces
  inflated junk pools first).
- **Curve**: enumerate multiple registries via `AddressProvider.get_address(id)`
  (`curve_registry_ids`, default `[0, 5]` = main stableswap + crypto; `7` = MetaRegistry covering
  all incl. factory/metapool, slower), each `pool_count() → pool_list(i) → coins(i)`, validating
  each pool with one `get_dy` probe.
- **Filtering**: `min_liquidity`, `min_volume_24h`, `min_pool_age_days`; plus a **default
  major-token allowlist** (V2/V3 require both coins; Curve requires all coins). Override via
  `token_allowlist`.
- `max_pools` caps per-platform pool count to bound graph size and RPC cost.

## Routing algorithm

- Maximize `∏ rate` ⟺ minimize `∑ (-ln(rate) + fee + liquidity)`.
- Coarse edge weight: `-ln(mid_price) + fee_penalty(kind) + λ·(amount_in/reserve_in)`; when
  `reserve_ratio_k > 0`, skip edges with `reserve_in < k·amount_in`.
- **K-best DP**: keep top-K paths per `(token, hop)`, bounded by `max_hops`, no repeated token.
- **Direct fallback**: direct pools are always included as candidates, so a coarse misrank cannot
  push the obviously-best direct route out.
- **Parallelism**: coarse and refine both use bounded/concurrent futures to hide RPC latency.
- Slippage is absorbed by refine; the response also returns `price_impact_bps`.

## Configuration (`config.yaml`)

| Key | Description | Default |
|---|---|---|
| `host` / `port` | bind address | `127.0.0.1` / `8080` |
| `rpc_url` | third-party RPC | `https://eth.drpc.org` |
| `uniswap_v2_subgraph_id` / `uniswap_v3_subgraph_id` | The Graph decentralized-network IDs | see example |
| `curve_address_provider` | Curve AddressProvider | `0x0000…4383` |
| `curve_registry_ids` | Curve registries to enumerate (0=main stableswap, 5=crypto, 7=MetaRegistry all) | `[0, 5]` |
| `enabled_platforms` | enabled platforms | v2/v3/curve |
| `max_hops` / `top_k` / `max_pools` | routing & scale | 3 / 8 / 100 |
| `min_liquidity` | liquidity floor (USD-ish) | 50000 |
| `min_volume_24h` | volume floor (uses subgraph `volumeUSD` as proxy) | 0 |
| `min_pool_age_days` | minimum pool age (days) | 0 |
| `reserve_ratio_k` | shallow-pool threshold `reserve_in ≥ k·amount_in` | 10 |
| `slippage_penalty_lambda` | liquidity penalty coefficient | 0.0 |
| `refresh_interval_secs` | refresh interval / mid-price TTL | 30 |
| `request_timeout_ms` | per-request timeout (ms) | 5000 |
| `token_allowlist` | override the default major-token allowlist | empty (built-in) |

`GRAPH_API_KEY` lives **only in the gitignored `.env`** — never committed.

## Testing

```bash
cargo test                       # unit tests (offline)
cargo test -- --ignored          # real mainnet integration tests (read-only eth_call)

cd ui && npm install && npm run build   # frontend build
```

- Unit: mock `QuoteSource` multigraph, routing optimality, negative-weight/simple-path, encoding stability.
- Mainnet: V2/V3/Curve real quotes, V2 vs V3 best venue, Curve 3pool, tricrypto2 crypto pool,
  Multicall3 hot refresh, end-to-end multi-hop (V2/V3 + Curve), subgraph discovery. The V2 adapter
  also has a **fixed-block `getReserves` fixture** for deterministic offline tests.

## Limitations

- Ethereum mainnet only; exact-in; single best path (no split routing, deferred to v2).
- Native ETH unsupported (ERC-20 / WETH only; wrap/unwrap is the caller's job).
- The Curve adapter supports both stableswap (`get_dy(int128,…)`) and crypto (`get_dy(uint256,…)`);
  **discovery** covers the main stableswap + crypto registries by default (`curve_registry_ids: [0, 5]`);
  factory/metapool can be included by adding `7` (MetaRegistry) to `curve_registry_ids`, at a
  significant startup cost.
- Public RPC may rate-limit; coarse uses an in-memory **price snapshot** (O(edges) but no RPC), so
  latency is dominated by **refine** (real on-chain quotes for candidates, ~0.5–1s depending on RPC).
- fee-on-transfer / rebasing tokens quote inaccurately → excluded via the allowlist.
- Public HTTP has no auth, only simple rate limiting.

## Docs

- [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md): **detailed architecture** (data flow, modules,
  the three cache layers, routing, RPC, API, concurrency, limitations).
- [`docs/REQUEST-TRACE.en.md`](docs/REQUEST-TRACE.en.md): a function-by-function trace of one `/quote` (Chinese: `docs/REQUEST-TRACE.md`).
- `docs/PLAN.md`: learning milestones; `docs/0X-*.md`: per-stage notes.
- `AGENTS.md`: repository engineering conventions.
