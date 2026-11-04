# Capabilities

Fluxion exposes Web3 actions as capabilities. A capability defines the request/response contract;
providers define where and how it executes. The first delivered capability is `swap.quote`.

| Capability | Status | Providers | Docs |
|---|---|---|---|
| `swap.quote` | available | `local-cli`, `local-mcp`; `remote-mcp` planned | [swap/README.md](swap/README.md) |

## Contract vs provider

- Capability: what the agent wants to do (`swap.quote`)
- Provider: where it runs and how it is invoked (`local-cli`, `local-mcp`, `remote-mcp`)
- Data source: RPC, subgraphs, protocol APIs used by a provider
- Transport: CLI, MCP stdio, or remote MCP

Provider selection is explicit. Fluxion does not silently fall back between local and remote in v1.
See [swap/README.md](swap/README.md) for the current provider matrix.
