# Fluxion

Ethereum 主网多跳路由询价（RFQ）HTTP 服务。给定 `tokenIn` / `tokenOut` / `amountIn`，
在 Uniswap V2 / V3 / Curve 构成的有向多重图上搜索最优路由（支持多跳），返回最优
`amountOut` 与完整路径。

> 本项目同时是学习项目：目标不只是交付服务，更是带你**掌握** DEX 路由询价的原理与实现。
> 见 [docs/PLAN.md](docs/PLAN.md) 的教学版里程碑，以及 `docs/0X-*.md` 的逐阶段学习笔记。

## 范围（v1）

- 链：Ethereum 主网
- 形态：HTTP 服务（REST）
- 平台：Uniswap V2、Uniswap V3、Curve
- 路径：单条最优路径（不做拆分路由）
- 金额语义：exact-in（给定输入求最大输出）
- 数据源：第三方 RPC + 官方 Graph 托管 subgraph + Curve registry（不自建 indexer）

## 架构

```
                 ┌─────────────── 后台任务 ───────────────┐
                 │  池子发现(启动+定期)  →  热池储备刷新(multicall)    │
                 └──────────────┬─────────────────────────┘
                                ▼
  请求 ──► axum API ──► Router ──► Graph(内存, mid-price) 粗选路径
                          │
                          ▼
                   top-K 路径 exact quote (实时 RPC)
                          │
                          ▼
                 最优路径 + amountOut ──► 响应
```

两层定价，职责分离：

- **粗选层**：内存多重图存 mid-price（缓存态），跑路径搜索，不依赖请求时 RPC。
- **精算层**：对粗选出的 top-K 路径，实时调链上 quote 函数，逐跳算精确 `amountOut`。

## 技术栈

- Rust（edition 2024）
- `tokio`：异步运行时
- `alloy`：以太坊交互（provider、contract、Multicall）
- `axum`：HTTP 服务
- `serde` / `serde_json`：序列化
- `thiserror`：错误定义
- `tracing` / `tracing-subscriber`：结构化日志
- 金额：`alloy_primitives::{U256, I256}`

## 模块结构

```
src/
  model/       Token / Pool / PoolKind / Edge / Hop / Route / QuoteRequest
  graph/       自定义邻接表多重图（HashMap + 出边列表，支持平行边）
  discovery/   subgraph(V2/V3) + Curve registry 预载 + 池子过滤 + JSON 落盘
  source/      QuoteSource trait + V2/V3/Curve 适配器
  state/       Multicall3 批量刷新 + 热池集合 + TTL 缓存（mid-price 用）
  router/      K-best DP(限 hop + 简单路径) + 流动性惩罚 + top-K 精算
  api/         axum POST /quote
  config.rs    配置
  main.rs      入口
```

## 数据模型

```rust
struct Token { address: Address, symbol: String, decimals: u8 }

enum PoolKind {
    UniswapV2,
    UniswapV3 { fee: u32 },
    Curve { pool_type: CurvePoolType },   // stableswap / crypto / metapool
}

struct Pool {
    kind: PoolKind,
    address: Address,
    coins: Vec<Address>,   // 支持多币池（3pool: DAI/USDC/USDT）
}

// 有向边 = 池子 + 币索引对
struct Edge { pool_idx: usize, i: usize, j: usize }   // i 卖出币索引, j 买入币索引

struct Hop {
    pool: Pool, i: usize, j: usize,
    token_in: Address, token_out: Address,
    amount_in: U256, amount_out: U256,
}

struct Route { hops: Vec<Hop>, amount_out: U256 }
```

**多重图本质**：同一 token 对在多平台各有一条边 = 平行边。

**多币池展开**：建图时对每个池子的 `coins` 生成所有有序对 (i, j)，i≠j：

- V2/V3：2 币 → 2 条边
- Curve 3pool：3 币 → 6 条边
- 4 币池 → 12 条边

