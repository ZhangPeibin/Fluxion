# Multi-hop Routing RFQ - Requirements Discussion

**Status:** ready-for-prd
**Release:** v1
**Feature:** multi-hop-routing-rfq

## Problem

Liquidity for a token pair on Ethereum is fragmented across many DEX platforms (Uniswap V2,
Uniswap V3, Curve) and many pools. A single direct-pool lookup often misses the best
executable price, and naive quoting ignores multi-hop routes and underestimates slippage on
shallow pools. There is no single place to ask "for this input amount, what is the best
achievable output and the route to get it?"

## Users and Outcomes

- Primary users: developers in the open-source community who **self-host** Fluxion, each
  running their own instance with their own configuration (RPC endpoint, subgraph access,
  thresholds, enabled platforms).
- Desired outcome: given `tokenIn`, `tokenOut`, `amountIn`, obtain the best achievable
  `amountOut` and the route that produces it, quickly and reliably, from a service they run
  themselves.

## Current Experience

Callers must manually query individual pools/platforms, compare results themselves, and
guess at multi-hop paths and slippage. This is slow, error-prone, and misses better routes
that span several pools.

## Scope

### In Scope

- Ethereum mainnet only.
- HTTP REST service.
- Platforms: Uniswap V2, Uniswap V3, Curve.
- Multi-hop routing; single best path.
- Exact-in amount semantics.
- Two-layer pricing: cached mid-price for path discovery + live exact quote for top-K.
- Pool filtering (allowlist, liquidity/volume/age thresholds, dynamic reserve check).
- Defined error handling; safe defaults and an example config so others can self-host.

### Out of Scope

- Multi-chain support.
- Split routing (multi-path).
- Native ETH (WETH-only); wrap/unwrap by caller.
- Exact-out quoting.
- Oracle / Chainlink deviation checks.
- Managed/hosted multi-tenant service and built-in authentication.
- Self-hosted indexer.
- fee-on-transfer / rebasing tokens (excluded via allowlist).

## Required Behaviour

- `POST /quote` with `{tokenIn, tokenOut, amountIn, maxHops?, amountOutMin?}` returns
  `{amountOut, priceImpactBps, path[]}` where `path` lists each hop with platform, pool,
  per-hop amounts.
- `GET /health` for liveness.
- Runs via `cargo run` (or a built binary); the user supplies a **YAML config file** that
  carries all knobs (RPC, subgraph access, thresholds, enabled platforms, `max_hops`,
  top-K, `λ`). Ship a `config.example.yaml`.
- Exact-in: `amountOut` reflects real pool state including slippage.
- Default `maxHops` = 3 (counts edges).
- `priceImpactBps` reported in basis points.

## Edge and Failure Cases

- `tokenIn == tokenOut` → returns input amount (or a defined no-op response; to confirm).
- `amountIn == 0` → treated as invalid request.
- No route → `NoPathFound` error with appropriate HTTP status.
- Output below `amountOutMin` → `AmountOutBelowMin`.
- Shallow pool where `reserve_in < k * amount_in` → pool excluded from candidates.
- RPC failure → `RpcError`.

## Constraints and Dependencies

- Rust (edition 2024), tokio, alloy, axum, serde, thiserror, tracing.
- Configuration via a **YAML file** (single-file, many thresholds); run with `cargo run`
  or a built binary. No Docker required in v1.
- Third-party RPC endpoint and Graph subgraph access are supplied in the config.
- Official Graph subgraphs for Uniswap V2/V3 (paginated; may need API key).
- Curve `AddressProvider` → registry.
- Dependencies added incrementally per phase (alloy at P3, axum at P6).
- This is also a learning project; development is currently prioritized over teaching.

## Decisions

- **Info-only** (no execution): returns quote + route only. Swap calldata encoding, token
  `approve`, deadlines, and route-contract selection are out of scope for v1; the caller
  builds and executes transactions. Rationale: keeps the security boundary and scope small
  while still delivering standalone value.
- **Open-source, self-hosted**: distributed as source; each user runs their own instance
  with custom configuration. No built-in authentication or managed multi-tenant hosting;
  safe defaults + an example config are required so others can run it. Callers are the
  operators themselves, so a hosted-API SLA is out of scope.
