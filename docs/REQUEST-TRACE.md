# 一次 `/quote` 请求的逐函数调用栈

> 目标：把**一次询价**从 HTTP 入口到返回结果，按函数调用顺序拆开，看清每一层在做什么、
> 读写了什么数据、打了哪些 RPC。配合 `docs/ARCHITECTURE.md` 阅读。

示例请求（1 WETH → USDC）：

```bash
POST /quote
{"token_in":"0xC02a...","token_out":"0xA0b8...","amount_in":"1000000000000000000"}
```

---

## 0. 调用树（缩进 = 调用层级）

```
axum::serve
└─ api::quote(State<Arc<AppState>>, Json<QuoteRequestDto>)            [src/api.rs]
   ├─ RateLimiter::allow()                                            [src/api.rs]
   ├─ parse_address(&str) / parse_u256(&str)                          [src/api.rs]
   ├─ (构造 model::QuoteRequest)
   └─ Router::route(&graph, source, &request)                         [src/router.rs]
      ├─ (early) token_in==token_out → Route{echo}
      ├─ (early) amount_in==0 → Err(ZeroAmount)
      ├─ PriceSnapshot 读取：self.snapshot.read().clone()
      │  └─(未安装时) PriceSnapshot::build(graph, source)             [src/router.rs]
      │     ├─ (每边, buffer_unordered(16))
      │     │  ├─ CachingSource::mid_price(pool,i,j)                  [src/adapters.rs]
      │     │  │  └─ Dispatcher::mid_price → 适配器 → RPC
      │     │  └─ CachingSource::reserve_in(pool,i)
      │     │     └─ Dispatcher::reserve_in → 适配器 → RPC
      │     └─ 汇总成 per_edge: Vec<(mid_price, reserve_in)>
      ├─ 粗选：for edge → weight（纯内存，读 snapshot.per_edge）       [src/router.rs]
      │  └─ fee_penalty(pool.kind)                                   [src/router.rs]
      ├─ K-best DP：Graph::out_edge_indices/graph.edge/graph.pool     [src/graph.rs]
      │  └─ (简单路径、每节点 top_k 截断、收集到 token_out 的候选)
      ├─ 直连兜底：把 token_in→token_out 的直连边加入候选
      └─ 精算：futures::future::join_all(refine)                      [src/router.rs]
         └─(每条候选路径)
            └─ QuoteSource::amount_out(pool,i,j,amount)  (逐跳链式)
               └─ CachingSource::amount_out                        [src/adapters.rs]
                  └─ Dispatcher::amount_out → Dispatcher::source(kind)
                     ├─ V2: UniswapV2::amount_out                  [src/adapters.rs]
                     │  ├─ V2Reserves::get_reserves(pool)
                     │  │  └─ StateCache::get_reserves             [src/state.rs]
                     │  │     ├─(命中且未过期) 返回缓存储备
                     │  │     └─(未命中/过期) RpcClient::eth_call(getReserves 0x0902f1ac)
                     │  │        └─ RpcClient::post → hex_decode → decode_reserves
                     │  └─ source::constant_product_out(ri, rj, amount) [src/source.rs]
                     ├─ V3: UniswapV3::amount_out                  [src/adapters.rs]
                     │  └─ quote_exact_input_single
                     │     └─ RpcClient::eth_call(QuoterV2 0xc6a5026a) → decode word
                     └─ Curve: Curve::amount_out                   [src/adapters.rs]
                        └─ get_dy
                           └─ RpcClient::eth_call(get_dy int128 0x5e0d443f / uint256 0x556d6e9f)
      (回到 Router::route)
      ├─ 选 amount_out 最大的 Route
      ├─ Router::price_impact_bps(source, request, &route)            [src/router.rs]
      │  └─(每跳) QuoteSource::mid_price → 连乘 mid_rate → 与实际均价比较
      └─ amount_out_min 校验 → Ok(Route) 或 Err(AmountOutBelowMin)
   (回到 api::quote)
   ├─ route_to_dto(Route) → QuoteResponseDto                          [src/api.rs]
   ├─ tracing::info!("quote resolved")
   └─ Result<Json<QuoteResponseDto>, ApiError>
      └─(出错) ApiError::into_response() → 状态码 + JSON              [src/api.rs]
```

---

## 1. HTTP 入口

**`api::app(state)`**（`src/api.rs`）注册路由：`POST /quote → quote`、`GET /health`、`GET /ready`。
`axum::serve(listener, app)` 处理连接，反序列化 body 为 `QuoteRequestDto` 后进入
**`api::quote`**。

`QuoteRequestDto`：
```rust
{ token_in: String, token_out: String, amount_in: String,
  max_hops: Option<usize>, amount_out_min: Option<String> }
```

---

## 2. `api::quote`（`src/api.rs`）

