# Fluxion

Fluxion is a Web3 capability layer for AI agents. It turns onchain actions into stable MCP
capabilities and is designed for multi-chain support, not tied to one network or protocol.

The implementation ships 14 capabilities. Local wallet and transaction tools default to BSC;
Ethereum is available with `--chain ethereum`. `swap.quote` remains an Ethereum mainnet quote.
Quotes run against an immutable remote snapshot; wallet reads, WalletConnect sessions, and
simulation run locally. See [Capabilities](#capabilities) for the complete list.

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
fluxion wallet balance 0x... --tokens BNB,USDT
fluxion wallet balance 0x... --tokens ETH,USDC --chain ethereum
fluxion wallet assets 0x...
fluxion wallet collectibles 0x...
fluxion wallet connect
fluxion transaction inspect --to 0x... --data 0x...
fluxion transaction simulate --to 0x... --data 0x...
fluxion signature inspect --typed-data typed-data.json
```

`swap.quote` needs a remote service:

```bash
FLUXION_REMOTE_URL=http://127.0.0.1:8080 fluxion swap ldo-eth 100 --json
```

### Skills

Skills tell the agent when to call a tool and how to read the result. Four are included:

- [swap](skills/swap/SKILL.md): quotes, split allocations, and route output
- [wallet](skills/wallet/SKILL.md): balances, assets, allowances, NFTs, and WalletConnect
- [transaction](skills/transaction/SKILL.md): calldata inspection, simulation, and reverts
- [signature](skills/signature/SKILL.md): EIP-712, Permit2, 7702, and 4337 authorization

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

Skills are instructions only. Execution routing, pinned blocks, and error codes live in the
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
| `wallet.balance` | BNB/ETH and ERC20/BEP-20 balances at a pinned block |
| `wallet.assets` | Discover token holdings and verify balances locally |
| `wallet.allowance` | Token allowances for an owner/spender pair |
| `wallet.collectibles` | ERC721 and ERC1155 holdings |
| `wallet.connect` | Link a mobile wallet with WalletConnect on Ethereum or BSC |
| `wallet.status` | Inspect the local wallet session |
| `wallet.disconnect` | Remove the local wallet session |
| `wallet.agent-id` | Resolve a BSC `.ai` Agent ID |
| `wallet.sign` | Sign messages or EIP-712 typed data through WalletConnect |
| `wallet.send` | Send native or ERC20/BEP-20 transfers through WalletConnect |
| `transaction.inspect` | Decode unsigned calldata; verify effects, internal calls and source; return evidence, coverage and unknowns |
| `transaction.simulate` | Simulate calldata at a pinned block and return the result and gas |
| `signature.inspect` | Inspect EIP-712, Permit2, 7702 and 4337 authorization |

Local wallet and simulation tools default to BSC; pass `--chain ethereum` for Ethereum.
`swap.quote` remains an Ethereum mainnet quote.

Planned: `transaction.send`, `swap.build`, `swap.execute`. `wallet.sign` signs messages and
EIP-712 typed data only; `wallet.send` sends native/ERC20/BEP-20 transfers only and does not
support arbitrary calldata, approvals, or swap execution.

## How it works

Each capability has one fixed execution location. Agents and users do not choose remote or local:

- `swap.quote` is remote. A request reads one published immutable snapshot; the compute path makes
  no RPC calls.
- `wallet.*`, `transaction.inspect` and `transaction.simulate` are local; `wallet.agent-id` also uses the SPACE ID API
  and verifies against BSC. Reads and simulation are pinned to a block.
- Responses use stable JSON schemas and error codes; amounts, blocks, sources, and coverage are part
  of the result.

```text
Agent ──MCP──▶ fluxion mcp ──▶ capability router
                                 ├── swap.quote ─────────▶ remote snapshot ──▶ Ethereum
                                 ├── wallet.* / transaction.* ─▶ local runtime (RPC / WalletConnect)
                                 └── wallet.agent-id ────▶ SPACE ID API + BSC
```

## Security inspection

Pre-sign inspection is not “ask a model to look at it”. `transaction.inspect` /
`signature.inspect` run a deterministic pipeline:

1. Decode calldata or typed data; unknown selectors/types stay `unknown`.
2. Simulate the target transaction at a pinned block and parse logs.
3. Re-simulate `balanceOf` / `allowance` / `ownerOf` and verify logs against real state changes.
   Forged logs produce `EFFECT-007/009/010`.
4. When the RPC supports `debug_traceCall`, inspect internal calls and unexpected delegatecalls;
   otherwise report `coverage.trace=unavailable`.
5. Optionally verify source through BscScan / Etherscan; without a key, `verified=null`.
6. Return finding IDs, severity, evidence, coverage, and explicit unknowns.

Principles:

- Never output `safe`; no finding means “nothing found within current coverage”.
- LLMs do not decide risk. Explanations are deterministic and may be paraphrased by the host agent.
- Advisory only: no signing, broadcasting, or private keys.
- Missing data stays `unknown`; no heuristic guessing.

The Foundry + Anvil red-team corpus covers fake logs, hidden delegatecalls, proxy upgrades,
unlimited approvals, permits, 7702, and 4337. Current results: **15/15 detect**, **2/2 controls with
no findings**. Method and raw data: [red-team](docs/benchmarks/redteam-2026-11-11.md) and
[security benchmark](docs/benchmarks/security-inspection-2026-11-11.md).

Boundaries: this is not an audit or a safety guarantee; public BSC RPC has no trace; the real-world
malicious corpus is still expanding.

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

Pre-1.0. Local wallet, transaction and signature capabilities support BSC (default) and Ethereum;
`swap.quote` remains Ethereum mainnet.

- Account, quote, and simulation capabilities are read-only; `wallet.sign` signs messages/EIP-712
  only; `wallet.send` sends native/ERC20/BEP-20 transfers only and cannot execute arbitrary calls or
  swaps.
- All local wallet capabilities support Ethereum and BSC; local tools default to BSC.
- `transaction.inspect` / `signature.inspect` are advisory and not a safety guarantee.
- `wallet.connect` only establishes a WalletConnect session; keys stay in the phone wallet.
- `wallet.agent-id` only resolves BSC identity; it is not Binance Agentic Wallet login and does not
  check launch eligibility.
- Full `wallet.assets` / `wallet.collectibles` discovery depends on optional providers. Fallbacks
  report coverage and warnings.
- Non-enumerable ERC721 collections return a count but no token ids; ERC1155 enumeration needs an
  indexer.
- There is no USD valuation.
- `swap.quote` needs a separately deployed remote service.

Next: `transaction.send`, `swap.build`, and `swap.execute`.

## Documentation

- Integration: [MCP](docs/MCP.md)
- Capabilities: [swap.quote](docs/capabilities/swap/README.md) · [wallet](docs/capabilities/wallet/README.md) · [transaction](docs/capabilities/transaction/README.md)
- Architecture: [product architecture](docs/PRODUCT-ARCHITECTURE.md) · [engine architecture](docs/ARCHITECTURE.en.md) · [local state](docs/LOCAL-STATE.md)
- Operations: [HTTP API](docs/API.md) · [configuration](docs/CONFIGURATION.md) · [development](docs/DEVELOPMENT.md)
- Research: [benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- Contracts: [swap schema](schemas/swap_quote.v1.json) · [wallet schema](schemas/wallet_balances.v1.json) · [allowance schema](schemas/wallet_allowances.v1.json) · [collectibles schema](schemas/wallet_collectibles.v1.json) · [connect schema](schemas/wallet_connect.v1.json) · [status schema](schemas/wallet_status.v1.json) · [disconnect schema](schemas/wallet_disconnect.v1.json) · [agent-id schema](schemas/wallet_agent_id.v1.json) · [sign schema](schemas/wallet_sign.v1.json) · [send schema](schemas/wallet_send.v1.json) · [inspect schema](schemas/transaction_inspect.v1.json) · [transaction schema](schemas/transaction_simulate.v1.json) · [signature schema](schemas/signature_inspect.v1.json)
- Skills: [swap](skills/swap/SKILL.md) · [wallet](skills/wallet/SKILL.md) · [transaction](skills/transaction/SKILL.md) · [signature](skills/signature/SKILL.md)

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt
```
