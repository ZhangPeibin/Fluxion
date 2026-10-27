# Function-by-Function Call Stack of One `/quote` Request

English | [中文](REQUEST-TRACE.md)

> Goal: take **one quote request** from the HTTP entry point to the response and break it down
> in call order, showing what each layer does, what data it reads/writes, and which RPCs it makes.
> Read alongside `docs/ARCHITECTURE.en.md`.

Example request (1 WETH → USDC):

```bash
POST /quote
{"token_in":"0xC02a...","token_out":"0xA0b8...","amount_in":"1000000000000000000"}
```

---

## 0. Call tree (indentation = call depth)

```
axum::serve
└─ api::quote(State<Arc<AppState>>, Json<QuoteRequestDto>)            [src/api.rs]
   ├─ RateLimiter::allow()                                            [src/api.rs]
   ├─ parse_address(&str) / parse_u256(&str)                          [src/api.rs]
   ├─ (build model::QuoteRequest)
   └─ Router::route(&graph, source, &request)                         [src/router.rs]
      ├─ (early) token_in==token_out → Route{echo}
      ├─ (early) amount_in==0 → Err(ZeroAmount)
      ├─ read PriceSnapshot: self.snapshot.read().clone()
      │  └─(if not installed) PriceSnapshot::build(graph, source)     [src/router.rs]
      │     ├─ (per edge, buffer_unordered(16))
      │     │  ├─ CachingSource::mid_price(pool,i,j)                  [src/adapters.rs]
      │     │  │  └─ Dispatcher::mid_price → adapter → RPC
      │     │  └─ CachingSource::reserve_in(pool,i)
      │     │     └─ Dispatcher::reserve_in → adapter → RPC
      │     └─ assemble per_edge: Vec<(mid_price, reserve_in)>
      ├─ coarse: for edge → weight (pure memory, reads snapshot.per_edge) [src/router.rs]
      │  └─ fee_penalty(pool.kind)                                   [src/router.rs]
      ├─ K-best DP: Graph::out_edge_indices/graph.edge/graph.pool     [src/graph.rs]
      │  └─ (simple path, top_k per node, collect candidates reaching token_out)
      ├─ direct fallback: add token_in→token_out direct edges to candidates
      └─ refine: futures::future::join_all(refine)                    [src/router.rs]
         └─(per candidate path)
            └─ QuoteSource::amount_out(pool,i,j,amount)  (chained hop by hop)
               └─ CachingSource::amount_out                        [src/adapters.rs]
                  └─ Dispatcher::amount_out → Dispatcher::source(kind)
                     ├─ V2: UniswapV2::amount_out                  [src/adapters.rs]
                     │  ├─ V2Reserves::get_reserves(pool)
                     │  │  └─ StateCache::get_reserves             [src/state.rs]
                     │  │     ├─(hit & fresh) return cached reserves
                     │  │     └─(miss/stale) RpcClient::eth_call(getReserves 0x0902f1ac)
                     │  │        └─ RpcClient::post → hex_decode → decode_reserves
                     │  └─ source::constant_product_out(ri, rj, amount) [src/source.rs]
                     ├─ V3: UniswapV3::amount_out                  [src/adapters.rs]
                     │  └─ quote_exact_input_single
                     │     └─ RpcClient::eth_call(QuoterV2 0xc6a5026a) → decode word
                     └─ Curve: Curve::amount_out                   [src/adapters.rs]
                        └─ get_dy
                           └─ RpcClient::eth_call(get_dy int128 0x5e0d443f / uint256 0x556d6e9f)
      (back in Router::route)
      ├─ pick the Route with the largest amount_out
      ├─ Router::price_impact_bps(source, request, &route)            [src/router.rs]
      │  └─(per hop) QuoteSource::mid_price → multiply mid_rate → compare with exec price
      └─ amount_out_min check → Ok(Route) or Err(AmountOutBelowMin)
   (back in api::quote)
   ├─ route_to_dto(Route) → QuoteResponseDto                          [src/api.rs]
   ├─ tracing::info!("quote resolved")
   └─ Result<Json<QuoteResponseDto>, ApiError>
      └─(on error) ApiError::into_response() → status + JSON          [src/api.rs]
```

---

## 1. HTTP entry

**`api::app(state)`** (`src/api.rs`) registers routes: `POST /quote → quote`, `GET /health`,
`GET /ready`. `axum::serve(listener, app)` handles connections, deserializes the body into
`QuoteRequestDto`, and enters **`api::quote`**.

`QuoteRequestDto`:
```rust
{ token_in: String, token_out: String, amount_in: String,
  max_hops: Option<usize>, amount_out_min: Option<String> }
```

---

## 2. `api::quote` (`src/api.rs`)

