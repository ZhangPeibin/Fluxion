# v1 本地交付核对（2026-11-02）

关联需求：[v1 PRD #1](https://github.com/ZhangPeibin/Fluxion/issues/1)、
[基准测试 #29](https://github.com/ZhangPeibin/Fluxion/issues/29)。

本次工作在 `.worktrees/topn-pools` / `feat/topn-pools` 整合。GitHub 的 issue 状态和
`master` 并不自动反映这份本地交付；目前尚未提交、推送或合入 `master`。

## 已完成的工程收尾

- 整合 `feat/bench-harness` 中的语料、工具和历史结果；实际运行完整 90 组主网比较。
- 保留双币/全币白名单，扩大 V2/V3 分页与可配置池上限；默认上限 10000，不代表必然发现这么多池。
- 使用允许单池失败的 Multicall3 分批预热 V2 储备与 V3 `slot0`。
- 修正 Multicall3 动态结果数组解码，并拒绝越界偏移和结果数量不匹配。
- 分开热池刷新与全图快照刷新；近 5 分钟实际访问的 V2 池才加入热刷新，分批最多 400 个。
- 每种跳数的候选保留到精算阶段；精算并发有界；直连兜底不再被空粗选提前阻断，并遵守储备门槛。
- 修复同币种请求绕过零金额、最低输出检查的问题。
- 增加 `--config`、默认服务二进制、`/info`，配置格式错误不再静默使用默认值。
- 基准工具记录完整响应延迟、失败分组、语料、配置、区块范围、逐行时间与代码指纹，拒绝 mock。
- 0x 使用 v2 `/price`、`chainId=1` 与版本头；可选线上适配仍需密钥才能实测。
- 完成从运行实例读取 `/graph` 的图导出 CLI；支持焦点 token、深度、Mermaid 与 HTML。
  它复用服务端发现与过滤结果，避免另建一份可能不一致的图；根目录原型保留。
- 补齐带区块高度、区块哈希和原始 RPC 返回的 V2 fixture，输出断言使用固定预期值。

## v1 验收证据

| 要求 | 实现/验证依据 |
|---|---|
| exact-in HTTP 报价、逐跳信息与价格冲击 | `src/api.rs`、`src/router.rs`，HTTP 行为测试与本轮主网基准 |
| 多跳优于直连、平行池择优 | router 单测；主网 V2/V3/Curve 端到端测试 |
| 多币池展开全部有序对 | graph 3 币 6 边测试 |
| 简单路径、负权场景 | `tests/router_regressions.rs` 的获利环不重复 token 测试 |
| 短路径不被表面更优的长路径挤出 | `tests/router_regressions.rs` 的 top_k=1 反例 |
| 最低输出、零金额、无路径错误 | router/API 测试；同币种边界回归 |
| 静态发现过滤、双币白名单 | discovery 测试与真实 subgraph 发现测试 |
| 储备门槛与直连兜底 | router 直连回归测试；默认 `reserve_ratio_k=0`，需要时配置开启 |
| V2 固定区块 fixture | `tests/fixtures/v2-usdc-weth.json`，区块 26305057 + 哈希 |
| 热集合刷新而非全量储备刷新 | StateCache 冷/热行为测试、主网 Multicall3 刷新测试 |
| 基准测试语料、JSON、Markdown、首次结果 | `bench/corpus.yaml`、`docs/benchmarks/2026-11-02-mainnet.*` |

PRD 的早期字段命名和配置决策以当前 README 为准：HTTP 使用 snake_case；链交互使用
reqwest 手写 JSON-RPC + ABI；密钥仅在 `.env`。候选搜索是启发式的，不是全市场最优保证。

## 验证记录

- `cargo test`：40 项库测试、7 项基准工具测试通过；另有 2 项路由集成回归测试通过。
  主网用例默认 ignored。
- `cargo test -- --ignored`：8 项主网只读集成测试通过。
- `cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、`cargo build --bins`。
- `cargo run -- --help`：确认增加多个 binary 后默认服务入口可用。
- `graph --service http://127.0.0.1:8081 --token <WETH> --depth 1 --out <临时前缀>`：
  导出 45 个节点、408 条边的 Mermaid 和 HTML 文件。
- [90 组主网基准测试与限制](benchmarks/2026-11-02-analysis.md)。

例行代码审查覆盖整合后的已跟踪差异、新 binary、语料与公开配置；发现的默认 binary 启动
回归已修复。未提供密钥的 1inch/0x 线上行为没有验证；未执行交易。

## 学习与后续产品范围

学习检查点仍在 `PLAN.md`，必须由你亲自通过；测试通过不等于学习完成。
拆单、多链、交易执行和协议转换边不属于本次 v1 收尾。基准数据中 stETH/SHIB 大额交易
的尾部差距已经记录，作为下一阶段可量化的改进方向，不宣称已解决。
