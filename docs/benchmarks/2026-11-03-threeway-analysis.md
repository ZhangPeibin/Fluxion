# Fluxion / ParaSwap / 1inch 三方报价比较（2026-11-03）

## 结果

**三方均 90/90 成功。Fluxion 报价多数接近 ParaSwap，但完整池集 RPC 模式的延迟仍明显
落后；1inch 在部分大额交易上的输出优势更明显，不能只看中位数或“输出更高”的行数。**

| 指标 | Fluxion | ParaSwap | 1inch |
|---|---:|---:|---:|
| 成功 | 90/90 | 90/90 | 90/90 |
| HTTP p50 / p95 | 5086 / 7004 ms | 362 / 517 ms | 470 / 843 ms |

| Fluxion 相对输出 | ParaSwap | 1inch |
|---|---:|---:|
| 中位数 | −0.001641% | +0.098299% |
| 等权均值 | −0.038418% | −0.061586% |
| ±0.5% 内 | 87/90（96.7%） | 84/90（93.3%） |
| 更高 / 相同 / 更低 | 11 / 21 / 58 | 71 / 0 / 19 |

38 行选择拆单、52 行保留完整基线，实际选中方案最多 3 分支（配置上限 4）。完整运行约
616.856 秒，含等待；起止链头和 Fluxion quote_block 范围均为 **26332104–26332155**。
原始记录以基线提交 `9578cf7`、dirty 标志和代码指纹
`5d7aa2e201c771a8e4f0955791a813f7e677deef` 标识这次代码，包括 1inch 域名修复与新增 `--split`。

### 全部超过 ±0.5% 的样本

| 对手 | 输入方向 | Fluxion 相对输出 |
|---|---|---:|
| ParaSwap | 100,000 DAI→WETH | −1.211445% |
| ParaSwap | 33.333… stETH→USDC | +1.016693% |
| ParaSwap | 400 AAVE→USDC | −1.194655% |
| 1inch | 100,000 DAI→WETH | −0.952748% |
| 1inch | 33.333… stETH→LDO | −2.987733% |
| 1inch | 160,000 LDO→WETH | −5.075069% |
| 1inch | 400 AAVE→USDC | −1.438373% |
| 1inch | 33.333… WETH→SHIB | −2.824932% |
| 1inch | 8,000,000,000 SHIB→WETH | −2.013418% |

ParaSwap 的相关路径出现 Synapse、BalancerV1、Curve StableNG 等当前图之外的来源；1inch
路径出现 Uniswap V4、Fluid、PMM 等额外来源。JSON 保存协议/路径证据，但本轮没有对
这些来源做同块重放或去费归一化，不能把每个差额精确归因到单一因素。大额 LDO 与 SHIB
应列为后续覆盖/路由调查重点，不能由“71/90 高于 1inch”推导全面领先。

## 实验口径

- 使用当前代码、`config.benchmark.yaml` 与原始 `bench/corpus.yaml`，30 个方向 × 3 档
  固定 raw 输入，共 90 行。档位是编写语料时的近似美元量级，不是实验时重新按美元换算。
- Fluxion 完整发现图为 210 条池/转换记录、48 个 token、432 条有向边：V2 29 池、V3 169 池、
  Curve 5 池、ShibaSwap 6 池、wstETH 1 条转换记录。保留全部已发现平台和池。
- **本轮是 `pricing_mode: rpc`、`split=true`，不是全平台 local_snapshot 性能验收。**
  最多四个独立直连池分支，完整多跳/转换路径仍可作为基线。旧 Tricrypto 的本地模型尚未完成，
  因此本轮没有通过删除它或缩减 token 白名单来让 local 模式就绪。
- ParaSwap 使用无密钥价格接口；1inch 使用当前 `api.1inch.com/swap/v6.0/1/quote`，
  请求 `includeProtocols=true`，密钥仅来自 Git 忽略的 `.env`。0x 未参与。
- 每行顺序为 Fluxion → ParaSwap → 1inch；每个外部请求后等待 600 ms。三方使用相同方向和
  raw 输入金额，但外部 API 无法强制固定到同一 canonical block，不能当成同块路由数学比较。
- debug 构建、公共 Ethereum RPC、本机 Fluxion HTTP；请求耗时覆盖发起到完整响应体接收，
  不包含外部请求后的主动等待。p50/p95 使用 nearest rank，延迟统计包含失败请求。
- 输出差距为 `(Fluxion 输出 / 对手输出 − 1) × 100%`，只使用双方成功的行；成功率单列，
  不把失败当作零输出参与均值。均值按行等权，不按交易金额加权。

## 原始记录与复现

- [逐行报告](2026-11-03-threeway-aggregators.md)
- [完整 JSON](2026-11-03-threeway-aggregators.json)：语料、公开配置、代码指纹、每行时间、
  Fluxion 完整响应（区块、分支与搜索指标）、ParaSwap 路径和 1inch 原始响应/协议路径。

```bash
# .env 提供 GRAPH_API_KEY 和 ONEINCH_API_KEY
cargo run -- --config config.benchmark.yaml
# 另一个终端
cargo run --bin bench -- --service http://127.0.0.1:8081 --config config.benchmark.yaml \
  --split --delay-ms 600 --out docs/benchmarks/threeway-run.md
```

## 费用与解释边界

直接比较两家 API 返回的目标金额，不将其视为统一扣除 gas/服务费后的净成交价值。
[1inch 官方基础设施费说明](https://business.1inch.com/portal/documentation/faq/infrastructure-fee)
列明 Dev 套餐稳定币 10 bps、非稳定币 30 bps，不同套餐不同。本轮没有读取账号套餐或
反推“去费”报价；相对 1inch 的正差距可能包含费用口径影响，不能全部归因于路由更好。
也没有模拟授权、实际交易执行、MEV 或跨分支共享池状态。
