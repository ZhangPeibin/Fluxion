# Fluxion

[English](README.en.md) | 中文

Ethereum 主网多跳路由询价（RFQ）HTTP 服务。给定 `token_in` / `token_out` / `amount_in`，
将 Uniswap V2 / V3、Curve、ShibaSwap V2 的池子和可配置的协议转换统一建成**有向多重图**，
搜索多跳候选并精算（exact-in）；默认返回最佳单路径，`split=true` 可在独立直连池间拆单。
首个协议转换适配器支持 stETH↔wstETH。

> 开源、自托管：下载后用一份 YAML 配置 + 你自己的 RPC / Graph key 跑起来。

## 最新 Benchmark：本地快照 / ParaSwap / 1inch（2026-11-03）

**同轮 30 个交易方向 × 3 档金额，三方均 90/90 成功。Fluxion 与 ParaSwap 的输出差距在
±0.5% 内为 88/90，与 1inch 为 84/90。Fluxion 每行请求计算均为 0 RPC。**

| 指标 | Fluxion | ParaSwap | 1inch |
|---|---:|---:|---:|
| 报价成功 | **90/90** | **90/90** | **90/90** |
| 端到端 HTTP p50 | **31 ms** | 390 ms | 467 ms |
| 端到端 HTTP p95 | 3501 ms | **864 ms** | 1058 ms |

**状态年龄为 50–112 秒**（中位数 76.5 秒），实验显式采用 180 秒 / 15 块上限。中位延迟
已降到毫秒级，但尾延迟仍高，后台同步时效也尚未达到默认 60 秒 / 3 块策略的连续服务目标。

| Fluxion 输出相对对手 | ParaSwap | 1inch |
|---|---:|---:|
| 差距中位数 | −0.000094% | +0.098762% |
| 差距等权均值 | −0.028838% | −0.034276% |
| 差距在 ±0.5% 内 | **88/90（97.8%）** | **84/90（93.3%）** |
| Fluxion 输出更高 / 相同 / 更低 | 28 / 8 / 54 | 68 / 0 / 22 |

输出差距为 `(Fluxion / 对手 − 1)`。多数样本接近，但大额仍有明显缺口：100,000 DAI→WETH
相对 ParaSwap **−1.128%**；160,000 LDO→WETH 和 33.333… WETH→SHIB 相对 1inch 分别
**−4.708% / −2.975%**。1inch 的 API 套餐基础设施费可能影响返回金额，未做去费反推；
“68 组输出更高”不等于纯路由能力胜出。完整离群样本见分析。

本轮使用 **`config.benchmark-local.yaml` / `local_snapshot`**，按地址明确排除旧 Tricrypto，
其余 **209 条池/转换记录、48 个 token、426 条边**全部保留并装载，V3 使用全范围 bitmap/ticks。
`split=true`、最多 4 分支，35/90 组选拆单；实际报价快照区块 **26332461–26332478**。
三方使用相同 raw 输入，外部 API 未固定为同一区块。debug 构建、公共 RPC、本机 Fluxion HTTP；
耗时包含完整响应体接收，不含主动限速等待。后台仍需 RPC，报价未统一核算 gas/服务费或模拟成交。

[90 组逐行报告](docs/benchmarks/2026-11-03-local-threeway.md) ·
[原始 JSON、状态年龄与三方路径](docs/benchmarks/2026-11-03-local-threeway.json) ·
[结果分析与复现命令](docs/benchmarks/2026-11-03-local-threeway-analysis.md)

## 数据与计算分层（#31–#35：V2/V3、wstETH、三个稳定池与 Tricrypto2）

新增显式 `pricing_mode: local_snapshot`：后台同步 V2/ShibaSwap、V3、Lido v4、三个 Curve 稳定池和 Tricrypto2，发布不可变区块快照；
单路径、多跳和拆单的粗选/精算都在同一快照本地完成，**请求计算 0 RPC**。
已验证断网计算、重组/过期/同步失败，以及固定区块与部署 Router 的整数输出一致性。
V3、wstETH 和各 Curve 部署分别使用协议整数模型，并以独立链上 view 核对。
wstETH 使用 Lido 内部 ETH/shares；两币稳定池分别处理余额来源与取整顺序；Tricrypto2
使用 stored D 与动态费用。最新本地快照三方对比见上方 benchmark，本地模型细节见下方文档。

