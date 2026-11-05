# Fluxion 产品架构

Fluxion 是 **Web3 Agent Capability Infrastructure 的 runtime**。Agent 只通过 MCP 调用
capability；执行位置由 Fluxion 内部固定路由，Agent 和用户都不选择 local/remote。

## 总体架构

```text
                         ┌──────────────────────────────┐
                         │ Agent (opencode / Claude /…) │
                         │ reads Skills · calls MCP     │
                         └──────────────┬───────────────┘
                                        │ MCP stdio
┌───────────────────────────────────────▼─────────────────────────────────────┐
│                    Fluxion (installed on user machine)                     │
│                                                                             │
│   ┌──────────────┐        ┌──────────────────────────────────────────────┐  │
│   │  MCP Server  │───────▶│            Capability Router                 │  │
│   └──────────────┘        │                                              │  │
│                           │  swap.quote ───────────────────────────┐    │  │
│                           │  swap.build ───────────────┐          │    │  │
│                           │  swap.simulate ────────────┤          │    │  │
│                           │  swap.execute ─────────────┤          │    │  │
│                           │  wallet.* ─────────────────┤          │    │  │
│                           └────────────────────────────┼──────────┼────┘  │
│                                                        │          │       │
│   ┌────────────────────────────────┐                   │          │       │
│   │ Local Runtime                  │◀──────────────────┘          │       │
│   │  wallet session                │                              │       │
│   │  policy / authorization        │                              │       │
│   │  build / simulate / sign       │                              │       │
│   │  broadcast / verify            │                              │       │
│   └───────────────┬────────────────┘                              │       │
└───────────────────┼───────────────────────────────────────────────┼───────┘
                    │ WalletConnect / OS wallet                     │ HTTPS
                    ▼                                               ▼
          ┌─────────────────────┐                     ┌──────────────────────────┐
          │ User / Wallet UI    │                     │ Fluxion Remote Service   │
          │ approve · sign      │                     │  swap.quote engine       │
          └─────────────────────┘                     │  discovery / snapshot    │
                                                      │  market / risk / intel   │
                                                      └────────────┬─────────────┘
                                                                   │ RPC / subgraph
                                                                   ▼
                                                      ┌──────────────────────────┐
                                                      │ Ethereum / Data Providers│
                                                      └──────────────────────────┘
```

## 四层职责

| 层 | 位置 | 职责 |
|---|---|---|
| Agent | 用户环境 | 理解意图、规划任务、调用 capability |
| Fluxion local | 用户机器 | MCP 接入、capability 路由、wallet、授权、签名、执行 |
| Fluxion remote | 我们的服务端 | quote 引擎、discovery、市场/风险/情报数据 |
| Chain / data | 外部 | Ethereum RPC、subgraph、协议数据源 |

## Capability 路由

| Capability | 执行位置 | 说明 |
|---|---|---|
| `swap.quote` | remote | 只读报价，集中化数据和计算 |
| `swap.build` | local | 交易构造，需要本地账户和 nonce |
| `swap.simulate` | local | 本地策略检查和模拟 |
| `swap.execute` | local | 用户确认、钱包签名、广播、验证 |
| `swap.verify` | local | 链上执行确认 |
| `wallet.*` | local | 账户、余额、授权、会话 |

规则：

- Agent 不选择 provider，也不传 `provider` / `FLUXION_MODE`
- `swap.quote` 固定使用 remote 服务
- `wallet` / `execute` 固定使用 local runtime
- remote 永远不接触私钥，也不签名
- 没有自动跨执行位置 fallback

## MCP 与 Skills

- **MCP** 是执行接口：暴露 `swap_quote`、`wallet_*` 等 tool
- **Skills** 是 Agent 的使用说明：告诉 Agent 何时调用、参数含义、错误处理
- Skills 不在执行链路上，也不是安全边界
- MCP 和 Skills 都只是接入方式；真正的授权和安全策略在 Local Runtime

## 用户交互

用户只做两件事：

1. 安装 Fluxion
2. 在 Agent 中配置一次 MCP / Skills

之后：

```text
用户 → Agent → MCP → capability
```

不需要手动启动 runtime，不需要选择 local/remote，不需要在普通 managed 流程中提供
`GRAPH_API_KEY`。

## 安全边界

- 私钥只存在本地 wallet / OS keychain / 硬件钱包
- remote 只提供只读情报和报价，不执行授权
- 交易必须绑定精确参数：chain、to、value、calldata、token、amount、gas
- 用户确认和策略检查在 Local Runtime
- 模拟不是执行保证；状态、gas、MEV 和流动性都可能变化
- Skills 不能绕过 Local Runtime 的策略

## 代码布局

```text
crates/protocol   共享 capability 契约、swap DTO、错误码
crates/remote     remote server、discovery、snapshot、routing、bench/graph
crates/client     fluxion CLI、MCP bridge、remote client、token
```

`remote` 不依赖 `client`；`client` 不依赖 `remote`；两者只共享 `protocol`。
生产二进制：

```text
fluxion-remote serve   # 服务器
fluxion mcp            # 用户机器上的 MCP bridge
```

## 当前范围

已交付：

- `swap.quote` 的 remote 路由
- local MCP 接入
- capability contract / schema / skill

后续：

- `swap.build` / `swap.simulate` / `swap.execute`
- local wallet runtime、策略引擎、用户确认
- remote 托管服务与认证
