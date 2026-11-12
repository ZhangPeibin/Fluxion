# Fluxion

Fluxion 是面向 AI Agent 的 Web3 能力层。它把报价、账户读取、钱包连接、交易检查和签名授权
封装成稳定的 MCP 能力，让 Agent 能读链、报价、准备交易，并在用户签名前给出可审计的风险报告。

它解决两类问题：

- **链上接口碎片化**：把分散的 RPC、DEX、钱包和数据结构整理成固定输入输出。
- **签名前缺少可验证的检查**：在签名前解码 calldata、模拟状态变化、检查内部调用和源码，并
  明确哪些结论没有覆盖。

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

`transaction.inspect` / `signature.inspect` 只做**签名前检查**。它们不持有私钥、不签名、
不广播，也不构成审计或安全保证。检查结果用于辅助调用方和用户决定是否进入钱包确认流程。

### 信任边界

| 组件 | 信任假设 | 失败/限制 |
|---|---|---|
| 调用方（Agent 或用户） | 不可信规划器；可以构造任意 calldata，也可以绕过本地 MCP 直接调用 CLI 或 RPC | 不作为安全边界 |
| 手机钱包 / WalletConnect | 私钥和最终签名确认只在钱包内 | 拒绝返回 `USER_REJECTED`；超时返回 `TIMEOUT` |
| Local runtime | 无密钥，只读 RPC；进程和配置由本机控制 | 不承诺防御本机恶意进程或已被篡改的二进制 |
| RPC | RPC 返回用于区块、代码、模拟和 trace；没有轻客户端证明 | 不可用时返回 `RPC_ERROR` 或明确 `unavailable` |
| Remote quote service | 只返回已发布 snapshot 的报价 | 不可用时返回 `RPC_ERROR`，不回落本地报价 |
| Source verifier | 可选 BscScan / Etherscan；返回 `verified: true/false/null` | 无 key 或请求失败时为 `null`，不推断 |
| Trace backend | 可选 `debug_traceCall`；公共 BSC RPC 不提供 | 不支持时 `coverage.trace = unavailable` |

### 威胁模型

覆盖范围：

- 恶意或错误的 calldata：未知 selector、参数错位、隐藏 multicall、代理升级。
- 资产和授权：超出意图的转账、伪造 Transfer/Approval 日志、无限授权、NFT operator 授权。
- 合约执行：非预期 internal `DELEGATECALL`、未验证源码、EIP-1967 proxy。
- 地址和策略：零地址、burn 地址、recipient mismatch、policy cap/allowlist。

不覆盖：

- 私钥泄露、恶意钱包 App、已被控制的签名设备。
- 协议级经济攻击、预言机操纵、MEV、清算和跨链桥风险。
- 未提供的链下签名、非标准 token 的完整语义、真实世界的全部攻击模式。
- “没有 finding”不等于安全；它只表示当前覆盖范围内没有发现。

### 检查管线

| 阶段 | 输出 | 失败/降级 |
|---|---|---|
| 规范化与区块固定 | `chain`、`block_number`、`block_hash` | RPC 失败 -> capability error |
| Decode | selector、signature、参数、未知项 | 未知 selector -> `decode=partial` + `unknown` |
| Metadata | EOA/contract、EIP-1967 proxy/implementation | 代码或 storage 读取失败 -> error/unknown |
| Simulation | call status、logs、effects | trace/RPC 不支持 -> `simulation=unavailable/partial` |
| Effect verification | `balanceOf`、`allowance`、`ownerOf` 前后状态 | 非标准 token -> verified=false/unknown |
| Internal trace | internal calls、delegatecall | 无 `debug_traceCall` -> `trace=unavailable` |
| Source verification | `verified`、provider source | 无 key/provider 失败 -> `verified=null` |
| Detectors | finding ID、severity、evidence、confidence | 缺前置数据 -> 不猜，写 `unknown` |
| Policy | cap/allowlist 判断 | 缺 effects 或 policy -> `not_configured`/`partial` |

### Finding 和风险语义

每个 finding 固定包含：

- `id`：稳定规则 ID，例如 `AUTH-001`、`EFFECT-007`、`CONTRACT-004`。
- `category`：approval、asset-flow、contract、address、swap、signature、policy 等。
- `severity`：`critical | high | medium | low`。
- `evidence`：地址、金额、selector、日志、trace 或 provider 结果。
- `confidence`：`high | medium | low`。
- `deterministic`：规则本身是否确定性执行。

聚合规则：

1. 有 finding 时，`risk` 取最高 severity。
2. 没有 finding 时为 `unknown`，而不是 `low` 或 `safe`。
3. 关键覆盖缺失时，保留 `unknown`，不能降级成低风险。
4. `unknowns` 必须说明缺少什么；`warnings` 只做非阻塞提示。

### Coverage

`coverage` 有五个独立维度：

- `decode`：selector、参数、动态 bytes、multicall。
- `metadata`：EOA/contract、proxy、源码验证。
- `simulation`：状态模拟和 effects。
- `trace`：internal call / delegatecall。
- `policy`：调用方策略。

每个维度取值为 `complete | partial | not_run | unavailable | not_configured`。报告不会把
缺失维度当成通过；`unknowns` 是结果的一部分，不是附注。

### 确定性和可复现性

- 同一请求、同一区块、同一代码版本应产生相同 finding 集合。
- 模拟和读取使用 pinned block；不会混用不同区块的状态。
- LLM 不参与 detector、risk 或 coverage 计算；`explanation` 是确定性模板。
- 外部 provider 只提供输入，不改变规则语义。

### 强制执行

