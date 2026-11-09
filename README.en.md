# Fluxion

Fluxion is an open Web3 capability layer for AI agents. It turns common onchain actions into MCP
tools, so an agent doesn't need a separate integration for every protocol, and it never needs your
private keys.

It runs on Ethereum mainnet today. Everything is read-only: quotes, balances, token and NFT
holdings, allowances, and calldata simulation. Signing, approvals, and transaction execution are
not implemented yet.

English · [中文](README.md)

![status](https://img.shields.io/badge/status-pre--1.0-orange)
![MCP](https://img.shields.io/badge/MCP-stdio-blueviolet)
![Rust](https://img.shields.io/badge/Rust-2024-orange)
![Ethereum](https://img.shields.io/badge/Ethereum-mainnet-627eea)

## What you can do today

| Capability | What it does |
|---|---|
| `swap.quote` | Exact-in quotes on Ethereum mainnet, with multi-hop routing and split allocation |
| `wallet.balance` | Balances for a chosen token set at one pinned block |
| `wallet.assets` | Discover an address's ERC20 holdings, then verify each balance locally |
| `wallet.allowance` | ERC20 allowances for an owner/spender pair |
| `wallet.collectibles` | ERC721 and ERC1155 holdings |
| `wallet.connect` | Link a mobile wallet by scanning a WalletConnect QR code |
| `wallet.status` | Inspect the local wallet session |
| `wallet.disconnect` | Remove the local wallet session |
| `transaction.simulate` | Simulate calldata locally and return the result and gas |

`swap.build`, `swap.execute`, and other write operations are still planned.

## Install and use

```bash
cargo install --path crates/client
fluxion mcp
```

`fluxion mcp` exposes the capabilities as MCP tools. It does not start the remote quote service.
For running the remote, registering the MCP client, and configuring Skills, see
[docs/MCP.md](docs/MCP.md).

You can also use the CLI directly:

```bash
fluxion swap ldo-eth 100
fluxion wallet balance 0xabc... --tokens ETH,USDC
fluxion wallet collectibles 0xabc...
fluxion transaction simulate --to 0xabc... --data 0x...
```

`swap.quote` needs a remote service:

```bash
FLUXION_REMOTE_URL=http://127.0.0.1:8080 fluxion swap ldo-eth 100 --json
```

## Why the split

Quote computation needs continuous pool sync, snapshot maintenance, and route search, so it lives
in the remote service. A request reads one published snapshot; the quote path makes no RPC calls.

Wallet reads, WalletConnect sessions, and simulation need to stay close to the user, so they run
locally. Plain reads do not touch private keys; `wallet.connect` stores a session only, and signing
still happens in the phone wallet.

Each capability has one fixed execution location. Agents and users don't choose remote or local,
and there is no automatic fallback between them.

## Architecture

```text
Agent (opencode / Claude / Codex)
  │  MCP stdio
  ▼
fluxion mcp
  ├── swap.quote ─────────────────────► Fluxion remote ──► Ethereum RPC / subgraphs
  └── wallet.* / transaction.simulate ─► local RPC
```

- The `swap.quote` service handles discovery, snapshots, and routing. It is read-only.
- `wallet.*` and `transaction.simulate` run locally. They are read-only.
- Outputs are structured JSON, with schemas for fields and error codes.

See [product architecture](docs/PRODUCT-ARCHITECTURE.md) for the full responsibility split.

## Documentation

- Integration: [MCP](docs/MCP.md)
- Capabilities: [swap.quote](docs/capabilities/swap/README.md) · [wallet](docs/capabilities/wallet/README.md) · [transaction](docs/capabilities/transaction/README.md)
- Architecture: [product architecture](docs/PRODUCT-ARCHITECTURE.md) · [engine architecture](docs/ARCHITECTURE.en.md) · [local state](docs/LOCAL-STATE.md)
- Operations: [HTTP API](docs/API.md) · [configuration](docs/CONFIGURATION.md) · [development](docs/DEVELOPMENT.md)
- Research: [benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- Contracts: [swap schema](schemas/swap_quote.v1.json) · [wallet schema](schemas/wallet_balances.v1.json) · [allowance schema](schemas/wallet_allowances.v1.json) · [collectibles schema](schemas/wallet_collectibles.v1.json) · [transaction schema](schemas/transaction_simulate.v1.json)
- Skills: [swap](skills/swap/SKILL.md) · [wallet](skills/wallet/SKILL.md) · [transaction](skills/transaction/SKILL.md)

## Status and limits

Fluxion is pre-1.0. It supports Ethereum mainnet only. Account and simulation capabilities are
read-only; `wallet.connect` only establishes a local WalletConnect session.

- It cannot sign, approve, or broadcast transactions.
- Full `wallet.assets` and `wallet.collectibles` discovery depends on optional providers. Without
  a key, they fall back to the registry or an explicit collection, and report `coverage` and
  `warnings`.
- Non-enumerable ERC721 collections return a count but no token ids. ERC1155 enumeration needs an
  indexer.
- There is no USD valuation.
- The remote quote service has to be deployed separately.
- Quote benchmarks and reproduction steps live in [docs/benchmarks](docs/benchmarks/README.md).

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt
```