```bash
# 受限的真实 ShibaSwap 示例，默认 :8083；不是原全平台 benchmark 配置
cargo run -- --config config.local-v2.example.yaml
curl -s localhost:8083/state
# V2/V3/ShibaSwap 示例（需要 Graph key，默认 :8084）
cargo run -- --config config.local-v3.example.yaml
# stETH/wstETH 与 V3 多跳示例（需要 Graph key，默认 :8085）
cargo run -- --config config.local-wsteth.example.yaml
# 显式单部署 3pool HTTP 示例（无 Graph key，默认 :8086；不是全注册表发现）
cargo run --example local_curve_server
# 显式 Tricrypto2 单部署图（无 Graph key，默认 :8088）
cargo run --example local_curve_server -- --tricrypto2 --config config.local-tricrypto2.example.yaml
# 三个已核实稳定池，固定图 :8087（不是全注册表发现）
cargo run --example local_curve_server -- --all-stable --config config.local-curve-stable.example.yaml
```

默认仍为原有 `rpc` 模式。#34/#35 已联合集成，覆盖 4/5 Curve；本地实验显式排除剩余旧池，
默认新鲜度策略下的全图连续服务与尾延迟仍待进一步优化和验收。
未支持池型/协议版本、V3 覆盖不足会明确报告，不会静默删池或回退到请求内 RPC。
[设计与运行方法](docs/LOCAL-STATE.md) · [V3 实测](docs/benchmarks/2026-11-03-local-state-v3.md) ·
[wstETH 实测](docs/benchmarks/2026-11-03-local-state-wsteth.md) ·
[Curve 支持矩阵](docs/CURVE-SUPPORT.md) · [3pool 实测](docs/benchmarks/2026-11-03-local-state-curve.md) ·
[Tricrypto2 模型](docs/TRICRYPTO2.md) · [Tricrypto2 实测](docs/benchmarks/2026-11-03-local-state-tricrypto2.md) ·
[两币池实测](docs/benchmarks/2026-11-03-local-state-curve-plain.md)。

## 快速开始

```bash
# 1) 密钥（gitignored）—— .env 只放密钥；服务需要 GRAPH_API_KEY
cp .env.example .env
#   编辑 .env，填入 GRAPH_API_KEY（The Graph Studio 免费创建）

# 2) 配置（可选；缺省用内置默认）—— RPC/阈值/平台/host 等都在这里
cp config.example.yaml config.yaml

# 3) 运行（默认监听 127.0.0.1:8080）
cargo run
```

> 分工：**`.env` 只放密钥**（`GRAPH_API_KEY`）；**其余所有配置走 `config.yaml`**。

启动时读取配置、发现池子、验证协议转换实例，建图并预热价格快照后开始服务。
默认配置启用 Uniswap subgraph；缺少 `GRAPH_API_KEY` 时使用 mock universe，便于离线开发。
也可通过 `cargo run -- --config PATH` 指定其他配置文件。

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
- `max_hops` 可选，指**边数**（默认取配置）；每次 swap、wrap、unwrap 都计一跳。
- `amount_out_min` 可选，所选总输出低于它时返回 `AmountOutBelowMin`。
- `split` 可选，默认 `false`，旧请求保留下面的单路径响应。
- `split=true` 时可传 `split_max_branches`（至少 2，且不能超过服务配置上限，默认 4）；
  不开启拆单时传分支上限返回 400。

响应（`200`）：

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

- `operation` 为 `swap` / `wrap` / `unwrap`；转换步骤的 `pool` 字段指 wrapper 合约地址。
- 返回的是报价和逐跳信息，不包含交易 calldata，也不会发送交易。

