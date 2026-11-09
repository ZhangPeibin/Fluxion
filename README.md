# Fluxion

Fluxion 是一个开源的 Web3 能力层，给 AI Agent 用。它把常用的链上操作封装成 MCP 工具，
Agent 不需要为每个协议单独写集成，也不用接触私钥。

目前跑在 Ethereum 主网上，所有能力都是只读的：报价、余额、代币和 NFT 持仓、授权查询、
calldata 模拟。签名、授权和交易执行还没有实现。

[English](README.en.md) · 中文

![status](https://img.shields.io/badge/status-pre--1.0-orange)
![MCP](https://img.shields.io/badge/MCP-stdio-blueviolet)
![Rust](https://img.shields.io/badge/Rust-2024-orange)
![Ethereum](https://img.shields.io/badge/Ethereum-mainnet-627eea)

## 现在能做什么

| 能力 | 做什么 |
|---|---|
| `swap.quote` | 获取 Ethereum 主网的 exact-in 报价，支持多跳和拆单 |
| `wallet.balance` | 查询指定代币在固定区块的余额 |
| `wallet.assets` | 发现地址持有的 ERC20，并在本地逐个核对余额 |
| `wallet.allowance` | 查询 owner/spender 的 ERC20 授权额度 |
| `wallet.collectibles` | 查询 ERC721/ERC1155 持仓 |
| `wallet.connect` | 用 WalletConnect 扫码连接手机钱包 |
| `wallet.status` | 查看本地钱包会话 |
| `wallet.disconnect` | 删除本地钱包会话 |
| `transaction.simulate` | 在本地模拟一笔 calldata，返回执行结果和 gas |

`swap.build`、`swap.execute` 等写操作还在计划中。

## 安装和使用

```bash
cargo install --path crates/client
fluxion mcp
```

`fluxion mcp` 把能力暴露成 MCP tool，本身不会启动远程报价服务。远程服务的部署、MCP
客户端注册和 Skills 配置见 [docs/MCP.md](docs/MCP.md)。

也可以直接当命令行工具用：

```bash
fluxion swap ldo-eth 100
fluxion wallet balance 0xabc... --tokens ETH,USDC
fluxion wallet collectibles 0xabc...
fluxion transaction simulate --to 0xabc... --data 0x...
```

`swap.quote` 需要连接远程服务：

```bash
FLUXION_REMOTE_URL=http://127.0.0.1:8080 fluxion swap ldo-eth 100 --json
```

## 为什么这样拆分

报价计算需要持续同步池子、维护快照和搜索路由，放在远程服务里更合适。请求进来时只读一个
已经发布的快照，计算过程中不访问 RPC。

钱包读取、WalletConnect 会话和模拟需要贴近用户环境，所以放在本地。普通读取不接触私钥；
`wallet.connect` 只保存会话，签名仍在手机钱包里完成。

每个能力的执行位置是固定的，Agent 和用户都不需要选择 remote 或 local，也不存在两边自动
fallback。

## 架构

```text
Agent (opencode / Claude / Codex)
  │  MCP stdio
  ▼
fluxion mcp
  ├── swap.quote ─────────────────────► Fluxion remote ──► Ethereum RPC / subgraphs
  └── wallet.* / transaction.simulate ─► local RPC
```

- `swap.quote` 的远程服务负责 discovery、快照和路由，只读。
- `wallet.*` 和 `transaction.simulate` 在本地执行，只读。
- 输出都是结构化 JSON，字段和错误码有 schema 约束。

更完整的职责划分见 [产品架构](docs/PRODUCT-ARCHITECTURE.md)。

## 文档

- 接入：[MCP](docs/MCP.md)
- 能力：[swap.quote](docs/capabilities/swap/README.md) · [wallet](docs/capabilities/wallet/README.md) · [transaction](docs/capabilities/transaction/README.md)
- 架构：[产品架构](docs/PRODUCT-ARCHITECTURE.md) · [引擎架构](docs/ARCHITECTURE.md) · [本地状态](docs/LOCAL-STATE.md)
- 运维：[HTTP API](docs/API.md) · [配置](docs/CONFIGURATION.md) · [开发](docs/DEVELOPMENT.md)
- 研究：[benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- 契约：[swap schema](schemas/swap_quote.v1.json) · [wallet schema](schemas/wallet_balances.v1.json) · [allowance schema](schemas/wallet_allowances.v1.json) · [collectibles schema](schemas/wallet_collectibles.v1.json) · [transaction schema](schemas/transaction_simulate.v1.json)
- Skills：[swap](skills/swap/SKILL.md) · [wallet](skills/wallet/SKILL.md) · [transaction](skills/transaction/SKILL.md)

## 状态和限制

项目还在 pre-1.0，只支持 Ethereum 主网。账户和模拟能力只读；`wallet.connect` 只建立本地
WalletConnect 会话。

- 不能签名、授权或广播交易。
- `wallet.assets` 和 `wallet.collectibles` 的完整发现依赖可选 provider；没有 key 时会退回
  registry 或显式 collection，并通过 `coverage` 和 `warnings` 说明。
- 非 Enumerable 的 ERC721 只能返回持仓数量，拿不到 token id；ERC1155 枚举依赖 indexer。
- 不提供 USD 估值。
- 远程报价服务需要单独部署。
- 报价 benchmark 和复现方法见 [docs/benchmarks](docs/benchmarks/README.md)。

## 开发

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt
```
