# Eollo Context

Eollo is a **composable Web3 capability runtime for AI agents**. It exposes action-level
capabilities over MCP and CLI, routes each capability to a fixed internal execution location, and
returns structured results that agents can compose into workflows.

This file is the ubiquitous language for the project. Capability, transport, and engine documents
must use these terms consistently.

## Core terms

| Term | Meaning |
|---|---|
| **Eollo** | The project and runtime. It is not a wallet, an agent, or a single DEX product. |
| **Capability** | One action-level contract such as `swap.quote` or `transaction.inspect`. A capability has an id, input/output schema, chain support, execution location, side effects, errors, and docs. |
| **Capability ID** | Stable `domain.action` identifier, for example `wallet.balance` or `yield.discover`. |
| **Domain** | A group of related Web3 actions: `swap`, `wallet`, `transaction`, `signature`, `yield`, `position`, `fourmeme`. |
| **Action** | The user-level verb inside a domain: `quote`, `build`, `simulate`, `inspect`, `execute`, `read`, `discover`, `analyze`. |
| **Capability catalog** | The machine-readable source of truth for capabilities, surfaces, and workflows. See `capabilities/catalog.yaml`. |
| **Capability registry** | The runtime dispatch table that maps a capability id to its metadata and handler. |
| **Capability envelope** | The common response shape: `meta`, `chain`, `block`, `as_of`, `provenance`, `coverage`, `warnings`, `unknowns`, `data` or `error`. |
| **Execution location** | Where a capability runs: `local` or `remote`. It is fixed per capability and is an internal Eollo decision. Agents and users never choose it. |
| **Transport** | How an agent reaches a capability: MCP stdio, CLI, or HTTP. Transport is not a capability. |
| **Side effect** | What a capability does: `read_only`, `local_session`, `prepares_calldata`, `signs`, or `broadcasts`. |
| **Chain matrix** | The declared Ethereum/BSC support for each capability. Unsupported chains fail explicitly instead of falling back. |
| **Workflow** | An ordered composition of capabilities that produces a user result, for example quote → build → inspect → send → verify. |
| **Surface** | A user-facing entry point that is not an agent capability, such as the local `fourmeme.ui` page. Surfaces are listed separately in the catalog. |
| **Status** | Capability maturity: `proposed`, `experimental`, `stable`, or `deprecated`. |
| **Provenance** | Where the data came from, when it was observed, and how fresh it is. |
| **Coverage** | Which checks actually ran. A missing dimension is reported, never treated as a pass. |
| **Unknowns** | Facts the capability could not establish. Unknowns are part of the result, not a footnote. |
| **Warnings** | Non-blocking problems such as a provider fallback or a partial discovery window. |
| **Policy gate** | A caller-supplied constraint checked before signing or broadcasting. It can block with `POLICY_BLOCKED`. |
| **Advisory** | A result that informs the caller but does not guarantee safety and does not authorize execution. |

## Execution boundaries

- `swap.quote`, `yield.discover`, `yield.history`, and `position.analyze` run on the remote service.
- `wallet.*`, `transaction.*`, `signature.inspect`, `fourmeme.*`, `swap.build`, `yield.build`, and
  `yield.withdraw` run on the local runtime.
- Private keys never enter the remote service. Signing and broadcasting stay behind the user's
  wallet confirmation boundary.
- `transaction.inspect` and `signature.inspect` are advisory. `wallet.send`, `wallet.sign`, and
  `transaction.send` can enforce a caller policy before the wallet request.

## Decision records

- [ADR-0001](docs/adr/0001-eollo-capability-runtime.md): Eollo is a composable multi-domain runtime.
- [ADR-0002](docs/adr/0002-ethereum-bsc-chain-matrix.md): Ethereum and BSC are declared, first-class chains.
- [ADR-0003](docs/adr/0003-capability-envelope-v2.md): every capability uses envelope v2.
- [ADR-0004](docs/adr/0004-composable-workflows.md): workflows are first-class compositions.
- [ADR-0005](docs/adr/0005-capability-catalog.md): the catalog is the single source of truth.

## Specification map

- [Capability runtime spec](docs/feature/v3/capability-runtime/spec.md)
- [Catalog spec](docs/feature/v3/capability-runtime/catalog.md)
- [Envelope v2 spec](docs/feature/v3/capability-runtime/envelope-v2.md)
- [Chain matrix spec](docs/feature/v3/capability-runtime/chain-matrix.md)
- [Registry spec](docs/feature/v3/capability-runtime/registry.md)
- [Workflows spec](docs/feature/v3/capability-runtime/workflows.md)
- [Migration spec](docs/feature/v3/capability-runtime/migration.md)
- [Lifecycle spec](docs/feature/v3/capability-runtime/lifecycle.md)
