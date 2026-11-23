# Fluxion

Fluxion 是面向 AI Agent 的 Web3 能力层。它把报价、账户读取、钱包连接、交易检查和签名授权
封装成稳定的 MCP 能力，让 Agent 能读链、报价、准备交易，并在用户签名前给出可审计的风险报告。

它解决两类问题：

- **链上接口碎片化**：把分散的 RPC、DEX、钱包和数据结构整理成固定输入输出。
- **签名前缺少可验证的检查**：在签名前解码 calldata、模拟状态变化、检查内部调用和源码，并
  明确哪些结论没有覆盖。

[English](README.en.md) · 中文

产品官网：<https://zhangpeibin.github.io/Fluxion/> · 源码：[`site/`](site/)

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

能力元数据以 [`capabilities/catalog.yaml`](capabilities/catalog.yaml) 为唯一来源，
[`docs/capabilities/README.md`](docs/capabilities/README.md) 的表格由它生成。所有能力返回
capability envelope v2（`meta`、`chain`、`block`、`as_of`、`provenance`、`coverage`、
`warnings`、`unknowns`、`data`）；见 [ADR 0003](docs/adr/0003-capability-envelope-v2.md) 和
[ADR 0005](docs/adr/0005-capability-catalog.md)。

### 能力总览

#### 1. 报价

| Capability | Status | CLI | 说明 |
|---|---|---|---|
| `swap.quote` | stable | `fluxion swap ldo-eth 100` | Ethereum 主网 exact-in 报价、多跳、拆单 |
| `swap.build` | stable | `fluxion swap build --quote quote.json --recipient 0x...` | 从报价构建 V2 swap calldata |

#### 2. 钱包

| Capability | Status | CLI | 说明 |
|---|---|---|---|
| `wallet.balance` | stable | `fluxion wallet balance 0x... --tokens BNB,USDT` | 固定区块余额 |
| `wallet.assets` | stable | `fluxion wallet assets 0x...` | 资产发现 + 本地核对 |
| `wallet.allowance` | stable | `fluxion wallet allowance 0xowner --spender 0xspender --tokens USDT` | 授权额度 |
| `wallet.collectibles` | stable | `fluxion wallet collectibles 0x...` | NFT 持仓 |
| `wallet.connect` | stable | `fluxion wallet connect` | WalletConnect 连接 |
| `wallet.status` | stable | `fluxion wallet status` | 查看本地会话 |
| `wallet.disconnect` | stable | `fluxion wallet disconnect` | 删除本地会话 |
| `wallet.agent-id` | stable | `fluxion wallet agent-id alice.ai` | BSC `.ai` Agent ID |
| `wallet.sign` | stable | `fluxion wallet sign --typed-data typed-data.json` | 消息 / EIP-712 签名 |
| `wallet.send` | stable | `fluxion wallet send --to 0x... --token BNB --amount 0.01` | 原生 / 代币转账 |

#### 3. 交易安全

| Capability | Status | CLI | 说明 |
|---|---|---|---|
| `transaction.inspect` | stable | `fluxion transaction inspect --to 0x... --data 0x...` | 签名前 calldata 检查、effects、trace、源码、policy |
| `transaction.simulate` | stable | `fluxion transaction simulate --to 0x... --data 0x...` | 固定区块 `eth_call` + gas |
| `transaction.send` | stable | `fluxion transaction send --to 0x... --data 0x... --policy policy.json` | 模拟 + inspect + policy + 手机确认 + 广播 |
| `transaction.verify` | experimental | `fluxion transaction verify --hash 0x...` | 回执状态、确认数、实际 effects，可选对照 inspect 报告 |

#### 4. 签名授权

| Capability | Status | CLI | 说明 |
|---|---|---|---|
| `signature.inspect` | stable | `fluxion signature inspect --typed-data typed-data.json` | EIP-712、Permit2、7702、4337 |

#### 5. Four.meme

