# Capabilities

Fluxion exposes Web3 actions as capabilities. A capability defines the request/response contract;
its execution location is an internal Fluxion decision.

| Capability | Status | Docs |
|---|---|---|
| `swap.quote` | available | [swap/README.md](swap/README.md) · [graph/state](swap/GRAPH-AND-SNAPSHOT.md) |
| `swap.build` | available | [swap/README.md](swap/README.md) |
| `wallet.balance` | available | [wallet/README.md](wallet/README.md) |
| `wallet.assets` | available | [wallet/README.md](wallet/README.md) |
| `wallet.allowance` | available | [wallet/README.md](wallet/README.md) |
| `wallet.collectibles` | available | [wallet/README.md](wallet/README.md) |
| `wallet.connect` | available | [wallet/README.md](wallet/README.md) |
| `wallet.status` | available | [wallet/README.md](wallet/README.md) |
| `wallet.disconnect` | available | [wallet/README.md](wallet/README.md) |
| `wallet.agent-id` | available | [wallet/README.md](wallet/README.md) |
| `wallet.sign` | available | [wallet/README.md](wallet/README.md) |
| `wallet.send` | available | [wallet/README.md](wallet/README.md) |
| `transaction.inspect` | available | [transaction/README.md](transaction/README.md) |
| `transaction.simulate` | available | [transaction/README.md](transaction/README.md) |
| `transaction.send` | available | [transaction/README.md](transaction/README.md) |
| `signature.inspect` | available | [signature/README.md](signature/README.md) |
| `fourmeme.agent-tokens` | available | [fourmeme/README.md](fourmeme/README.md) |
| `fourmeme.token` | available | [fourmeme/README.md](fourmeme/README.md) |
| `yield.discover` | available | [yield/README.md](yield/README.md) |
| `yield.build` | available | [yield/README.md](yield/README.md) |
| `yield.history` | available | [yield/README.md](yield/README.md) |
| `yield.withdraw` | available | [yield/README.md](yield/README.md) |
| `position.analyze` | available | [position/README.md](position/README.md) |

## Contract vs execution

- Capability: what the agent wants to do (`swap.quote`)
- Execution location: fixed per capability (`swap.quote`, `yield.discover`, `yield.history` and `position.analyze` → remote service, `yield.build`, `yield.withdraw` and wallet/execution capabilities → local runtime)
- Data source: RPC, subgraphs, protocol APIs used internally
- Transport: CLI or [MCP stdio](../MCP.md)

Agents call capabilities; they do not choose an execution location. See [swap/README.md](swap/README.md) for
the execution model.


Chinese docs: [capability](swap/README.zh.md) · [technical flow](swap/ARCHITECTURE.zh.md) ·
[graph/state](swap/GRAPH-AND-SNAPSHOT.zh.md).