**可选多池模式：** 在同一个请求中增加 `"split": true`。响应另含 `plan_kind`、
`allocations`、`baseline_amount_out`、`search`；真实主网请求含 `quote_block`，全部精算固定在该区块。
每个 allocation 有 `amount_in`、`amount_out`、`input_share_percent` 和自己的顺序 `path`。
并行分支不会拼成顺序路径：多分支时顶层 `path` 为空，`amount_out` 是各分支输出之和。
保留原单路径时仍有一个 100% allocation；搜索预算耗尽保留最佳完整方案。
新选择的拆单/直连方案尚未计算聚合价格冲击，`price_impact_bps` 为 `null`。
完整字段和预算语义见 [多池接口说明](docs/MULTI-POOL.md)。

### `GET /graph`

只读地导出当前路由多重图，供 Web viewer 等外部工具使用：

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

- 每条边是一个有向交换或转换步骤；`i` / `j` 是 `Pool.coins` 中的币索引。
- `mid_price` 是输出 raw units / 输入 raw units 的比值；换算整币价格需要结合 token decimals。
- `mid_price` / `reserve` 来自当前价格快照；没有快照或该边数据不可用时**省略这两个字段**（不报错）。
- 协议转换边省略 `reserve`，避免把内部哨兵值显示成真实流动性。
- 只读：不触发新快照、不消耗限流额度，也不影响 `/quote`。

### `GET /info`

返回 `mode`（`mainnet` / `mock`）、生效配置、图规模和 `platform_edges`。配置不包含 RPC URL；
`pools` 是图内记录数，也包含 wrapper link。基准工具用它确认实例模式与实际加载的平台。

### `GET /health` / `GET /ready`

- `/health`：进程存活。
- `/ready`：默认对上游 RPC 做一次 `eth_blockNumber`，不通返回 `503`；local 模式只检查快照是否可服务。

### `GET /state`

local 模式返回快照区块/hash、年龄、相对最后观测链头的落后、池覆盖、同步 RPC/耗时、
最近错误和重组次数。成功报价附加 `state` 元数据及 `quote_block`；Viewer 显示模式、
区块、年龄和落后。冷启动、过期、不完整或不支持的本地状态返回明确的 503，不回退到请求内 RPC。
字段、策略与首阶段支持范围见 [LOCAL-STATE](docs/LOCAL-STATE.md)。

### 错误

`QuoteError` → HTTP 状态：`NoPathFound`(404)、`AmountOutBelowMin`/`InsufficientLiquidity`/
`Overflow`(422)、`ZeroAmount`(400)、`PoolNotFound`(404)、`RpcError`(502)、限流(429)。

## 图可视化（Web viewer）

`ui/` 是一个独立的 React + Vite + vis-network 前端，只读地轮询 `GET /graph` 查看整张多重图，
并可试算路由。

```bash
# 终端 1：服务（默认 :8080）
cargo run

# 终端 2：前端（默认 :5173）；首次需安装依赖
cd ui
npm install
npm run dev
```

打开 <http://localhost:5173>。Vite dev server 把 `/api` 代理到 `http://localhost:8080` 并去掉
`/api` 前缀，浏览器保持同源，服务端无需 CORS。

- 渲染全部 token（节点）与有向池跳（边）；节点优先显示常见 token 符号（USDC/WETH/…，未知 token
  回退为短地址），边标注平台 + pool（默认关闭，可在侧栏打开），平行边按方向和序号弯折区分。
- 平台图例兼作筛选器，并显示各平台边数；另有 token 搜索（淡出无关节点/边）与最小流动性
  （raw reserve）过滤。
- ShibaSwap 与 wstETH 转换有独立图例；转换边用虚线表示，不参与池子储备阈值过滤。
- 悬停显示完整地址、pool、platform、`mid_price`、`reserve`；点击节点/边打开详情面板（含 Etherscan 链接）。
- From/To 可点击节点或直接输入地址，金额默认填入 **1 个整 token**（按 token decimals 换算为 raw units，
  也可手动改成任意 raw 数值）；调用 `POST /quote` 后高亮最优路由、
   其余边淡出，并显示 `amount_out`、`price_impact_bps` 与逐跳明细。
