# #31 第一阶段：V2 本地快照验证

这是 **V2/ShibaSwap 受限样本**，用于验证数据与计算的隔离和整数一致性，不是完整 90 组
语料的优化前后比较。旧四方 benchmark 的报价质量数字继续保留，不用本结果替换。

## 主网精算核对

命令：

```bash
cargo test --test local_state_mainnet -- --ignored --nocapture
```

- 区块：`26326008`
- 哈希：`0x4bf8c945f04ae16f54a63baa92d8bd25cc99a55a94f5502f29293921fd7017d6`
- RPC：公共 `https://eth.drpc.org`，debug 构建。
- 图：SHIB/WETH Uniswap V2、SHIB/WETH ShibaSwap V2、USDC/WETH Uniswap V2，共三池。
- 四个请求：0.333… WETH→SHIB、33.333… WETH→SHIB、80 亿 SHIB→WETH、10000 USDC→SHIB。
- 初次后台装载：**1107 ms，3 次 RPC**（header + 固定 hash 的 Multicall + header 核验）。
- 四次本地完整计算：**1844 / 1662 / 1078 / 334 μs**，请求计算 **0 RPC**。
- 每个分支逐跳与同区块已部署的 V2 Router 独立核对，raw 整数输出全部相等；检查输入和
  输出守恒、保留基线和最后区块 hash。多跳样本确认确实使用至少两跳。
- 上述时间不含发现、独立 Router 核验与网络耗时；仅四条样本，不报告虚假的 p95/SLA。

## 持续更新的 HTTP 冒烟

```bash
cargo run -- --config tests/configs/v2.yaml
```

示例发现一个真实 ShibaSwap WETH/SHIB 池，不需要 Graph key。

- 初次观察：区块 `26326017`、hash
  `0xf4f7b848c066c5ef8f6c6a7563cc63de92e015566ddc5c095c6d16bf793a7be6`，1/1 池，ready。
- 33.333… WETH→SHIB，`split=true`，HTTP 200，总墙钟 **0.001040 s**。
- 返回 raw 输出 `15476073313658788665341577775`，报价区块 `26326017`，state age 9 秒，lag 0。
  只有一个池，所以正确保留 100% baseline；这不是多池收益证明。
- 后续 `/ready` 观察到区块 `26326022`，已发布版本 6，证明服务在真实新区块上持续更新。
- 此单次 HTTP 时间包含本机 HTTP 开销；未做大图、多并发、长时间或全平台性能测试。

## 离线失败路径

新增行为测试证明：RPC 断开后仍能从已装载快照完成不同金额的单路径和四池拆单；
不完整更新不会覆盖完整版本；重组（包括装载中换叉）和旧句柄失效得到处理；超时、
过期、落后、冷启动和未支持 V3 状态都有可区分的结果。

## 尚未证明的目标

V3/Curve/wstETH 本地精算、原全图相同候选集的 90 行整数一致性、全量内存和同步成本、
长时间 HTTP 分位延迟均未在本阶段交付。实现说明见 [LOCAL-STATE](../LOCAL-STATE.md)。
