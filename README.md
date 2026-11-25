# Eollo

Eollo is a Web3 capability layer for AI agents. It packages quoting, account reads, wallet
linking, transaction inspection, and signature authorization into stable MCP capabilities, so an
agent can read chains, quote, prepare transactions, and show an auditable risk report before the user
signs.

It addresses three problems:

- **Every protocol is a new integration**: RPCs, DEXes, wallets, and data models become fixed inputs
  and outputs, so agents stop rewriting glue code per protocol.
- **Every agent action is a trust decision**: decode calldata, simulate state changes, inspect
  internal calls and source, and state clearly what was not covered.
- **Autonomy needs a handoff**: the agent prepares the transaction, but keys stay on the phone and
  every send is confirmed in the user's wallet.

English · [中文](README.zh.md)

Website: <https://zhangpeibin.github.io/eollo/en.html> · source: [`site/`](site/)

[![CI](https://github.com/ZhangPeibin/eollo/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/ZhangPeibin/eollo/actions/workflows/ci.yml)
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
EOLLO_REMOTE_URL=http://127.0.0.1:8080 eollo swap ldo-eth 100
```

Local wallet, transaction inspection, and simulation default to BSC; use `--chain ethereum` for
Ethereum.

## Quick start

### MCP

```bash
eollo mcp
```

`eollo mcp` exposes capabilities over stdio and does not start the remote service. Client
registration and full setup live in [docs/MCP.md](docs/MCP.md).

### Skills

Skills tell the agent when to call a capability and how to read the result. Add the repository
`skills/` directory to your client config:

```json
{
  "skills": {
    "paths": ["/path/to/Eollo/skills"]
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
that is internal Eollo routing.

Capability metadata lives in [`capabilities/catalog.yaml`](capabilities/catalog.yaml) as the single
source of truth, and [`docs/capabilities/README.md`](docs/capabilities/README.md) is generated from
it. Every capability returns capability envelope v2 (`meta`, `chain`, `block`, `as_of`,
`provenance`, `coverage`, `warnings`, `unknowns`, `data`); see
[ADR 0003](docs/adr/0003-capability-envelope-v2.md) and
[ADR 0005](docs/adr/0005-capability-catalog.md).

### Capability overview

#### 1. Quotes

| Capability | Status | CLI | Description |
|---|---|---|---|
| `swap.quote` | stable | `eollo swap ldo-eth 100` | Ethereum mainnet exact-in quotes, multi-hop, split allocation |
| `swap.build` | stable | `eollo swap build --quote quote.json --recipient 0x...` | Build V2 swap calldata from a quote |

#### 2. Wallet

| Capability | Status | CLI | Description |
|---|---|---|---|
| `wallet.balance` | stable | `eollo wallet balance 0x... --tokens BNB,USDT` | Pinned-block balances |
| `wallet.assets` | stable | `eollo wallet assets 0x...` | Asset discovery + local verification |
| `wallet.allowance` | stable | `eollo wallet allowance 0xowner --spender 0xspender --tokens USDT` | Allowances |
| `wallet.collectibles` | stable | `eollo wallet collectibles 0x...` | NFT holdings |
| `wallet.connect` | stable | `eollo wallet connect` | WalletConnect session |
| `wallet.status` | stable | `eollo wallet status` | Inspect local session |
| `wallet.disconnect` | stable | `eollo wallet disconnect` | Remove local session |
| `wallet.agent-id` | stable | `eollo wallet agent-id alice.ai` | BSC `.ai` Agent ID |
| `wallet.sign` | stable | `eollo wallet sign --typed-data typed-data.json` | Message / EIP-712 signing |
| `wallet.send` | stable | `eollo wallet send --to 0x... --token BNB --amount 0.01` | Native / token transfer |

#### 3. Transaction safety

| Capability | Status | CLI | Description |
|---|---|---|---|
| `transaction.inspect` | stable | `eollo transaction inspect --to 0x... --data 0x...` | Pre-sign calldata, effects, trace, source, policy |
| `transaction.simulate` | stable | `eollo transaction simulate --to 0x... --data 0x...` | Pinned `eth_call` + gas |
| `transaction.send` | stable | `eollo transaction send --to 0x... --data 0x... --policy policy.json` | Simulate + inspect + policy + phone + broadcast |
| `transaction.verify` | experimental | `eollo transaction verify --hash 0x...` | Receipt status, confirmations and effects, optionally compared with an inspect report |

#### 4. Signature authorization

| Capability | Status | CLI | Description |
|---|---|---|---|
| `signature.inspect` | stable | `eollo signature inspect --typed-data typed-data.json` | EIP-712, Permit2, 7702, 4337 |

#### 5. Four.meme

| Capability | Status | CLI | Description |
|---|---|---|---|
| `fourmeme.agent-tokens` | experimental | `eollo fourmeme agent-tokens --wallet 0x... --before-block N` | Discover Binance Agent ID Exclusive tokens with block continuation and wallet eligibility |
| `fourmeme.token` | experimental | `eollo fourmeme token 0x... --wallet 0x...` | Detail: phase, market/momentum, socials, holders, tradability and 1% cap, safety signals, eligibility |

#### 6. Yield discovery

| Capability | Status | CLI | Description |
|---|---|---|---|
| `yield.discover` | experimental | `eollo yield discover --asset ETH --amount 10` | Ethereum ETH staking/lending: safety tier, APY, TVL, withdrawal conditions, recommendation |
| `yield.build` | experimental | `eollo yield build --opportunity lido-steth --amount 10 --recipient 0x...` | Local stake/supply/deposit calldata and approval steps |
| `yield.history` | experimental | `eollo yield history --opportunity lido-steth --days 30` | Historical APY, base/reward split, TVL and stability summary |
| `yield.withdraw` | experimental | `eollo yield withdraw --opportunity aave-v3-weth --amount 10 --recipient 0x...` | Local withdraw/redeem/burn exit calldata |

#### 7. Position analysis

| Capability | Status | CLI | Description |
|---|---|---|---|
| `position.analyze` | experimental | `eollo position analyze --owner 0x...` | Cross-market ETH positions, valuation, APY, withdrawal conditions, risk and optional PnL |

Full usage and "ask the agent" examples are below in each capability section.

Local surfaces such as `fourmeme.ui` (`eollo fourmeme ui --open`) are read-only local tools, not
capabilities; they are listed under `surfaces` in the catalog.

### Quotes

#### `swap.quote`

Exact-in quotes on Ethereum mainnet, with multi-hop routing and split allocation. Returns
`amountOut`, price impact, path, and per-hop allocation. It does not build calldata or execute a
swap.

```bash
eollo swap ldo-eth 100
eollo swap quote --in LDO --out WETH --amount 100 --json
```

Ask the agent:

```text
Quote 100 LDO to ETH and show the best route.
```

Skill: [swap](skills/swap/SKILL.md) · Docs: [swap.quote](docs/capabilities/swap/README.md)

### Yield discovery

#### `yield.discover`

Read-only discovery of ETH-denominated staking and lending opportunities on Ethereum. It returns
a curated safety tier, APY components, TVL, withdrawal conditions, risk factors and evidence, plus
one recommendation with explicit tradeoffs. It never builds, signs or executes a transaction.

```bash
eollo yield discover --asset ETH --amount 10 --json
eollo yield discover --asset ETH --amount 10 --risk balanced --json
eollo yield build --opportunity lido-steth --amount 10 --recipient 0x... --json
eollo yield history --opportunity lido-steth --days 30 --json
eollo yield withdraw --opportunity aave-v3-weth --amount 10 --recipient 0x... --json
```

`yield.build` / `yield.withdraw` only return calldata and approval steps; they never sign or
broadcast. `yield.history` returns historical APY, which is not current APY and not a guarantee.

Ask the agent:

```text
I have 10 ETH. Find the safest opportunity with a reasonable APY, give me the conservative
recommendation, and explain what I am giving up.
```

Skill: [yield](skills/yield/SKILL.md) · Docs: [yield](docs/capabilities/yield/README.md)

### Position analysis

#### `position.analyze`

Read a wallet's positions across curated Lido/Rocket Pool/Aave/SparkLend/Morpho markets. Balances
are pinned to one Ethereum block; APY and risk context come from the `yield.discover` snapshot.
Optional cost basis enables PnL.

```bash
eollo position analyze --owner 0x... --json
eollo position analyze --owner 0x... --cost-basis lido-steth=10 --json
```

Ask the agent:

```text
Show my ETH yield positions, what they are worth, the APY, and the withdrawal conditions.
```

Skill: [position](skills/position/SKILL.md) · Docs: [position.analyze](docs/capabilities/position/README.md)

### Account reads

Skill: [wallet](skills/wallet/SKILL.md) · Docs: [wallet](docs/capabilities/wallet/README.md)

#### `wallet.balance`

Read native and token balances at a pinned block. BSC is the default; Ethereum is explicit.

```bash
eollo wallet balance 0x... --tokens BNB,USDT
eollo wallet balance 0x... --tokens ETH,USDC --chain ethereum
```

Ask the agent:

```text
Show the BNB and USDT balances for 0x... at the latest block.
```

#### `wallet.assets`

Discover token holdings and verify each balance locally. Without a provider it falls back to the
built-in registry and explains completeness through `coverage` / `warnings`.

```bash
eollo wallet assets 0x...
eollo wallet assets 0x... --include-unknown
```

Ask the agent:

```text
Show which tokens this address holds and flag unknown tokens.
```

#### `wallet.allowance`

Read ERC20/BEP-20 allowances. Zero allowances are returned so approval risk is unambiguous.

```bash
eollo wallet allowance 0xowner --spender 0xspender --tokens USDT,CAKE
```

Ask the agent:

```text
Check the remaining USDT allowance from owner to spender.
```

#### `wallet.collectibles`

Read ERC721 / ERC1155 holdings. Without an NFT indexer it checks explicit collections or the
built-in registry only.

```bash
eollo wallet collectibles 0x...
eollo wallet collectibles 0x... --collections 0xcollection
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
eollo wallet connect
eollo wallet connect --chain ethereum
```

Ask the agent:

```text
Link my wallet on BSC; show the QR first, then poll the session status.
```

#### `wallet.status`

Read the local WalletConnect session without contacting the wallet.

```bash
eollo wallet status
eollo wallet status --wait 90
```

Ask the agent:

```text
Is my wallet connected? If not, keep waiting for up to 90 seconds.
```

#### `wallet.disconnect`

Remove the local session and abort any pending pairing.

```bash
eollo wallet disconnect
```

Ask the agent:

```text
Disconnect the current wallet session.
```

#### `wallet.agent-id`

Resolve a SPACE ID `.ai` Agent ID on BSC, with optional on-chain registry verification. This is
identity resolution only; it is not Binance Agentic Wallet login.

```bash
eollo wallet agent-id alice.ai
eollo wallet agent-id 0xA506... --json
```

Ask the agent:

```text
Resolve alice.ai to its BSC address and Agent ID.
```

#### `wallet.sign`

Sign a message or EIP-712 typed data through WalletConnect. The user confirms on their phone; a
rejection returns `USER_REJECTED`.

```bash
eollo wallet sign --message "hello"
eollo wallet sign --typed-data typed-data.json
```

Ask the agent:

```text
Sign this message with my connected wallet: hello.
```

#### `wallet.send`

Build a native or ERC20/BEP-20 transfer, simulate it locally, then ask the phone to confirm.
Simulation failure never reaches the phone.

```bash
eollo wallet send --to 0x... --token BNB --amount 0.01
eollo wallet send --to 0x... --token USDT --amount 10
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
eollo transaction inspect --to 0x... --data 0x...
eollo transaction inspect --to 0x... --data 0x... --policy policy.json
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
eollo transaction simulate --to 0x... --data 0x...
eollo transaction simulate --to 0x... --data 0x... --from 0x...
```

Ask the agent:

```text
Simulate this transaction, tell me if it reverts, and estimate gas.
```

#### `transaction.verify`

Read a sent transaction's receipt and return status, confirmations, gas/fee, and effects decoded
from logs; optionally compare against a `transaction.inspect` report and return `match` or
`mismatch`. Read-only; never signs or broadcasts.

```bash
eollo transaction verify --hash 0x... --json
eollo transaction verify --hash 0x... --chain ethereum --confirmations 3 --json
eollo transaction verify --hash 0x... --expect inspect.json --json
```

Status is `success`, `reverted`, `pending`, or `not_found`; `pending` and `not_found` are normal
results, not errors. Without a receipt, effects are marked `not_run`.

Ask the agent:

```text
Did transaction 0x... succeed? What did it actually move? If it differs from the report I
inspected before signing, show me the differences.
```

### Signature authorization

Skill: [signature](skills/signature/SKILL.md) · Docs: [signature](docs/capabilities/signature/README.md)

#### `signature.inspect`

Inspect authorization that is not ordinary calldata: EIP-712 typed data, Permit2, EIP-7702
authorization lists, and ERC-4337 user operations.

```bash
eollo signature inspect --typed-data permit.json
eollo signature inspect --authorization-list auth.json
eollo signature inspect --user-operation userop.json
```

Ask the agent:

```text
Inspect this Permit2 typed data: check the amount, expiration, spender, and chain ID.
```

### Four.meme Agent ID Exclusive

Skill: [fourmeme](skills/fourmeme/SKILL.md) · Docs: [fourmeme](docs/capabilities/fourmeme/README.md)

Discover Four.meme tokens using the `Binance Agent ID Exclusive` template
(`presetId=1778027615735`) and optionally check Agent ID eligibility. An `aiCreator` label or
`.ai` tag is not sufficient.

```bash
eollo fourmeme agent-tokens --json
eollo fourmeme agent-tokens --wallet 0x... --limit 20 --json
eollo fourmeme agent-tokens --lookback-blocks 100000 --json
eollo fourmeme agent-tokens --lookback-blocks 100000 --before-block 26350000 --json
eollo fourmeme token 0x... --wallet 0x... --json
eollo fourmeme ui --open
```

`fourmeme.agent-tokens` scans OpenFour Core `TokenCreated` logs and filters by template.
`--before-block` continues backwards from an older block; `next_before_block` in the response is the
cursor for the next page.

`fourmeme.token` returns image, description, phase, template, market/momentum (price, max price,
market cap, liquidity, 24h volume/change), social links, holder count, tradability (inner transfer
lock, creation-time maxSupply, 1% wallet cap), safety signals (raw buy/sell tax, min/max buy,
optional BscScan verification) and wallet eligibility. If the detail API is unavailable it falls
back to OpenFour Core `tokens`, vault phase and BSC ERC20 fields; only when Core is also unavailable
does it report `phase: unknown`. Missing fields are omitted with a warning, never reported as zero.

`fourmeme.ui` starts a local read-only page on `127.0.0.1:8787` by default. List cards link to
`#/token/<address>` detail pages, and "load earlier" continues paging. It never connects a wallet,
signs, buys or sells.

During the bonding phase only a Binance Agentic Wallet with an Agent ID can trade, each address
is capped at 1% of creation-time `maxSupply`, and inner-market transfers are locked. Trading opens
after graduation. `fourmeme.token` reports `wallet_cap` (balance, 1% cap, remaining, reached).
This capability only discovers and checks; it never buys, sells, or signs.

## Security model

`transaction.inspect` / `signature.inspect` are **pre-sign inspection** only: they do not hold
private keys, sign, or broadcast, and they are not an audit or a safety guarantee. Their output
helps the caller and the user decide whether to enter the wallet confirmation flow.

- **Risk semantics**: with findings, `risk` is the highest severity; without findings it is
  `unknown`, never `low` or `safe`; missing critical coverage keeps `unknown`.
- **Coverage**: five independent dimensions — `decode`, `metadata`, `simulation`, `trace`, and
  `policy`; `unknowns` is part of the result, not an appendix.
- **Enforcement**: `wallet.send --policy` and `wallet.sign --policy` enforce policy only on
  Eollo's own send and typed-data signing paths, returning `POLICY_BLOCKED` on high risk; this is
  not a general signer boundary.
- **Determinism**: the same request, block, and code version produce the same finding set; LLMs do
  not participate in detectors, risk, or coverage.

The rule catalog, trust boundaries, threat model, inspection pipeline, coverage semantics, and
red-team boundary are in
[`docs/capabilities/transaction/ASSURANCE.md`](docs/capabilities/transaction/ASSURANCE.md);
threat-intel configuration is in
[`docs/capabilities/transaction/THREAT-INTEL.md`](docs/capabilities/transaction/THREAT-INTEL.md);
method and raw data are in [red-team](docs/benchmarks/redteam-2026-11-11.md),
[historical calldata](docs/benchmarks/historical-calddata-2026-11-11.md), and
[security benchmark](docs/benchmarks/security-inspection-2026-11-11.md).

## Execution model

Every capability has a fixed execution location; the caller cannot choose remote/local. `eollo`
only routes.

| Capability | Location | Data path | Failure semantics |
|---|---|---|---|
| `swap.quote` | remote | remote snapshot -> local route computation | Remote unavailable -> `RPC_ERROR` |
| `swap.build` | local | quote JSON -> local ABI encoding | Non-V2/split/non-Ethereum -> `UNSUPPORTED` |
| `yield.discover` | remote | curated registry + DefiLlama + Ethereum RPC snapshot | Provider/RPC failure -> `RPC_ERROR`; stale data -> `STALE_STATE` |
| `yield.build` | local | curated registry -> ABI encoding -> steps/approvals | Unsupported market -> `UNSUPPORTED` |
| `yield.history` | remote | DefiLlama chart + in-process cache | Provider failure -> `RPC_ERROR`; stale data -> `STALE_STATE` |
| `yield.withdraw` | local | curated registry -> withdraw/redeem/burn ABI encoding | Unsupported market/amount -> `UNSUPPORTED`/`INVALID_ARGUMENT` |
| `position.analyze` | remote | Ethereum RPC pinned block + yield snapshot | RPC failure -> `RPC_ERROR` (`PositionUnavailable`); snapshot not ready -> `STALE_STATE` |
| `wallet.balance` / `assets` / `allowance` / `collectibles` | local | local RPC at a pinned block | RPC failure -> `RPC_ERROR` |
| `wallet.connect` / `status` / `disconnect` | local | local session + WalletConnect relay | No session, pairing failure, or expiry -> explicit error |
| `wallet.agent-id` | local | SPACE ID API + BSC RPC verification | API failure -> configured on-chain fallback or unknown |
| `wallet.sign` / `wallet.send` | local | local session -> relay -> phone confirmation | Rejection -> `USER_REJECTED`; timeout -> `TIMEOUT`; policy block -> `POLICY_BLOCKED` |
| `transaction.inspect` | local | local RPC: block, code, simulate, trace, source | Missing dimensions -> `coverage` / `unknowns` |
| `transaction.simulate` | local | local RPC: `eth_call` + `eth_estimateGas` | RPC/revert -> capability error or `success=false` |
| `transaction.send` | local | RPC simulate/inspect -> WalletConnect -> phone | Simulation failure, policy block, rejection or timeout never broadcast |
| `transaction.verify` | local | local RPC: tx + receipt + receipt logs | Missing/unconfirmed -> `pending`/`not_found`; RPC failure -> `RPC_ERROR` |
| `signature.inspect` | local | caller-provided typed data / authorization / userOp JSON | Undecodable -> `SIG-006` or invalid argument |
| `fourmeme.agent-tokens` | local | BSC `TokenCreated` logs + Four.meme API + BSC `AgentIdentifier` | API/RPC failure -> explicit warning/coverage |
| `fourmeme.token` | local | Four.meme API + BSC ERC20 fallback + `AgentIdentifier` | API unavailable -> on-chain basics + warning |

### Request flows

```text
Quote:
CLI/MCP -> eollo client -> EOLLO_REMOTE_URL -> snapshot -> quote response

Local read:
CLI/MCP -> eollo client -> chain RPC -> pinned block -> response

Wallet:
CLI/MCP -> local session file -> WalletConnect relay -> phone -> response

Pre-sign inspection:
CLI/MCP -> local RPC -> decode/metadata/simulate/verify/trace/source -> report
```

### Runtime and configuration

- `EOLLO_REMOTE_URL`: remote quote service. Without it, `swap.quote` returns an explicit error.
- `EOLLO_RPC_URL` / `RPC_URL`: Ethereum RPC.
- `EOLLO_BSC_RPC_URL`: BSC RPC; default chain for local capabilities.
- `FOURMEME_API_URL`: Four.meme REST API; defaults to `https://four.meme/meme-api/v1`.
- `FOURMEME_BSC_RPC_URL`: BSC RPC with `eth_getLogs`, also used for ERC20 fallback; defaults to `https://bsc-rpc.publicnode.com`.
- `WALLETCONNECT_PROJECT_ID`, `WALLETCONNECT_RELAY_URL`: wallet session.
- `BSCSCAN_API_KEY` / `ETHERSCAN_API_KEY`: optional source verification.
- `EOLLO_VERIFY_RPC_URL`: optional secondary RPC for block-hash and target-code cross-check.
- `EOLLO_THREAT_INTEL_FILE`: optional local threat-intel JSON; matches emit `ADDR-007`.
  `EOLLO_THREAT_INTEL_PUBKEY` enables Ed25519 signature verification. The registry is
  operator-provided and may be incomplete or stale.
- `EOLLO_BSCSCAN_API_URL` / `EOLLO_ETHERSCAN_API_URL`: replaceable verification endpoints.
- Private keys never enter the runtime; the WalletConnect session is stored only at
  `~/.eollo/walletconnect/session.json`.

### State and failure behavior

- Quotes read a published immutable snapshot; the snapshot is never written back on-chain.
- Local reads and simulations use one pinned block; cross-block consistency is not guaranteed by a
  single response.
- `effects`, `trace`, and `source` are optional enhancements in `transaction.inspect`. Missing data
  is reported explicitly and does not block decode or deterministic findings.
- All capabilities use stable error codes; callers should not infer error types from HTTP/CLI text.

## Documentation

- Integration: [MCP](docs/MCP.md)
- Capabilities: [swap.quote](docs/capabilities/swap/README.md) · [wallet](docs/capabilities/wallet/README.md) · [transaction](docs/capabilities/transaction/README.md) · [signature](docs/capabilities/signature/README.md)
- Architecture: [product architecture](docs/PRODUCT-ARCHITECTURE.md) · [engine architecture](docs/ARCHITECTURE.en.md) · [local state](docs/LOCAL-STATE.md)
- Operations: [HTTP API](docs/API.md) · [configuration](docs/CONFIGURATION.md) · [development](docs/DEVELOPMENT.md)
- Security: [rule catalog](docs/capabilities/transaction/ASSURANCE.md) · [red-team](docs/benchmarks/redteam-2026-11-11.md)
- Contracts: [capability catalog](capabilities/catalog.yaml) · [catalog schema](schemas/capability_catalog.v2.json) · [capability docs index](docs/capabilities/README.md)
- Research: [benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- Skills: [swap](skills/swap/SKILL.md) · [yield](skills/yield/SKILL.md) · [position](skills/position/SKILL.md) · [wallet](skills/wallet/SKILL.md) · [transaction](skills/transaction/SKILL.md) · [signature](skills/signature/SKILL.md) · [fourmeme](skills/fourmeme/SKILL.md)

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

Next: shared approval handling and workflow stabilization.

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt

# Foundry security red-team
python3 bench/security/redteam.py
```