- 默认勾选 Multi-pool routing，可切回单路径；多池显示分支比例、输入/输出和报价区块，
  同时高亮所有分支，未计算的聚合价格冲击显示为 “—”。
- 定时刷新（默认 5s，可调）；增量更新数据，保留节点位置、选择与路由高亮。

> 节点符号来自 `ui/src/graph.js` 的内置主网 token 表（覆盖默认 allowlist）；自定义 token 可自行补充。
> 生产打包暂缓：v1 只支持上述双进程开发模式。`npm run build` 会产出静态资源，但尚未由服务托管或部署。

## 架构

### 启动与运行结构

```text
config.yaml + .env
        │
        ├─ 池发现：Uniswap subgraph / Curve registry / ShibaSwap factory
        └─ 转换实例：配置声明 → 校验 wrapper 的 underlying 与启用方向
        │
        ▼
Graph：token 为节点，AMM swap 与协议转换为有向边
        │
        ▼
批量预热 V2 储备、V3 slot0 → CachingSource → PriceSnapshot
        │
        ▼
axum HTTP 服务
  POST /quote → Router → 快照粗选 → 分组 K-best DP → 候选精算 → 单条最佳路径
    split=true → 固定区块精算 → 两池细化 / 多池分配 DP → 最佳完整方案
  GET /graph  → 读取拓扑与已安装快照
  GET /info   → 模式、配置与图规模

后台独立循环：
  refresh_interval_secs  → StateCache 分批刷新近期用过的 V2 池储备
  snapshot_refresh_secs → 全图粗选数据预热、重建并替换 PriceSnapshot
```

拓扑在启动时确定；运行期间更新报价状态和快照，不会在后台重新发现池子。

### 一次报价如何决定路径

1. **粗选**：从已安装快照读取 mid-price，加入费用和可选流动性惩罚；正常 HTTP 报价的粗选不发 RPC。
2. **保留候选多样性**：按 `(token, hop, 能力分组)` 保留 top-K，禁止重复 token。四组分别为
   原有 AMM、含协议转换、含 ShibaSwap、两者都有；各跳数的候选都保留到精算。
3. **精算**：候选之间最多并发 16 条，每条路径内部按顺序传递实际输出金额；单条候选失败不影响其他候选。
4. **择优**：以精算的 `amount_out` 最大者为结果，计算价格冲击并检查 `amount_out_min`。
5. **可选拆单**：保留单路径，先两池细化，再对最多四个独立直连池做离散分配 DP，
   整数余数精确重报价；只在总输出严格增加时替换。默认 256 次附加报价、8 秒搜索预算。

这仍是有界启发式搜索：分组避免新能力被其他路径全部挤掉，但不保证全市场的全局最优。

### 统一接口，不同报价语义

`QuoteSource` 提供 `amount_out`、`mid_price`、`reserve_in`。`Dispatcher` 按 `PoolKind` 分派：

| 类型 | 精算方式 | 语义 |
|---|---|---|
| Uniswap V2 / ShibaSwap V2 | 短 TTL 储备 + 本地常数积计算 | 复用 V2 数学，平台身份独立；费用 0.3% |
| Uniswap V3 | QuoterV2 `eth_call` | 使用池对和费率，计算输入金额对应的输出 |
| Curve | `get_dy`，按接口分派 | 支持 stableswap / crypto 的不同索引类型 |
| wstETH 协议转换 | RPC view / 本地 Lido v4 整数状态 | 实际金额取整，不假设 1:1，不附加 AMM 费用 |

例如 `stETH → wrap → wstETH → V3 → WETH → V3 → USDC` 是三跳路径。
**代码实现接口类型，配置声明部署实例**：增加同类 wrapper 可改配置，增加其他协议接口需要对应适配器。

### 缓存职责

