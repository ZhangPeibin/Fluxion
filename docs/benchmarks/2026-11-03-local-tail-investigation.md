# 本地快照 HTTP 尾延迟定位（2026-11-03）

> 本文保留修复前的诊断过程。后续实现及持续更新验收见
> [#36 读写执行隔离修复](2026-11-03-writer-isolation-fix.md)。

## 已确认的主要原因

后台 `v3_sync::load` 在 Tokio 异步任务里同步构造/校验所有 `V3State`，调用
`src/v3_local.rs` 的 `V3State::new`。全范围模式下校验逐个遍历所有 bitmap word 的
256 个 bit，在本次 debug 构建中连续占用 worker 约 5.4 秒，造成 HTTP/定时器调度饥饿。
报价数学已经放入 blocking worker，但后台同步的这段 CPU 工作没有隔离。

macOS `sample` 捕获的对应调用链：

```text
Tokio multi_thread worker
  -> StateSynchronizer::sync_once / load
    -> v3_sync::load
      -> V3State::new
        -> ruint Uint bit operations / bitand_assign
```

## 复现方法

在 `.worktrees/oneinch-api-host` 中加入诊断埋点，以同一份 `config.benchmark-local.yaml`
启动 debug 服务，记录：HTTP 入口、handler 入口、取快照锁、blocking worker 排队/执行、
响应元数据、完整响应就绪、同步阶段、发布锁和两种心跳。

本机一个线程按 150 ms 间隔发顺序报价（WETH→UNI、UNI→USDC、DAI→USDC 三方向轮换），
另一个线程按 50 ms 间隔调用 `/health`。每个请求有 `x-diag-id`，客户端记录发送/接收时间。
另设 50 ms Tokio 心跳和 50 ms 独立 OS 线程心跳；后者用于区分异步调度饥饿和全机停顿。

同一进程完成三次后台同步后，仅停止同步任务，保留同一个 HTTP 服务、图和快照，
继续相同探测作为对照。未改变路由算法、池集或放宽请求错误保护。过程中有单独 CPU 采样，
因此这些探测不作为新的官方三方 benchmark。

## 连续三轮证据

| 阶段 | 第 1 轮 | 第 2 轮 | 第 3 轮 |
|---|---:|---:|---:|
| bitmap 置位展开 CPU | 5210 ms | 5259 ms | 5275 ms |
| `V3State::new` 全量校验 CPU | **5388 ms** | **5404 ms** | **5599 ms** |
| 同期 Tokio 心跳间隔 | **5472 ms** | **5484 ms** | **5663 ms** |
| 发布写锁等待 | 15 μs | 0 μs | 2 μs |
| 快照替换及旧值释放（含锁内工作） | 0 ms | 12 ms | 13 ms |

每轮读取 108992 个 bitmap words，只有约 16776 个 initialized ticks；目前置位展开和
状态校验都扫描每个 word 的 256 位，即各约 **2790 万次 bit 检查**，两处合计约 5580 万次。
置位展开也是 CPU 热点；本轮秒级 HTTP 停顿与后面的全量状态校验阶段逐次对齐。
OS 线程心跳没有秒级告警，最大观测告警为 191 ms。

## 一笔成功报价的分解

请求 `q00973`（33.333… WETH→UNI）：

| 区间 | 实测 |
|---|---:|
| 客户端完整 HTTP | **5558.090 ms** |
| 发送到 HTTP middleware 首次执行 | **5528.163 ms** |
| handler 取快照 | 3 μs |
| blocking worker 排队 | 20 μs |
| blocking worker 总执行 | 28.440 ms |
| 其中 baseline / search | 16 / 12 ms |
| 响应状态元数据构造 | 255 μs |
| HTTP 入口到完整响应就绪（含序列化） | 29.722 ms |

同一校验阶段的 `h03022`：`/health` 完整 HTTP **5609.799 ms**，其中入口等待
**5609.504 ms**，实际 handler/响应准备只有 **48 μs**。
另一个更新周期的成功请求 `q00682` 为 5392.757 ms，其中入口等待 5365.411 ms，
worker 25.827 ms，取快照 4 μs。

因此，秒级延迟已经发生在路由/精算之前；既不是请求内 RPC，也不是秒级 JSON 序列化、
取快照锁或 blocking worker 排队。

## 同进程停止同步后的对照

停止同步后留出 2 秒边界，再继续约 56.29 秒，结果：

| 探测 | 成功次数 | p50 | p95 | 最大 |
|---|---:|---:|---:|---:|
| 三个方向的报价 | 270 | 48.946 ms | 76.176 ms | 80.823 ms |
| `/health` | 1016 | 0.949 ms | 3.466 ms | 10.224 ms |

该对照用于定位干扰源，不意味着停止同步可以作为生产修复，也不能把此 p95 当成修复后
全量 90 组的性能承诺。

## 修复应针对的位置

1. 将后台同步中的 CPU 展开、构造和校验移出 Tokio I/O worker，使用有界 CPU/blocking 执行。
2. bitmap 跳过全零 words、仅遍历置位 bits；保留完整性验证，避免两处各做约 2790 万次扫描。
3. 保留原子发布、重组和新鲜度检查，修复后在连续更新下重跑原始 90 行及 `/health` 对照。

本次仅定位并加诊断埋点，未将上述业务修复混入主工作区。原始服务器日志、客户端 JSONL
和两份 CPU 采样保留在本机临时目录 `opencode`，文件前缀为 `fluxion-tail-`；诊断代码保留在
上述 worktree，未提交。