1. **限流**：`state.limiter.allow()`，超限 → `ApiError::RateLimited`（429）。
2. **解析**：
   - `parse_address(&req.token_in/.token_out)` → `Address`（失败 400）
   - `parse_u256(&req.amount_in)` → `U256`（十进制，失败 400）
   - `amount_out_min` 可选解析
3. **构造** `model::QuoteRequest { token_in, token_out, amount_in, max_hops, amount_out_min }`。
4. **调用** `state.router.route(&state.graph, state.source.as_ref(), &request).await`。
5. 成功 → `route_to_dto`（遍历 hops，`hop_to_dto` 组装 `platform/pool/i/j/token_*`）→
   `tracing::info!` → `Json<QuoteResponseDto>`。
6. 失败 → `?` 转 `ApiError`，`IntoResponse` 映射状态码（见 §8）。

---

## 3. `Router::route`（`src/router.rs`）—— 核心

签名：`route(&self, graph: &Graph, source: &(dyn QuoteSource+Send+Sync), request) -> Result<Route>`。

### 3.1 早返回
- `token_in == token_out` → `Route { hops: [], amount_out: amount_in, price_impact_bps: 0 }`
- `amount_in == 0` → `Err(ZeroAmount)`

### 3.2 取价格快照
```rust
let installed = { self.snapshot.read().unwrap().clone() }; // 短锁，随即释放
let snapshot = match installed {
    Some(s) => s,
    None => Arc::new(PriceSnapshot::build(graph, source).await), // 测试/无后台时兜底
};
```
服务运行时快照由 `main` 启动构建 + 后台定时重建（§6），所以通常走 `Some` 分支。

### 3.3 粗选：算边权（纯内存）
对 `idx in 0..graph.edge_count()`：
```
ep = snapshot.per_edge[idx]                     // (mid_price: f64, reserve_in: U256)
if !ep.mid_price.is_finite() || ep.mid_price<=0 → w = ∞
w = -ln(mid_price) + fee_penalty(pool.kind)
if reserve_ratio_k>0 and ep.reserve_in < k*amount_in → w = ∞   // 浅池（仅 V2 有真实 reserve）
if λ>0 → w += λ * (amount_in / reserve_in)
```
`fee_penalty(kind)`：V2 0.3%、V3 `fee/1e6`、Curve ~4bp（`-ln(1-fee)`）。
> 此阶段**不发任何 RPC**。

### 3.4 K-best DP（限 hop、简单路径）
- `prev: HashMap<Address, Vec<Candidate>>`，起点 `token_in`，`cost=0`。
- 每跳：
  - 遍历 `graph.out_edge_indices(u)` 取出边；
  - `v = pool.coins[edge.j]`；若 `v` 已在 `visited` → 跳过（简单路径）；
  - `w = ∞` → 跳过；
  - 生成 `next`；放入 `curr[v]`；
  - 每个 `curr[v]` 按 cost 排序并 `truncate(top_k)`；
  - 若 `v == token_out`，把该候选收集进 `candidates`。
- 结束后 `candidates` 排序、再 `truncate(top_k)`。

### 3.5 直连兜底
把所有 `token_in → token_out` 的直连单边（若不在候选里）追加进 `candidates`，避免粗选把明显最优直连挤掉。

### 3.6 精算（并发）
```rust
let refine = candidates.iter().map(|cand| async move {
    let mut amount_in = request.amount_in;
    for &eidx in &cand.edges {
        let edge = graph.edge(eidx); let pool = graph.pool(edge.pool_idx);
        let out = source.amount_out(pool, edge.i, edge.j, amount_in).await?; // ← RPC 在适配器里
        // 记录 Hop，amount_in = out
    }
    Ok::<Route,_>(Route { hops, amount_out: amount_in, price_impact_bps: 0 })
});
// join_all 并发；取 amount_out 最大者
```
失败（`Err`）的候选被 `flatten()` 丢弃，不影响其它候选。

### 3.7 price impact + 兜底校验
- `price_impact_bps(source, request, &route)`：对最优路由的每跳取 `mid_price` 连乘得
  `mid_rate`，与实际 `amount_out/amount_in` 比较，`(1 - exec/mid)*10000`。
- `amount_out_min` 不足 → `Err(AmountOutBelowMin)`。

---

## 4. 报价源解析链：`source.amount_out(...)`

`source` 实际是 `Arc<CachingSource>`（包 `Dispatcher`）：

```
CachingSource::amount_out        → 直接委托（amount_out 不缓存）
  Dispatcher::amount_out         → source(pool.kind) 选适配器
    ├─ UniswapV2 / UniswapV3 / Curve::amount_out
```

### 4.1 UniswapV2
```
UniswapV2::amount_out
  → V2Reserves::get_reserves(pool)   // 实际是 StateCache
     → StateCache::get_reserves(pool):
        lock; 命中且未过期 → 返回 (零 RPC)
        否则 RpcClient::eth_call(pool, 0x0902f1ac getReserves) → decode_reserves
  → constant_product_out(ri, rj, amount)   // in*997*r_out/(r_in*1000+in*997)
```