| 组件 | 保存什么 | 更新方式 |
|---|---|---|
| `StateCache` | V2 精算使用的储备 | 冷池懒加载；近 5 分钟访问过的池分批热刷新；TTL 默认 30 秒 |
| `CachingSource` | 粗选 mid-price 与 reserve | 批量预热或按需读取；TTL 为两个刷新间隔的较大值 |
| `PriceSnapshot` | 与图边索引对齐的只读价格向量 | 默认每轮等待 120 秒后重建，完成后替换共享快照 |

默认单路径的 V2 精算可能使用短 TTL 缓存，其他精算读取实时状态。
`split=true` 的主网请求另建固定 `quote_block` 的定价缓存，不混入其他区块；粗选仍可使用旧快照。
报价不等于实际执行整条方案的交易模拟。

技术栈：

- Rust（edition 2024）、`tokio`、`axum`、`serde`、`thiserror`、`tracing`
- `alloy-primitives`：`U256` / `Address`
- 链交互：**原始 JSON-RPC（`reqwest`）**，手写 ABI 编码/解码
  （`eth_call getReserves` / QuoterV2 / Curve `get_dy` / Multicall3 `aggregate3`）

## 模块结构

```
src/
  lib.rs         库模块导出
  model.rs       Token、PoolKind、ConversionKind、Edge、Hop、Route 等模型
  graph.rs       邻接表多重图；多币池展开；转换边按启用方向加入
  discovery.rs   Uniswap subgraph / Curve registry 发现与白名单过滤
  discovery/
    shibaswap.rs ShibaSwap factory 查询、储备验证、估值过滤和排序
  source.rs      QuoteSource trait、V2 数学与 mock 实现
  adapters.rs    V2/V3/Curve 适配器、Dispatcher、CachingSource
  conversion.rs 协议转换适配器；目前为 wstETH 接口
  state.rs       StateCache：V2 储备 TTL 缓存 + Multicall3 热池刷新
  rpc.rs         原始 JSON-RPC 客户端（含 429/瞬时错误重试退避）
  router.rs      快照、四类候选 K-best DP、有界精算与价格冲击
  split.rs       两池细化 / 多池分配 DP、单路径兜底、输入守恒与搜索预算
  reference.rs   ParaSwap 报价与同区块分配回放（覆盖诊断，不扩展生产图）
  universe.rs    服务与实验共享的池发现、报价源装配和批量预热
  api.rs         /quote、/graph、/health、/ready；DTO、错误映射与限流
  error.rs       共享报价错误类型
  config.rs      YAML 配置、类型化转换实例与校验
  util.rs        ABI 编码、hex、数值转换
  main.rs        启动装配、/info、后台刷新与 HTTP 服务
  bin/
    bench.rs     配对基准；聚合器对比、完整路径、JSON / Markdown 报告
    split_bench.rs 固定区块单路径/两池/多池/ParaSwap 比较与调用成本报告
    graph.rs     从运行实例导出 Mermaid / HTML

config.example.yaml           全部配置示例
config.benchmark.yaml         开启新能力的基准实例（:8081）
config.benchmark-control.yaml  关闭转换与 ShibaSwap 的对照实例（:8082）
bench/corpus.yaml             固定交易对与 raw 输入金额
tests/                        路由 / 转换行为、主网集成测试与固定区块 fixture
ui/                           独立 React + Vite + vis-network 查看器
docs/QUOTE-QUALITY.md          转换、ShibaSwap 和候选多样性的实现说明
docs/benchmarks/               基准方法、原始路径数据与结果分析
```

## 数据模型

- `Token { address, symbol, decimals }`
- `Pool { kind: PoolKind, address, coins: Vec<Address> }`
  - `PoolKind = UniswapV2 | ShibaSwapV2 | UniswapV3 { fee } | Curve | Conversion { kind, wrap, unwrap }`
  - `ConversionKind` 当前支持 `WstEth`；转换复用 `Pool` 容器，`address` 为 wrapper，`coins` 为 `[underlying, wrapped]`。
- `Edge { pool_idx, i, j }`（有向：卖出币索引 `i` → 买入币索引 `j`）
- 同一 token 对在多平台 = **平行边**；多币池（如 Curve 3pool）展开为所有有序对：
  3 币 → 6 条边。
