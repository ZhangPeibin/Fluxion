# #32：wstETH 本地整数转换与 stETH 多跳（2026-11-03）

这是 [#31](https://github.com/ZhangPeibin/Fluxion/issues/31) 的
[#32 实施切片](https://github.com/ZhangPeibin/Fluxion/issues/32)。受限示例明确排除 Curve，
不是全平台 90 行或默认模式切换的验收，也不与旧 4–6 秒大图实验直接计算提速倍数。

## 协议核实与整数一致性

```bash
cargo test --test local_wsteth_mainnet -- --ignored --nocapture
```

初步主网检查在区块 `26330992` 确认 stETH 协议版本为 **4**。正式直接转换核对：

- 区块 `26331038`，hash
  `0x89e9597b52aee133b3e56977c00cd32922aad209a0bc342861c8e2426a39f930`。
- 内部 ETH（wei）：`9687025048250923417444097`。
- 内部 shares（raw）：`7760397052110999063417643`。
- unwrap 这份内部 shares 时，协议及本地均精确得到上面的内部 ETH。
  若改用公开 total Ether/total shares 比例，则只得到 `9687025048250923417444096`，少 **1 wei**。
- 双向各 9 个输入：1、2、1e18、1e18+1、100000e18、对应内部整数边界的前/本/后三个值，
  以及 `uint128.max−1`。共 18 个场景：17 个正输出 raw 整数完全相等；1 wei stETH 包装为
  零输出时，两边的报价适配器都拒绝正输出路由。未把这个预期零输出称为成功交易。
- 另对两向 `uint128.max` 非法输入验证均拒绝。当前协议上界是严格小于，不是小于等于。
- 初始化 **1128 ms / 5 RPC**；18 次本地 route 计算 **54–188 μs**，计算阶段 0 RPC。
  初始化包含 header、身份/版本、totals、模型 probe 和最终 header 验证。

本地读取真实状态的精确整数比例，不使用 1:1、浮点数或单位报价反推比例。
完整协议说明和固定源码 revision 见 [LOCAL-STATE](../LOCAL-STATE.md)。

强化零输出错误分类后的复核在区块 `26331105`（hash
`0x224d7aab29a0957697f91a155898c6ff7441e848b5e4fe906fa227066bf96a52`）再次通过 18 个场景。
该次初始化 1165 ms / 5 RPC，本地耗时 175–363 μs；总量比在内部 shares 边界仍少 1 wei。

## 同块转换 + V3 多跳

固定图包含 wstETH 转换、wstETH/WETH V3 和 USDC/WETH V3，区块 `26331039`，hash
`0xbc29aed9e1ce56c624e120635de43f392591d4530d42090616dfe1bf869a9b6d`。

| 输入方向 | 操作序列 | raw 输出 | 本地完整计算 μs |
|---|---|---|---:|
| 33.333… stETH→WETH | wrap → swap | 33321351846128772865 | 741 |
| 33.333… WETH→stETH | swap → unwrap | 33338358197600306197 | 374 |
| 33.333… stETH→USDC | wrap → swap → swap | 75110656945 | 1015 |
| 100000 USDC→stETH | swap → swap → unwrap | 44266240590481463964 | 610 |

四条均通过独立协议 view / QuoterV2 的同块逐跳核对。split=true 结果正确保留一个完整
baseline allocation，转换未被当作独立 AMM 拆单池。初始化 **4443 ms / 13 RPC**；计算阶段
0 RPC；初始化和独立参考网络请求不计入本地计算时间。

## 真实 HTTP

`config.local-wsteth.example.yaml` 使用真实 Graph 发现，mode=mainnet，6 个图记录（含一个
wrapper）。V3 + stETH/wstETH/WETH/USDC 白名单，显式 100 万流动性门槛，不是原全图。
每个方向串行 5 次、共 **20/20 成功**；均使用 split=true。以下为各方向末次响应，记录的
代表区块为 `26331065`，hash
`0xc1ebb87b1e238bcd159473234e1d163f919cc47f841d2f201cee2622db048d2a`。

| 输入方向 | raw 输出 | HTTP p50 / p95 ms |
|---|---|---:|
| 1 stETH→wstETH | 801112520454585405 | 0.6351 / 0.8770 |
| 1 wstETH→stETH | 1248264100818892903 | 0.5385 / 0.5822 |
| 33.333… stETH→WETH | 33321349713906224597 | 0.6913 / 0.7757 |
| 100000 USDC→stETH | 44297236628180456393 | 1.2152 / 1.3137 |

响应的 baseline/additional RPC 均为 0。内部状态为 Ether `9687026246535144261127498`、shares
`7760398012071491444459897`，协议版本 4；两向启用，报价包含对应 wrap/unwrap 操作。
p50 为中位数，p95 为 nearest rank（每组仅 5 次，因此是最大值），包含本机 HTTP/JSON。
debug 构建、公共 RPC。这是短时小样本，不构成长时间、并发、内存或全平台 SLA 证明。
后续 readiness 观察到已发布版本 30、6/6 记录，当前快照 `26331093`、最后观测链头
`26331094`、lag 1、age 17 秒，表明后台持续发布完整状态。低计算延迟不意味着零状态年龄；
报价始终携带实际 block/hash 和新鲜度，允许的滞后受配置策略约束。

## 失败行为

离线 HTTP 验证双向、多金额、转换后输入逐跳传递、最终 min 输出、禁用方向与隐藏虚构
储备；后台比例更新不修改旧读者；缺失状态保留完整旧版。underlying/version/view 不符时
不发布部分状态。运行时版本不匹配可继续重试，恢复兼容版本后能够就绪，不因首次失败
永久停止同步。已有重组和新鲜度规则继续适用。

未知协议版本和 Curve 仍明确报告，不在请求内回退 RPC。完整 90 行迁移验收待 Curve
支持矩阵和本地精算完成后进行，默认模式仍为 RPC。
