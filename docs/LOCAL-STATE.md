# 数据与计算分层：V2/V3 与 wstETH 本地精算（#31 / #32）

## 交付范围

已打通真实 V2 / ShibaSwap 储备、Uniswap V3 bitmap/tick 和 Lido v4 转换状态的后台同步、不可变快照、
单路径/多跳/拆单本地计算、HTTP 状态与错误处理。通过 `pricing_mode: local_snapshot`
显式启用，默认仍为 `rpc`。

**这不是 #31 全部完成。** Curve 支持矩阵与本地数学、全量 90 组质量/
延迟对照仍待后续阶段。包含尚不支持池型的图会整体报告
`UnsupportedLocalPool`，不会悄悄删掉这些池后宣称无损提速。

## 运行

```bash
# 真实 ShibaSwap WETH/SHIB 示例，不需要 Graph key，默认 :8083
cargo run -- --config config.local-v2.example.yaml
curl -s localhost:8083/state
curl -s localhost:8083/ready
curl -s localhost:8083/quote -H 'content-type: application/json' \
  -d '{"token_in":"0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2","token_out":"0x95aD61b0a150d79219dCF64E1E6Cc01f0B64C4cE","amount_in":"33333333333333333333","split":true}'
```

要验证多个候选池，将 `uniswap-v2` 加入示例的 `enabled_platforms`，并通过 `.env` 提供
Graph key。白名单仍双币过滤；发现规则不变。此受限示例不是原有全平台 benchmark 配置。
只有一个池时拆单自然保留 100% 单路径，不会产生虚构分支。

V2/V3/ShibaSwap 四池示例：`cargo run -- --config config.local-v3.example.yaml`，默认
`:8084`，需要 Graph key。同样仅覆盖 WETH/SHIB，不包含 Curve 或转换。

转换与多跳示例：`cargo run -- --config config.local-wsteth.example.yaml`，默认 `:8085`。
该示例显式使用 V3、stETH/wstETH/WETH/USDC 白名单和 100 万流动性门槛，需要 Graph key；
不是默认全平台池集。把示例的 `enabled_platforms` 改成 `[]` 可在不需要 Graph key 的情况下
只验证真实协议转换。

local 模式若进入 mock universe，不启动链上同步，也不会把 mock 地址拿去主网询价；
状态保持 `SnapshotNotReady`。日常离线 mock 开发继续使用默认 rpc 模式，已有 mock 报价
行为保持不变。没有成功快照也能访问 `/health`、`/info` 和 `/state`。

## 数据层

`StateSynchronizer` 持有 RPC，`SnapshotStore` 只持有图、策略、已发布状态和同步指标。
服务进入 local 模式后不启动旧 V2 TTL 热刷新或旧粗选快照循环。

每轮流程：

1. 读取完整链头 header：number、hash、parentHash、timestamp。
2. 若与已发布版本完全一致，不重新加载，也不重置状态年龄。新区块到来时重新确认上一版
   的 canonical hash；发现分叉立即撤销当前版本，并推进重组代际。
3. 用 EIP-1898 `blockHash + requireCanonical` 固定全部 `eth_call`。按 `batch_size` 分批
   Multicall 读取当前图的 V2 储备、V3 header/bitmap/ticks 以及转换协议状态。任一必要子调用失败、ABI
   不完整、非法状态或身份不符都中止本次发布。
4. 在新储备上构建本地粗选 `PriceSnapshot` 和独立 `Router`，发布前再校验目标区块 header。
5. 原子替换整个 `Arc<LocalSnapshot>`；已持有旧版本的读者不受正常发布影响。

**初版每个新区块重读 V2、覆盖窗口内的 V3 和转换协议状态**，没有引入日志增量缓存。
它用完整重建避开日志断档和重组回滚的缓存正确性问题；后续规模优化再引入有连续性证明的变更集合。不能把本阶段
说成已经实现 #31 中完整的日志增量同步器。

同步超时/失败时，不发布半成品。上一版在新鲜度策略允许的范围内可以继续服务，状态端点
仍展示失败；超过限制后拒绝报价。断档或深重组通过从新 canonical head 全量重建恢复，不
需要保留无界的历史快照。存储器仅持有当前版本；旧版本由在途请求的 Arc 生命周期回收。

## 计算层

`LocalSnapshot` 包含冻结池状态、图、同版本粗选数据和路由器，**没有 RPC、TTL 冷加载或
联网适配器**。复用现有 `QuoteSource`、Router 和 SplitOptimizer，避免产生第二套路由算法。

一个 `/quote` 只获取一次快照。粗选、逐跳金额、基线、两池细化与多池 DP 都使用这个版本。
不同金额在本地执行整数池数学，不是查询有限金额的报价缓存。空 V2 储备是流动性不足，
不意味着缺少状态；未知/不支持的池型则在进入计算前拒绝。

HTTP 返回前再次检查状态年龄、已观测链头落后和重组代际。发生重组后，即使请求拿的是
比“被替换的当前版本”更老的句柄，也会被统一拒绝。正常新区块发布不会使旧句柄失效，
但其年龄/落后仍须符合策略。

