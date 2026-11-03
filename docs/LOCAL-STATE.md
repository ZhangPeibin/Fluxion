# 数据与计算分层：V2 纵向切片（#31）

## 交付范围

本阶段打通真实 V2 / ShibaSwap 的后台同步、不可变快照、单路径/多跳/拆单本地计算、HTTP
状态与错误处理。通过 `pricing_mode: local_snapshot` 显式启用，默认仍为 `rpc`。

**这不是 #31 全部完成。** V3 tick 状态和本地数学、wstETH 整数转换、Curve 支持矩阵与本地
数学、全量 90 组质量/延迟对照仍待后续阶段。包含尚不支持池型的图会整体报告
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
   Multicall 读取当前图的全部 V2 储备。任一子调用失败、ABI 不完整或非法储备都中止本次发布。
4. 在新储备上构建本地粗选 `PriceSnapshot` 和独立 `Router`，发布前再校验目标区块 header。
5. 原子替换整个 `Arc<LocalSnapshot>`；已持有旧版本的读者不受正常发布影响。

**初版每个新区块全量重读 V2 状态**，没有引入日志增量缓存。它用完整重建避开日志断档和
重组回滚的缓存正确性问题；后续 V3/规模优化再引入有连续性证明的变更集合。不能把本阶段
说成已经实现 #31 中完整的日志增量同步器。

同步超时/失败时，不发布半成品。上一版在新鲜度策略允许的范围内可以继续服务，状态端点
仍展示失败；超过限制后拒绝报价。断档或深重组通过从新 canonical head 全量重建恢复，不
需要保留无界的历史快照。存储器仅持有当前版本；旧版本由在途请求的 Arc 生命周期回收。

## 计算层

`LocalSnapshot` 包含冻结储备、图、同版本粗选数据和路由器，**没有 RPC、TTL 冷加载或
联网适配器**。复用现有 `QuoteSource`、Router 和 SplitOptimizer，避免产生第二套路由算法。

一个 `/quote` 只获取一次快照。粗选、逐跳金额、基线、两池细化与多池 DP 都使用这个版本。
不同金额在本地执行整数常数积计算，不是查询有限金额的报价缓存。空储备是流动性不足，
不意味着缺少状态；未知/不支持的池型则在进入计算前拒绝。

HTTP 返回前再次检查状态年龄、已观测链头落后和重组代际。发生重组后，即使请求拿的是
比“被替换的当前版本”更老的句柄，也会被统一拒绝。正常新区块发布不会使旧句柄失效，
但其年龄/落后仍须符合策略。

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
- [第一阶段实测记录](benchmarks/2026-11-02-local-state-v2.md)。它不是原全平台 90 行回归，
  也不证明 V3/Curve 已有本地计算能力。

下一阶段是 V3 状态覆盖与整数跨 tick 计算；Curve 仍先建立支持矩阵。保持 #31 开放，待
全部协议覆盖及同候选集质量对照通过后才能切换默认模式。
