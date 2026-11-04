# Curve 部署支持矩阵：三个稳定池与 Tricrypto2（#33–#35）

## 先按部署识别，不能按 ABI 猜公式

默认注册表 `[0,5]`、现有 all-token allowlist 和发现规则在区块 **26331147** 找到 5 个池。
hash `0x71a65ca350e51d79469e037fb86aa86f098587988f7311b6b4e0017214c41618`，结束时校验一致。
[原始清点](benchmarks/2026-11-03-curve-inventory.json) 保存币序、接口数值和 runtime bytecode
指纹，共 322 次 RPC 尝试（含发现、可选接口探测、code 和 header）。这是清点成本，不是询价成本。

| 部署 | 币序 | 清点时 A / A_precise | 本地支持 |
|---|---|---|---|
| `0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7` | DAI / USDC / USDT | 4000 / 不提供该 getter | **经典 3pool 已实现**，A 精度 1 |
| `0x1005f7406f32a61bd760cfa14accd2737913d546` | USDC / USDT | 1500 / 150000 | **2CRV 已实现**，A 精度 100、实时净余额 |
| `0xdcef968d416a41cdac0ed8702fac8128a64241a2` | FRAX / USDC | 1500 / 150000 | **FRAX/USDC 已实现**，A 精度 100、存储余额 |
| `0x80466c64868e1ab14a1ddf27a676c3fcbe638fe5` | USDT / WBTC / WETH | 3645 / 364500 | tricrypto，未实现 |
| `0xd51a44d3fae010294c616388b506acda1bfaae46` | USDT / WBTC / WETH | 1707629 / 探测未成功 | **Tricrypto2 本地 view 已实现** |

可选接口探测的 null 仅表示该次调用未成功，不能单凭它证明某个部署的完整接口/公式。
两个 crypto 部署都返回 gamma、price_scale、mid_fee/out_fee。Tricrypto2 已完成固定 Views/Math
调用链审读与本地模型；旧 Tricrypto 仍须独立核实，不能混用经典 StableSwap 或 Tricrypto2。
同样，A_precise=100×A 不足以证明两个稳定池的余额、rates、费用与迭代公式完全相同。

只对四个已核实的**具体地址和精确币序**启用对应本地模型。其他池继续留在默认发现图里，
本地模式整体报告 UnsupportedLocalPool，不会自动删池或改成请求内 RPC。原 RPC 模式支持
范围保持不变。工厂/元池、LP/underlying 兑换、原生 ETH 均不在本切片范围。

## 经典 3pool 模型

