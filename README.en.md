# Fluxion

English | [中文](README.md)

**Runtime for the Web3 Agent Capability Infrastructure.** Fluxion exposes Web3 actions as stable
capabilities for local CLIs and MCP agents. Execution location (remote service or local runtime)
is decided internally by Fluxion. The first delivered capability is `swap.quote`: exact-in
multi-hop quotes, split allocation and state freshness on Ethereum mainnet.

> Open-source and self-hosted. See the full
> [Web3 Agent Capability Infrastructure whitepaper](docs/WHITEPAPER.md).

## Capabilities

| Capability | Status | Docs |
|---|---|---|
| `swap.quote` | available | [docs/capabilities/swap/README.md](docs/capabilities/swap/README.md) |

A capability defines *what* the agent wants; execution location is an internal implementation
detail. Agents do not select local/remote. `swap.quote` defaults to the managed service;
development/self-hosting uses `FLUXION_MODE=self_hosted`. See the
[capability index](docs/capabilities/README.md).

## Install / Agent setup

```bash
cargo install --path .
fluxion swap ldo-eth 100 --json
fluxion mcp
```

`fluxion mcp` is the agent entry point. MCP registration, fields and errors are documented in the
[swap.quote capability](docs/capabilities/swap/README.md) and [agent skill](skills/swap/SKILL.md).

Development / self-hosted:

```bash
FLUXION_MODE=self_hosted GRAPH_API_KEY=... fluxion start
```

## Execution model

`managed` uses the remote service (`FLUXION_REMOTE_URL` / `FLUXION_API_KEY`); `self_hosted` uses
the local runtime (`GRAPH_API_KEY` + RPC). `FLUXION_MODE=auto` chooses based on whether a remote
URL is configured. Agents do not choose a provider. See
[execution routing](docs/CONFIGURATION.md#execution-routing) (Chinese).

## Status

- `swap.quote` delivered; latest benchmark: 90/90, lag 0/1 blocks, HTTP p95 133 ms
- `swap.build` / `simulate` / `execute` and wallet handling belong to a later Local Runtime
- Ethereum mainnet, exact-in, read-only quotes

## Documentation

- **Capabilities**: [index](docs/capabilities/README.md) · [swap.quote](docs/capabilities/swap/README.md) · [skill](skills/swap/SKILL.md) · [schema](schemas/swap_quote.v1.json)
- **Product**: [whitepaper](docs/WHITEPAPER.md) · [architecture](docs/ARCHITECTURE.en.md) · [local state](docs/LOCAL-STATE.md)
- **Operations**: [HTTP API](docs/API.md) · [configuration](docs/CONFIGURATION.md) · [development](docs/DEVELOPMENT.md) · [viewer](docs/VIEWER.md) · [limitations](docs/LIMITATIONS.md) (Chinese)
- **Benchmarks**: [index](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)

## Development

```bash
cargo build
cargo test
cargo test -- --ignored      # mainnet, read-only
cargo clippy --all-targets
cargo fmt
```
