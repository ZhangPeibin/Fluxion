# Fluxion

Fluxion 是一个面向 AI Agent 的 Web3 能力层。它把链上操作封装成稳定的 MCP 能力，目标是全链支持，
而不是绑定某一个网络或协议。

当前实现覆盖 Ethereum mainnet，并在 BSC 上提供 Agent ID 解析，已交付 10 个只读 capability、
消息签名和 ETH/ERC20 转账。报价在远端不可变快照上计算；
钱包读取、WalletConnect 会话和模拟在本地执行。完整的 capability 列表见 [能力](#能力)。

[English](README.en.md) · 中文

[![CI](https://github.com/ZhangPeibin/Fluxion/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/ZhangPeibin/Fluxion/actions/workflows/ci.yml)
![status](https://img.shields.io/badge/status-pre--1.0-orange)
![MCP](https://img.shields.io/badge/MCP-stdio-blueviolet)
![Rust](https://img.shields.io/badge/Rust-2024-orange)
![Ethereum](https://img.shields.io/badge/Ethereum-mainnet-627eea)

## 安装

```bash
cargo install --path crates/client
```

## 使用

### CLI

```console
$ fluxion swap ldo-eth 100
LDO → WETH
100 LDO → 0.015452346279312091 WETH
plan: baseline · block 26377973 · lag 0 · 21 ms
```

其它命令：

```bash
fluxion wallet balance 0x... --tokens ETH,USDC
fluxion wallet assets 0x...
fluxion wallet collectibles 0x...
fluxion wallet connect
fluxion transaction simulate --to 0x... --data 0x...
```

`swap.quote` 需要连接 remote 服务：

```bash
FLUXION_REMOTE_URL=http://127.0.0.1:8080 fluxion swap ldo-eth 100 --json
```

### Skills

Skills 告诉 Agent 什么时候该用哪个工具，以及结果怎么读。仓库带了三个：

- [swap](skills/swap/SKILL.md)：报价、拆单和路由结果
- [wallet](skills/wallet/SKILL.md)：余额、资产、授权、NFT 和 WalletConnect
- [transaction](skills/transaction/SKILL.md)：calldata 模拟和 revert

以 opencode 为例，把仓库的 `skills/` 加入配置：

```json
{
  "skills": {
    "paths": ["/path/to/Fluxion/skills"]
  }
}
```

配置后可以直接对 Agent 说：

```text
帮我看看 0x... 的 ETH、USDC 和 NFT
LDO 换 ETH，100 个，报个价
模拟这笔 calldata
连接钱包
```

Skills 只是提示词；执行路由、固定区块和错误码都在 MCP/runtime 里。

### MCP

```bash
fluxion mcp
```

`fluxion mcp` 通过 stdio 暴露能力，不会启动 remote 服务。客户端注册、远程部署和完整配置见
[docs/MCP.md](docs/MCP.md)。

## 能力

| Capability | 说明 |
|---|---|
| `swap.quote` | Ethereum 主网 exact-in 报价，支持多跳和拆单 |
| `wallet.balance` | 固定区块的 ETH/ERC20 余额 |
| `wallet.assets` | 发现 ERC20 持仓并在本地核对余额 |
| `wallet.allowance` | owner/spender 的 ERC20 授权额度 |
| `wallet.collectibles` | ERC721/ERC1155 持仓 |
| `wallet.connect` | WalletConnect 扫码连接手机钱包 |
| `wallet.status` | 查看本地钱包会话 |
| `wallet.disconnect` | 删除本地钱包会话 |
| `wallet.agent-id` | 解析 BSC 上的 `.ai` Agent ID |
| `wallet.sign` | 通过 WalletConnect 签名消息或 EIP-712 |
| `wallet.send` | 通过 WalletConnect 发送 ETH/ERC20 转账 |
| `transaction.simulate` | 固定区块模拟 calldata，返回结果和 gas |

计划中：`transaction.send`、`swap.build`、`swap.execute`。`wallet.sign` 只签名消息和
EIP-712 typed data；`wallet.send` 只发送 ETH/ERC20 transfer，不支持任意 calldata、授权或
swap 执行。

## 工作方式

每个 capability 的执行位置固定，Agent 和用户都不选择 remote/local：

- `swap.quote` 走 remote。请求只读一个已发布的不可变 snapshot，计算阶段不访问 RPC。
- `wallet.*` 和 `transaction.simulate` 固定在 local；`wallet.agent-id` 额外使用 SPACE ID API
  并以 BSC 校验。读取和模拟固定在指定区块。
- 响应使用稳定的 JSON schema 和错误码；金额、区块、来源和 coverage 都在结果里。

```text
Agent ──MCP──▶ fluxion mcp ──▶ capability router
                                 ├── swap.quote ─────────▶ remote snapshot ──▶ Ethereum
                                 ├── wallet.* / simulate ─▶ local runtime (RPC / WalletConnect)
                                 └── wallet.agent-id ────▶ SPACE ID API + BSC
```

## 仓库结构

```text
crates/protocol   capability contract、JSON types、错误码
crates/remote     discovery、snapshot、routing、split search、HTTP API
crates/client     CLI、MCP bridge、wallet、WalletConnect、本地模拟
schemas/          响应 JSON schema
skills/           Agent 使用说明
docs/benchmarks/  benchmark 方法和原始数据
```

链交互使用手写 JSON-RPC 和 ABI 编解码；alloy 只用 `alloy-primitives`。CI 跑 fmt、clippy 和
unit tests。

## 基准

Ethereum mainnet 的 90 行 runtime benchmark（2026-11-06，309 个池子）：90/90 报价成功，
HTTP p50/p95 为 35/197 ms，snapshot lag 0/1/1 blocks。82/90 在 ParaSwap ±50 bps 内，79/90
在 1inch ±50 bps 内。方法和原始数据见 [docs/benchmarks](docs/benchmarks/README.md)。

## 状态

Pre-1.0。当前实现覆盖 Ethereum mainnet，目标支持更多链。

- 账户、报价和模拟能力只读；`wallet.sign` 只签名消息/EIP-712；`wallet.send` 只发送
  ETH/ERC20 transfer，不能任意合约调用、授权或执行 swap。
- `wallet.connect` 只建立 WalletConnect 会话，私钥留在手机钱包。
- `wallet.agent-id` 只解析 BSC 身份，不是 Binance Agentic Wallet 登录，也不检查内盘资格。
- `wallet.assets` / `wallet.collectibles` 的完整发现依赖可选 provider；回退时通过 coverage
  和 warnings 说明。
- 非 Enumerable ERC721 只能返回数量；ERC1155 枚举依赖 indexer。
- 不提供 USD 估值。
- `swap.quote` 需要单独部署 remote 服务。

下一步是 `transaction.send`、`swap.build` 和 `swap.execute`。

## 文档

- 接入：[MCP](docs/MCP.md)
- 能力：[swap.quote](docs/capabilities/swap/README.md) · [wallet](docs/capabilities/wallet/README.md) · [transaction](docs/capabilities/transaction/README.md)
- 架构：[产品架构](docs/PRODUCT-ARCHITECTURE.md) · [引擎架构](docs/ARCHITECTURE.md) · [本地状态](docs/LOCAL-STATE.md)
- 运维：[HTTP API](docs/API.md) · [配置](docs/CONFIGURATION.md) · [开发](docs/DEVELOPMENT.md)
- 研究：[benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- 契约：[swap schema](schemas/swap_quote.v1.json) · [wallet schema](schemas/wallet_balances.v1.json) · [allowance schema](schemas/wallet_allowances.v1.json) · [collectibles schema](schemas/wallet_collectibles.v1.json) · [connect schema](schemas/wallet_connect.v1.json) · [status schema](schemas/wallet_status.v1.json) · [disconnect schema](schemas/wallet_disconnect.v1.json) · [agent-id schema](schemas/wallet_agent_id.v1.json) · [sign schema](schemas/wallet_sign.v1.json) · [send schema](schemas/wallet_send.v1.json) · [transaction schema](schemas/transaction_simulate.v1.json)
- Skills：[swap](skills/swap/SKILL.md) · [wallet](skills/wallet/SKILL.md) · [transaction](skills/transaction/SKILL.md)

## 开发

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt
```
