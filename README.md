# Eollo

Eollo is a Web3 capability layer for AI agents. It exposes quoting, account reads, wallet linking,
transaction inspection, and signature authorization as MCP capabilities. An agent can read a chain,
request a quote, prepare a transaction, and produce a risk report before the user signs.

It addresses these problems:

- Each protocol has its own RPCs, DEX interfaces, wallet flow, and data model. Eollo exposes fixed
  inputs and outputs, so agents do not rewrite integration code per protocol.
- An agent action also needs review. Eollo decodes calldata, simulates state changes, inspects
  internal calls and source, and reports what it did not cover.
- The agent prepares the transaction, but the user keeps the keys and confirms every send in the
  phone wallet.

English · [中文](README.zh.md)

Website: <https://eollo.xyz/> · Chinese: <https://eollo.xyz/zh.html> · Source: [`site/`](site/)

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
EOLLO_BSC_REMOTE_URL=http://127.0.0.1:8081 eollo swap bnb-usdt 1 --chain bsc
```

`configs/bsc.yaml` is the BSC swap and yield profile. Run it as a second remote instance on port
8081. Local wallet, transaction inspection, and simulation default to BSC; use `--chain ethereum`
for Ethereum.

## Quick start

### MCP

```bash
eollo mcp
```

`eollo mcp` exposes capabilities over stdio and does not start the remote service. Client
registration and full setup live in [docs/MCP.md](docs/MCP.md).

### Skills

Skills tell the agent when to call a capability and how to read the result. Install them with the
[`skills` CLI](https://github.com/vercel-labs/skills). The CLI requires Node.js 22.20 or later:

```bash
npx skills add ZhangPeibin/eollo -g -a opencode
```

Use `--list` to preview the seven skills, omit `-g` to install into the current project, or install
a subset with `--skill swap --skill wallet`. GitHub CLI users can run
`gh skill install ZhangPeibin/eollo swap --agent opencode --scope user` instead.

If the repository is already checked out, add its absolute `skills/` path to your client config:

```json
{
  "skills": ["/absolute/path/to/eollo/skills"]
}
```

See [`skills/README.md`](skills/README.md) for the skill list and release pinning. Then ask the
agent:

```text
Quote 100 LDO to ETH and show the best route
Quote 1 BNB to USDT on BSC and show the best route
Show the BNB and USDT balances for 0x...
Show this address's Venus position on BSC
Inspect this calldata before I sign
Inspect this Permit2 typed data
Verify this BSC transaction
```

See [agent examples](docs/AGENT-EXAMPLES.md) for the full bilingual list.

## Capabilities

Every capability has a fixed execution location. Remote or local is internal Eollo routing; agents
and users do not choose it.

Capability metadata lives in [`capabilities/catalog.yaml`](capabilities/catalog.yaml), and
[`docs/capabilities/README.md`](docs/capabilities/README.md) is generated from it. Every capability
returns capability envelope v2 with `meta`, `chain`, `block`, `as_of`, `provenance`, `coverage`,
`warnings`, `unknowns`, and `data`. See
[ADR 0003](docs/adr/0003-capability-envelope-v2.md) and
[ADR 0005](docs/adr/0005-capability-catalog.md).

### Capability overview

#### 1. Quotes

| Capability | Status | CLI | Description |
|---|---|---|---|
| `swap.quote` | stable | `eollo swap ldo-eth 100` | Ethereum and BSC exact-in quotes, multi-hop, split allocation |
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
| `yield.discover` | experimental | `eollo yield discover --asset ETH --amount 10` | ETH or BNB staking/lending: safety tier, APY, TVL, withdrawal conditions, recommendation |
| `yield.build` | experimental | `eollo yield build --opportunity lido-steth --amount 10 --recipient 0x...` | Local stake/supply/deposit calldata and approval steps |
| `yield.history` | experimental | `eollo yield history --opportunity lido-steth --days 30` | Historical APY, base/reward split, TVL and stability summary |
| `yield.withdraw` | experimental | `eollo yield withdraw --opportunity aave-v3-weth --amount 10 --recipient 0x...` | Local withdraw/redeem/burn exit calldata |

#### 7. Position analysis

| Capability | Status | CLI | Description |
|---|---|---|---|
| `position.analyze` | experimental | `eollo position analyze --owner 0x...` | Cross-market ETH positions, valuation, APY, withdrawal conditions, risk and optional PnL |

Local UI tools such as `fourmeme.ui` (`eollo fourmeme ui --open`) are read-only and are not
capabilities. They are listed in the catalog's `surfaces` list.

### Quotes

#### `swap.quote`

`swap.quote` returns exact-in quotes on Ethereum and BSC with multi-hop routing and split
allocation. It returns `amountOut`, price impact, path, and per-hop allocation. It does not build
calldata or execute a swap. BSC quotes use PancakeSwap V2 and V3.

```bash
eollo swap ldo-eth 100
eollo swap quote --in LDO --out WETH --amount 100 --json
eollo swap bnb-usdt 1 --chain bsc --json
```

Ask the agent:

```text
Quote 100 LDO to ETH and show the best route.
```

Skill: [swap](skills/swap/SKILL.md) · Docs: [swap.quote](docs/capabilities/swap/README.md)

### Yield discovery

#### `yield.discover`

`yield.discover` reads native-asset staking and lending opportunities. Use ETH on Ethereum and BNB
on BSC with `--chain bsc`. It returns a
curated safety tier, APY components, TVL, withdrawal conditions, risk factors, and evidence, plus
one recommendation with tradeoffs. It never builds, signs, or executes a transaction.

```bash
eollo yield discover --asset ETH --amount 10 --json
eollo yield discover --asset ETH --amount 10 --risk balanced --json
eollo yield discover --asset BNB --chain bsc --amount 10 --risk aggressive --json
eollo yield build --opportunity lido-steth --amount 10 --recipient 0x... --json
eollo yield history --opportunity lido-steth --days 30 --json
eollo yield withdraw --opportunity aave-v3-weth --amount 10 --recipient 0x... --json
```

`yield.build` and `yield.withdraw` only return calldata and approval steps; they never sign or
broadcast. `yield.history` returns historical APY, which is not current APY and not a guarantee.

Ask the agent:

```text
I have 10 ETH. Find the safest opportunity with a reasonable APY, give me the conservative
recommendation, and explain what I am giving up.
```

Skill: [yield](skills/yield/SKILL.md) · Docs: [yield](docs/capabilities/yield/README.md)

### Position analysis

#### `position.analyze`

`position.analyze` reads a wallet's positions across curated Lido, Rocket Pool, Aave, SparkLend,
and Morpho markets. It pins balances to one Ethereum block and takes APY and risk context from the
`yield.discover` snapshot. Pass `--cost-basis` to compute PnL.

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

`wallet.balance` reads native and token balances at a pinned block. BSC is the default; Ethereum is
explicit.

```bash
eollo wallet balance 0x... --tokens BNB,USDT
eollo wallet balance 0x... --tokens ETH,USDC --chain ethereum
```

Ask the agent:

```text
Show the BNB and USDT balances for 0x... at the latest block.
```

#### `wallet.assets`

`wallet.assets` discovers token holdings and verifies each balance locally. Without a provider it
uses the built-in registry and reports what it could not check in `coverage` and `warnings`.
`--include-unknown` resolves at most 50 unknown tokens per request; the rest stay counted in
`unknown_count`.

```bash
eollo wallet assets 0x...
eollo wallet assets 0x... --include-unknown
```

Ask the agent:

```text
Show which tokens this address holds and flag unknown tokens.
```

#### `wallet.allowance`

`wallet.allowance` reads ERC20 and BEP-20 allowances. It returns zero allowances, so approval risk
is visible.

```bash
eollo wallet allowance 0xowner --spender 0xspender --tokens USDT,CAKE
```

Ask the agent:

```text
Check the remaining USDT allowance from owner to spender.
```

#### `wallet.collectibles`

`wallet.collectibles` reads ERC721 and ERC1155 holdings. Without an NFT indexer it checks explicit
collections or the built-in registry only.

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

`wallet.connect` starts a WalletConnect v2 session and returns a URI and a terminal QR. It defaults
to BSC; pass `--chain ethereum` for Ethereum. Private keys stay in the phone wallet.

```bash
eollo wallet connect
eollo wallet connect --chain ethereum
```

Ask the agent:

```text
Link my wallet on BSC; show the QR first, then poll the session status.
```

#### `wallet.status`

`wallet.status` reads the local WalletConnect session without contacting the wallet.

```bash
eollo wallet status
eollo wallet status --wait 90
```

Ask the agent:

```text
Is my wallet connected? If not, keep waiting for up to 90 seconds.
```

#### `wallet.disconnect`

`wallet.disconnect` removes the local session and aborts any pending pairing.

```bash
eollo wallet disconnect
```

Ask the agent:

```text
Disconnect the current wallet session.
```

#### `wallet.agent-id`

`wallet.agent-id` resolves a SPACE ID `.ai` Agent ID on BSC, with optional on-chain registry
verification. It does not log in to a Binance Agentic Wallet.

```bash
eollo wallet agent-id alice.ai
eollo wallet agent-id 0xA506... --json
```

Ask the agent:

```text
Resolve alice.ai to its BSC address and Agent ID.
```

#### `wallet.sign`

`wallet.sign` signs a message or EIP-712 typed data through WalletConnect. The user confirms in the
phone wallet; a rejection returns `USER_REJECTED`.

```bash
eollo wallet sign --message "hello"
eollo wallet sign --typed-data typed-data.json
```

Ask the agent:

```text
Sign this message with my connected wallet: hello.
```

#### `wallet.send`

`wallet.send` builds a native or ERC20/BEP-20 transfer, simulates it locally, and requests
confirmation in the phone wallet. A failed simulation is not sent for confirmation.

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

`transaction.inspect` performs deterministic pre-sign inspection. It returns intent, asset and
allowance changes, findings, coverage, and unknowns. It is advisory, not a safety guarantee, and
never signs or broadcasts.

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

`transaction.simulate` runs `eth_call` and `eth_estimateGas` at a pinned block. It returns success
or revert status, return data, and gas.

```bash
eollo transaction simulate --to 0x... --data 0x...
eollo transaction simulate --to 0x... --data 0x... --from 0x...
```

Ask the agent:

```text
Simulate this transaction, tell me if it reverts, and estimate gas.
```

#### `transaction.verify`

`transaction.verify` reads a sent transaction's receipt. It returns status, confirmations, gas and
fee, and effects decoded from logs. Pass `--expect` to compare the effects with a
`transaction.inspect` report; the result is `match` or `mismatch`. It is read-only and never signs
or broadcasts.

```bash
eollo transaction verify --hash 0x... --json
eollo transaction verify --hash 0x... --chain ethereum --confirmations 3 --json
eollo transaction verify --hash 0x... --expect inspect.json --json
```

Status is `success`, `reverted`, `pending`, or `not_found`; `pending` and `not_found` are normal
results, not errors. Without a receipt, effects are marked `not_run`.

Ask the agent:

```text
Did transaction 0x... succeed? What did it move? If it differs from the report I
inspected before signing, show me the differences.
```

### Signature authorization

Skill: [signature](skills/signature/SKILL.md) · Docs: [signature](docs/capabilities/signature/README.md)

#### `signature.inspect`

`signature.inspect` inspects authorization that is not ordinary calldata: EIP-712 typed data,
Permit2, EIP-7702 authorization lists, and ERC-4337 user operations.

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

`fourmeme.agent-tokens` discovers Four.meme tokens using the `Binance Agent ID Exclusive` template
(on-chain preset `1778027615733`, API template `1778027615735`) and optionally checks Agent ID
eligibility. An `aiCreator` label or `.ai`
tag is not sufficient.

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

`fourmeme.token` returns:

- Token fields: image, description, phase, template, and market data (price, max price, market cap,
  liquidity, 24-hour volume, and change).
- Social links and holder count.
- Tradability: inner-market transfer lock, creation-time `maxSupply`, and the 1% wallet cap.
- Safety signals: raw buy and sell tax, minimum and maximum buy, and optional BscScan verification.
- Wallet eligibility.

If the detail API is unavailable, the capability falls back to OpenFour Core `tokens`, vault phase,
and BSC ERC20 fields. It reports `phase: unknown` only when Core is also unavailable. Missing fields
are omitted with a warning; the response never reports them as zero.

`fourmeme.ui` starts a local read-only page on `127.0.0.1:8787` by default. List cards link to
`#/token/<address>` detail pages, and "load earlier" continues paging. It never connects a wallet,
signs, buys or sells.

