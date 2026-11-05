# Fluxion

**Give your AI agent a fast, safe way to use Web3.**

English · [中文](README.md)

![status](https://img.shields.io/badge/status-pre--1.0-orange)
![MCP](https://img.shields.io/badge/MCP-stdio-blueviolet)
![Rust](https://img.shields.io/badge/Rust-2024-orange)
![Ethereum](https://img.shields.io/badge/Ethereum-mainnet-627eea)

Fluxion is a **capability runtime for AI agents**. Instead of teaching every agent every protocol,
Fluxion exposes Web3 actions as stable capabilities over MCP. The first capability is
`swap.quote`: exact-in multi-hop quotes on Ethereum mainnet, with fresh state and split routing.

> The agent decides what to do. Fluxion provides the capability. The user keeps control of assets.

## Why Fluxion

- **One interface for agents** — `fluxion mcp` exposes capabilities as MCP tools; no protocol SDK
  for the agent to learn.
- **Fast by design** — quotes come from a warm remote service, not a cold local rebuild.
  Latest benchmark: **0/1 blocks** behind head, **p95 133 ms**.
- **Safety boundary built in** — read-only market data can be remote; wallet, authorization,
  signing and execution stay local. The agent never chooses where a capability runs.
- **Structured for tools** — stable JSON, error codes, schemas and Skills instead of screenshots
  and prose.
- **Open and inspectable** — Rust engine, benchmark corpus, raw JSON, reproducible mainnet tests.

## What works today

```bash
fluxion swap ldo-eth 100 --json
```

Or let the agent call the MCP tool:

```text
Use Fluxion's swap_quote tool to quote 100 LDO to ETH.
```

## Quick start

### 1. Install the local client

```bash
cargo install --path crates/client
```

Users install only the client. `fluxion-remote` is our server and normally runs on Fluxion
infrastructure.

### 2. Run a remote service (local testing)

```bash
cargo install --path crates/remote
FLUXION_REQUIRE_AUTH=1 fluxion-remote serve --config configs/runtime.yaml
```

The remote owns discovery, snapshot warmup and routing. `fluxion mcp` never starts it.

### 3. Quote

```bash
FLUXION_REMOTE_URL=http://127.0.0.1:8080 fluxion swap ldo-eth 100
```

The first call registers an installation token automatically; the token is stored in
`~/.fluxion/auth.json` (0600).

### 4. Connect your agent

```json
{
  "mcp": {
    "fluxion": {
      "type": "local",
      "command": ["fluxion", "mcp"],
      "enabled": true,
      "environment": { "FLUXION_REMOTE_URL": "http://127.0.0.1:8080" }
    }
  },
  "skills": { "paths": ["/path/to/Fluxion/skills"] }
}
```

MCP registration, Skills and troubleshooting: [docs/MCP.md](docs/MCP.md).

## Capabilities

| Capability | Status | Execution | Docs |
|---|---|---|---|
| `swap.quote` | available | remote | [swap.quote](docs/capabilities/swap/README.md) |
| `swap.build` | planned | local | — |
| `swap.simulate` | planned | local | — |
| `swap.execute` | planned | local | — |
| `wallet.*` | planned | local | — |

Execution location is fixed per capability. Agents and users do not select local/remote.
See [product architecture](docs/PRODUCT-ARCHITECTURE.md).

## How it works

```text
Agent ──MCP──▶ fluxion mcp ──▶ fluxion-remote ──▶ Ethereum RPC / subgraphs
                  │
                  └── future: local runtime → wallet / authorization / signing
```

`swap.quote` is read-only. It never signs, approves or broadcasts.

## Performance

90 rows, 209 pool/conversion records, full-range V3:

| Metric | Result |
|---|---:|
| Snapshot lag blocks p50/p95/max | **0 / 1 / 1** |
| Snapshot lag seconds p50/p95/max | **0 / 12 / 12 s** |
| Fluxion HTTP p50/p95 | **49 / 133 ms** |
| Success | **90 / 90** |
| ParaSwap within ±50 bps | **89 / 90** |
| 1inch within ±50 bps | **82 / 90** |

Split allocation includes disjoint multi-hop routes. Full methodology and raw data:
[docs/benchmarks](docs/benchmarks/README.md).

## Repository layout

```text
crates/protocol   capability contract, swap DTOs, error codes
crates/remote     discovery, snapshots, routing, HTTP API, fluxion-remote
crates/client     CLI, MCP bridge, remote client, token storage, fluxion
skills/           agent instructions
schemas/          machine-readable JSON schema
```

## Documentation

- **Start here**: [MCP](docs/MCP.md) · [swap.quote](docs/capabilities/swap/README.md) · [product architecture](docs/PRODUCT-ARCHITECTURE.md)
- **Vision**: [whitepaper](docs/WHITEPAPER.md) · [architecture](docs/ARCHITECTURE.en.md) · [local state](docs/LOCAL-STATE.md)
- **Operations**: [HTTP API](docs/API.md) · [configuration](docs/CONFIGURATION.md) · [development](docs/DEVELOPMENT.md)
- **Research**: [benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- **Boundaries**: [limitations](docs/LIMITATIONS.md) · [schema](schemas/swap_quote.v1.json)

## Development

```bash
cargo build --workspace
cargo test --workspace
cargo test --workspace -- --ignored   # mainnet, read-only
cargo clippy --workspace --all-targets
cargo fmt
```

## Status

Pre-1.0. Ethereum mainnet, exact-in, read-only quotes. `swap.build` / `simulate` / `execute` and
wallet support are the next local-runtime milestones. Feedback and issues are welcome.
