# Capabilities

Fluxion exposes Web3 actions as capabilities. A capability defines the request/response contract;
its execution location is an internal Fluxion decision. The first delivered capability is
`swap.quote`.

| Capability | Status | Docs |
|---|---|---|
| `swap.quote` | available | [swap/README.md](swap/README.md) |

## Contract vs execution

- Capability: what the agent wants to do (`swap.quote`)
- Execution location: decided by Fluxion (`managed` → remote service, `self_hosted` → local runtime)
- Data source: RPC, subgraphs, protocol APIs used internally
- Transport: CLI or [MCP stdio](../MCP.md)

Agents call capabilities; they do not choose a provider. `FLUXION_MODE` is operator configuration,
not an agent input. See [swap/README.md](swap/README.md) for the execution model.