During the bonding phase, only a Binance Agentic Wallet with an Agent ID can trade. Four.meme caps
each address at 1% of creation-time `maxSupply`, and inner-market transfers stay locked. Trading
opens after graduation. `fourmeme.token` reports `wallet_cap` with the balance, the 1% cap, the
remaining amount, and whether the cap is reached. This capability only discovers and checks; it
never buys, sells, or signs.

## Security model

`transaction.inspect` and `signature.inspect` perform pre-sign inspection only. They do not hold
private keys, sign, or broadcast, and they are not an audit or a safety guarantee. Their output
helps the agent and the user decide whether to confirm the transaction in the wallet.

- `risk` is the highest severity among the findings. With no findings, `risk` is `unknown`, never
  `low` or `safe`. Missing critical coverage also keeps `risk` at `unknown`.
- `coverage` has five dimensions: `decode`, `metadata`, `simulation`, `trace`, and `policy`.
  `unknowns` is always part of the result.
- `wallet.send --policy` and `wallet.sign --policy` enforce policy on Eollo's own send and
  typed-data signing paths. A high-risk result returns `POLICY_BLOCKED`. This is not a general
  signer boundary.
- The same request, block, and code version produce the same finding set. LLMs do not participate
  in detectors, risk, or coverage.

The rule catalog, trust boundaries, threat model, inspection pipeline, coverage semantics, and
red-team boundary are in
[`docs/capabilities/transaction/ASSURANCE.md`](docs/capabilities/transaction/ASSURANCE.md);
threat-intel configuration is in
[`docs/capabilities/transaction/THREAT-INTEL.md`](docs/capabilities/transaction/THREAT-INTEL.md);
method and raw data are in [red-team](docs/benchmarks/redteam-2026-11-11.md),
[historical calldata](docs/benchmarks/historical-calddata-2026-11-11.md), and
[security benchmark](docs/benchmarks/security-inspection-2026-11-11.md).