1. **Rate limit**: `state.limiter.allow()`; over limit → `ApiError::RateLimited` (429).
2. **Parse**:
   - `parse_address(&req.token_in/.token_out)` → `Address` (400 on failure)
   - `parse_u256(&req.amount_in)` → `U256` (decimal, 400 on failure)
   - `amount_out_min` parsed if present
3. **Build** `model::QuoteRequest { token_in, token_out, amount_in, max_hops, amount_out_min }`.
4. **Call** `state.router.route(&state.graph, state.source.as_ref(), &request).await`.
5. Success → `route_to_dto` (iterate hops, `hop_to_dto` assembles `platform/pool/i/j/token_*`) →
   `tracing::info!` → `Json<QuoteResponseDto>`.
6. Failure → `?` converts to `ApiError`; `IntoResponse` maps to status (see §5).

---

## 3. `Router::route` (`src/router.rs`) — the core

Signature: `route(&self, graph: &Graph, source: &(dyn QuoteSource+Send+Sync), request) -> Result<Route>`.

### 3.1 Early returns
- `token_in == token_out` → `Route { hops: [], amount_out: amount_in, price_impact_bps: 0 }`
- `amount_in == 0` → `Err(ZeroAmount)`

### 3.2 Get the price snapshot
```rust
let installed = { self.snapshot.read().unwrap().clone() }; // short lock, released immediately
let snapshot = match installed {
    Some(s) => s,
    None => Arc::new(PriceSnapshot::build(graph, source).await), // fallback for tests/no background
};
```
At runtime the snapshot is built at startup and rebuilt by the background task (§6), so it usually
takes the `Some` branch.

### 3.3 Coarse: compute edge weights (pure memory)
For `idx in 0..graph.edge_count()`:
```
ep = snapshot.per_edge[idx]                     // (mid_price: f64, reserve_in: U256)
if !ep.mid_price.is_finite() || ep.mid_price<=0 → w = ∞
w = -ln(mid_price) + fee_penalty(pool.kind)
if reserve_ratio_k>0 and ep.reserve_in < k*amount_in → w = ∞   // shallow (only V2 has a real reserve)
if λ>0 → w += λ * (amount_in / reserve_in)
```
`fee_penalty(kind)`: V2 0.3%, V3 `fee/1e6`, Curve ~4bp (`-ln(1-fee)`).
> This stage makes **no RPC calls**.

### 3.4 K-best DP (bounded hops, simple path)
- `prev: HashMap<Address, Vec<Candidate>>`, start at `token_in`, `cost=0`.
- Each hop:
  - Iterate `graph.out_edge_indices(u)` to get edges;
  - `v = pool.coins[edge.j]`; if `v` is already in `visited` → skip (simple path);
  - Skip if `w == ∞`;
  - Build `next`; insert into `curr[v]`;
  - Sort each `curr[v]` by cost and `truncate(top_k)`;
  - If `v == token_out`, collect the candidate into `candidates`.
- After all hops, sort `candidates` and `truncate(top_k)` again.

### 3.5 Direct fallback
Append all direct single edges `token_in → token_out` (if not already a candidate) to `candidates`,
so a coarse misrank can't drop the obviously-best direct route.

### 3.6 Refine (concurrent)
```rust
let refine = candidates.iter().map(|cand| async move {
    let mut amount_in = request.amount_in;
    for &eidx in &cand.edges {
        let edge = graph.edge(eidx); let pool = graph.pool(edge.pool_idx);
        let out = source.amount_out(pool, edge.i, edge.j, amount_in).await?; // ← RPC lives in the adapter
        // record Hop, amount_in = out
    }
    Ok::<Route,_>(Route { hops, amount_out: amount_in, price_impact_bps: 0 })
});
// join_all runs them concurrently; pick the largest amount_out
```
Candidates that return `Err` are dropped via `flatten()` and don't affect the others.

### 3.7 Price impact + final checks
- `price_impact_bps(source, request, &route)`: multiply `mid_price` across hops to get `mid_rate`,
  compare with the actual `amount_out/amount_in`: `(1 - exec/mid)*10000`.
- `amount_out_min` too low → `Err(AmountOutBelowMin)`.

---

## 4. Quote source resolution: `source.amount_out(...)`

`source` is actually `Arc<CachingSource>` (wrapping `Dispatcher`):

```
CachingSource::amount_out        → delegates directly (amount_out is not cached)
  Dispatcher::amount_out         → source(pool.kind) picks the adapter
    ├─ UniswapV2 / UniswapV3 / Curve::amount_out
```

### 4.1 UniswapV2
```
UniswapV2::amount_out
  → V2Reserves::get_reserves(pool)   // really StateCache
     → StateCache::get_reserves(pool):
        lock; fresh hit → return (zero RPC)
        else RpcClient::eth_call(pool, 0x0902f1ac getReserves) → decode_reserves
  → constant_product_out(ri, rj, amount)   // in*997*r_out/(r_in*1000+in*997)
```

