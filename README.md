# Fluxion

Fluxion 是面向 AI Agent 的 Web3 能力层。它把报价、账户读取、钱包连接、交易检查和签名授权
封装成稳定的 MCP 能力，让 Agent 能读链、报价、准备交易，并在用户签名前给出可审计的风险报告。

它解决两类问题：

- **Agent 不会用链**：把分散的 RPC、DEX、钱包和数据结构整理成固定输入输出。
- **Agent 不能盲签**：在签名前解码 calldata、模拟状态变化、检查内部调用和源码，并明确哪些
  结论没有覆盖。

[English](README.en.md) · 中文

[![CI](https://github.com/ZhangPeibin/Fluxion/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/ZhangPeibin/Fluxion/actions/workflows/ci.yml)
![status](https://img.shields.io/badge/status-pre--1.0-orange)
![MCP](https://img.shields.io/badge/MCP-stdio-blueviolet)
![Rust](https://img.shields.io/badge/Rust-2024-orange)
![Ethereum](https://img.shields.io/badge/Ethereum-mainnet-627eea)
![BSC](https://img.shields.io/badge/BSC-mainnet-f3ba2f)

## 安装

```bash
cargo install --path crates/client
```

`swap.quote` 需要连接 remote 服务：

```bash
FLUXION_REMOTE_URL=http://127.0.0.1:8080 fluxion swap ldo-eth 100
```

本地 wallet、交易检查和模拟默认使用 BSC；Ethereum 通过 `--chain ethereum` 显式选择。

## 快速开始

### MCP

```bash
fluxion mcp
```

`fluxion mcp` 通过 stdio 暴露能力，不启动 remote 服务。客户端注册和完整配置见
[docs/MCP.md](docs/MCP.md)。

### Skills

Skills 告诉 Agent 何时调用哪个能力，以及如何解释结果。把仓库的 `skills/` 加入客户端配置：

```json
{
  "skills": {
    "paths": ["/path/to/Fluxion/skills"]
  }
}
```

然后可以直接对 Agent 说：

```text
把 100 LDO 换成 ETH，给我最佳报价
查一下 0x... 的 BNB 和 USDT
签名前检查这笔 calldata
检查这个 Permit2 typed data
```

每个能力下面的“对 Agent 说”示例都可以直接使用。

## 能力

所有能力都有固定执行位置。Agent 和用户不选择 remote/local；这是 Fluxion 的内部路由。

### 报价

#### `swap.quote`

Ethereum 主网 exact-in 报价，支持多跳和拆单。返回 `amountOut`、价格影响、路径和每个 hop 的
分配；不构造 calldata，也不执行 swap。

```bash
fluxion swap ldo-eth 100
fluxion swap quote --in LDO --out WETH --amount 100 --json
```

对 Agent 说：

```text
把 100 LDO 换成 ETH，给我最佳报价和路径。
```

Skill：[swap](skills/swap/SKILL.md) · 文档：[swap.quote](docs/capabilities/swap/README.md)

### 账户读取

Skill：[wallet](skills/wallet/SKILL.md) · 文档：[wallet](docs/capabilities/wallet/README.md)

#### `wallet.balance`

在固定区块读取原生币和指定代币余额。默认 BSC，Ethereum 显式指定。

```bash
fluxion wallet balance 0x... --tokens BNB,USDT
fluxion wallet balance 0x... --tokens ETH,USDC --chain ethereum
```

对 Agent 说：

```text
查一下 0x... 的 BNB 和 USDT 余额，固定在最新区块。
```

#### `wallet.assets`

发现地址持有的代币，并在本地逐个核对余额。没有 provider 时回退到内置 registry，并通过
`coverage` / `warnings` 说明完整性。

```bash
fluxion wallet assets 0x...
fluxion wallet assets 0x... --include-unknown
```

对 Agent 说：

```text
看看这个地址持有哪些代币，并标出未知代币。
```

#### `wallet.allowance`

读取 ERC20/BEP-20 授权额度。零授权也会返回，方便判断是否存在 approve 风险。

```bash
fluxion wallet allowance 0xowner --spender 0xspender --tokens USDT,CAKE
```

对 Agent 说：

```text
查一下 owner 给 spender 的 USDT 授权还有多少。
```

#### `wallet.collectibles`

读取 ERC721 / ERC1155 持仓。没有 NFT indexer 时只查显式 collections 或内置 registry。

```bash
fluxion wallet collectibles 0x...
fluxion wallet collectibles 0x... --collections 0xcollection
```

对 Agent 说：

```text
看看这个地址有哪些 NFT；没有 indexer 就只查我给的这个 collection。
```

### 钱包会话与转账

Skill：[wallet](skills/wallet/SKILL.md) · 文档：[wallet](docs/capabilities/wallet/README.md)

#### `wallet.connect`

建立 WalletConnect v2 会话，返回 URI 和终端 QR。默认 BSC，Ethereum 用 `--chain ethereum`。
私钥始终留在手机钱包。

```bash
fluxion wallet connect
fluxion wallet connect --chain ethereum
```

对 Agent 说：

```text
连接我的钱包，用 BSC；先展示二维码，再轮询连接状态。
```

#### `wallet.status`

读取本地 WalletConnect 会话，不访问钱包。

```bash
fluxion wallet status
fluxion wallet status --wait 90
```

对 Agent 说：

```text
我的钱包连上了吗？没连上就继续等 90 秒。
```

#### `wallet.disconnect`

删除本地会话并中止 pending pairing。

```bash
fluxion wallet disconnect
```

对 Agent 说：

```text
断开当前钱包会话。
```

#### `wallet.agent-id`

解析 BSC 上的 SPACE ID `.ai` Agent ID，可选做链上 registry 校验。只做身份解析，不是
Binance Agentic Wallet 登录。

```bash
fluxion wallet agent-id alice.ai
fluxion wallet agent-id 0xA506... --json
```

对 Agent 说：

```text
解析 alice.ai 对应的 BSC 地址和 Agent ID。
```

#### `wallet.sign`

通过 WalletConnect 签名消息或 EIP-712 typed data。手机确认后返回签名；用户拒绝返回
`USER_REJECTED`。

```bash
fluxion wallet sign --message "hello"
fluxion wallet sign --typed-data typed-data.json
```

对 Agent 说：

```text
用我连接的钱包签这条消息：hello。
```

#### `wallet.send`

构造原生币或 ERC20/BEP-20 transfer，本地模拟通过后再让手机确认。失败不会发到手机。

```bash
fluxion wallet send --to 0x... --token BNB --amount 0.01
fluxion wallet send --to 0x... --token USDT --amount 10
```

对 Agent 说：

```text
发送 0.01 BNB 到 0x...，先模拟，失败就不要发。
```

### 交易安全

Skill：[transaction](skills/transaction/SKILL.md) · 文档：[transaction](docs/capabilities/transaction/README.md)

#### `transaction.inspect`

签名前的确定性检查。返回交易意图、资产/授权变化、风险 findings、覆盖范围和未知项。
它是 advisory，不是安全保证，也不签名或广播。

```bash
fluxion transaction inspect --to 0x... --data 0x...
fluxion transaction inspect --to 0x... --data 0x... --policy policy.json
```

对 Agent 说：

```text
签名前检查这笔 calldata：告诉我它在做什么、会动哪些资产、有没有授权或 delegatecall 风险。
```

#### `transaction.simulate`

固定区块执行 `eth_call` 和 `eth_estimateGas`，返回 success/revert、return data 和 gas。

```bash
fluxion transaction simulate --to 0x... --data 0x...
fluxion transaction simulate --to 0x... --data 0x... --from 0x...
```

对 Agent 说：

```text
模拟这笔交易，看看会不会 revert，以及大概要多少 gas。
```

### 签名与授权检查

Skill：[signature](skills/signature/SKILL.md) · 文档：[signature](docs/capabilities/signature/README.md)

#### `signature.inspect`

检查非 calldata 的授权内容：EIP-712 typed data、Permit2、EIP-7702 authorization list 和
ERC-4337 user operation。

```bash
fluxion signature inspect --typed-data permit.json
fluxion signature inspect --authorization-list auth.json
fluxion signature inspect --user-operation userop.json
```

对 Agent 说：

```text
检查这个 Permit2 typed data：金额、过期时间、spender 和链 ID 有没有问题。
```

## 安全模型

Fluxion 的安全检查不依赖“让模型看一眼”。`transaction.inspect` / `signature.inspect` 走确定性
管线：

1. 解码 calldata 或 typed data；未知 selector / 类型明确标 `unknown`。
2. 固定区块模拟目标交易，解析 logs。
3. 再模拟 `balanceOf` / `allowance` / `ownerOf`，用真实状态变化校验日志；日志造假会触发
   `EFFECT-007/009/010`。
4. RPC 支持时用 `debug_traceCall` 检查 internal delegatecall；不支持就明确
   `coverage.trace=unavailable`。
5. 可选 BscScan / Etherscan 源码验证；没有 key 时 `verified=null`，不猜测。
6. 每个结果带 finding ID、severity、evidence、coverage 和 unknowns。

原则：

- 不输出 `safe`；没有 finding 只代表“当前覆盖范围内未发现”。
- LLM 不参与 risk 判定；解释是确定性的，宿主 Agent 可以转述。
- advisory only：不签名、不广播、不保管私钥。
- 缺数据就是 `unknown`，不用启发式硬猜。

红队使用 Foundry + Anvil，覆盖 fake logs、hidden delegatecall、proxy upgrade、无限授权、
permit、7702 和 4337。当前语料 **15/15 detect**，**2/2 controls 无误报**。方法和原始数据见
[red-team](docs/benchmarks/redteam-2026-11-11.md) 与
[security benchmark](docs/benchmarks/security-inspection-2026-11-11.md)。

边界：这不是审计或安全保证；BSC public RPC 不提供 trace；真实恶意交易 corpus 仍在扩展。

## 执行模型

每个 capability 的执行位置固定，Agent 和用户都不选择 remote/local：

- `swap.quote` 走 remote，只读一个已发布的不可变 snapshot。
- `wallet.*`、`transaction.*`、`signature.*` 固定在 local。
- `wallet.agent-id` 额外使用 SPACE ID API，并以 BSC 校验。
- 读取和模拟固定在指定区块；缺失的覆盖范围写入 `coverage` / `unknowns`。

```text
Agent ──MCP──▶ fluxion mcp ──▶ capability router
                                 ├── swap.quote ─────────▶ remote snapshot ──▶ Ethereum
                                 ├── wallet.* / transaction.* / signature.*
                                 │                       ▶ local runtime (RPC / WalletConnect)
                                 └── wallet.agent-id ────▶ SPACE ID API + BSC
```

## 文档

- 接入：[MCP](docs/MCP.md)
- 能力：[swap.quote](docs/capabilities/swap/README.md) · [wallet](docs/capabilities/wallet/README.md) · [transaction](docs/capabilities/transaction/README.md) · [signature](docs/capabilities/signature/README.md)
- 架构：[产品架构](docs/PRODUCT-ARCHITECTURE.md) · [引擎架构](docs/ARCHITECTURE.md) · [本地状态](docs/LOCAL-STATE.md)
- 运维：[HTTP API](docs/API.md) · [配置](docs/CONFIGURATION.md) · [开发](docs/DEVELOPMENT.md)
- 研究：[benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- Skills：[swap](skills/swap/SKILL.md) · [wallet](skills/wallet/SKILL.md) · [transaction](skills/transaction/SKILL.md) · [signature](skills/signature/SKILL.md)

## 状态与边界

Pre-1.0。本地 wallet / transaction / signature 支持 BSC（默认）和 Ethereum；`swap.quote` 仍是
Ethereum 主网。

- `wallet.sign` 只签名消息和 EIP-712；`wallet.send` 只发送原生/ERC20/BEP-20 transfer。
- `transaction.inspect` / `signature.inspect` 是 advisory，不构成安全保证。
- `wallet.connect` 只建立 WalletConnect 会话，私钥留在手机钱包。
- `wallet.agent-id` 只解析 BSC 身份，不是 Binance Agentic Wallet 登录。
- `wallet.assets` / `wallet.collectibles` 的完整发现依赖可选 provider；回退时通过
  `coverage` 和 `warnings` 说明。
- 非 Enumerable ERC721 只返回数量；ERC1155 枚举依赖 indexer。
- 不提供 USD 估值。
- `swap.quote` 需要单独部署 remote 服务。

下一步：`transaction.send`、`swap.build`、`swap.execute`。

## 开发

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt

# Foundry 安全红队
python3 bench/security/redteam.py
```
