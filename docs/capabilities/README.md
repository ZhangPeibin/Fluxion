# Capabilities

Fluxion exposes Web3 actions as capabilities. A capability defines the request/response contract;
its execution location is an internal Fluxion decision. The first delivered capability is
`swap.quote`.

| Capability | Status | Docs |
|---|---|---|
| `swap.quote` | available | [swap/README.md](swap/README.md) · [graph/state](swap/GRAPH-AND-SNAPSHOT.md) |
| `wallet.balance` | available | [wallet/README.md](wallet/README.md) |
| `wallet.assets` | available | [wallet/README.md](wallet/README.md) |
| `wallet.allowance` | available | [wallet/README.md](wallet/README.md) |
| `wallet.collectibles` | available | [wallet/README.md](wallet/README.md) |
| `wallet.connect` | available | [wallet/README.md](wallet/README.md) |
| `wallet.status` | available | [wallet/README.md](wallet/README.md) |
| `wallet.disconnect` | available | [wallet/README.md](wallet/README.md) |
| `wallet.agent-id` | available | [wallet/README.md](wallet/README.md) |
| `transaction.simulate` | available | [transaction/README.md](transaction/README.md) |

## Contract vs execution

- Capability: what the agent wants to do (`swap.quote`)
- Execution location: fixed per capability (`swap.quote` → remote service, `wallet`/`execute` → local runtime)
- Data source: RPC, subgraphs, protocol APIs used internally
- Transport: CLI or [MCP stdio](../MCP.md)

Agents call capabilities; they do not choose an execution location. See [swap/README.md](swap/README.md) for
the execution model.


Chinese docs: [capability](swap/README.zh.md) · [technical flow](swap/ARCHITECTURE.zh.md) ·
[graph/state](swap/GRAPH-AND-SNAPSHOT.zh.md).
