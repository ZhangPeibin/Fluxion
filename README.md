# Fluxion

[English](README.en.md) | 中文

**Web3 Agent Capability Infrastructure 的 runtime。** Fluxion 把 Web3 动作暴露为稳定的
capability，由本机 CLI 或 MCP Agent 接入；执行位置（remote 服务或 local runtime）由 Fluxion
内部决定。第一个交付的 capability 是 `swap.quote`：Ethereum 主网的 exact-in 多跳报价、拆单和
状态新鲜度。

> 开源。完整产品愿景见 [Web3 Agent Capability Infrastructure 白皮书](docs/WHITEPAPER.md)。

## Capabilities

| Capability | 状态 | 文档 |
|---|---|---|
| `swap.quote` | available | [docs/capabilities/swap/README.md](docs/capabilities/swap/README.md) |

Capability 定义“做什么”；执行位置由 Fluxion 内部固定：`swap.quote` 走 remote 服务，
`wallet/execute` 走 local runtime。Agent 和用户都不选择执行位置。完整说明见
[capability index](docs/capabilities/README.md)。

## Install / Agent setup

```bash
cargo install --path crates/client
fluxion swap ldo-eth 100 --json
fluxion mcp
```

`fluxion mcp` 是 Agent 接入点。MCP 安装与客户端注册见 [MCP](docs/MCP.md)；`swap_quote`
的字段和错误见 [swap.quote capability](docs/capabilities/swap/README.md) 和
[Agent skill](skills/swap/SKILL.md)。

## Execution model

`swap.quote` 固定使用 Fluxion remote 服务（`FLUXION_REMOTE_URL` / `FLUXION_API_KEY`）。
开发/测试可以把 `FLUXION_REMOTE_URL` 指向 mock 或本地 server；这是 endpoint override，不是模式。
`wallet` / `execute` 等执行能力固定在 local runtime。详见
[execution routing](docs/CONFIGURATION.md#execution-routing)。

## Status

- `swap.quote` 已交付；最新 benchmark：90/90、lag 0/1 块、HTTP p95 133 ms
- `swap.build` / `simulate` / `execute` 和钱包属于后续 Local Runtime
- 当前 Ethereum mainnet、exact-in、只读报价

## Documentation

- **Capabilities**：[index](docs/capabilities/README.md) · [swap.quote](docs/capabilities/swap/README.md) · [skill](skills/swap/SKILL.md) · [schema](schemas/swap_quote.v1.json)
- **Product**：[whitepaper](docs/WHITEPAPER.md) · [architecture](docs/ARCHITECTURE.md) · [local state](docs/LOCAL-STATE.md)
- **Operations**：[MCP](docs/MCP.md) · [HTTP API](docs/API.md) · [configuration](docs/CONFIGURATION.md) · [development](docs/DEVELOPMENT.md) · [viewer](docs/VIEWER.md) · [limitations](docs/LIMITATIONS.md)
- **Benchmarks**：[index](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)

## Development

```bash
cargo build --workspace
cargo test --workspace
cargo test --workspace -- --ignored   # mainnet, read-only
cargo clippy --workspace --all-targets
cargo fmt
```