- 转换只加入启用方向；`Hop` 经 API 映射后使用 `operation` 区分 swap、wrap 和 unwrap。

## 池子发现与过滤

- **协议转换**：默认将 stETH↔wstETH 加入路由图。`protocol_conversions` 配置类型化实例，
  `wrap` / `unwrap` 可独立启用；精算读取协议的实际整数转换结果，不假设 1:1、不收取虚构的 AMM 费用。
  每个转换操作计一跳。`protocol_conversions: []` 可关闭；新增接口类型需要对应代码适配器。
- **ShibaSwap V2**：通过 `shibaswap_v2_factory.getPair()` 分批查询白名单中的代币对，
  验证币序与非零储备，复用 0.3% 费用的 V2 报价数学；单独标识为 `shibaswap-v2`。
  使用稳定币/WETH 锚定的储备比例估算过滤 `min_liquidity`，再按估值排序取上限；
  不能估值的池在流动性门槛非零时排除。该来源不提供成交量/池龄，设置相应非零门槛会跳过 ShibaSwap 并记录警告。

- **Uniswap V2**：V2 subgraph，按 `reserveUSD` 排序、分页。
- **Uniswap V3**：V3 subgraph，按 `volumeUSD` 排序、分页（注意：按 TVL 排序会先冒出
  TVL 虚高的垃圾池）。
- **Curve**：通过 `AddressProvider.get_address(id)` 枚举多个注册表（`curve_registry_ids`，
  默认 `[0, 5]` = 主 stableswap + crypto；`7` = MetaRegistry 覆盖全部含 factory/metapool，较慢），
  每个注册表 `pool_count() → pool_list(i) → coins(i)`，并用一次 `get_dy` 探测验证可报。
- **过滤**：按数据来源支持的条件应用流动性、活动代理和池龄门槛，并保留默认约 50 个主流币的
  白名单。`token_allowlist` 为空时使用内置名单，非空时覆盖；V2/V3、ShibaSwap 与转换要求两币都在名单内，Curve 要求所有币都在。
- V2/V3 分别按流动性/成交量排序，在过滤后保留最多 `max_pools` 个池（默认 10000）。
- ShibaSwap 按估算流动性排序截断；Curve 的 `max_pools` 是每个注册表的枚举上限；转换实例由配置列表控制。

## 路由算法

- 粗选以 `-ln(rate)` 将汇率乘积变成可累加的成本，再加入费用与流动性惩罚；最终按精算输出择优。
- 粗选边权：`-ln(mid_price) + fee_penalty(kind) + λ·(amount_in/reserve_in)`；
  `reserve_ratio_k > 0` 时跳过 `reserve_in < k·amount_in` 的浅边。
- **K-best DP**：按 `(token, hop, 能力分组)` 保留 top-K 路径。分组区分是否含协议转换、ShibaSwap，
  最多 4 组，避免新路径与原有路径在精算前互相挤掉；限 `max_hops`，禁止重复 token。
- **直连兜底**：符合已配置储备门槛的直连边补入候选，避免粗选把明显最优的直连路由挤掉。
- **并行**：快照构建与候选精算各最多并发 16 项；快照上的边权计算与 DP 是同步内存计算。
- 滑点被精算吸收；响应额外返回 `price_impact_bps`。

## 配置（`config.yaml`）

