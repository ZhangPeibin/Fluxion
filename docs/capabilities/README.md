# Capabilities

Fluxion exposes Web3 actions as capabilities. A capability defines the request/response contract;
its execution location is an internal Fluxion decision.

> Capability metadata is defined in [`capabilities/catalog.yaml`](../../capabilities/catalog.yaml).
> The table below is generated from the catalog and validated by `cargo test`; edit the catalog,
> not the table.

<!-- catalog-table:start -->
| Capability | Status | Chains | Side effects | Docs |
|---|---|---|---|---|
| `swap.quote` | stable | ethereum | read_only | [swap](swap/README.md) |
| `swap.build` | stable | ethereum | prepares_calldata | [swap](swap/README.md) |
| `wallet.balance` | stable | ethereum, bsc | read_only | [wallet](wallet/README.md) |
| `wallet.assets` | stable | ethereum, bsc | read_only | [wallet](wallet/README.md) |
| `wallet.allowance` | stable | ethereum, bsc | read_only | [wallet](wallet/README.md) |
| `wallet.collectibles` | stable | ethereum, bsc | read_only | [wallet](wallet/README.md) |
| `wallet.connect` | stable | ethereum, bsc | local_session | [wallet](wallet/README.md) |
| `wallet.status` | stable | ethereum, bsc | read_only | [wallet](wallet/README.md) |
| `wallet.disconnect` | stable | ethereum, bsc | local_session | [wallet](wallet/README.md) |
| `wallet.agent-id` | stable | bsc | read_only | [wallet](wallet/README.md) |
| `wallet.sign` | stable | ethereum, bsc | signs | [wallet](wallet/README.md) |
| `wallet.send` | stable | ethereum, bsc | broadcasts | [wallet](wallet/README.md) |
| `transaction.simulate` | stable | ethereum, bsc | read_only | [transaction](transaction/README.md) |
| `transaction.inspect` | stable | ethereum, bsc | read_only | [transaction](transaction/README.md) |
| `transaction.send` | stable | ethereum, bsc | broadcasts | [transaction](transaction/README.md) |
| `transaction.verify` | experimental | ethereum, bsc | read_only | [transaction](transaction/README.md) |
| `signature.inspect` | stable | ethereum, bsc | read_only | [signature](signature/README.md) |
| `fourmeme.agent-tokens` | experimental | bsc | read_only | [fourmeme](fourmeme/README.md) |
| `fourmeme.token` | experimental | bsc | read_only | [fourmeme](fourmeme/README.md) |
| `yield.discover` | experimental | ethereum | read_only | [yield](yield/README.md) |
| `yield.build` | experimental | ethereum | prepares_calldata | [yield](yield/README.md) |
| `yield.history` | experimental | ethereum | read_only | [yield](yield/README.md) |
| `yield.withdraw` | experimental | ethereum | prepares_calldata | [yield](yield/README.md) |
| `position.analyze` | experimental | ethereum | read_only | [position](position/README.md) |
<!-- catalog-table:end -->

## Contract vs execution

- Capability: what the agent wants to do (`swap.quote`)
- Execution location: fixed per capability (`swap.quote`, `yield.discover`, `yield.history` and `position.analyze` → remote service, `yield.build`, `yield.withdraw` and wallet/execution capabilities → local runtime)
- Data source: RPC, subgraphs, protocol APIs used internally
- Transport: CLI or [MCP stdio](../MCP.md)

Agents call capabilities; they do not choose an execution location. See [swap/README.md](swap/README.md) for
the execution model.


Chinese docs: [capability](swap/README.zh.md) · [technical flow](swap/ARCHITECTURE.zh.md) ·
[graph/state](swap/GRAPH-AND-SNAPSHOT.zh.md).