金额按各 token 自身 decimals 在边内处理，路径计算直接链式传递 `amount_out`。

## 池子筛选策略

进图前多层过滤，阈值全部配置化：

| 维度 | 规则 | 目的 |
|---|---|---|
| Token 白名单 | 仅收录官方 tokenlist/subgraph 验证 token | 挡 scam/honeypot |
| 流动性下限 | TVL/reserve 低于 `min_liquidity` 丢弃 | 挡灰尘池 |
| 成交量 | 24h volume 低于 `min_volume_24h` 降级 | 挡僵尸池 |
| 池龄 | 新建（< `min_pool_age`）不进图 | 挡短命池 |
| 动态判断 | 询价时跳过 `reserve_in < k * amount_in`（k=`reserve_ratio_k`） | 挡大额打穿 |

无交易的池不硬剔除：储备仍是有效的，真实风险是浅深度与高滑点，故用
「volume 降级 + 流动性硬门槛」处理。

## 报价适配器

```rust
trait QuoteSource {
    async fn amount_out(&self, pool: &Pool, i: usize, j: usize, amount_in: U256) -> Result<U256>;
}
```

- **Uniswap V2**：常数积 `x*y=k`。mid-price 读 `getReserves()`；exact 用
  `amount_in*997*r_out/(r_in*1000 + amount_in*997)`。
- **Uniswap V3**：集中流动性，用 QuoterV2 `quoteExactInputSingle`（自动处理 fee 与
  tick 跨越）；mid-price 由缓存 `slot0.sqrtPriceX96` 换算。
- **Curve**：按池类型分派——stableswap `get_dy`、crypto pool `get_dy`、metapool
  `get_dy_underlying`；注意不同池 `get_dy` 参数类型（`int128` vs `uint256`）。
  mid-price 用小额 `dy/dx`。

## 状态刷新（热池策略）

- 不做全量刷新（V3 上万池子不可行）。
- 维护**热池集合**：被近期询价涉及的 token 邻域池子纳入热池，按 `refresh_interval`
  （默认 3s）用 Multicall3 批量刷新其储备/slot0。
- **冷池懒加载**：查询到未加载的池子时按需拉取一次，再转热池。
- 缓存供粗选 mid-price 使用；仅 top-K 精算时实时打 RPC。

## 路由算法

最大化 `∏ rate` ⟺ 最小化 `∑ -ln(rate)`。因 `-ln(rate)` 可为负（rate>1），不能用
Dijkstra。

1. 用缓存 mid-price 算边权，含流动性惩罚：
   `w = -ln(r_out/r_in) + λ * (amount_in/reserve_in)`（λ=`slippage_penalty_lambda`）
2. **K-best DP**：按 `(token, hop)` 维护 K 条最优路径，限 `max_hops`（默认 3）轮松弛，
   保留 top-K 候选。
3. **简单路径约束**：禁止重复访问 token，避免负权下 A→B→A 自循环。
4. 逐跳链式精算：`out1 = amount_out(edge1, in)`，`out2 = amount_out(edge2, out1)` …
5. 返回 `amount_out` 最大者。

滑点天然被精确 quote 函数吸收；响应额外返回 `priceImpactBps` 供上游滑点保护。

## HTTP API

`POST /quote`

```json
{ "tokenIn": "0x..", "tokenOut": "0x..", "amountIn": "1000000000000000000",
  "maxHops": 3, "amountOutMin": "0" }
```

响应：

```json
{
  "amountOut": "123456",
  "priceImpactBps": 42,
  "path": [
    { "platform": "UniswapV3", "pool": "0x..", "i": 0, "j": 1,
      "tokenIn": "0x..", "tokenOut": "0x..", "amountIn": "..", "amountOut": ".." }
  ]
}
```

`GET /health` 探活。

## 错误处理

`thiserror` 枚举，映射 HTTP 状态码：

- `RpcError`
- `PoolNotFound`
- `NoPathFound`
- `InsufficientLiquidity`
- `AmountOutBelowMin`

