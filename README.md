# Fluxion

[English](README.en.md) | 中文

**Web3 Agent Capability Infrastructure 的 runtime。** Fluxion 把 Web3 动作暴露为稳定的
capability，供本机 CLI、MCP Agent 或后续的 remote provider 调用。第一个交付的 capability
是 `swap.quote`：Ethereum 主网的 exact-in 多跳报价、拆单和状态新鲜度。

> 开源、自托管。完整产品愿景见 [Web3 Agent Capability Infrastructure 白皮书](docs/WHITEPAPER.md)。

## Capabilities

| Capability | 状态 | Providers | 文档 |
|---|---|---|---|
| `swap.quote` | available | `local-cli`, `local-mcp`；`remote-mcp` planned | [swap.quote](docs/capabilities/swap/README.md) |

Capability 定义“做什么”，provider 定义“在哪、怎么调用”。当前只实现 local provider；
`--provider remote` 返回 `UNSUPPORTED`，不会自动 fallback。完整矩阵见
[docs/capabilities/README.md](docs/capabilities/README.md)。

## Quick start

### CLI / MCP

首次启动暖 runtime，之后 quote 复用热状态：

```bash
export GRAPH_API_KEY=...     # local provider 使用你自己的 Graph/RPC 额度
fluxion start                # 后台预热，首次约 1 分钟

fluxion swap ldo-eth 100
fluxion swap ldo-eth 100 --json
fluxion ldo-eth 100          # 顶层简写

fluxion status               # readiness / lag
fluxion stop
fluxion mcp                  # 本机 Agent 的 MCP stdio server
```

机器接口：

```bash
fluxion swap quote --in LDO --out ETH --amount 100 --json
fluxion capability describe swap.quote --json
```

默认配置 `configs/runtime.yaml`（local snapshot + incremental），默认地址
`http://127.0.0.1:8080`。如果当前目录没有配置，首次 `start` 会把内置默认配置写到
`~/.fluxion/config.yaml`；日志在 `~/.fluxion/runtime.log`。

### HTTP server

保留原有 HTTP 服务：

```bash
cp .env.example .env        # 填入 GRAPH_API_KEY
cp config.example.yaml config.yaml
cargo run -- serve --config config.yaml
```

接口、请求/响应和错误见 [HTTP API](docs/API.md)；配置项见
[Configuration](docs/CONFIGURATION.md)。

## Providers and keys

| Provider | 数据 key | 调用方认证 | 状态 |
|---|---|---|---|
| `local-cli` / `local-mcp` | 用户提供 `GRAPH_API_KEY` + RPC | 无 | available |
| `remote-mcp` | Fluxion 服务端持有 | Fluxion API key / 配额 | planned |

仓库 `.env` 仅用于开发，不分发；不要把任何 secret 写进配置或提交到仓库。

## Latest benchmark

2026-11-04 三方 90 行，209 个池/转换记录，full-range V3：

| 指标 | 结果 |
|---|---:|
| 快照落后区块 p50/p95/max | **0 / 1 / 1** |
| 快照落后时间 p50/p95/max | **0 / 12 / 12 s** |
| Fluxion HTTP p50/p95 | **49 / 133 ms** |
| 成功率 | **90 / 90** |
| ParaSwap 平均偏差 / ±50 bps 内 | **−2.6 bps / 89** |
| 1inch 平均偏差 / ±50 bps 内 | **−8.7 bps / 82** |

拆单已支持互不共享池的多跳路径。剩余大额偏差主要来自 LDO/SHIB 等 venue 覆盖不足，而不是
新鲜度或分配算法。完整方法与原始数据见 [benchmark index](docs/benchmarks/README.md)。

## Documentation

- [Capability index](docs/capabilities/README.md) · [swap.quote](docs/capabilities/swap/README.md) · [Agent skill](skills/swap/SKILL.md)
- [Whitepaper](docs/WHITEPAPER.md) · [Architecture](docs/ARCHITECTURE.md) · [Local state](docs/LOCAL-STATE.md)
- [HTTP API](docs/API.md) · [Configuration](docs/CONFIGURATION.md) · [Development](docs/DEVELOPMENT.md) · [Viewer](docs/VIEWER.md)
- [Benchmarks](docs/benchmarks/README.md) · [Multi-pool](docs/MULTI-POOL.md) · [Split routing](docs/SPLIT-ROUTING.md) · [Quote quality](docs/QUOTE-QUALITY.md)
- [Limitations](docs/LIMITATIONS.md) · [JSON schema](schemas/swap_quote.v1.json)

## Development

```bash
cargo build
cargo test
cargo test -- --ignored      # mainnet, read-only
cargo clippy --all-targets
cargo fmt
```

## Status

Ethereum mainnet、exact-in、只读报价。`swap.build` / `simulate` / `execute` 和钱包属于后续
Local Runtime；当前不构造或模拟交易。共享池的独立报价相加不支持；公开 HTTP 无鉴权，只有
简单限流。详见 [limitations](docs/LIMITATIONS.md)。