| 键 | 说明 | 默认 |
|---|---|---|
| `host` / `port` | 监听地址 | `127.0.0.1` / `8080` |
| `rpc_url` | 第三方 RPC | `https://eth.drpc.org` |
| `uniswap_v2_subgraph_id` / `uniswap_v3_subgraph_id` | The Graph 去中心化网络部署 ID | 见示例 |
| `curve_address_provider` | Curve AddressProvider | `0x0000…4383` |
| `curve_registry_ids` | 枚举的 Curve 注册表 id（0=主 stableswap，5=crypto，7=MetaRegistry 全部） | `[0, 5]` |
| `enabled_platforms` | 启用的 AMM 平台；与转换列表独立 | `uniswap-v2` / `uniswap-v3` / `curve` / `shibaswap-v2` |
| `shibaswap_v2_factory` | ShibaSwap V2 工厂地址 | 见示例 |
| `protocol_conversions` | 已支持接口类型的部署实例；`[]` 关闭转换 | Lido stETH↔wstETH，双向开启 |
| `max_hops` / `top_k` / `max_pools` | 最大边数 / 每状态每能力组保留数 / 发现上限 | 3 / 8 / 10000 |
| `min_liquidity` | 流动性下限（USD 量级） | 50000 |
| `min_volume_24h` | 成交量下限（用 subgraph `volumeUSD` 作代理） | 0 |
| `min_pool_age_days` | 最小池龄（天） | 0 |
| `reserve_ratio_k` | 动态浅池阈值 `reserve_in ≥ k·amount_in`（0 = 关闭） | 0 |
| `slippage_penalty_lambda` | 流动性惩罚系数 | 0.0 |
| `refresh_interval_secs` | 热池储备刷新间隔与储备 TTL；近 5 分钟访问过的池才是热池 | 30 |
| `snapshot_refresh_secs` | 全量价格快照重建间隔 | 120 |
| `request_timeout_ms` | 单请求超时（毫秒） | 10000 |
| `pricing_mode` / `local_state` | 精算模式 / 后台同步与新鲜度策略 | `rpc` / 见示例 |
| `split` | 可选拆单的服务预算；请求仍需 `split=true` | 4 分支 / 4 候选池 / 20 段 / 256 次 / 8000 ms |
| `token_allowlist` | 覆盖内置主流币白名单 | 空（使用内置） |

`GRAPH_API_KEY` 只放在 **gitignored 的 `.env`** 里，不进版本库。
转换实例字段为 `kind`、`contract`、`underlying`、`wrapped`、`enabled`、`wrap`、`unwrap`；
完整示例见 [config.example.yaml](config.example.yaml)。汇率来自协议，不在 YAML 中填写固定价格。

粗选价格缓存 TTL 取 `max(refresh_interval_secs, snapshot_refresh_secs)`。全图预热使用
Multicall3 分批读取 V2 储备和 V3 `slot0`，不会将所有池标成热池；精算时才懒加载 V2 的短 TTL 储备。

## 基准测试与图导出

```bash
# 单独配置启动一个真实主网实例（默认 :8081；需要 GRAPH_API_KEY）
cargo run -- --config config.benchmark.yaml

# 另一个终端：30 个交易对 × 3 档固定 raw 输入金额
cargo run --bin bench -- --service http://127.0.0.1:8081 --config config.benchmark.yaml

# 从同一实例导出 Mermaid 与交互式 HTML，可用 --token ADDRESS --depth 2 聚焦
cargo run --bin graph -- --service http://127.0.0.1:8081 --out graph
```

`GET /info` 返回主网/mock 模式、图规模及生效配置（不含 RPC URL），基准工具拒绝 mock 模式。
`/quote` 的每跳与 `/graph` 的每条边增加 `operation: swap | wrap | unwrap`；转换边的
`pool` 字段为 wrapper 合约地址，`/graph` 不为转换边伪造储备值。Viewer 为转换边使用虚线。
ParaSwap 无需密钥；1inch/0x 可通过 `.env` 中的 `ONEINCH_API_KEY` / `ZEROX_API_KEY` 启用。
方法、结果与限制见 [docs/benchmarks/README.md](docs/benchmarks/README.md)。

同轮对照：另启动 `cargo run -- --config config.benchmark-control.yaml`（:8082），然后运行
`cargo run --bin bench -- --service http://127.0.0.1:8081 --control-service http://127.0.0.1:8082 --config config.benchmark.yaml`。
两实例使用相同语料、相同基础配置；对照实例关闭转换和 ShibaSwap。报告按 stETH、SHIB、其他样本分组，
JSON 保留逐跳路径、对手拆单比例和池信息。详细实现见 [报价质量改进](docs/QUOTE-QUALITY.md)。

