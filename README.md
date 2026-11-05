# Fluxion

**让你的 AI Agent 快速、安全地使用 Web3。**

[English](README.en.md) · 中文

![status](https://img.shields.io/badge/status-pre--1.0-orange)
![MCP](https://img.shields.io/badge/MCP-stdio-blueviolet)
![Rust](https://img.shields.io/badge/Rust-2024-orange)
![Ethereum](https://img.shields.io/badge/Ethereum-mainnet-627eea)

Fluxion 是面向 AI Agent 的 **capability runtime**。Agent 不需要学习每个协议；Fluxion 把
Web3 动作暴露为稳定的 MCP capability。第一个 capability 是 `swap.quote`：Ethereum 主网的
exact-in 多跳报价，带新鲜状态和拆单路由。

> Agent 决定做什么，Fluxion 提供能力，用户保留资产控制权。

## 为什么用 Fluxion

- **一个 Agent 接口** — `fluxion mcp` 把 capability 暴露为 MCP tool，Agent 不需要接每个协议 SDK。
- **快** — 报价来自已经预热的 remote 服务，不是每次冷启动重建。最新 benchmark：
  落后链头 **0/1 块**，**p95 133 ms**。
- **内置安全边界** — 只读行情可以走 remote；wallet、授权、签名和执行留在本地。
  Agent 不选择执行位置。
- **为工具而设计** — 稳定 JSON、错误码、schema 和 Skills，而不是截图和散文。
- **开放可检查** — Rust 引擎、benchmark 语料、原始 JSON、可复现的主网测试。

## 现在能做什么

```bash
fluxion swap ldo-eth 100 --json
```

```json
{
  "schema_version": 1,
  "capability": "swap.quote",
  "token_in":  { "symbol": "LDO",  "address": "0x5a98…", "decimals": 18 },
  "token_out": { "symbol": "WETH", "address": "0xc02a…", "decimals": 18 },
  "amount_in":  { "raw": "100000000000000000000", "decimal": "100" },
  "amount_out": { "raw": "14287837911207341",  "decimal": "0.014287837911207341" },
  "plan_kind": "baseline",
  "provenance": { "execution": "remote", "pricing_mode": "local_snapshot", "lag_blocks": 0 }
}
```

也可以让 Agent 直接调用 MCP tool：

```text
用 Fluxion 的 swap_quote 工具查一下 100 LDO 能换多少 ETH。
```

## 快速开始

### 1. 安装本地 client

```bash
cargo install --path crates/client
```

普通用户只安装 client。`fluxion-remote` 是我们的服务器，通常运行在 Fluxion 基础设施上。

### 2. 启动 remote 服务（本地测试）

```bash
cargo install --path crates/remote
FLUXION_REQUIRE_AUTH=1 fluxion-remote serve --config configs/runtime.yaml
```

remote 负责 discovery、快照预热和路由。`fluxion mcp` 不会启动它。

### 3. 报价

```bash
FLUXION_REMOTE_URL=http://127.0.0.1:8080 fluxion swap ldo-eth 100
```

第一次调用会自动注册安装 token，保存在 `~/.fluxion/auth.json`（0600）。

### 4. 接入 Agent

```json
{
  "mcp": {
    "fluxion": {
      "type": "local",
      "command": ["fluxion", "mcp"],
      "enabled": true,
      "environment": { "FLUXION_REMOTE_URL": "http://127.0.0.1:8080" }
    }
  },
  "skills": { "paths": ["/path/to/Fluxion/skills"] }
}
```

MCP 注册、Skills 和排错见 [docs/MCP.md](docs/MCP.md)。

## Capabilities

| Capability | 状态 | 执行位置 | 文档 |
|---|---|---|---|
| `swap.quote` | available | remote | [swap.quote](docs/capabilities/swap/README.md) |
| `swap.build` | planned | local | — |
| `swap.simulate` | planned | local | — |
| `swap.execute` | planned | local | — |
| `wallet.*` | planned | local | — |

执行位置按 capability 固定，Agent 和用户都不选择 local/remote。
详见 [product architecture](docs/PRODUCT-ARCHITECTURE.md)。

## 工作原理

```text
Agent ──MCP──▶ fluxion mcp ──▶ fluxion-remote ──▶ Ethereum RPC / subgraphs
                  │
                  └── future: local runtime → wallet / authorization / signing
```

`swap.quote` 是只读的，不会签名、授权或广播。

## 性能

90 行、209 个池/转换记录、full-range V3：

| 指标 | 结果 |
|---|---:|
| 快照落后区块 p50/p95/max | **0 / 1 / 1** |
| 快照落后时间 p50/p95/max | **0 / 12 / 12 s** |
| Fluxion HTTP p50/p95 | **49 / 133 ms** |
| 成功率 | **90 / 90** |
| ParaSwap ±50 bps 内 | **89 / 90** |
| 1inch ±50 bps 内 | **82 / 90** |

拆单包含互不共享池的多跳路径。完整方法和原始数据见
[docs/benchmarks](docs/benchmarks/README.md)。

## 仓库结构

```text
crates/protocol   capability 契约、swap DTO、错误码
crates/remote     discovery、快照、路由、HTTP API、fluxion-remote
crates/client     CLI、MCP bridge、remote client、token 存储、fluxion
skills/           Agent 使用说明
schemas/          机器可读 JSON schema
```

## 文档

- **从这里开始**：[MCP](docs/MCP.md) · [swap.quote](docs/capabilities/swap/README.md) · [产品架构](docs/PRODUCT-ARCHITECTURE.md)
- **愿景**：[白皮书](docs/WHITEPAPER.md) · [架构](docs/ARCHITECTURE.md) · [本地状态](docs/LOCAL-STATE.md)
- **运维**：[HTTP API](docs/API.md) · [配置](docs/CONFIGURATION.md) · [开发](docs/DEVELOPMENT.md)
- **研究**：[benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- **边界**：[limitations](docs/LIMITATIONS.md) · [schema](schemas/swap_quote.v1.json)

## 开发

```bash
cargo build --workspace
cargo test --workspace
cargo test --workspace -- --ignored   # 主网，只读
cargo clippy --workspace --all-targets
cargo fmt
```

## 状态

Pre-1.0。当前是 Ethereum mainnet、exact-in、只读报价。`swap.build` / `simulate` /
`execute` 和 wallet 是下一阶段的 local runtime 里程碑。欢迎反馈和 issue。