- `transaction.inspect` / `signature.inspect` 本身是 advisory，不签名、不广播、不保管私钥。
- `wallet.send --policy` 和 `wallet.sign --policy` 在 Fluxion 自己的发送和 typed-data 签名路径上
  执行 enforce gate；命中高风险策略时返回 `POLICY_BLOCKED`，不会进入手机确认。
- 这不是通用 signer 边界。任意 calldata、绕过 Fluxion 的调用、或已被篡改的本地进程不受约束；
  通用强制仍需 smart account、session key 或 signer policy。

### 验证方式

- Rust 单元测试和 fixture corpus 覆盖 decode、effects、trace、approval、asset-flow、swap、
  policy 和 signature。
- Foundry/Anvil 红队合约覆盖伪造日志、hidden delegatecall、proxy upgrade、无限授权、permit、
  7702 和 4337。
- 当前红队语料 **17/17 detect**，**3/3 controls 无误报**。
- 真实历史 BSC calldata：两笔真实 unlimited approval phishing 交易命中 `AUTH-001`；两个 opaque
  exploit entrypoint 明确保持 `unknown`。
- 方法和原始数据见 [red-team](docs/benchmarks/redteam-2026-11-11.md)、
  [historical calldata](docs/benchmarks/historical-calddata-2026-11-11.md) 与
  [security benchmark](docs/benchmarks/security-inspection-2026-11-11.md)。

## 执行模型

每个 capability 的执行位置固定，调用方不能选择 remote/local。`fluxion` 只做路由。

| Capability | 执行位置 | 数据路径 | 失败语义 |
|---|---|---|---|
| `swap.quote` | remote | remote snapshot -> 本地路由计算 | remote 不可用 -> `RPC_ERROR` |
| `wallet.balance` / `assets` / `allowance` / `collectibles` | local | 本地 RPC，固定区块 | RPC 失败 -> `RPC_ERROR` |
| `wallet.connect` / `status` / `disconnect` | local | 本地 session + WalletConnect relay | 无 session、配对失败或过期 -> 明确错误 |
| `wallet.agent-id` | local | SPACE ID API + BSC RPC 校验 | API 失败时按配置回退链上或返回 unknown |
| `wallet.sign` / `wallet.send` | local | 本地 session -> relay -> 手机钱包确认 | 拒绝 -> `USER_REJECTED`；超时 -> `TIMEOUT`；policy block -> `POLICY_BLOCKED` |
| `transaction.inspect` | local | 本地 RPC：block、code、simulate、trace、source | 缺失维度写入 `coverage` / `unknowns` |
| `transaction.simulate` | local | 本地 RPC：`eth_call` + `eth_estimateGas` | RPC/revert 按 capability error 或 `success=false` 返回 |
| `signature.inspect` | local | 调用方提供的 typed data / authorization / userOp JSON | 无法解码 -> `SIG-006` 或 invalid argument |

### 请求流

```text
Quote:
CLI/MCP -> fluxion client -> FLUXION_REMOTE_URL -> snapshot -> quote response

Local read:
CLI/MCP -> fluxion client -> chain RPC -> pinned block -> response

Wallet:
CLI/MCP -> local session file -> WalletConnect relay -> phone -> response

Pre-sign inspection:
CLI/MCP -> local RPC -> decode/metadata/simulate/verify/trace/source -> report
```

### 运行时和配置

- `FLUXION_REMOTE_URL`：remote quote 服务地址；没有配置时 `swap.quote` 返回明确错误。
- `FLUXION_RPC_URL` / `RPC_URL`：Ethereum RPC。
- `FLUXION_BSC_RPC_URL`：BSC RPC，本地能力默认链。
- `WALLETCONNECT_PROJECT_ID`、`WALLETCONNECT_RELAY_URL`：钱包会话。
- `BSCSCAN_API_KEY` / `ETHERSCAN_API_KEY`：可选源码验证。
- `FLUXION_VERIFY_RPC_URL`：可选第二 RPC，用于 block hash 和 target code 交叉校验。
- `FLUXION_THREAT_INTEL_FILE`：可选本地 threat-intel JSON，命中地址输出 `ADDR-007`；
  `FLUXION_THREAT_INTEL_PUBKEY` 可启用 Ed25519 签名校验。registry 由运营方提供，可能不完整或过期。
- `FLUXION_BSCSCAN_API_URL` / `FLUXION_ETHERSCAN_API_URL`：可替换验证端点。
- 私钥不进入 runtime；WalletConnect session 只保存在本地 `~/.fluxion/walletconnect/session.json`。

### 状态和失败

- Quote 只读已发布 snapshot；不会把 snapshot 写回链。
- 本地读取和模拟固定在同一区块；跨区块一致性不在单个响应内保证。
- `transaction.inspect` 的 `effects`、`trace`、`source` 是可选增强：缺失时报告明确降级，不阻塞
  decode 和确定性 findings。
- 所有 capability 使用稳定错误码；调用方不应从 HTTP/CLI 文本猜测错误类型。

## 文档

- 接入：[MCP](docs/MCP.md)
- 能力：[swap.quote](docs/capabilities/swap/README.md) · [wallet](docs/capabilities/wallet/README.md) · [transaction](docs/capabilities/transaction/README.md) · [signature](docs/capabilities/signature/README.md)
- 架构：[产品架构](docs/PRODUCT-ARCHITECTURE.md) · [引擎架构](docs/ARCHITECTURE.md) · [本地状态](docs/LOCAL-STATE.md)
- 运维：[HTTP API](docs/API.md) · [配置](docs/CONFIGURATION.md) · [开发](docs/DEVELOPMENT.md)
- 安全：[规则目录](docs/capabilities/transaction/ASSURANCE.md) · [red-team](docs/benchmarks/redteam-2026-11-11.md)
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
