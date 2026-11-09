# Fluxion

Fluxion is a Web3 capability layer for AI agents. It turns onchain actions into stable MCP
capabilities and is designed for multi-chain support, not tied to one network or protocol.

The current implementation covers Ethereum mainnet with 9 read-only capabilities. Quotes run
against an immutable remote snapshot; wallet reads, WalletConnect sessions, and simulation run
locally. See [Capabilities](#capabilities) for the complete list.

English · [中文](README.md)

[![CI](https://github.com/ZhangPeibin/Fluxion/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/ZhangPeibin/Fluxion/actions/workflows/ci.yml)
![status](https://img.shields.io/badge/status-pre--1.0-orange)
![MCP](https://img.shields.io/badge/MCP-stdio-blueviolet)
![Rust](https://img.shields.io/badge/Rust-2024-orange)
![Ethereum](https://img.shields.io/badge/Ethereum-mainnet-627eea)

## Install

```bash
cargo install --path crates/client
```

## Usage

### CLI

```console
$ fluxion swap ldo-eth 100
LDO → WETH
100 LDO → 0.015452346279312091 WETH
plan: baseline · block 26377973 · lag 0 · 21 ms
```

Other commands:

```bash
fluxion wallet balance 0x... --tokens ETH,USDC
fluxion wallet assets 0x...
fluxion wallet collectibles 0x...
fluxion wallet connect
fluxion transaction simulate --to 0x... --data 0x...
```

`swap.quote` needs a remote service:

```bash
FLUXION_REMOTE_URL=http://127.0.0.1:8080 fluxion swap ldo-eth 100 --json
```

### Skills

Skills are agent instructions, not an execution path. The repository ships three:

- [swap](skills/swap/SKILL.md): when to quote, and how to read split, sources, and children
- [wallet](skills/wallet/SKILL.md): balances, assets, allowances, NFTs, and WalletConnect
- [transaction](skills/transaction/SKILL.md): calldata simulation and revert handling

For opencode, add the repository `skills/` directory to your config:

```json
{
  "skills": {
    "paths": ["/path/to/Fluxion/skills"]
  }
}
```

Then ask the agent normally:

```text
Show me the ETH, USDC and NFTs for 0x...
Quote 100 LDO to ETH
Simulate this calldata
Connect my wallet
```

Skills only guide tool use. Execution routing, pinned blocks, and error codes remain in the
MCP/runtime.

### MCP

```bash
fluxion mcp
```

`fluxion mcp` exposes the capabilities over stdio and does not start the remote service. Client
registration, remote deployment, and full setup live in [docs/MCP.md](docs/MCP.md).

## Capabilities

| Capability | Description |
|---|---|
| `swap.quote` | Exact-in quotes on Ethereum mainnet, with multi-hop routing and split allocation |
| `wallet.balance` | ETH/ERC20 balances at a pinned block |
| `wallet.assets` | Discover ERC20 holdings and verify balances locally |
| `wallet.allowance` | ERC20 allowances for an owner/spender pair |
| `wallet.collectibles` | ERC721 and ERC1155 holdings |
| `wallet.connect` | Link a mobile wallet with WalletConnect |
| `wallet.status` | Inspect the local wallet session |
| `wallet.disconnect` | Remove the local wallet session |
| `transaction.simulate` | Simulate calldata at a pinned block and return the result and gas |

Planned: `wallet.sign`, `swap.build`, `swap.execute`. The current WalletConnect session only
establishes a connection; it does not sign, approve or broadcast.

## How it works

Each capability has one fixed execution location. Agents and users do not choose remote or local:

- `swap.quote` is remote. A request reads one published immutable snapshot; the compute path makes
  no RPC calls.
- `wallet.*` and `transaction.simulate` are local. Reads and simulation are pinned to a block.
- Responses use stable JSON schemas and error codes; amounts, blocks, sources, and coverage are part
  of the result.

```text
Agent ──MCP──▶ fluxion mcp ──▶ capability router
                                 ├── swap.quote ─────────▶ remote snapshot ──▶ Ethereum
                                 └── wallet.* / simulate ─▶ local RPC
```

## Repository layout

```text
crates/protocol   capability contract, JSON types, error codes
crates/remote     discovery, snapshot, routing, split search, HTTP API
crates/client     CLI, MCP bridge, wallet, WalletConnect, local simulation
schemas/          response JSON schemas
skills/           agent usage instructions
docs/benchmarks/  benchmark methodology and raw data
```

Chain interaction uses hand-written JSON-RPC and ABI encoding; only `alloy-primitives` is used from
alloy. CI runs fmt, clippy, and unit tests.

## Benchmarks

The Ethereum mainnet 90-row runtime benchmark from 6 Nov 2026 (309 pools) completed 90/90 quotes,
with HTTP p50/p95 of 35/197 ms and snapshot lag of 0/1/1 blocks. 82/90 rows were within ±50 bps of
ParaSwap and 79/90 within ±50 bps of 1inch. Methodology and raw data:
[docs/benchmarks](docs/benchmarks/README.md).

## Status

Pre-1.0. The current implementation covers Ethereum mainnet; more chains are the target.

- Account, quote, and simulation capabilities are read-only; they cannot sign, approve, or broadcast.
- `wallet.connect` only establishes a WalletConnect session; keys stay in the phone wallet.
- Full `wallet.assets` / `wallet.collectibles` discovery depends on optional providers. Fallbacks
  report coverage and warnings.
- Non-enumerable ERC721 collections return a count but no token ids; ERC1155 enumeration needs an
  indexer.
- There is no USD valuation.
- `swap.quote` needs a separately deployed remote service.

Next: `wallet.sign`, `swap.build`, and `swap.execute`.

## Documentation

- Integration: [MCP](docs/MCP.md)
- Capabilities: [swap.quote](docs/capabilities/swap/README.md) · [wallet](docs/capabilities/wallet/README.md) · [transaction](docs/capabilities/transaction/README.md)
- Architecture: [product architecture](docs/PRODUCT-ARCHITECTURE.md) · [engine architecture](docs/ARCHITECTURE.en.md) · [local state](docs/LOCAL-STATE.md)
- Operations: [HTTP API](docs/API.md) · [configuration](docs/CONFIGURATION.md) · [development](docs/DEVELOPMENT.md)
- Research: [benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- Contracts: [swap schema](schemas/swap_quote.v1.json) · [wallet schema](schemas/wallet_balances.v1.json) · [allowance schema](schemas/wallet_allowances.v1.json) · [collectibles schema](schemas/wallet_collectibles.v1.json) · [connect schema](schemas/wallet_connect.v1.json) · [status schema](schemas/wallet_status.v1.json) · [disconnect schema](schemas/wallet_disconnect.v1.json) · [transaction schema](schemas/transaction_simulate.v1.json)
- Skills: [swap](skills/swap/SKILL.md) · [wallet](skills/wallet/SKILL.md) · [transaction](skills/transaction/SKILL.md)

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt
```
