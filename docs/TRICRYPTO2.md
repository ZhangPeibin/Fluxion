# Tricrypto2 本地 view 精算（#35）

## 已验证的具体部署

本切片只接受 pool `0xd51a44d3fae010294c616388b506acda1bfaae46` 及精确币序
USDT/WBTC/WETH，不把同币种的旧 Tricrypto `0x80466…` 或其他 CryptoSwap 自动归为同一模型。
旧 RPC 模式仍调用部署 get_dy；本地模式把参数同步放到后台，计算器不持有 RPC。

该 pool 的 `get_dy(uint256,uint256,uint256)` 调用固定 Views，Views 再调用固定 Math：

| 合约 | 地址 | runtime code hash（两份原始快照均一致） |
|---|---|---|
| Pool | `0xd51a44d3fae010294c616388b506acda1bfaae46` | `0x9cb858c1d8e8ab8ba7dc273f146d428965b0ca686d05fb02cb4ffc62149ec928` |
| Views | `0x40745803c2faa8e8402e2ae935933d07ca8f355c` | `0x0b6010f39f5d26850f80f06fe69f0886bb50618ab98ddeda4c46a409b0ef6fe2` |
| Math | `0x8f68f4810cce3194b6cb6f3d50fa58c2c9bdd1d5` | `0x7bead5c85d7bce833e933c384038394a1b5c0200d8a0f0b78e9b93e820d583c8` |

Views 的 math 字段仅构造赋值，构造参数也是上面的 Math。审读的 Etherscan decoded source
SHA256（保留 CRLF，指纹不等于源码编译认证）：

- Pool：`9acc8ccc88aa39fadca4e7a4a5e028e35a4f55e296f9e9324770ce633e6bf275`
- Views：`9d98d6371b392dc035db1f55fc1c3f98377f8c309b6f3eda81be405bab82662b`
- Math：`037886074f0c14606d7355f6086ee0bec6577944c936cafeb1075e7b7b5113d4`

来源是各地址的 Etherscan code 页面，模型另有独立固定块向量、同块 probe 和实时核对。
未声称在每次装载时执行 bytecode 等价认证，也未覆盖 helpers 不同的其他部署。

## 状态：stored D，不是重新拟合的不变量

每个目标 canonical block hash 读取 14 项：币序 3、存储余额 3、有效 A/gamma、stored D、
price_scale 2、mid_fee/out_fee/fee_gamma。A/gamma getter 已应用目标块时间下的 ramp。

**这个部署的 Views 始终把 pool.D() 传给 newton_y。** 本模型不以 newton_D、余额之和或
浮点拟合的 D 替换；exchange 的状态更新与 view 不完全相同，本切片不是交易模拟。

状态、币序、模型探针全部验证后才原子发布，与经典 3pool、V2/V3、wstETH 使用同一快照
契约。旧读者保留自己的 D/价格尺度/余额；失败不发布半成品，也不转成请求内 RPC。

## 精确整数流程

1. 输入先加到对应 **raw 余额**，再整体规范化；不能把分别取整的 cached xp 与 dx 相加。
   precisions 固定 `[1e12,1e10,1]`。WBTC/WETH 依次乘 price_scale、precision，再除 1e18。
2. newton_y 保留输出位置排除、降序排序、小值先初始化 y、大值先初始化 K_i、逐步 checked
   uint256 运算、halving 分支和至多 255 次循环。
3. 保留严格收敛比较和 Math 的范围约束：ANN 2700…270000000、gamma 1e10…5e16、
   D 1e17…1e33，参与/结果余额相对 D 的规范化 fraction 在 1e16…1e20 范围。
   A() 已是带 N^N 和 A_MULTIPLIER=10000 的 ANN，**不能再乘 N 或 N^N**。
4. dy 减一，依次按 price_scale 与 precision 换回 raw 输出，再扣 fee。
5. 动态 fee 用输出替换为 y 后的 xp 计算 reduction_coefficient，保留原币序逐步乘除，
   按 mid/out fee 混合。不能缓存池的一个静态 fee() 值推广到所有金额。

构造时还校验该具体池的可达费率参数：mid_fee 不高于 out_fee，fee_gamma 严格在
0…1e18 之间（端点不含），out_fee 不低于 500000；pool 的 gamma 上界 1e16 比通用 Math
上界更窄。初始化参数及治理更新规则均符合此范围，畸形参数不以
“所有 probe 都 revert”冒充可用快照。

源码定义的安全范围/不收敛/revert 都是可观察行为，不放宽容差或编造零输出。独立数据
同时包含成功零输出和真正 revert，HTTP 必须区分。已知未初始化的 D/空余额可作为无流动性
状态记录；不意味着缺少参数或可以伪造可用报价。

## 元数据和路由

`curve_state` 增加 `profile=tricrypto2`、`balance_source=stored` 和
`fee_model=dynamic-post-trade`。静态 fee 字段对本 profile 省略；`crypto` 对象包含 gamma、
stored_d、两个 price_scale、mid/out/fee_gamma 和 helper 地址。

amplification 是原始 ANN，a_precision=10000 表示本 profile 的 A_MULTIPLIER；它并不是
普通稳定池 A()/A_precise() 的同一种归一化约定。原有经典 3pool 的 fee 仍按原字符串返回。

Curve 仍不是独立拆单分支，只作为完整单路径/多跳基线参与 split=true。保持 min 输出、
hop/operation、储备阈值及既有 1e6 raw 粗选采样。微量采样可能为零或失败，因此不保证
所有可行多跳都被有界候选搜索保留。

## 重现和范围

```bash
# 捕获真实部署状态、原始 view 输出和失败标志，不需要 Graph key
cargo run --example tricrypto2_snapshot -- --block 20000000
cargo run --example tricrypto2_snapshot -- --block 26331640

cargo test --test local_tricrypto2 --test local_tricrypto2_http
cargo test --test local_tricrypto2_mainnet -- --ignored --nocapture

# 显式单部署 HTTP 图，默认 :8088，无 Graph key，不运行全注册表发现
cargo run --example local_curve_server -- --tricrypto2 --config tests/configs/tricrypto2.yaml
```

两份固定块 fixture 共 104 条（90 成功、14 预期 revert），另有当前主网和组合路径核对。
[实测记录](benchmarks/2026-11-03-local-state-tricrypto2.md) 明确区分初始化、计算和 HTTP。

#34 两个两币模型与 #35 已联合集成，默认五个 Curve 部署可覆盖 4 个；离线 HTTP
覆盖四模型同批装载，实时组合测试也同时装载四个 Curve 部署、V3 与 wstETH。
旧 Tricrypto 仍需独立模型。当前不切换默认 RPC 模式，
不以单部署短样本替代全平台 90 行迁移验收。
