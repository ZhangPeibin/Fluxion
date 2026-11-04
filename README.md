# Fluxion

[English](README.en.md) | 中文

**Web3 Agent Capability Infrastructure 的 runtime。** Fluxion 把 Web3 动作暴露为稳定的
capability，由本机 CLI 或 MCP Agent 接入；执行位置（remote 服务或 local runtime）由 Fluxion
内部决定。第一个交付的 capability 是 `swap.quote`：Ethereum 主网的 exact-in 多跳报价、拆单和
状态新鲜度。

> 开源、自托管。完整产品愿景见 [Web3 Agent Capability Infrastructure 白皮书](docs/WHITEPAPER.md)。

## Capabilities

| Capability | 状态 | 文档 |
|---|---|---|
| `swap.quote` | available | [docs/capabilities/swap/README.md](docs/capabilities/swap/README.md) |

Capability 定义“做什么”；执行位置是内部实现，Agent 不选择 local/remote。`swap.quote` 默认
走托管服务，开发/自托管时可用 `FLUXION_MODE=self_hosted` 切到本地 runtime。完整说明见
[capability index](docs/capabilities/README.md)。

## Install / Agent setup

```bash
cargo install --path .
fluxion swap ldo-eth 100 --json
fluxion mcp
```

`fluxion mcp` 是 Agent 接入点；MCP 注册、字段和错误处理见
[swap.quote capability](docs/capabilities/swap/README.md) 和 [Agent skill](skills/swap/SKILL.md)。

开发/自托管：

```bash
FLUXION_MODE=self_hosted GRAPH_API_KEY=... fluxion start
```

## Execution model

`managed` 使用 remote 服务（`FLUXION_REMOTE_URL` / `FLUXION_API_KEY`）；`self_hosted` 使用
本地 runtime（`GRAPH_API_KEY` + RPC）。`FLUXION_MODE=auto` 默认按是否配置 remote URL 选择。
Agent 不参与 provider 选择。详见 [execution routing](docs/CONFIGURATION.md#execution-routing)。

## Status

- `swap.quote` 已交付；最新 benchmark：90/90、lag 0/1 块、HTTP p95 133 ms
- `swap.build` / `simulate` / `execute` 和钱包属于后续 Local Runtime
- 当前 Ethereum mainnet、exact-in、只读报价

## Documentation

- **Capabilities**：[index](docs/capabilities/README.md) · [swap.quote](docs/capabilities/swap/README.md) · [skill](skills/swap/SKILL.md) · [schema](schemas/swap_quote.v1.json)
- **Product**：[whitepaper](docs/WHITEPAPER.md) · [architecture](docs/ARCHITECTURE.md) · [local state](docs/LOCAL-STATE.md)
- **Operations**：[HTTP API](docs/API.md) · [configuration](docs/CONFIGURATION.md) · [development](docs/DEVELOPMENT.md) · [viewer](docs/VIEWER.md) · [limitations](docs/LIMITATIONS.md)
- **Benchmarks**：[index](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)

## Development

```bash
cargo build
cargo test
cargo test -- --ignored      # mainnet, read-only
cargo clippy --all-targets
cargo fmt
```
