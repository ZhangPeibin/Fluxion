# 本地快照 / ParaSwap / 1inch 三方比较（2026-11-03）

## 结论

显式排除旧 Tricrypto 后，其余 209 条池/转换记录全部进入本地快照。三方均 **90/90 成功**，
每行 Fluxion 的 `baseline_rpc_requests` 和 `additional_rpc_requests` 均为 **0**。

| 指标 | Fluxion local_snapshot | ParaSwap | 1inch |
|---|---:|---:|---:|
| 成功 | 90/90 | 90/90 | 90/90 |
| HTTP p50 / p95 | **31 / 3501 ms** | 390 / 864 ms | 467 / 1058 ms |

| Fluxion 相对 API 返回金额 | ParaSwap | 1inch |
|---|---:|---:|
| 中位差距 | −0.000094% | +0.098762% |
| 等权均值差距 | −0.028838% | −0.034276% |
| ±0.5% 内 | 88/90（97.8%） | 84/90（93.3%） |
| 更高 / 相同 / 更低 | 28 / 8 / 54 | 68 / 0 / 22 |

**这证明请求内本地计算已经跑通，但没有证明默认新鲜度下的生产性能目标达成。**
状态年龄 **50–112 秒，中位数 76.5 秒**；实验显式使用 180 秒 / 15 块策略。默认仍为
60 秒 / 3 块。低中位延迟、高尾延迟和较老的状态必须一起报告。

## 池集、同步与覆盖

- `config.benchmark-local.yaml` 保留原 benchmark 的平台、白名单、50,000 流动性门槛、
  max_hops=3、top_k=8 与四分支搜索配置，只按地址排除旧 Tricrypto
  `0x80466c64868e1ab14a1ddf27a676c3fcbe638fe5`。Tricrypto2 仍保留。
- 实际图为 209 条池/转换记录、48 个 token、426 条边：29 V2、169 V3、4 Curve、6 ShibaSwap、
  1 wstETH 转换记录。`/info.config.excluded_pools` 明确公布排除策略。
- 默认 ±2 words 和试验 ±16 words 都曾触发真实 `SnapshotIncomplete`。最终使用
  `v3_full_range: true` 读取全部合法 bitmap words 与 initialized ticks，未跳过失败候选。
  `v3_max_loaded_ticks=250000` 仍为硬上限，不截断数据。结束时观察到 16776 个 initialized ticks。
- V3 读取最多 8 个批次并发，固定同一 EIP-1898 canonical block hash，保持返回顺序和完整
  校验后原子发布。协议全范围完全为空且活跃流动性为零时，才能直接证明流动性不足；缺失
  bitmap word 仍是状态不完整，不能被当成空池。
- 公共 RPC 上全量同步约 46–55 秒，结束状态最近一轮 49135 ms、8 个发布版本、累计 2615 RPC
  尝试、无同步错误。后台 RPC 成本没有计为“请求内零 RPC”，也没有伪装为零状态年龄。
- 本轮每行观测 lag 为 0–5 块，这是相对后台最后观测到的链头，不是连续最新链头探针。

## 时间与原始数据

- 原语料 30 方向 × 3 档固定 raw 输入，共 90 行，无删行；运行约 **222.874 秒**（含等待）。
- 链头起止区块 **26332465–26332484**；实际报价快照 **26332461–26332478**，共使用 5 个版本。
- 35 行选拆单，55 行保留完整基线；实际最多 3 分支，配置上限 4。逐行验证输入分配和输出和
  的整数守恒，以及两项计算 RPC 计数均为 0。
- 基线提交 `9578cf7`，dirty 标志为 true，代码内容指纹
  `7a4b6c1ed7879024dc6d9b62d529728ea6daf093`。
- [逐行 Markdown](2026-11-03-local-threeway.md) / [完整 JSON](2026-11-03-local-threeway.json)
  保存公开配置、原始输入/输出、完整分支、状态年龄、区块/hash、V3 覆盖及外部协议路径。

## 尾延迟：不能只看 p50

| 方向与输入 | HTTP ms | baseline ms | search ms |
|---|---:|---:|---:|
| 33.333… WETH→UNI | 5280 | 13 | 11 |
| 14000 UNI→USDC | 4182 | 12 | 0 |
| 33.333… stETH→WETH | 3918 | 8 | 0 |
| 100000 DAI→USDC | 3625 | 9 | 2 |
| 33.333… WETH→DAI | 3501 | 9 | 24 |

这些尾部样本的差额不在已记录的基线/拆单计时内，也不是请求 RPC 等待。后续分阶段计时、
CPU 采样和同进程停止同步对照已确认，主要干扰来自后台 V3 全量状态校验在 Tokio worker
中连续执行约 5.4 秒，导致请求入口调度停顿；见[尾延迟定位记录](2026-11-03-local-tail-investigation.md)。
此处保留原始未修复版本的全部样本，未通过剔除尾部或替换诊断结果改善 p95。

## 全部超过 ±0.5% 的输出差距

差距定义为 `(Fluxion 输出 / 对手输出 − 1) × 100%`。三方均成功，因此每列分母都是 90。

| 对手 | 方向与输入 | 差距 |
|---|---|---:|
| ParaSwap | 100000 DAI→WETH | −1.127773% |
| ParaSwap | 33.333… stETH→USDC | +0.964655% |
| 1inch | 100000 DAI→WETH | −0.880428% |
| 1inch | 33.333… stETH→LDO | −2.982047% |
| 1inch | 160000 LDO→WETH | −4.707990% |
| 1inch | 3.333… WETH→SHIB | −0.525987% |
| 1inch | 33.333… WETH→SHIB | −2.974928% |
| 1inch | 8000000000 SHIB→WETH | −1.708920% |

外部 API 无法强制固定同一区块，本地状态年龄、外部额外流动性/路由与费用口径都会影响
差距。没有执行 gas/MEV/授权或真实交易模拟。[1inch 官方基础设施费说明](https://business.1inch.com/portal/documentation/faq/infrastructure-fee)
列明不同套餐不同费率；本轮直接比较返回金额，不反推“去费”金额，也不将正差距全部解释
成纯路由优势。此前 RPC 参考轮与本轮时间、池集不同，不能据此宣称同状态无损提速。

## 复现与工程验证

```bash
# .env 提供 GRAPH_API_KEY 与 ONEINCH_API_KEY
cargo run -- --config config.benchmark-local.yaml
# 等待 /ready 为 true，另一个终端：
cargo run --bin bench -- --service http://127.0.0.1:8089 --config config.benchmark-local.yaml \
  --split --delay-ms 600 --out docs/benchmarks/local-threeway-run.md
```

127 项离线测试、严格 Clippy 与构建通过。新增排除策略与完整空 bitmap/缺洞区分的回归；
主网 V3 两项检查通过，包含 30/30 QuoterV2 整数对照和全范围 V3/V2 拆单/多跳逐跳核对。
主网组合检查区块 26332494，hash
`0xf60e24df5bc36df34edbe4c176df91cd895b8d393d7c600cb23bf3d331f0412a`。