| Capability | Status | CLI | 说明 |
|---|---|---|---|
| `fourmeme.agent-tokens` | experimental | `fluxion fourmeme agent-tokens --wallet 0x... --before-block N` | 发现 Binance Agent ID Exclusive token，支持按区块向前翻页、查询钱包资格 |
| `fourmeme.token` | experimental | `fluxion fourmeme token 0x... --wallet 0x...` | 单币详情：阶段、市场/动量、社交、持有人、交易限制与 1% 额度、安全信号、资格 |

#### 6. 收益发现

| Capability | Status | CLI | 说明 |
|---|---|---|---|
| `yield.discover` | experimental | `fluxion yield discover --asset ETH --amount 10` | Ethereum ETH 质押/借贷机会：安全层级、APY、TVL、提现条件、推荐 |
| `yield.build` | experimental | `fluxion yield build --opportunity lido-steth --amount 10 --recipient 0x...` | 本地构造 stake/supply/deposit calldata 与授权步骤 |
| `yield.history` | experimental | `fluxion yield history --opportunity lido-steth --days 30` | 历史 APY、base/reward 拆分、TVL 与稳定性摘要 |
| `yield.withdraw` | experimental | `fluxion yield withdraw --opportunity aave-v3-weth --amount 10 --recipient 0x...` | 本地构造 withdraw/redeem/burn 退出 calldata |

#### 7. 仓位分析

| Capability | Status | CLI | 说明 |
|---|---|---|---|
| `position.analyze` | experimental | `fluxion position analyze --owner 0x...` | 跨 curated 市场的 ETH 仓位、估值、APY、提现条件、风险与可选 PnL |

完整用法和“对 Agent 说”的示例见下面各能力小节。

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

### 收益发现

#### `yield.discover`

Ethereum 主网 ETH 本位的质押和借贷机会发现。它返回 curated safety tier、APY 拆分、TVL、
提现条件、风险因子和证据，并给出一个推荐和取舍理由；不构造、不签名、不执行。

```bash
fluxion yield discover --asset ETH --amount 10 --json
fluxion yield discover --asset ETH --amount 10 --risk balanced --json
fluxion yield build --opportunity lido-steth --amount 10 --recipient 0x... --json
fluxion yield history --opportunity lido-steth --days 30 --json
fluxion yield withdraw --opportunity aave-v3-weth --amount 10 --recipient 0x... --json
```

`yield.build` / `yield.withdraw` 只构造 calldata 和授权步骤，不签名、不广播；`yield.history`
读取历史 APY，历史值不是当前值，也不构成保证。

对 Agent 说：

```text
我有 10 ETH，想找最安全且 APY 合理的机会；先按 conservative 给我推荐，并说明放弃了什么。
```

Skill：[yield](skills/yield/SKILL.md) · 文档：[yield](docs/capabilities/yield/README.md)

### 仓位分析

#### `position.analyze`

读取地址在 Lido/Rocket Pool/Aave/SparkLend/Morpho 等 curated 市场中的仓位。余额固定在同一
Ethereum 区块，APY 和风险上下文来自 `yield.discover` 快照；可选提供 cost basis 计算 PnL。

```bash
fluxion position analyze --owner 0x... --json
fluxion position analyze --owner 0x... --cost-basis lido-steth=10 --json
```

对 Agent 说：

```text
看看这个地址在哪些 ETH 收益市场有仓位，值多少 ETH，年化收益和提现条件是什么。
```

Skill：[position](skills/position/SKILL.md) · 文档：[position.analyze](docs/capabilities/position/README.md)

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

#### `transaction.verify`

读取已发送交易的 receipt，返回状态、确认数、gas/手续费和从日志解码的 effects；可选对照
`transaction.inspect` 报告并返回 `match` / `mismatch`。只读，不签名、不广播。

```bash
fluxion transaction verify --hash 0x... --json
fluxion transaction verify --hash 0x... --chain ethereum --confirmations 3 --json
fluxion transaction verify --hash 0x... --expect inspect.json --json
```

状态为 `success`、`reverted`、`pending` 或 `not_found`；`pending` / `not_found` 是正常结果，
不是错误。没有 receipt 时 effects 标记为 `not_run`。

对 Agent 说：

