# Fluxion

[English](README.en.md) | 中文

Ethereum 主网多跳路由询价（RFQ）HTTP 服务。给定 `token_in` / `token_out` / `amount_in`，
在 Uniswap V2 / V3 / Curve 构成的**有向多重图**上搜索最优路由（支持多跳），返回最优
`amount_out` 与完整路径（exact-in）。

> 开源、自托管：下载后用一份 YAML 配置 + 你自己的 RPC / Graph key 跑起来。

## 快速开始

```bash
# 1) 密钥（gitignored）—— .env 只放 GRAPH_API_KEY
cp .env.example .env
#   编辑 .env，填入 GRAPH_API_KEY（The Graph Studio 免费创建）

# 2) 配置（可选；缺省用内置默认）—— RPC/阈值/平台/host 等都在这里
cp config.example.yaml config.yaml

# 3) 运行（默认监听 127.0.0.1:8080）
cargo run
```

> 分工：**`.env` 只放密钥**（`GRAPH_API_KEY`）；**其余所有配置走 `config.yaml`**。

启动时会从 subgraph / 链上注册表发现池子、预热 mid-price 缓存，然后开始服务。
（没有 `GRAPH_API_KEY` 时会退回到内置的 mock universe，便于离线开发。）

```bash
curl -s localhost:8080/health   # {"status":"ok"}
curl -s localhost:8080/ready    # 探测上游 RPC；不通返回 503

curl -s -X POST localhost:8080/quote \
  -H 'content-type: application/json' \
  -d '{"token_in":"0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
       "token_out":"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
       "amount_in":"1000000000000000000"}'
```

## API

### `POST /quote`

请求：

```json
{
  "token_in": "0xC02a...",
  "token_out": "0xA0b8...",
  "amount_in": "1000000000000000000",
  "max_hops": 3,
  "amount_out_min": "0"
}
```

- 金额均为**十进制字符串**（`U256`）。
- `max_hops` 可选，指**边数**（默认取配置）。
- `amount_out_min` 可选，结果低于它时返回 `AmountOutBelowMin`。

响应（`200`）：

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

### `GET /health` / `GET /ready`

- `/health`：进程存活。
- `/ready`：对上游 RPC 做一次 `eth_blockNumber`，不通返回 `503`。

### 错误

`QuoteError` → HTTP 状态：`NoPathFound`(404)、`AmountOutBelowMin`/`InsufficientLiquidity`/
`Overflow`(422)、`ZeroAmount`(400)、`PoolNotFound`(404)、`RpcError`(502)、限流(429)。

## 架构

```
请求 → axum /quote → Router
                        ├─ 粗选：读缓存 mid-price 算边权 → K-best DP（限 hop、简单路径、含费用/流动性惩罚）→ top-K 候选
                        └─ 精算：对候选路径并行链式调用链上 quote，取 amount_out 最大者
后台：启动发现池子 + 预热 mid-price；定期刷新热池储备（V2 走 Multicall3）
```

两层定价：**粗选**用缓存 mid-price 做快速路径搜索；**精算**只对少量候选用链上真实
`amount_out`（含滑点、费用）定胜负。

技术栈：

- Rust（edition 2024）、`tokio`、`axum`、`serde`、`thiserror`、`tracing`
- `alloy-primitives`：`U256` / `Address`
- 链交互：**原始 JSON-RPC（`reqwest`）**，手写 ABI 编码/解码
  （`eth_call getReserves` / QuoterV2 / Curve `get_dy` / Multicall3 `aggregate3`）

## 模块结构

```
src/
  model.rs       Token / Pool / PoolKind / Edge / Hop / Route / QuoteRequest
  graph.rs       自定义邻接表多重图（平行边；多币池展开为所有有序对）
  discovery.rs   V2/V3 subgraph 发现 + Curve 主注册表发现 + 过滤
  source.rs      QuoteSource trait + mock 实现
  adapters.rs    真实适配器（V2/V3/Curve）+ Dispatcher + CachingSource
  state.rs       StateCache：V2 储备 TTL 缓存 + Multicall3 热池刷新
  rpc.rs         原始 JSON-RPC 客户端（含 429/瞬时错误重试退避）
  router.rs      K-best DP 粗选 + 并行精算 + 价格冲击
  api.rs         axum（/quote、/health、/ready）、错误映射、限流
  config.rs      YAML 配置
  util.rs        ABI 编码、hex、数值转换
  main.rs        入口：发现 → 建图 → 预热 → 服务
```

## 数据模型

- `Token { address, symbol, decimals }`
- `Pool { kind: PoolKind, address, coins: Vec<Address> }`
  - `PoolKind = UniswapV2 | UniswapV3 { fee } | Curve`
- `Edge { pool_idx, i, j }`（有向：卖出币索引 `i` → 买入币索引 `j`）
- 同一 token 对在多平台 = **平行边**；多币池（如 Curve 3pool）展开为所有有序对：
  3 币 → 6 条边。

## 池子发现与过滤