### 4.2 UniswapV3
```
UniswapV3::amount_out
  → quote_exact_input_single(tokenIn,tokenOut,amount,fee)
     → RpcClient::eth_call(QuoterV2 0x61fF..., selector 0xc6a5026a, sqrtPriceLimit=0)
     → 解第一字 (amountOut)
```

### 4.3 Curve
```
Curve::amount_out → get_dy
  → 先 RpcClient::eth_call(pool, 0x5e0d443f get_dy(int128,..))
     失败(空/错误) → 0x556d6e9f get_dy(uint256,..)
  → 解第一字 (dy)
```

### 4.4 RPC 底层
```
RpcClient::eth_call(to, data)
  → 补 0x 前缀
  → RpcClient::post(json body)   // 3 次重试：429/rate/网络错误 → 退避 100/200/300ms
  → serde_json 解析 → result 字符串 → hex_decode → Vec<u8>
```

---

## 5. 返回与错误路径

成功：`Route` → `route_to_dto` → `QuoteResponseDto` →
```json
{"amount_out":"...","price_impact_bps":32,
 "path":[{"platform":"uniswap-v3","pool":"0x..","i":1,"j":0,
          "token_in":"0x..","token_out":"0x..","amount_in":"..","amount_out":".."}]}
```

失败：`QuoteError` → `ApiError`（`IntoResponse`）映射：

| QuoteError | HTTP |
|---|---|
| `NoPath` | 404 |
| `UnknownPool` | 404 |
| `AmountOutBelowMin` / `InsufficientLiquidity` / `Overflow` | 422 |
| `ZeroAmount` | 400 |
| `RpcError` | 502 |
| 解析失败 / 限流 | 400 / 429 |

---

## 6. 后台/启动期相关函数（理解快照从哪来）

启动（`main`）：
```
build_universe(config, rpc)
  ├─ discover_v2 / discover_v3       (subgraph, 需 GRAPH_API_KEY)
  ├─ discover_curve                  (AddressProvider→registry→pool_list→coins，get_dy 验证)
  ├─ Graph::add_pool × N             (展开所有有序对为边)
  └─ Dispatcher(V2←StateCache, V3, Curve) → CachingSource
PriceSnapshot::build(graph, cached)  // 逐边 mid_price/reserve（有界并发 16）
router.set_snapshot(Arc<snapshot>)
spawn 后台循环（每 refresh_interval_secs）:
  ├─ StateCache::refresh_hot()       // Multicall3 aggregate3 批量 V2 getReserves
  └─ PriceSnapshot::build(...) → router.set_snapshot(Arc::new(new))
```

---

## 7. 并发与锁（一次请求视角）

- 粗选：单线程同步循环（读不可变 `PriceSnapshot`，无锁竞争）。
- 精算：`join_all` 让所有候选的链式报价**并发**；单个候选内 hops 串行。
- 缓存：`StateCache`/`CachingSource` 用 `std::sync::Mutex`，都是「加锁查 → 放锁 → await →
  加锁写」的短临界区，**持锁不跨 await**。
- 快照：`RwLock<Option<Arc<PriceSnapshot>>>`，请求开始克隆 `Arc` 后即放锁。

---

## 8. 延迟构成（实测，公共 RPC）

| 阶段 | 量级 | 说明 |
|---|---|---|
| HTTP 解析 + 限流 | ~0ms | |
| 粗选（读快照） | **~0ms** | 纯内存 |
| K-best DP | <1ms | 图规模小 |
| 精算（并发 live 报价） | **~0.6–0.9s** | 被**最慢的一次** `amount_out` 拖住 |
| price_impact（读 mid_price 缓存） | ~0ms–几十 ms | 命中缓存时几乎为 0 |
| **合计** | **~0.8s** | 主要受 RPC 波动影响 |

---

## 9. 一个具体数值示例（WETH→USDC, 1 WETH）

1. 候选里通常有：V3 USDC/WETH(0.05%)、V2 USDC/WETH、以及若干 2 跳。
2. 精算并发执行，各自拿到真实 `amount_out`；V3 0.05% 池一般最优。
3. 结果示例：`amount_out=2331667580`（≈2331.67 USDC），`price_impact_bps≈5`，
   `path=[{platform:"uniswap-v3", pool:"0xE0554…", i:1, j:0, ...}]`。

---

## 10. 想改代码时看哪

| 想改 | 位置 |
|---|---|
| 入参/出参/错误码/限流 | `api.rs` |
| 路由策略/权重/候选 | `router.rs` |
| 平台报价实现/选择子 | `adapters.rs` |
| V2 储备刷新策略 | `state.rs` |
| 池子发现/过滤 | `discovery.rs` |
| RPC 行为/重试 | `rpc.rs` |
