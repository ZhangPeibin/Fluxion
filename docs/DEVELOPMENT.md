# Development and testing

```bash
# 单独配置启动一个真实主网实例（默认 :8081；需要 GRAPH_API_KEY）
cargo run -- --config bench/configs/threeway.yaml

# 另一个终端：30 个交易对 × 3 档固定 raw 输入金额
cargo run --bin bench -- --service http://127.0.0.1:8081 --config bench/configs/threeway.yaml

# 从同一实例导出 Mermaid 与交互式 HTML，可用 --token ADDRESS --depth 2 聚焦
cargo run --bin graph -- --service http://127.0.0.1:8081 --out graph
```

`GET /info` 返回主网/mock 模式、图规模及生效配置（不含 RPC URL），基准工具拒绝 mock 模式。
`/quote` 的每跳与 `/graph` 的每条边增加 `operation: swap | wrap | unwrap`；转换边的
`pool` 字段为 wrapper 合约地址，`/graph` 不为转换边伪造储备值。Viewer 为转换边使用虚线。
ParaSwap 无需密钥；1inch/0x 可通过 `.env` 中的 `ONEINCH_API_KEY` / `ZEROX_API_KEY` 启用。
方法、结果与限制见 [docs/benchmarks/README.md](benchmarks/README.md)。

同轮对照：另启动 `cargo run -- --config bench/configs/control.yaml`（:8082），然后运行
`cargo run --bin bench -- --service http://127.0.0.1:8081 --control-service http://127.0.0.1:8082 --config bench/configs/threeway.yaml`。
两实例使用相同语料、相同基础配置；对照实例关闭转换和 ShibaSwap。报告按 stETH、SHIB、其他样本分组，
JSON 保留逐跳路径、对手拆单比例和池信息。详细实现见 [报价质量改进](QUOTE-QUALITY.md)。

多池三方比较使用 `--split`。最新结果见本文前部的 **Benchmark** 章节及
[读写隔离修复实测](benchmarks/2026-11-03-writer-isolation-fix.md)。

## 测试

```bash
cargo test                       # 单元测试（离线）
cargo test -- --ignored          # 真实主网集成测试（只读 eth_call）

cd ui && npm install && npm run build   # 前端构建
```

- 单元：mock `QuoteSource` 的多重图、路由择优、负权/简单路径、编码稳定性。
- 转换与候选回归：整数舍入、方向开关、配置/underlying 校验、转换加多跳组合、API 操作标记，
  以及 `top_k=1` 时转换路径和原有深池路径不被另一类薄池挤掉。
- 主网：V2/V3/Curve 真实报价、V2 vs V3 择优、Curve 3pool、Multicall3 热刷新、
  端到端多跳（V2/V3 + Curve）、subgraph 发现。V2 适配器另有**固定区块 `getReserves`
  fixture** 做不联网的确定性测试。
- 新增固定区块主网核对：wstETH 往返舍入、ShibaSwap 发现与部署 Router 的输出一致性。
- 多池：四池优于两池、整数余数/溢出/相等、预算回退、HTTP 兼容/区块固定/超时/min 输出、mock 离线，
  以及主网每个分支独立精算后总额一致性。

