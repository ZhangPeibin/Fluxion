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
| `swap.quote` | available | [swap.quote](docs/capabilities/swap/README.md) |

A capability defines *what* the agent wants; execution location is a Fluxion implementation
detail. Agents do not select local/remote. `swap.quote` defaults to the managed service;
development/self-hosting uses `FLUXION_MODE=self_hosted`. See the
[execution model](docs/capabilities/README.md).

## Quick start

### CLI / MCP

Start a warm runtime once; later quotes reuse state:

```bash
fluxion swap ldo-eth 100
fluxion swap ldo-eth 100 --json
fluxion ldo-eth 100

# Development/self-hosted: use the local runtime
FLUXION_MODE=self_hosted GRAPH_API_KEY=... fluxion start
fluxion status
fluxion stop

fluxion mcp                  # local MCP stdio server
```

Machine interface:

```bash
fluxion swap quote --in LDO --out ETH --amount 100 --json
fluxion capability describe swap.quote --json
```

The default runtime config is `configs/runtime.yaml` (local snapshot + incremental) and the
default endpoint is `http://127.0.0.1:8080`. If no config exists in the current directory, the
first `start` writes the embedded default to `~/.fluxion/config.yaml`; logs go to
`~/.fluxion/runtime.log`.

### Self-hosted HTTP server

```bash
cp .env.example .env        # set GRAPH_API_KEY
cp config.example.yaml config.yaml
cargo run -- serve --config config.yaml
```

See [HTTP API](docs/API.md) and [Configuration](docs/CONFIGURATION.md) (Chinese).

## Execution and keys

| Internal mode | `swap.quote` execution | Key |
|---|---|---|---|
| `managed` (`FLUXION_REMOTE_URL` set) | Fluxion hosted service | `FLUXION_API_KEY` |
| `self_hosted` | local runtime | `GRAPH_API_KEY` + RPC |

`FLUXION_MODE=auto` (default) uses remote when `FLUXION_REMOTE_URL` is configured, otherwise
local. This is deployment/operator configuration, not an agent provider choice.

The repository `.env` is development-only and is not distributed. Never commit secrets.

## Latest benchmark

90 rows on 4 Nov 2026, 209 pool/conversion records, full-range V3:

| Metric | Result |
|---|---:|
| Snapshot lag blocks p50/p95/max | **0 / 1 / 1** |
| Snapshot lag seconds p50/p95/max | **0 / 12 / 12 s** |
| Fluxion HTTP p50/p95 | **49 / 133 ms** |
| Success | **90 / 90** |
| ParaSwap mean delta / within ±50 bps | **−2.6 bps / 89** |
| 1inch mean delta / within ±50 bps | **−8.7 bps / 82** |

Splitting now includes disjoint multi-hop routes. Remaining large-order gaps are mostly venue
coverage limits (for example LDO and SHIB), not freshness or allocation. See the
[benchmark index](docs/benchmarks/README.md).

## Documentation

- [Capability index](docs/capabilities/README.md) · [swap.quote](docs/capabilities/swap/README.md) · [Agent skill](skills/swap/SKILL.md)
- [Whitepaper](docs/WHITEPAPER.md) · [Architecture](docs/ARCHITECTURE.en.md) · [Local state](docs/LOCAL-STATE.md) (Chinese)
- [HTTP API](docs/API.md) · [Configuration](docs/CONFIGURATION.md) · [Development](docs/DEVELOPMENT.md) · [Viewer](docs/VIEWER.md) (Chinese)
- [Benchmarks](docs/benchmarks/README.md) · [Multi-pool](docs/MULTI-POOL.md) · [Split routing](docs/SPLIT-ROUTING.md) · [Quote quality](docs/QUOTE-QUALITY.md)
- [Limitations](docs/LIMITATIONS.md) · [JSON schema](schemas/swap_quote.v1.json)

## Development

```bash
cargo build
cargo test
cargo test -- --ignored      # mainnet, read-only
cargo clippy --all-targets
cargo fmt
```

## Status

Ethereum mainnet, exact-in, read-only quotes. `swap.build` / `simulate` / `execute` and wallet
handling belong to a later Local Runtime; quotes do not construct or simulate transactions.
Independent quotes for pools shared across routes are unsupported. Public HTTP has no auth,
only simple rate limiting. See [limitations](docs/LIMITATIONS.md) (Chinese).
