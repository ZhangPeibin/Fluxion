# Fluxion

Fluxion is a Web3 capability layer for AI agents. It packages quoting, account reads, wallet
linking, transaction inspection, and signature authorization into stable MCP capabilities, so an
agent can read chains, quote, prepare transactions, and show an auditable risk report before the user
signs.

It solves two problems:

- **Agents do not know how to use chains**: fragmented RPCs, DEXes, wallets, and data models become
  fixed inputs and outputs.
- **Agents must not blind-sign**: decode calldata before signing, simulate state changes, inspect
  internal calls and source, and state clearly what was not covered.

English · [中文](README.md)

[![CI](https://github.com/ZhangPeibin/Fluxion/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/ZhangPeibin/Fluxion/actions/workflows/ci.yml)
![status](https://img.shields.io/badge/status-pre--1.0-orange)
![MCP](https://img.shields.io/badge/MCP-stdio-blueviolet)
![Rust](https://img.shields.io/badge/Rust-2024-orange)
![Ethereum](https://img.shields.io/badge/Ethereum-mainnet-627eea)
![BSC](https://img.shields.io/badge/BSC-mainnet-f3ba2f)

## Install

```bash
cargo install --path crates/client
```

`swap.quote` needs a remote service:

```bash
FLUXION_REMOTE_URL=http://127.0.0.1:8080 fluxion swap ldo-eth 100
```

Local wallet, transaction inspection, and simulation default to BSC; use `--chain ethereum` for
Ethereum.

## Quick start

### MCP

```bash
fluxion mcp
```

`fluxion mcp` exposes capabilities over stdio and does not start the remote service. Client
registration and full setup live in [docs/MCP.md](docs/MCP.md).

### Skills

Skills tell the agent when to call a capability and how to read the result. Add the repository
`skills/` directory to your client config:

```json
{
  "skills": {
    "paths": ["/path/to/Fluxion/skills"]
  }
}
```

Then ask the agent normally:

```text
Quote 100 LDO to ETH and show the best route
Show the BNB and USDT balances for 0x...
Inspect this calldata before I sign
Inspect this Permit2 typed data
```

The “Ask the agent” examples under each capability are ready to use.

## Capabilities

Every capability has one fixed execution location. Agents and users do not choose remote/local;
that is internal Fluxion routing.

### Quotes

#### `swap.quote`

Exact-in quotes on Ethereum mainnet, with multi-hop routing and split allocation. Returns
`amountOut`, price impact, path, and per-hop allocation. It does not build calldata or execute a
swap.

```bash
fluxion swap ldo-eth 100
fluxion swap quote --in LDO --out WETH --amount 100 --json
```

Ask the agent:

```text
Quote 100 LDO to ETH and show the best route.
```

Skill: [swap](skills/swap/SKILL.md) · Docs: [swap.quote](docs/capabilities/swap/README.md)

### Account reads

Skill: [wallet](skills/wallet/SKILL.md) · Docs: [wallet](docs/capabilities/wallet/README.md)

#### `wallet.balance`

Read native and token balances at a pinned block. BSC is the default; Ethereum is explicit.

```bash
fluxion wallet balance 0x... --tokens BNB,USDT
fluxion wallet balance 0x... --tokens ETH,USDC --chain ethereum
```

Ask the agent:

```text
Show the BNB and USDT balances for 0x... at the latest block.
```

#### `wallet.assets`

Discover token holdings and verify each balance locally. Without a provider it falls back to the
built-in registry and explains completeness through `coverage` / `warnings`.

```bash
fluxion wallet assets 0x...
fluxion wallet assets 0x... --include-unknown
```

Ask the agent:

```text
Show which tokens this address holds and flag unknown tokens.
```

#### `wallet.allowance`

Read ERC20/BEP-20 allowances. Zero allowances are returned so approval risk is unambiguous.

```bash
fluxion wallet allowance 0xowner --spender 0xspender --tokens USDT,CAKE
```

Ask the agent:

```text
Check the remaining USDT allowance from owner to spender.
```

#### `wallet.collectibles`

Read ERC721 / ERC1155 holdings. Without an NFT indexer it checks explicit collections or the
built-in registry only.

```bash
fluxion wallet collectibles 0x...
fluxion wallet collectibles 0x... --collections 0xcollection
```

Ask the agent:

```text
Show this address's NFTs; if there is no indexer, only check the collection I gave you.
```

### Wallet sessions and transfers

Skill: [wallet](skills/wallet/SKILL.md) · Docs: [wallet](docs/capabilities/wallet/README.md)

#### `wallet.connect`

Start a WalletConnect v2 session and return a URI plus terminal QR. Defaults to BSC; use
`--chain ethereum` for Ethereum. Private keys stay in the phone wallet.

```bash
fluxion wallet connect
fluxion wallet connect --chain ethereum
```

Ask the agent:

```text
Link my wallet on BSC; show the QR first, then poll the session status.
```

#### `wallet.status`

Read the local WalletConnect session without contacting the wallet.

```bash
fluxion wallet status
fluxion wallet status --wait 90
```

Ask the agent:

```text
Is my wallet connected? If not, keep waiting for up to 90 seconds.
```

#### `wallet.disconnect`

Remove the local session and abort any pending pairing.

```bash
fluxion wallet disconnect
```

Ask the agent:

```text
Disconnect the current wallet session.
```

#### `wallet.agent-id`

Resolve a SPACE ID `.ai` Agent ID on BSC, with optional on-chain registry verification. This is
identity resolution only; it is not Binance Agentic Wallet login.

```bash
fluxion wallet agent-id alice.ai
fluxion wallet agent-id 0xA506... --json
```

Ask the agent:

```text
Resolve alice.ai to its BSC address and Agent ID.
```

#### `wallet.sign`

Sign a message or EIP-712 typed data through WalletConnect. The user confirms on their phone; a
rejection returns `USER_REJECTED`.

```bash
fluxion wallet sign --message "hello"
fluxion wallet sign --typed-data typed-data.json
```

Ask the agent:

```text
Sign this message with my connected wallet: hello.
```

#### `wallet.send`

Build a native or ERC20/BEP-20 transfer, simulate it locally, then ask the phone to confirm.
Simulation failure never reaches the phone.

```bash
fluxion wallet send --to 0x... --token BNB --amount 0.01
fluxion wallet send --to 0x... --token USDT --amount 10
```

Ask the agent:

```text
Send 0.01 BNB to 0x...; simulate first and do not send if it fails.
```

### Transaction safety

Skill: [transaction](skills/transaction/SKILL.md) · Docs: [transaction](docs/capabilities/transaction/README.md)

#### `transaction.inspect`

Deterministic pre-sign inspection. Returns intent, asset/allowance changes, findings, coverage, and
unknowns. It is advisory, not a safety guarantee, and never signs or broadcasts.

```bash
fluxion transaction inspect --to 0x... --data 0x...
fluxion transaction inspect --to 0x... --data 0x... --policy policy.json
```

Ask the agent:

```text
Inspect this calldata before I sign: tell me what it does, which assets move, and whether there is
an approval or delegatecall risk.
```

#### `transaction.simulate`

Run `eth_call` and `eth_estimateGas` at a pinned block and return success/revert, return data, and
gas.

```bash
fluxion transaction simulate --to 0x... --data 0x...
fluxion transaction simulate --to 0x... --data 0x... --from 0x...
```

Ask the agent:

```text
Simulate this transaction, tell me if it reverts, and estimate gas.
```

### Signature authorization

Skill: [signature](skills/signature/SKILL.md) · Docs: [signature](docs/capabilities/signature/README.md)

#### `signature.inspect`

Inspect authorization that is not ordinary calldata: EIP-712 typed data, Permit2, EIP-7702
authorization lists, and ERC-4337 user operations.

```bash
fluxion signature inspect --typed-data permit.json
fluxion signature inspect --authorization-list auth.json
fluxion signature inspect --user-operation userop.json
```

Ask the agent:

```text
Inspect this Permit2 typed data: check the amount, expiration, spender, and chain ID.
```

## Security model

Fluxion does not rely on “ask a model to look at it”. `transaction.inspect` /
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

## Execution model

Every capability has one fixed execution location. Agents and users do not choose remote/local:

- `swap.quote` is remote and reads one published immutable snapshot.
- `wallet.*`, `transaction.*`, and `signature.*` are local.
- `wallet.agent-id` also uses the SPACE ID API and verifies against BSC.
- Reads and simulations are pinned to a block; missing coverage is reported through
  `coverage` / `unknowns`.

```text
Agent ──MCP──▶ fluxion mcp ──▶ capability router
                                 ├── swap.quote ─────────▶ remote snapshot ──▶ Ethereum
                                 ├── wallet.* / transaction.* / signature.*
                                 │                       ▶ local runtime (RPC / WalletConnect)
                                 └── wallet.agent-id ────▶ SPACE ID API + BSC
```

## Documentation

- Integration: [MCP](docs/MCP.md)
- Capabilities: [swap.quote](docs/capabilities/swap/README.md) · [wallet](docs/capabilities/wallet/README.md) · [transaction](docs/capabilities/transaction/README.md) · [signature](docs/capabilities/signature/README.md)
- Architecture: [product architecture](docs/PRODUCT-ARCHITECTURE.md) · [engine architecture](docs/ARCHITECTURE.en.md) · [local state](docs/LOCAL-STATE.md)
- Operations: [HTTP API](docs/API.md) · [configuration](docs/CONFIGURATION.md) · [development](docs/DEVELOPMENT.md)
- Research: [benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- Skills: [swap](skills/swap/SKILL.md) · [wallet](skills/wallet/SKILL.md) · [transaction](skills/transaction/SKILL.md) · [signature](skills/signature/SKILL.md)

## Status and boundaries

Pre-1.0. Local wallet, transaction, and signature capabilities support BSC (default) and Ethereum;
`swap.quote` remains Ethereum mainnet.

- `wallet.sign` signs messages and EIP-712 only; `wallet.send` sends native/ERC20/BEP-20 transfers
  only.
- `transaction.inspect` / `signature.inspect` are advisory and not a safety guarantee.
- `wallet.connect` only establishes a WalletConnect session; keys stay in the phone wallet.
- `wallet.agent-id` only resolves BSC identity; it is not Binance Agentic Wallet login.
- Full `wallet.assets` / `wallet.collectibles` discovery depends on optional providers. Fallbacks
  report coverage and warnings.
- Non-enumerable ERC721 collections return a count but no token ids; ERC1155 enumeration needs an
  indexer.
- There is no USD valuation.
- `swap.quote` needs a separately deployed remote service.

Next: `transaction.send`, `swap.build`, and `swap.execute`.

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt

# Foundry security red-team
python3 bench/security/redteam.py
```
