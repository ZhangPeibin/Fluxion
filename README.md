# Fluxion

Fluxion 是一个面向 AI Agent 的 Web3 能力层。它把链上操作封装成稳定的 MCP 能力，目标是全链支持，
而不是绑定某一个网络或协议。

当前实现交付 14 个 capability：本地 wallet / transaction 默认 BSC，Ethereum 通过
`--chain ethereum` 使用；`swap.quote` 仍是 Ethereum 主网报价。报价在远端不可变快照上计算；
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
fluxion wallet balance 0x... --tokens BNB,USDT
fluxion wallet balance 0x... --tokens ETH,USDC --chain ethereum
fluxion wallet assets 0x...
fluxion wallet collectibles 0x...
fluxion wallet connect
fluxion transaction inspect --to 0x... --data 0x...
fluxion transaction simulate --to 0x... --data 0x...
fluxion signature inspect --typed-data typed-data.json
```

`swap.quote` 需要连接 remote 服务：

```bash
FLUXION_REMOTE_URL=http://127.0.0.1:8080 fluxion swap ldo-eth 100 --json
```

### Skills

Skills 告诉 Agent 什么时候该用哪个工具，以及结果怎么读。仓库带了四个：

- [swap](skills/swap/SKILL.md)：报价、拆单和路由结果
- [wallet](skills/wallet/SKILL.md)：余额、资产、授权、NFT 和 WalletConnect
- [transaction](skills/transaction/SKILL.md)：calldata 检查、模拟和 revert
- [signature](skills/signature/SKILL.md)：EIP-712、Permit2、7702 和 4337 授权检查

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
| `wallet.balance` | 固定区块的 BNB/ETH 与 ERC20/BEP-20 余额 |
| `wallet.assets` | 发现代币持仓并在本地核对余额 |
| `wallet.allowance` | owner/spender 的代币授权额度 |
| `wallet.collectibles` | ERC721/ERC1155 持仓 |
| `wallet.connect` | WalletConnect 扫码连接手机钱包 |
| `wallet.status` | 查看本地钱包会话 |
| `wallet.disconnect` | 删除本地钱包会话 |
| `wallet.agent-id` | 解析 BSC 上的 `.ai` Agent ID |
| `wallet.sign` | 通过 WalletConnect 签名消息或 EIP-712 |
| `wallet.send` | 通过 WalletConnect 发送原生或 ERC20/BEP-20 转账 |
| `transaction.inspect` | 签名前解码 calldata，校验 effects / internal calls / 源码，返回证据、覆盖范围与未知项 |
| `transaction.simulate` | 固定区块模拟 calldata，返回结果和 gas |
| `signature.inspect` | 检查 EIP-712、Permit2、7702、4337 授权 |

本地 wallet / simulate 默认 BSC，Ethereum 使用 `--chain ethereum`；`swap.quote` 仍是
Ethereum 主网报价。

计划中：`transaction.send`、`swap.build`、`swap.execute`。`wallet.sign` 只签名消息和
EIP-712 typed data；`wallet.send` 只发送原生/ERC20/BEP-20 transfer，不支持任意 calldata、
授权或 swap 执行。

## 工作方式

每个 capability 的执行位置固定，Agent 和用户都不选择 remote/local：

- `swap.quote` 走 remote。请求只读一个已发布的不可变 snapshot，计算阶段不访问 RPC。
- `wallet.*`、`transaction.inspect` 和 `transaction.simulate` 固定在 local；`wallet.agent-id` 额外使用 SPACE ID API
  并以 BSC 校验。读取和模拟固定在指定区块。
- 响应使用稳定的 JSON schema 和错误码；金额、区块、来源和 coverage 都在结果里。

```text
Agent ──MCP──▶ fluxion mcp ──▶ capability router
                                 ├── swap.quote ─────────▶ remote snapshot ──▶ Ethereum
                                 ├── wallet.* / transaction.* ─▶ local runtime (RPC / WalletConnect)
                                 └── wallet.agent-id ────▶ SPACE ID API + BSC
```

## 安全检测

签名前检查不依赖“让模型看一眼”。`transaction.inspect` / `signature.inspect` 走确定性管线：

1. 解码 calldata 或 typed data；未知 selector / 类型明确标 `unknown`。
2. 固定区块模拟目标交易，解析 logs。
3. 再模拟 `balanceOf` / `allowance` / `ownerOf`，用真实状态变化校验日志；日志造假会触发
   `EFFECT-007/009/010`。
4. RPC 支持时用 `debug_traceCall` 检查 internal delegatecall；不支持就明确
   `coverage.trace=unavailable`。
5. 可选 BscScan / Etherscan 源码验证；没有 key 时 `verified=null`，不猜测。
6. 每个结果带 finding ID、severity、evidence、coverage 和 unknowns。

原则：

- 不输出 `safe`，没有 finding 也只代表“当前覆盖范围内未发现”。
- LLM 不参与 risk 判定；解释是确定性的，宿主 Agent 可以转述。
- advisory only：不签名、不广播、不保管私钥。
- 缺数据就是 `unknown`，不用启发式硬猜。

红队使用 Foundry + Anvil，覆盖 fake logs、hidden delegatecall、proxy upgrade、无限授权、
permit、7702 和 4337。当前语料 **15/15 detect**，**2/2 controls 无误报**。方法和原始数据见
[red-team](docs/benchmarks/redteam-2026-11-11.md) 与
[security benchmark](docs/benchmarks/security-inspection-2026-11-11.md)。

边界：这不是审计或安全保证；BSC public RPC 不提供 trace；真实恶意交易 corpus 仍在扩展。

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

Pre-1.0。本地 wallet / transaction / signature 支持 BSC（默认）和 Ethereum；`swap.quote` 仍是 Ethereum 主网。

- 账户、报价和模拟能力只读；`wallet.sign` 只签名消息/EIP-712；`wallet.send` 只发送
  原生/ERC20/BEP-20 transfer，不能任意合约调用、授权或执行 swap。
- 所有本地 wallet 能力支持 Ethereum 和 BSC；本地工具默认 BSC。
- `transaction.inspect` / `signature.inspect` 是 advisory，不构成安全保证。
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
- 契约：[swap schema](schemas/swap_quote.v1.json) · [wallet schema](schemas/wallet_balances.v1.json) · [allowance schema](schemas/wallet_allowances.v1.json) · [collectibles schema](schemas/wallet_collectibles.v1.json) · [connect schema](schemas/wallet_connect.v1.json) · [status schema](schemas/wallet_status.v1.json) · [disconnect schema](schemas/wallet_disconnect.v1.json) · [agent-id schema](schemas/wallet_agent_id.v1.json) · [sign schema](schemas/wallet_sign.v1.json) · [send schema](schemas/wallet_send.v1.json) · [inspect schema](schemas/transaction_inspect.v1.json) · [transaction schema](schemas/transaction_simulate.v1.json) · [signature schema](schemas/signature_inspect.v1.json)
- Skills：[swap](skills/swap/SKILL.md) · [wallet](skills/wallet/SKILL.md) · [transaction](skills/transaction/SKILL.md) · [signature](skills/signature/SKILL.md)

## 开发

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt
```