```text
这笔交易 0x... 成功了吗？实际转了什么？如果和我签名前的检查不一致，告诉我差异。
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

### Four.meme Agent ID Exclusive

Skill：[fourmeme](skills/fourmeme/SKILL.md) · 文档：[fourmeme](docs/capabilities/fourmeme/README.md)

发现 Four.meme 上使用 `Binance Agent ID Exclusive` 模板（`presetId=1778027615735`）的近期
token，并可检查钱包是否持有 Agent ID。`aiCreator` 或 `.ai` 标签不等于该模板。

```bash
fluxion fourmeme agent-tokens --json
fluxion fourmeme agent-tokens --wallet 0x... --limit 20 --json
fluxion fourmeme agent-tokens --lookback-blocks 100000 --json
fluxion fourmeme agent-tokens --lookback-blocks 100000 --before-block 26350000 --json
fluxion fourmeme token 0x... --wallet 0x... --json
fluxion fourmeme ui --open
```

`fourmeme.agent-tokens` 扫描 OpenFour Core `TokenCreated` 日志并按模板过滤；`--before-block`
可以从更早的区块继续向前翻页，响应里的 `next_before_block` 就是下一次的游标。

`fourmeme.token` 返回图标、描述、阶段、模板、市场/动量（价格、max price、市值、流动性、
24h 成交量/涨跌幅）、社交链接、持有人数、交易限制（内盘转账锁定、创建时 maxSupply、1% 钱包
额度）、安全信号（原始买卖税、min/max buy、可选 BscScan 合约验证）和钱包资格。API 不可用时
回退到 OpenFour Core `tokens`、vault phase 和 BSC ERC20 只读字段；Core 也不可用时才标记
`phase: unknown`。缺失字段会省略并给出 warning，不会当成 0。

`fourmeme.ui` 启动本地只读网页（默认 `127.0.0.1:8787`），列表卡片可点击进入
`#/token/<address>` 详情页，并支持“加载更早”继续翻页。页面不连接钱包、不签名、不买不卖。

内盘阶段只有 Binance Agentic Wallet + Agent ID 可交易，单地址最多持有创建时 `maxSupply`
的 1%，且内盘转账被锁定；毕业后开放交易。`fourmeme.token` 会返回 `wallet_cap`（余额、1%
上限、剩余额度、是否触顶）。该能力只发现和检查，不买、不卖、不签名。

## 安全模型

`transaction.inspect` / `signature.inspect` 只做**签名前检查**：不持有私钥、不签名、不广播，
也不构成审计或安全保证。检查结果用于辅助调用方和用户决定是否进入钱包确认流程。

- **风险语义**：有 finding 时 `risk` 取最高 severity；没有 finding 时是 `unknown`，不是
  `low` 或 `safe`；关键覆盖缺失时保持 `unknown`。
- **覆盖**：`decode`、`metadata`、`simulation`、`trace`、`policy` 五个独立维度；
  `unknowns` 是结果的一部分，不是附注。
- **强制执行**：`wallet.send --policy` 和 `wallet.sign --policy` 只在 Fluxion 自己的发送和
  typed-data 签名路径上执行 enforce gate，命中高风险返回 `POLICY_BLOCKED`；这不是通用
  signer 边界。
- **确定性**：同一请求、同一区块、同一代码版本产生相同 finding；LLM 不参与 detector、
  risk 或 coverage 计算。

规则目录、信任边界、威胁模型、检查管线、覆盖率语义和红队边界见
[`docs/capabilities/transaction/ASSURANCE.md`](docs/capabilities/transaction/ASSURANCE.md)；
威胁情报配置见 [`docs/capabilities/transaction/THREAT-INTEL.md`](docs/capabilities/transaction/THREAT-INTEL.md)；
红队方法和原始数据见 [red-team](docs/benchmarks/redteam-2026-11-11.md)、
[historical calldata](docs/benchmarks/historical-calddata-2026-11-11.md) 与
[security benchmark](docs/benchmarks/security-inspection-2026-11-11.md)。

## 执行模型

每个 capability 的执行位置固定，调用方不能选择 remote/local。`fluxion` 只做路由。