## Execution model

The table below lists each capability's data path and failure behavior.

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

- `EOLLO_REMOTE_URL`: remote service for Ethereum quotes, yield, and positions.
- `EOLLO_BSC_REMOTE_URL`: remote service for BSC quotes, yield, and positions. Falls back to
  `EOLLO_REMOTE_URL` for single-instance development.
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

- `swap.quote` reads a published immutable snapshot and never writes it back on-chain.
- Local reads and simulations use one pinned block. A single response does not guarantee cross-block
  consistency.
- `transaction.inspect` treats `effects`, `trace`, and `source` as optional. It reports missing data
  and still returns decode results and deterministic findings.
- All capabilities use stable error codes. Do not infer error types from HTTP or CLI text.

## Documentation

- Integration: [MCP](docs/MCP.md)
- Examples: [agent examples](docs/AGENT-EXAMPLES.md)
- Capabilities: [swap.quote](docs/capabilities/swap/README.md) · [wallet](docs/capabilities/wallet/README.md) · [transaction](docs/capabilities/transaction/README.md) · [signature](docs/capabilities/signature/README.md)
- Architecture: [product architecture](docs/PRODUCT-ARCHITECTURE.md) · [engine architecture](docs/ARCHITECTURE.en.md) · [local state](docs/LOCAL-STATE.md)
- Operations: [HTTP API](docs/API.md) · [configuration](docs/CONFIGURATION.md) · [development](docs/DEVELOPMENT.md)
- Security: [security boundaries](docs/SECURITY.md) · [rule catalog](docs/capabilities/transaction/ASSURANCE.md) · [red-team](docs/benchmarks/redteam-2026-11-11.md)
- Contracts: [capability catalog](capabilities/catalog.yaml) · [catalog schema](schemas/capability_catalog.v2.json) · [capability docs index](docs/capabilities/README.md)
- Research: [benchmarks](docs/benchmarks/README.md) · [multi-pool](docs/MULTI-POOL.md) · [split routing](docs/SPLIT-ROUTING.md) · [quote quality](docs/QUOTE-QUALITY.md)
- Skills: [swap](skills/swap/SKILL.md) · [yield](skills/yield/SKILL.md) · [position](skills/position/SKILL.md) · [wallet](skills/wallet/SKILL.md) · [transaction](skills/transaction/SKILL.md) · [signature](skills/signature/SKILL.md) · [fourmeme](skills/fourmeme/SKILL.md)

## Status and boundaries

Eollo is pre-1.0. Wallet, transaction, signature, swap, yield, and position capabilities support
BSC and Ethereum. `swap.quote`, `yield.*`, and `position.analyze` need the matching remote instance:
`configs/runtime.yaml` for Ethereum and `configs/bsc.yaml` for BSC.

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
- `swap.quote`, `yield.*`, and `position.analyze` need the remote instance for their chain.

Next: add a shared approval helper and close the workflow gaps listed in the catalog.

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt

# Foundry security red-team
python3 bench/security/redteam.py
```

## License

MIT. See [LICENSE](LICENSE).