加上 `--split` 可比较多池模式；最新结果展示在本文前部的 **Benchmark** 章节，完整数据与
复现步骤见 [最新本地三方实测](docs/benchmarks/2026-11-03-local-threeway-analysis.md)。

## 测试

```bash
cargo test                       # 单元测试（离线）
cargo test -- --ignored          # 真实主网集成测试（只读 eth_call）

cd ui && npm install && npm run build   # 前端构建
```

- 单元：mock `QuoteSource` 的多重图、路由择优、负权/简单路径、编码稳定性。
- 转换与候选回归：整数舍入、方向开关、配置/underlying 校验、转换加多跳组合、API 操作标记，
  以及 `top_k=1` 时转换路径和原有深池路径不被另一类薄池挤掉。
- 主网：V2/V3/Curve 真实报价、V2 vs V3 择优、Curve 3pool、Multicall3 热刷新、
  端到端多跳（V2/V3 + Curve）、subgraph 发现。V2 适配器另有**固定区块 `getReserves`
  fixture** 做不联网的确定性测试。
- 新增固定区块主网核对：wstETH 往返舍入、ShibaSwap 发现与部署 Router 的输出一致性。
- 多池：四池优于两池、整数余数/溢出/相等、预算回退、HTTP 兼容/区块固定/超时/min 输出、mock 离线，
  以及主网每个分支独立精算后总额一致性。

## 边界与限制

- 仅 Ethereum 主网；exact-in；默认单路径，可选独立直连 AMM 池拆单；不保证全市场最优。
- 多池分配有候选、网格和调用预算限制；不支持任意多跳分支拆单或共享池状态联动。
- 原生 ETH 不支持；ETH↔WETH 包装由调用方处理。stETH↔wstETH 是独立的、已支持的协议转换。
- Curve 适配器同时支持 stableswap（`get_dy(int128,…)`）与 crypto（`get_dy(uint256,…)`）；
  **发现**默认覆盖主 stableswap + crypto 注册表（`curve_registry_ids: [0, 5]`）；
  factory/metapool 可通过把 `7`（MetaRegistry）加入 `curve_registry_ids` 纳入，但会显著变慢。
- 公共 RPC 可能限流；正常请求的粗选在内存执行，主要等待来自候选精算。增加候选能力组会增加
  RPC 开销；延迟参考上面的实测结果，不提供固定 SLA。
- 不通用支持 fee-on-transfer 或任意 rebasing token；stETH 转换有专用适配器，不意味着其他特殊 token 自动可用。
- 默认单路径的快照、V2 缓存和实时调用可能来自不同区块；多池精算固定区块，但不构建或模拟交易。
- 公开 HTTP 无鉴权，仅简单限流。

## 相关文档

- [docs/MULTI-POOL.md](docs/MULTI-POOL.md)：多池分配、固定区块 HTTP、Viewer 与对手回放的保证边界。
- [docs/SPLIT-ROUTING.md](docs/SPLIT-ROUTING.md)：两池实验算法、预算、缓存和固定区块保证。
- [docs/QUOTE-QUALITY.md](docs/QUOTE-QUALITY.md)：本轮转换、ShibaSwap、候选分组的设计与验证。
- [docs/benchmarks/README.md](docs/benchmarks/README.md)：基准方法、配对结果及完整原始数据入口。
- `docs/ARCHITECTURE.md`：**详细架构设计文档**（数据流、模块、缓存三层、路由算法、RPC、API、并发、限制）。英文版：`docs/ARCHITECTURE.en.md`。
- `docs/REQUEST-TRACE.md`：**一次 `/quote` 请求的逐函数调用栈**。
- `docs/PLAN.md`：教学版里程碑（含学习目标与检查点）。
- `docs/0X-*.md`：逐阶段学习笔记。
- `AGENTS.md`：仓库工程约定（构建/测试命令、模块边界、约束）。
- 英文 README：`README.en.md`。
