# #35：Tricrypto2 本地 CryptoSwap view（2026-11-03）

模型和具体部署调用链见 [TRICRYPTO2](../TRICRYPTO2.md)。本记录是单部署/受限组合验证，
不是全平台 90 行、执行模拟或生产 SLA；默认 RPC 模式保持不变。

## 独立历史向量

`tests/fixtures/tricrypto2-20000000.json` 与 `tricrypto2-26331640.json` 直接捕获部署余额、
A/gamma、stored D、price_scale、动态费参数、币序、三个合约 code 指纹和 get_dy 结果。
每份 52 条，共 **104/104** 语义精确一致：90 个成功输出（含真正零输出）、14 个预期 revert。
期望值不是由生产数学代码生成。老快照 D 约 2.80e25，当前快照 D 约 1.06e25，价格尺度
也不同；离线替换后旧读者仍保留旧参数与结果。

## 实时主网

```bash
cargo test --test local_tricrypto2_mainnet -- --ignored --nocapture
```

最终复测区块 **26331860**，hash
`0x3bf2c06369586609aff9b3a10a30909ac72794d809d76fd34d947ed8c24477b2`。
六方向、多精度微量、不整除、常规/相对大输入共 **52 个场景**：44 个成功 raw 输出完全相等、
8 个 view 拒绝本地也拒绝。没有用正输出成功率掩盖错误，也没有把 revert 伪装为零。

- 初始化 **1244 ms / 4 RPC**，含 header、14 项状态、同块 probe 和最终 header 核验。
- 完整本地 split=true 计算 **201–3048 μs**（含拒绝场景），成功时保留 100% Curve baseline。
- 计算阶段 0 RPC，独立 get_dy 参考使用另一个 pinned RPC 客户端，计时不含参考请求。
- ANN=1707629、gamma=11809167828997、stored D=10635658594565251423304270；
  price_scale=[76807765240273767916599,2251245190164814457049]。
- mid/out fee=[3000000,30000000]、fee_gamma=500000000000000，实际 fee 随交易后 xp 变化。

大档代表 raw 输出：

| i→j | raw 输入 | raw 输出 |
|---|---|---|
| USDT→WBTC | 100000000000 | 126799501 |
| USDT→WETH | 100000000000 | 43076600421376654490 |
| WBTC→USDT | 167000000 | 124418196295 |
| WBTC→WETH | 167000000 | 54908967085994046560 |
| WETH→USDT | 33333333333333333300 | 74222766419 |
| WETH→WBTC | 33333333333333333300 | 96441543 |

## 同块组合路径

同块 26331860 的五记录图包含经典 3pool、Tricrypto2、两个 V3 和 wstETH 转换，初始化计数
15 RPC。每个选中 hop 独立核对 get_dy/QuoterV2/协议 view，raw 输出逐跳相等：

| 输入方向 | 路径 | raw 输出 | 本地 μs |
|---|---|---|---:|
| 100000 USDC→WBTC | 3pool→Tricrypto2 | 126934095 | 1338 |
| 1.67 WBTC→USDC | Tricrypto2→3pool | 124243350028 | 4619 |
| 0.167 WBTC→stETH | Tricrypto2→V3→unwrap | 5683727908175761554 | 6291 |

均计算 0 RPC；模型校验和运行时引用同一 block hash，结束时再次验证 canonical hash。

## 显式单部署 HTTP

```bash
cargo run --example local_curve_server -- --tricrypto2 --config tests/configs/tricrypto2.yaml
```

`profile=explicit-tricrypto2-only`，不运行全注册表发现。六方向各 5 次，共 **30/30 成功**，
均 split=true、max_hops=1。客户端记录全部 30 次使用区块 **26331864**，hash
`0x9d2b862aa891e76b14cd1357d9835f9a0abec6eae245d957601d461c4f405c10`。

| 方向 | raw 输入 | raw 输出 | HTTP p50 / p95 ms |
|---|---|---|---:|
| USDT→WBTC | 100000000000 | 126741090 | 0.8389 / 12.1763 |
| USDT→WETH | 100000000000 | 43037059691412775374 | 0.7947 / 0.8041 |
| WBTC→USDT | 167000000 | 124468884204 | 0.7404 / 0.8653 |
| WBTC→WETH | 167000000 | 54882137365212941864 | 0.8055 / 0.8422 |
| WETH→USDT | 33333333333333333333 | 74282543816 | 0.7619 / 0.8193 |
| WETH→WBTC | 33333333333333333333 | 96479468 | 0.7730 / 0.7850 |

baseline/additional RPC 均为 0。p50 为各组中位数，p95 为 nearest rank（5 次时是最大值）。
保留第一次计时请求的 12.1763 ms 尾部样本，不做事后剔除。
debug 构建、公共 RPC、本机 HTTP；初始化与后台状态同步不在这些请求时间内。
低计算延迟不是零状态年龄，不是长时间并发/内存或全图 SLA 证明。

后续 readiness 观察到已发布版本 41、区块 26331845、lag 0、age 14 秒，余额和 stored D
继续随新区块更新；动态 fee 参数明确报告，不把后台同步计为请求内 RPC。

离线 HTTP 还覆盖 52 个成功/失败向量、min 输出、纯本地多跳、状态/价格尺度更新与旧读者、
缺失/畸形参数/币序/不符 probe 不部分发布，以及旧 Tricrypto 同币序也不被错误套用。

## 最终工程检查

- `cargo test`：115 项离线测试通过；网络测试默认 ignored。
- `cargo test --test local_tricrypto2_mainnet -- --ignored --nocapture`：2 项通过。
- `cargo clippy --all-targets -- -D warnings`、`cargo build --bins --examples`、
  `cargo fmt --check` 和 `git diff --check` 均通过。

## #34 / #35 联合集成复核

两个分支整合后，完整离线测试为 **124 项通过**。新增同块 20000000 的四部署同批加载与
断网 HTTP 回归，覆盖不同币数/字段数、两币池余额来源、固定/动态 fee 元数据和计算 0 RPC。
完整五池发现图保留全部记录，只把旧 Tricrypto 报告为未支持。

联合主网命令：

```bash
cargo test --test local_curve_plain_mainnet --test local_tricrypto2_mainnet -- --ignored --nocapture
```

**4 项测试通过**：两币池 29 个输出及各自 V3 组合、Tricrypto2 六向 52 场景
（44 成功/8 预期拒绝），以及四个 Curve 部署 + 两个 V3 + wstETH 的七记录同块图。
2CRV 检查使用区块 26331901，hash
`0xdbe8f22617f5aba195ee7f941fd8983e841a2f4a078b8fa1ee8bc66615bbf0e8`；
FRAX/USDC 与 Tricrypto2/七记录图使用区块 **26331902**，hash
`0x30a05d64a0997ccb0428d82e91bb1708209bdc56bdf47356a693e8db8f9847e5`。

七记录图初始化计数 15 RPC，计算仍 0 RPC，选中 hop 均与独立 get_dy/QuoterV2/协议 view 相等：

| 输入方向 | raw 输出 | 本地 μs |
|---|---|---:|
| 100000 USDC→WBTC | 126231484 | 3365 |
| 1.67 WBTC→USDC | 124872809222 | 3180 |
| 0.167 WBTC→stETH | 5683351203215572087 | 1471 |

联合版本的严格 Clippy、全部 bins/examples 构建与格式检查通过。上面的单部署 HTTP 数字
保留为该阶段样本，未将它冒充七记录图的 HTTP 延迟或全平台 90 行结果。