### V3 状态与计算

- 加载 slot0、活跃流动性、tickSpacing、fee、币序与 factory；核对实际合约身份和发现的池记录。
- 在当前 compressed tick 所在 bitmap word 两侧装载配置的窗口（默认 ±2，即最多 5 words），
  再读取每个置位 bit 对应的 tick liquidityNet。所有读取固定在同一 canonical block hash。
- Q64.96 价格、TickMath 常量、完整 512-bit 中间乘积、token0/token1 的不同取整、费用和
  跨 tick 流动性变化在本地计算。经过空 bitmap word 时也保留协议的分步取整，不直接跳到
  任意远的下一个 initialized tick。负 tick 使用向负无穷的压缩/分组规则。
- 到达全局价格边界后仍有未消耗输入时返回流动性不足，不能把 Quoter 可能返回的部分交换
  输出冒充一个消耗完整输入的 exact-in 分支。
- 只对已覆盖状态精算；未知 word、缺失 tick 或非法流动性返回 `SnapshotIncomplete`。
  请求级 guard 会在基线和拆单搜索结束时检查这些失败：即便另一个 V2 候选成功，也不能
  静默跳过 V3 后返回看似完整的结果。该 guard 不会污染后续请求。
- `/state.v3_coverage` 和报价 `state.v3_coverage` 显示各池装载的 word 范围及 initialized tick 数。
  ready 表示已发布窗口完整，不表示任意大输入都在窗口内。窗口超出时需调整后台配置并
  重新装载；本阶段尚未实现按请求自动拓展、日志增量 tick 缓存或任意金额全区间覆盖。
- tick 数上限是整个快照的硬限制。超过上限拒绝本次发布，不截断 ticks 或把缺失值当作零。

HTTP 的本地计算使用有界 CPU worker，不占用 Tokio 的同步/RPC worker。并发额度用尽返回
429，不建立无界 CPU 队列；超时任务的额度在工作实际结束/取消后才释放。RPC 模式沿用旧路径。

### wstETH：精确的 Lido v4 状态模型

主网部署当前 `getContractVersion()` 为 **4**。wrapper 的双向 view 委托到 stETH，Lido v4
用内部 ETH/内部 shares 进行转换，不是 `getTotalPooledEther()/getTotalShares()` 的直接比例。
同一 block hash 上的 public total 减去 public external 值可精确恢复内部值：

- `internalEther = totalPooledEther − externalEther`
- `internalShares = totalShares − externalShares`
- wrap：`floor(amount × internalShares / internalEther)`
- unwrap：`floor(amount × internalEther / internalShares)`

这不是用单位报价拟合的比例。比如内部比例 10/7、external shares=2、external ETH=2 时，
公开总量为 12/9；unwrap 7 正确得到 10，直接套用总量比例却得到 9。
主网核对同样发现一个实际整数边界相差 **1 wei**，见下面的实测记录。

加载器校验 wrapper 的 `stETH()`、配置币序和协议版本；分批读取四个 totals，再用同块的
双向单位量及内部 numerator/denominator 量级 probe 验证模型。失败时不发布半成品。
当前只声明支持验证过的 v4 语义；其他版本明确拒绝，不能猜测沿用公式。运行时版本不匹配
与静态不支持的池型不同：同步循环继续重试，以便 canonical 链/协议版本恢复后重新就绪。

两向都向下取整，输入必须 **严格小于 `uint128.max`**。本地保持这一上界和 checked uint256
乘法语义；正输入转换成 0 的情况与旧 QuoteSource 一致，作为流动性不足处理，不伪造正输出。
转换没有虚构 AMM 费、储备或拆单分支；禁用方向不会进入图。它可以作为单路径，或作为
包含 V2/V3 的完整基线参与 split=true 询价，最终 min 输出检查仍作用于所选总输出。

`/state.conversion_state` 和成功报价的 `state.conversion_state` 提供 wrapper/underlying、
协议版本、两向启用标志和 decimal-string 内部 ETH/shares（均为 raw units），与该响应的
block/hash 一致。没有转换时，旧报价不增加这个元数据字段。Viewer 原有 wrap/unwrap、虚线
转换边和模式/区块/年龄展示继续适用。

状态每个新区块都读取，不依赖 wrapper 自身是否发出 swap 事件，因此不会漏掉仅发生在底层
Lido 的比例更新。仅复现报价 view，不代表模拟 wrap/unwrap 交易中的转账、余额、授权或 gas。