### 4.2 UniswapV3
```
UniswapV3::amount_out
  → quote_exact_input_single(tokenIn,tokenOut,amount,fee)
     → RpcClient::eth_call(QuoterV2 0x61fF..., selector 0xc6a5026a, sqrtPriceLimit=0)
     → decode first word (amountOut)
```

### 4.3 Curve
```
Curve::amount_out → get_dy
  → RpcClient::eth_call(pool, 0x5e0d443f get_dy(int128,..))
     failure(empty/error) → 0x556d6e9f get_dy(uint256,..)
  → decode first word (dy)
```

### 4.4 RPC internals
```
RpcClient::eth_call(to, data)
  → prefix 0x
  → RpcClient::post(json body)   // 3 retries: 429/rate/network → backoff 100/200/300ms
  → serde_json parse → result string → hex_decode → Vec<u8>
```

---

## 5. Response and error path

Success: `Route` → `route_to_dto` → `QuoteResponseDto` →
```json
{"amount_out":"...","price_impact_bps":32,
 "path":[{"platform":"uniswap-v3","pool":"0x..","i":1,"j":0,
          "token_in":"0x..","token_out":"0x..","amount_in":"..","amount_out":".."}]}
```

Failure: `QuoteError` → `ApiError` (`IntoResponse`) mapping:

| QuoteError | HTTP |
|---|---|
| `NoPath` | 404 |
| `UnknownPool` | 404 |
| `AmountOutBelowMin` / `InsufficientLiquidity` / `Overflow` | 422 |
| `ZeroAmount` | 400 |
| `RpcError` | 502 |
| Parse failure / rate limit | 400 / 429 |

---

## 6. Background/startup functions (where the snapshot comes from)

Startup (`main`):
```
build_universe(config, rpc)
  ├─ discover_v2 / discover_v3       (subgraph, needs GRAPH_API_KEY)
  ├─ discover_curve                  (AddressProvider→registry→pool_list→coins, get_dy validation)
  ├─ Graph::add_pool × N             (expand all ordered pairs into edges)
  └─ Dispatcher(V2←StateCache, V3, Curve) → CachingSource
PriceSnapshot::build(graph, cached)  // per-edge mid_price/reserve (bounded concurrency 16)
router.set_snapshot(Arc<snapshot>)
spawn background loop (every refresh_interval_secs):
  ├─ StateCache::refresh_hot()       // Multicall3 aggregate3 batch V2 getReserves
  └─ PriceSnapshot::build(...) → router.set_snapshot(Arc::new(new))
```

---

## 7. Concurrency & locks (from one request's view)

- Coarse: single-threaded synchronous loop (reads the immutable `PriceSnapshot`; no lock contention).
- Refine: `join_all` runs all candidates' chained quotes **concurrently**; hops within a candidate
  are sequential.
- Caches: `StateCache`/`CachingSource` use `std::sync::Mutex` with very short critical sections
  ("lock-read → unlock → await → lock-write"), **never holding the lock across `await`**.
- Snapshot: `RwLock<Option<Arc<PriceSnapshot>>>`; a request clones the `Arc` and releases the lock.

---

## 8. Latency breakdown (measured, public RPC)

| Stage | Scale | Notes |
|---|---|---|
| HTTP parse + rate limit | ~0ms | |
| Coarse (read snapshot) | **~0ms** | pure memory |
| K-best DP | <1ms | small graph |
| Refine (concurrent live quotes) | **~0.6–0.9s** | bounded by the **slowest** `amount_out` |
| price_impact (reads mid_price cache) | ~0–tens of ms | near 0 on cache hit |
| **Total** | **~0.8s** | mostly RPC variance |

---

## 9. A concrete numeric example (WETH→USDC, 1 WETH)

1. Candidates typically include: V3 USDC/WETH (0.05%), V2 USDC/WETH, and several 2-hop paths.
2. Refine runs concurrently; each gets a real `amount_out`; the V3 0.05% pool usually wins.
3. Example result: `amount_out=2331667580` (~2331.67 USDC), `price_impact_bps≈5`,
   `path=[{platform:"uniswap-v3", pool:"0xE0554…", i:1, j:0, ...}]`.

---

## 10. Where to change what

| Want to change | Location |
|---|---|
| Request/response DTOs, error codes, rate limiting | `api.rs` |
| Routing strategy/weights/candidates | `router.rs` |
| Platform quote implementation/selectors | `adapters.rs` |
| V2 reserve refresh strategy | `state.rs` |
| Pool discovery/filtering | `discovery.rs` |
| RPC behavior/retries | `rpc.rs` |
