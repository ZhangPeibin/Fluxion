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
| `swap.quote` | available | [swap.quote](docs/capabilities/swap/README.md) |

Capability 定义“做什么”；执行位置是 Fluxion 的内部实现，Agent 不需要也不应该选择
local/remote。`swap.quote` 默认走托管服务，开发/自托管时可通过 `FLUXION_MODE=self_hosted`
切到本地 runtime。详见 [execution model](docs/capabilities/README.md)。

## Quick start

### CLI / MCP

首次启动暖 runtime，之后 quote 复用热状态：

```bash
fluxion swap ldo-eth 100
fluxion swap ldo-eth 100 --json
fluxion ldo-eth 100          # 顶层简写

# 开发/自托管：切到本地 runtime
FLUXION_MODE=self_hosted GRAPH_API_KEY=... fluxion start
fluxion status
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

### Self-hosted HTTP server

保留原有 HTTP 服务：

```bash
cp .env.example .env        # 填入 GRAPH_API_KEY
cp config.example.yaml config.yaml
cargo run -- serve --config config.yaml
```

接口、请求/响应和错误见 [HTTP API](docs/API.md)；配置项见
[Configuration](docs/CONFIGURATION.md)。

## Execution and keys

| 内部模式 | `swap.quote` 执行位置 | Key |
|---|---|---|---|
| `managed`（配置 `FLUXION_REMOTE_URL`） | Fluxion 托管服务 | `FLUXION_API_KEY` |
| `self_hosted` | 本地 runtime | `GRAPH_API_KEY` + RPC |

`FLUXION_MODE=auto`（默认）会在配置了 `FLUXION_REMOTE_URL` 时使用 remote，否则使用 local。
这是部署/运维配置，不是 Agent 的 provider 选择。

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