协议公式依据固定 revision 的
[Lido 源码](https://github.com/lidofinance/core/tree/2bd532298d85cdf520169fdc0c5fb0cc9baf03c3)，
并以运行时版本和同块 view 验证部署语义。

## HTTP 与可观测性

原请求字段保持不变，`split=true`、分支 cap、整数输入守恒、最佳完整方案回退和最终
`amount_out_min` 检查沿用现有语义。

local 成功响应附加：

```json
{
  "quote_block": 26326008,
  "state": {
    "pricing_mode": "local_snapshot",
    "block_hash": "0x4bf8c945f04ae16f54a63baa92d8bd25cc99a55a94f5502f29293921fd7017d6",
    "parent_hash": "<actual parent hash>",
    "block_timestamp": 1793609399,
    "state_age_secs": 8,
    "lag_blocks": 0
  }
}
```

上例仅展示字段形状；区块身份以实际响应为准。旧 rpc 模式不增加 `state` 字段。local 拆单
统计的 baseline/additional RPC 均为 0；后台 RPC 在 `/state.sync_rpc_requests` 单独统计。
Viewer 同时显示模式、报价区块、年龄和落后区块数。

- `/state`：总池数、已装载池数、不支持池清单、当前块和最后观测链头、年龄/落后、最近
  同步错误、累计同步 RPC、最近同步耗时、发布版本数和重组次数。无凭据或 RPC URL。
- `/ready`：local 模式检查已发布状态，不探测 RPC；没有可服务快照返回 503。
- `/graph`：local 模式从同一个快照导出粗选价格；状态不可用时返回相同 503 原因。
- `/health`：只检查进程存活，保持 200。

local 的不可用错误均为 HTTP 503，带 `pricing_mode: local_snapshot`：
`SnapshotNotReady`、`SnapshotStale`、`SnapshotLagging`、`SnapshotReorg`、
`SnapshotIncomplete`、`UnsupportedLocalPool`、`SnapshotSyncFailed`。
不会转而在请求内调用旧 RPC 报价路径。

## 策略

| 配置 | 默认 | 含义 |
|---|---:|---|
| `pricing_mode` | `rpc` | `local_snapshot` 为迁移显式开关 |
| `local_state.poll_interval_ms` | 2000 | 每轮完成后等待时间；同块不重建 |
| `local_state.sync_timeout_ms` | 30000 | 整轮装载的时间上限 |
| `local_state.batch_size` | 200 | 每批 Multicall 子调用数，1–400 |
| `local_state.max_age_secs` | 60 | 最大状态年龄，相对区块时间，不是刷新时间 |
| `local_state.max_lag_blocks` | 3 | 相对最后观测链头的最大落后 |
| `local_state.v3_bitmap_radius` | 2 | 当前 word 两侧的装载半径，1–16 |
| `local_state.v3_max_loaded_ticks` | 50000 | 整个快照的 initialized tick 总上限 |
| `local_state.compute_concurrency` | 4 | 本地 HTTP CPU worker 上限，1–64 |

年龄同时受区块时间和发布后的单调时钟约束；重复读到旧 head 或本机时钟向后调整不能
让旧快照恢复年轻。链头落后是“相对最后观测值”，不是离线时仍知道实时链头。
这些是可配置策略默认值，不是生产 SLA。RPC 必须支持 EIP-1898；不支持时明确同步失败，
不偷偷改用 `latest` 或只有区块号的弱一致性读取。

## 验证与后续

- 离线：RPC 服务关闭后的 HTTP 单路径/四池拆单/多金额询价；min 输出与分支上限；正常
  发布保持旧版本；深重组使更老句柄失效；装载中换叉；失败批次不部分发布；超时重试；
  同步断档恢复；冷启动、缺失平台、过期/落后状态和 readiness 无 RPC。
- 主网：同区块逐跳与部署的 Uniswap V2/ShibaSwap Router `getAmountsOut` 核对，覆盖
  WETH/SHIB 双向、多金额、拆单和 USDC→WETH→SHIB 多跳。
- V3：负 tick、bitmap word 边界、initialized tick 跨越、零流动性空段、微量/大整数和
  取整测试；断开 RPC 后的 V2/V3 HTTP 拆单；覆盖不足不能躲在成功基线后；fee 不符不发布。
- 主网 V3：五池、四档费率、双向三档金额 **30/30** raw 输出等于 QuoterV2；六池混合图的
  拆单和多跳逐跳独立核对。四池 HTTP 20 次样本 p50/p95 约 **31.05/31.33 ms**。
- wstETH：断网后的双向/多金额/多跳、方向/min 输出/虚构储备保护；比例变更不修改旧版本；
  缺失/畸形状态、版本不符和 view 模型验证失败不部分发布，运行时版本恢复后可重试。
- 主网 wstETH：18 个双向整数场景（含零输出）与协议 view 语义一致，另验证两向非法输入
  上界；四条二/三跳转换+V3 路径逐跳 raw 输出相等。六池 HTTP 四个方向各 5 次均成功。
- [V2 历史记录](benchmarks/2026-11-02-local-state-v2.md) ·
  [V3 阶段实测](benchmarks/2026-11-03-local-state-v3.md) ·
  [wstETH 实测](benchmarks/2026-11-03-local-state-wsteth.md)。都不是原全平台 90 行回归。

下一阶段是 Curve 支持矩阵与逐类本地精算。保持 #31 开放，待
全部协议覆盖及同候选集质量对照通过后才能切换默认模式。