- **Uniswap V2**：V2 subgraph，按 `reserveUSD` 排序、分页。
- **Uniswap V3**：V3 subgraph，按 `volumeUSD` 排序、分页（注意：按 TVL 排序会先冒出
  TVL 虚高的垃圾池）。
- **Curve**：主 stableswap 注册表
  `AddressProvider.get_registry() → pool_count()/pool_list(i) → coins(i)`，
  并用一次 `get_dy` 探测验证「我们的适配器能报价」。
- **过滤**：`min_liquidity`、`min_volume_24h`、`min_pool_age_days`；以及**默认主流币
  白名单**（V2/V3 要求池子两币都在白名单；Curve 要求所有币都在）。白名单可用
  `token_allowlist` 覆盖。
- `max_pools` 限制每类平台的池子数，控制图规模与 RPC 压力。

## 路由算法

- 最大化 `∏ rate` ⟺ 最小化 `∑ (-ln(rate) + fee + liquidity)`。
- 粗选边权：`-ln(mid_price) + fee_penalty(kind) + λ·(amount_in/reserve_in)`；
  `reserve_ratio_k > 0` 时跳过 `reserve_in < k·amount_in` 的浅边。
- **K-best DP**：按 `(token, hop)` 保留 top-K 路径，限 `max_hops`，禁止重复 token。
- **直连兜底**：直连池始终纳入候选，避免粗选把明显最优的直连路由挤掉。
- **并行**：粗选与精算均用有界/并发 futures，隐藏 RPC 延迟。
- 滑点被精算吸收；响应额外返回 `price_impact_bps`。

## 配置（`config.yaml`）

| 键 | 说明 | 默认 |
|---|---|---|
| `host` / `port` | 监听地址 | `127.0.0.1` / `8080` |
| `rpc_url` | 第三方 RPC | `https://eth.drpc.org` |
| `uniswap_v2_subgraph_id` / `uniswap_v3_subgraph_id` | The Graph 去中心化网络部署 ID | 见示例 |
| `curve_address_provider` | Curve AddressProvider | `0x0000…4383` |
| `enabled_platforms` | 启用的平台 | v2/v3/curve |
| `max_hops` / `top_k` / `max_pools` | 路由与规模 | 3 / 8 / 100 |
| `min_liquidity` | 流动性下限（USD 量级） | 50000 |
| `min_volume_24h` | 成交量下限（用 subgraph `volumeUSD` 作代理） | 0 |
| `min_pool_age_days` | 最小池龄（天） | 0 |
| `reserve_ratio_k` | 动态浅池阈值 `reserve_in ≥ k·amount_in` | 10 |
| `slippage_penalty_lambda` | 流动性惩罚系数 | 0.0 |
| `refresh_interval_secs` | 刷新间隔 / mid-price TTL | 30 |
| `token_allowlist` | 覆盖默认主流币白名单 | 空（用内置） |

`GRAPH_API_KEY` 只放在 **gitignored 的 `.env`** 里，不进版本库。

## 测试

```bash
cargo test                       # 单元测试（离线）
cargo test -- --ignored          # 真实主网集成测试（只读 eth_call）
```

- 单元：mock `QuoteSource` 的多重图、路由择优、负权/简单路径、编码稳定性。
- 主网：V2/V3/Curve 真实报价、V2 vs V3 择优、Curve 3pool、Multicall3 热刷新、
  端到端多跳（V2/V3 + Curve）、subgraph 发现。V2 适配器另有**固定区块 `getReserves`
  fixture** 做不联网的确定性测试。

## 边界与限制

- 仅 Ethereum 主网；exact-in；单条最优路径（不做 split routing，留 v2）。
- 原生 ETH 不支持（仅 ERC-20 / WETH；wrap/unwrap 由上游处理）。
- Curve 适配器同时支持 stableswap（`get_dy(int128,…)`）与 crypto（`get_dy(uint256,…)`）；
  但**池子发现**仍只覆盖主 stableswap 注册表，crypto(tricrypto) / factory / metapool
  的注册表枚举尚未纳入。
- 依赖公共 RPC 时可能遇到限流；粗选基于内存**价格快照**同步计算（O(边数) 但无 RPC），
  延迟主要由**精算**（对候选路径的链上真实报价）决定（约 0.5–1s，取决于 RPC）。
- fee-on-transfer / rebasing token 报价会失真 → 白名单排除。
- 公开 HTTP 无鉴权，仅简单限流。

## 相关文档

- `docs/ARCHITECTURE.md`：**详细架构设计文档**（数据流、模块、缓存三层、路由算法、RPC、API、并发、限制）。英文版：`docs/ARCHITECTURE.en.md`。
- `docs/REQUEST-TRACE.md`：**一次 `/quote` 请求的逐函数调用栈**。
- `docs/PLAN.md`：教学版里程碑（含学习目标与检查点）。
- `docs/0X-*.md`：逐阶段学习笔记。
- `AGENTS.md`：仓库工程约定（构建/测试命令、模块边界、约束）。
- 英文 README：`README.en.md`。