| Capability | 执行位置 | 数据路径 | 失败语义 |
|---|---|---|---|
| `swap.quote` | remote | remote snapshot -> 本地路由计算 | remote 不可用 -> `RPC_ERROR` |
| `swap.build` | local | 读取 quote JSON -> 本地 ABI 编码 | 非 V2/拆单/非 Ethereum -> `UNSUPPORTED` |
| `yield.discover` | remote | curated registry + DefiLlama + Ethereum RPC 后台快照 | provider/RPC 失败 -> `RPC_ERROR`；数据过期 -> `STALE_STATE` |
| `yield.build` | local | curated registry -> ABI 编码 -> steps/approvals | 不支持的市场 -> `UNSUPPORTED` |
| `yield.history` | remote | DefiLlama chart + 进程内缓存 | provider 失败 -> `RPC_ERROR`；数据过期 -> `STALE_STATE` |
| `yield.withdraw` | local | curated registry -> withdraw/redeem/burn ABI 编码 | 不支持的市场/数量 -> `UNSUPPORTED`/`INVALID_ARGUMENT` |
| `position.analyze` | remote | Ethereum RPC pinned block + yield snapshot | RPC 失败 -> `RPC_ERROR`（`PositionUnavailable`）；快照未就绪 -> `STALE_STATE` |
| `wallet.balance` / `assets` / `allowance` / `collectibles` | local | 本地 RPC，固定区块 | RPC 失败 -> `RPC_ERROR` |
| `wallet.connect` / `status` / `disconnect` | local | 本地 session + WalletConnect relay | 无 session、配对失败或过期 -> 明确错误 |
| `wallet.agent-id` | local | SPACE ID API + BSC RPC 校验 | API 失败时按配置回退链上或返回 unknown |
| `wallet.sign` / `wallet.send` | local | 本地 session -> relay -> 手机钱包确认 | 拒绝 -> `USER_REJECTED`；超时 -> `TIMEOUT`；policy block -> `POLICY_BLOCKED` |
| `transaction.inspect` | local | 本地 RPC：block、code、simulate、trace、source | 缺失维度写入 `coverage` / `unknowns` |
| `transaction.simulate` | local | 本地 RPC：`eth_call` + `eth_estimateGas` | RPC/revert 按 capability error 或 `success=false` 返回 |
| `transaction.send` | local | RPC simulate/inspect -> WalletConnect -> 手机确认 | 模拟失败、policy block、拒绝或超时均不广播 |
| `transaction.verify` | local | 本地 RPC：tx + receipt + receipt logs | 未找到/未确认 -> `pending`/`not_found`；RPC 失败 -> `RPC_ERROR` |
| `signature.inspect` | local | 调用方提供的 typed data / authorization / userOp JSON | 无法解码 -> `SIG-006` 或 invalid argument |
| `fourmeme.agent-tokens` | local | BSC `TokenCreated` logs + Four.meme API + BSC `AgentIdentifier` | API/RPC 失败 -> 明确 warning/coverage |
| `fourmeme.token` | local | Four.meme API + BSC ERC20 fallback + `AgentIdentifier` | API 不可用 -> 链上基础字段 + warning |

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
- `FOURMEME_API_URL`：Four.meme REST API，默认 `https://four.meme/meme-api/v1`。
- `FOURMEME_BSC_RPC_URL`：支持 `eth_getLogs` 的 BSC RPC，也用于 ERC20 fallback；默认 `https://bsc-rpc.publicnode.com`。
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
- 契约：[能力目录](capabilities/catalog.yaml) · [catalog schema](schemas/capability_catalog.v2.json) · [能力文档索引](docs/capabilities/README.md)
- 研究：[benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- Skills：[swap](skills/swap/SKILL.md) · [yield](skills/yield/SKILL.md) · [position](skills/position/SKILL.md) · [wallet](skills/wallet/SKILL.md) · [transaction](skills/transaction/SKILL.md) · [signature](skills/signature/SKILL.md) · [fourmeme](skills/fourmeme/SKILL.md)

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

下一步：`swap.execute`。

## 开发

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt

# Foundry 安全红队
python3 bench/security/redteam.py
```