公式依据固定 revision 的
[Curve 3pool 源码](https://github.com/curvefi/curve-contract/blob/574f44027d089de0eac765f5a74ea5ae96aba968/contracts/pools/3pool/StableSwap3Pool.vy)，
并用部署 get_dy 的固定块向量与实时只读核对验证。清点指纹不是对源码编译结果的正式认证。
该具体部署是非代理池；本实现没有根据某个通用 getter 自动接受新部署，也未声称逐块验证
其 runtime bytecode 与源码等价。

- 固定币序 DAI/USDC/USDT，rates `[1e18,1e30,1e30]`，规范化精度 `1e18`。
- 读取池的存储 `balances(i)`，不把包含 admin fees 的 ERC20 balanceOf 当成 LP 余额。
- 读取同块 `A()`（本部署精度为 1）与 `fee()`。A() 已包含目标区块时间的 ramp 插值；
  本地不使用请求墙钟时间外推。Ann 为该链上 A 值乘 3，不套用另一个 A_PRECISION=100 模板。
- 独立实现 checked uint256 的 D/y 迭代，保留逐步乘除、减一保护、至多 255 轮和原收敛规则。
  不能任意合并乘除或扩大中间运算以放行原 view 会溢出的输入。
- D 只依赖快照余额/A，可以在构造时缓存；每次报价仍按实际金额计算 y，不是有限金额报价表。
- **本票复现 get_dy 的 view 顺序**：先把 `xp[j]−y−1` 转换到输出 raw units，再扣 fee。
  此部署 exchange 的费用/单位转换顺序不同，不能把本结果称为完整交易模拟。
- 空余额是已知的无流动性状态，不等于漏读；有完整空状态时可发布，但不能虚构交换输出。

加载器按同一 EIP-1898 blockHash 读取币序、三项余额、A、fee，并用六币向的微量/不整除金额
get_dy probe 核对模型。身份、ABI、状态或 probe 不符时不发布半成品。新区块重新装载并
重建 D；旧读者保持旧余额/A/fee。现有 canonical hash、重组代际、新鲜度与失败回退规则不变。

## Tricrypto2：stored D、专用求根与动态费率

本部署 get_dy 委托固定 Views，Views 使用指定 Math 的 newton_y 和 pool stored D，不在本地
重算 D。输入先加到 raw 余额再整体规范化，输出减一后依次换回 raw units，动态 fee 基于
把输出位置替换为 y 的 xp。A() 已是带 N^N/A_MULTIPLIER 的 ANN，不能按稳定池再乘 N。
范围、halving、严格收敛和 checked uint256 次序均保留，成功零输出与真正 revert 分别处理。

状态元数据标记 fee_model=dynamic-post-trade，省略静态 fee，并在 crypto 对象提供 gamma、
stored D、两个 price_scale、mid/out/fee_gamma 和 helper 地址。完整来源与模型契约见
[TRICRYPTO2](TRICRYPTO2.md)，独立向量/主网/HTTP见
[实测](benchmarks/2026-11-03-local-state-tricrypto2.md)。

## 两币稳定池：保留不同的余额和 D 迭代规则

两个部署均读取同块 `A_precise()`，A_PRECISION=100。不能改成已经截断的 `A()*100`：
ramp 期间 precise 值可能带有不足一个 A 单位的部分。状态元数据的 amplification 是原始
A_precise 值（例如 150000），配合 a_precision=100 解读。

| profile | 余额 getter 语义 | rates | D product 次序 |
|---|---|---|---|
| `plain-2crv` | ERC20 balanceOf(pool) − admin_balances | [1e30,1e30] | D×D/x0×D/x1/4 |
| `plain-frax-usdc` | 存储的 LP balances | [1e18,1e30] | 逐次 D_P×D/(x×2) |

两种 D 次序不能做整数意义上的代数替换。y 系数中的 A_PRECISION 缩放也保留原次序；
get_dy 先在规范化 units 扣 fee，再转换到输出 raw units，不能套用经典 3pool 的 view 顺序。
两者的 D/y 至多 255 轮，不收敛时拒绝；经典 3pool 返回末次迭代的行为保持不变。

2CRV 的净余额会在外部 token 转账时变化，即便没有 pool swap 事件。因此后台每块读取
协议的 balances getter，不自己缓存“仅交易事件推演”的余额。其 immutable rates 在该
部署的构造参数解码中确认。FRAX/USDC 不用带 admin 余额的 ERC20 balanceOf 替代存储余额。

来源与证据（源码指纹为浏览器 decoded content，保留 CRLF；不是编译等价证明）：

- [2CRV](https://etherscan.io/address/0x1005f7406f32a61bd760cfa14accd2737913d546#code)：
  Etherscan 标记 Exact Match、Vyper 0.3.1；source SHA256
  `2633b4e1c6a548a67d8c85b66217d5f856f282028dd4df782764e7149212938b`。
  构造参数确认两个 rates 均为 1000000000000000000000000000000。
- [FRAX/USDC](https://etherscan.io/address/0xdcef968d416a41cdac0ed8702fac8128a64241a2#code)：
  Etherscan 标记 **Similar Match**，模板 `0x9ac7dd81a1a749513e4f953d1c7eb3e237114575`，不是 Exact Match；
  审读 source SHA256 `a36ea2025bfbaf9325d054c1d51a2bd2bc34f38bc963def9eafa2c94ad0a45c7`。
  本地模型还通过固定块独立向量、同块多向 probe 和实时原始 get_dy 核对，不冒充正式源码认证。

加载器支持不同币数的同批布局，不把 2 币数据按 3 币偏移读取。元数据必须全部成功；
probe 区分“模型预测的协议 revert”与意外失败，RPC 传输失败不会降格为已验证的 view revert。
任何币序、参数、probe 不一致不部分发布。未知地址即使接口相似仍不被自动接受。

## 路由和 HTTP

三币池产生 6 条有向边，精算索引不能仍限制成 0/1。Curve 不作为新增独立拆单分支，而是
作为完整单路径/多跳基线参加 split=true 比较；min 输出检查仍针对所选总输出。

`state.curve_state` / `/state.curve_state` 提供 pool/profile、币序、余额来源、有效余额、A、A 精度、
fee 分母和流动性标志。余额/参数均用 decimal raw strings；无 Curve 的旧报价不增加该字段。
本地 Curve 的粗选 reserve_in 保持旧 RPC 适配器的哨兵约定，避免悄悄改变显式储备阈值/
惩罚下的多跳行为。图展示与状态元数据独立使用协议有效余额，不把哨兵显示成真实储备。

粗选仍保持旧适配器的 1e6 raw-unit get_dy 采样习惯。因此 DAI→6-decimal token 的微量结果
可能为零；FRAX 也存在同样情况。本票没有改写候选算法或宣称解决全部粗选质量问题。
直接边仍有既有兜底，不保证所有方向的多跳都能被当前微量采样保留。

## 重现

```bash
# 原有发现规则的只读部署清点，无 Graph key
cargo run --example curve_inventory -- --block 26331147

# 捕获链上状态与 get_dy 原始向量（--out 可写入调用方指定位置）
cargo run --example curve_snapshot -- --block 20000000
# 独立 Tricrypto2 状态/成功与失败 view 向量
cargo run --example tricrypto2_snapshot -- --block 26331640
cargo run --example curve_snapshot -- --pool 2crv --block 26331147
cargo run --example curve_snapshot -- --pool frax-usdc --block 26331147

# 固定已核实部署的 HTTP 示例，默认 :8086，无 Graph key
cargo run --example local_curve_server
curl -s localhost:8086/state
# 显式 Tricrypto2-only HTTP 示例，默认 :8088，不运行全注册表发现
cargo run --example local_curve_server -- --tricrypto2 --config tests/configs/tricrypto2.yaml
# 三个已核实稳定池的固定图，默认 :8087，不运行全注册表发现
cargo run --example local_curve_server -- --all-stable --config tests/configs/curve-stable.yaml
```

HTTP 示例使用 `tests/configs/curve.yaml`，**显式只组装经典 3pool**，不运行注册表发现。
`--all-stable` 显式选择三个稳定池，单独 `--tricrypto2` 仅选择 Tricrypto2，两者同时传入则
选择四个已核实部署。不要把示例当作默认全图实例；直接用普通 main 发现五池时，旧
Tricrypto 仍明确阻止完整本地快照。平台与 all-token allowlist
仍须允许所有选中池的币，不修改生产发现/白名单规则。

已有链上固定块 fixture：`20000000`（A=2000、fee=1000000）与 `26331147`（A=4000、fee=1500000），
每块 32 条原始向量共 64 条，覆盖六方向、不同精度、微量零输出和不整除数量。
离线 HTTP 还验证按批装载、真实历史参数更新、旧读者、min 输出、Curve→V2 多跳以及未知
部署不被静默删除，并覆盖自定义储备阈值的兼容性。主网另核对六方向及 V2/V3/wstETH 组合路径，见
[3pool 实测](benchmarks/2026-11-03-local-state-curve.md)。两个两币池各在 20000000/26331147
保存原始 fixture，共新增 58 条，包括微量零输出与相对余额极大的输入；见
[两币池实测](benchmarks/2026-11-03-local-state-curve-plain.md)。

## 下一阶段

独立核实旧 Tricrypto。只有默认全池集都可完整覆盖后，才执行同块/
同候选规则的 90 行迁移验收并考虑切换默认模式。本切片不是 #31 全部完成。