- **YAML config, `cargo run`**: all settings live in one YAML file (shipped with
  `config.example.yaml`); users run via `cargo run` or a built binary. YAML chosen over
  env vars because the configuration has many thresholds and structured settings.
- **Latency**: soft interactive target — design for low latency via caching, hot-pool
  refresh, and refining only the top-K paths; **no hard SLA** in v1. Measure and tune with
  real data rather than inventing thresholds up front.
- **`tokenIn == tokenOut`**: default is to echo `amountIn` as `amountOut` with an empty
  `path` (easy to change; revisit only if a caller needs stricter validation).
- **Single best path** (no split routing in v1): split routing materially more complex;
  deferred to v2.
- **Exact-in only**: matches typical aggregator default and "询价" intent.
- **Stale/no-trade pools degraded, not dropped**: reserves remain valid; real risk is
  shallow depth, handled by liquidity thresholds + dynamic reserve check.
- **Liquidity thresholds**: static lower bound plus dynamic `reserve_in >= k * amount_in`.
- **Third-party RPC + official subgraph**, no self-hosted indexer.
- **Multi-coin pools**: `Pool.coins: Vec<Address>`, expand to all ordered pairs (i≠j) at
  graph build; Curve 3-pool → 6 directed edges.
- **Token / pool universe**: auto-discover pools from the subgraph, filtered by configured
  thresholds; an **optional token allowlist** in YAML narrows the universe. Rationale:
  works out of the box without hand-listing thousands of pools, while keeping user control.
- **Refresh**: hot-pool set + cold-pool lazy load; no full-graph refresh.
- **Routing**: K-best DP bounded by `max_hops` with simple-path constraint; negative
  `-ln(rate)` edges rule out Dijkstra; edge weight includes liquidity penalty
  `λ * (amount_in / reserve_in)`.
- **Adapters**: V2 constant product; V3 via QuoterV2; Curve via `get_dy`/`get_dy_underlying`
  by pool type.

## Open Questions

- **RPC + Graph credentials**: provisioning of a third-party `RPC_URL` and Graph endpoint/API
  key (now delivered via the YAML config). Blocks real adapters and discovery (M2); does not
  block offline engine (M1).
- **Tunables** (`top-K`, `λ`, thresholds): defaults chosen; refine with evidence. Carried to
  technical design.

## Evidence

- Prior conversation produced a technical design (`README.md`) and teaching milestones
  (`docs/PLAN.md`); P0 Rust warm-up examples exist under `examples/`.
- A formal spec was published prematurely as GitHub issue #1
  (https://github.com/ZhangPeibin/Fluxion/issues/1); to be reconciled with this discussion
  before or during the next `to-spec`.

## Session Log

- 2026-10-27: Backfilled this discussion from the ongoing design conversation after it was
  noted that `requirements-analysis` was skipped before `to-spec`. Captured agreed scope,
  constraints, and decisions; opened execution-intent, actor/access, credentials, and
  tunables as open questions. No implementation performed.
- 2026-10-27: Resolved execution intent → **info-only** for v1 (no calldata/approve/
  deadline). Set a default for `tokenIn == tokenOut` (echo input, empty path). Next:
  primary actor / access model.
- 2026-10-27: Resolved actor/access → **open-source, self-hosted**; each user runs their own
  instance with custom config; no built-in auth or hosted multi-tenant SLA; safe defaults +
  example config required. Opened run/configuration-surface question. Next: how users run
  and configure it.
- 2026-10-27: Resolved run/config → **YAML config file** (shipped `config.example.yaml`)
  and `cargo run` / built binary; no Docker in v1. Opened token/pool-universe and latency
  questions. Next: how the token/pool universe is defined.
- 2026-10-27: Resolved token/pool universe → **auto-discover from subgraph + configurable
  thresholds, with optional token allowlist**. Resolved latency → **soft interactive
  target, no hard SLA**. All material product branches now resolved; ready to gate PRD
  handoff.
- 2026-10-27: User explicitly confirmed the analysis is ready for PRD. Status set to
  `ready-for-prd`. Next: invoke `$to-spec` in this conversation to publish the formal PRD
  (reconciling/rewriting the premature issue #1).
