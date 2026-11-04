# #34：两个两币 Curve 稳定池（2026-11-03）

本切片支持具体 2CRV USDC/USDT 与 FRAX/USDC 部署。本地 Curve 覆盖为默认五池中的
**3/5**；两个 crypto 仍明确未支持，默认仍为 RPC，不代表全平台 90 行验收。
完整部署与公式差异见 [CURVE-SUPPORT](../CURVE-SUPPORT.md)。

## 原始历史向量与余额语义

```bash
cargo run --example curve_snapshot -- --pool 2crv --block 20000000
cargo run --example curve_snapshot -- --pool frax-usdc --block 26331147
cargo test --test local_curve_plain --test local_curve_plain_http
```

四个新增 fixture 保存区块 20000000 与 26331147 的币序、有效余额、A_precise、fee、成功/
revert 标记及直接链上 raw 输出。2CRV 每块 14 条、FRAX/USDC 每块 15 条，新增 **58/58**
本地整数一致，经典 3pool 原有 **64/64** 仍通过。

两个池 A_precise 都为 150000（精度 100），但读取余额和 D 乘除顺序不同。2CRV 在 26331147
净余额仅 `[54912085,161578687]` raw，100000 整币输入远大于储备，得到小输出是实际
get_dy 语义，不能将它误当作深流动性或报价质量改善。

FRAX/USDC 余额从历史块的 `[13556539064227344474578535,3003914196936]` 变为
`[1764307424718242137005985,187186145568]`；两个历史版本在离线 HTTP 中替换后，旧读者
保持旧结果。三币/两币混合批次、微量零输出、min 输出、错误状态/probe 的全量拒绝通过。
源码差异独立审读，mock 仅回放链上向量，不调用生产数学生成期望值。

## 主网直接核对

```bash
cargo test --test local_curve_plain_mainnet -- --ignored --nocapture
```

两向覆盖 1、2 raw、1e6 raw、单位量、单位量+1、1000/10000/100000 整币（去重），
本轮 **29/29 raw 输出一致**，包括 4 个零输出；这些零输出不是正收益/成交成功声明。
计算阶段 0 RPC。2CRV 还独立核对 snapshot.balance 等于同块 ERC20.balanceOf−admin_balances。

| 部署 | 区块 / hash | 初始化 ms / RPC | 本地完整计算 μs |
|---|---|---:|---|
| 2CRV | 26331438 / `0xfd008b7513c932a482955c310499a779ab3e3f42febce5cd3bd69c81725cea71` | 1358 / 4 | 127–584 |
| FRAX/USDC | 26331439 / `0x9409030e0a9fec58e21eaced4eebc31c977c0b75b044d0dfef0f10cf17dadaea` | 1317 / 4 | 147–866 |

100000 整币输入的代表 raw 输出：

| 方向 | raw 输出 |
|---|---|
| USDC→USDT，2CRV | 161530213 |
| USDT→USDC，2CRV | 54895611 |
| FRAX→USDC | 98085942967 |
| USDC→FRAX | 100576503284152114476762 |

## 强制 V3 组合

分别只组装一个两币池 + USDC/WETH V3，避免 3pool 择优掩盖被测部署。1 WETH 输入，
V3→两币池两跳逐跳匹配独立 QuoterV2/get_dy：

- 2CRV，同块 26331438，raw 输出 `161530037` USDT，本地 **834 μs**。
- FRAX/USDC，同块 26331439，raw 输出 `2268866251301930315205` FRAX，本地 **259 μs**。
- 各初始化 12 RPC；计算 0 RPC，结束时各块 hash 一致。

## 三个已核实稳定池 HTTP

```bash
cargo run --example local_curve_server -- --all-stable --config tests/configs/curve-stable.yaml
```

固定三个已核实部署，不运行生产注册表发现，`profile=explicit-verified-stable-only`。
每个方向 5 次，共 **25/25 成功**；均 split=true，单跳 max_hops=1，最后一个方向 max_hops=2。
全部 25 次记录在区块 **26331454**，hash
`0x24893d76de885b5e36787591bd123fda7925b5d506e59a43fd07039f358732b8`。

| 输入方向（各 1000 整币） | raw 输出 | HTTP p50 / p95 ms |
|---|---|---:|
| USDC→USDT | 1001072370 | 1.6235 / 1.7295 |
| USDT→USDC | 998629011 | 1.6118 / 2.6256 |
| FRAX→USDC | 990842713 | 1.6143 / 1.6557 |
| USDC→FRAX | 1008941317535206826485 | 1.4964 / 1.5384 |
| USDT→FRAX | 1007558137279030218211 | 1.6639 / 1.7641 |

前两条择优为经典 3pool，第三/四条为 FRAX/USDC，最后一条为 3pool→FRAX/USDC。
因此不能把这个三池示例当作 2CRV 每条都被选中的证明；2CRV 已通过单部署和强制 V3 组合
单独核对。baseline/additional RPC 均为 0；没有新增 Curve 拆单分支。

p50 为每组中位数，p95 nearest rank（5 次时为最大值），包含本机 HTTP/JSON。debug 构建、
公共 RPC；短样本不代表长时间并发、内存或全平台 SLA，低计算延迟也不代表数据零年龄。
旧 Curve 1e6 raw 粗选采样未改，FRAX→6-decimal token 的微量结果可能为零，不能宣称
所有多跳方向都被当前启发式保留。剩余两个 crypto 模型及 90 行迁移验收另行推进。

持续运行后 readiness 观察到已发布版本 36、3/3 池、区块 26331487、lag 0、age 8 秒，
三个 profile 的余额来源和 A 精度仍分别报告；后台同步成本不算作请求内 RPC。