## 配置

| 键 | 说明 |
|---|---|
| `RPC_URL` | 第三方 RPC 端点 |
| `UNISWAP_V2_SUBGRAPH` / `UNISWAP_V3_SUBGRAPH` | 官方 subgraph 端点（可能需 Graph API key） |
| `CURVE_ADDRESS_PROVIDER` | Curve AddressProvider（`0x0000000022D53366457F9d5E68Ec105046FC4383`），registry 由其 `get_registry()` 取得 |
| `ENABLED_PLATFORMS` | 启用的平台 |
| `MAX_HOPS` | 默认最大跳数（3，指边数） |
| `REFRESH_INTERVAL` | 热池刷新间隔（3s） |
| `MIN_LIQUIDITY` | 流动性下限 |
| `MIN_VOLUME_24H` | 24h 成交下限 |
| `MIN_POOL_AGE` | 最小池龄 |
| `RESERVE_RATIO_K` | 动态 reserve 比例（10） |
| `SLIPPAGE_PENALTY_LAMBDA` | 流动性惩罚系数 |
| `TOKEN_ALLOWLIST` | token 白名单来源 |

## 适配器 / RPC 注意事项

- **subgraph**：Graph 托管端点正在迁移，可能需 API key / 新 gateway；池子列表必须**分页**拉取。
- **Curve**：`0x0000...4383` 是 AddressProvider 而非 registry；池类型/`get_dy` 签名差异需分派。
- **QuoterV2**：`eth_call` 较重，top-K 精算每请求约 `K × hops` 次调用；加**短 TTL quote 缓存** + 相同请求合并，避免 RPC 限流。

## 边界与限制（v1）

- 仅接受 ERC-20 / WETH 地址；**原生 ETH 不支持**（wrap/unwrap 由上游处理）。
- fee-on-transfer / rebasing token 报价会失真 → 白名单排除。
- `tokenIn == tokenOut` 直接返回输入金额；`amountIn == 0` 视为非法。
- `maxHops` 指**边数**；`priceImpactBps` 单位为基点。
- 不做 split routing（多路径拆分留 v2）。
- 公开 HTTP 无鉴权 → 至少加简单限流。

## 开发计划

开发优先：先用 mock 打通端到端，再换真实数据源。教学版里程碑（含学习目标与检查点）见
[docs/PLAN.md](docs/PLAN.md)。

### M1 · 离线引擎（不依赖网络）

- 领域模型 + 多重图（`Pool.coins`、平行边、有序对展开）
- `QuoteSource` trait + mock 实现（V2 常数积、固定汇率）
- K-best DP 多跳路由 + 简单路径 + 流动性惩罚
- 单元测试：直接询价、多跳择优、负权/套环反例
- 验收：`cargo test` 全绿

### M2 · 接真实数据（需 RPC + Graph）

- 池子发现：subgraph 分页 + 过滤 + JSON 落盘
- 真实适配器：V2 → V3 → Curve
- 热池 multicall 刷新 + 冷池懒加载
- 验收：真实 RPC 上单跳、多跳报价正确

### M3 · 服务化

- axum `/quote` + `/health` + 错误映射 + 限流
- 集成测试（anvil fork）+ tracing
- 验收：`curl /quote` 拿到正确报价

## 测试

- 单元：mock `QuoteSource` 构造含负权/套环的小多重图，断言最优路径。
- 集成：`anvil` fork 主网，跑真实 V2/V3/Curve 询价。

## 已定决策

- 单条最优路径（不做 split routing，留 v2）
- exact-in 金额语义
- 无交易池：降级不剔除
- 流动性阈值：静态下限 + 动态 `reserve_in ≥ k·amount_in`
- 第三方 RPC + 官方 subgraph，不自建 indexer
- 多币池：`Pool.coins: Vec`，按所有有序对展开边
- 刷新：热池集合 + 冷池懒加载，不全量
- 路由：K-best DP + 简单路径约束
