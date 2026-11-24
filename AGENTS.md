## Product and documentation layering

Eollo is the project. A capability is one action-level contract inside it. Execution location
is fixed per capability and is an internal Eollo decision; agents and users do not choose an
execution location. `swap.quote` uses the remote service; `wallet` / `execute` use the local runtime.

| Layer | Location | Owns |
|---|---|---|
| Project | `README.md`, `docs/WHITEPAPER.md` | positioning, install, capability list |
| Transport | `docs/MCP.md`, `docs/API.md` | how agents/integrators connect |
| Capability | `docs/capabilities/<name>/` | tool schema, inputs, outputs, errors, benchmarks |
| Execution | `docs/CONFIGURATION.md`, `docs/LOCAL-STATE.md` | fixed execution routing, keys, freshness |
| Engine | `docs/ARCHITECTURE.md`, `docs/MULTI-POOL.md`, ... | internals |
| Agent usage | `skills/<name>/SKILL.md` | when/how to call a capability |
| Contract | `schemas/` | machine-readable JSON schema |
| Catalog | `capabilities/catalog.yaml` | capability metadata, chains, status, side effects, workflows |
| Benchmarks | `docs/benchmarks/` | methodology and full results |

Rules:
- MCP/CLI client registration belongs in `docs/MCP.md`, never in a capability doc.
- Capability docs link to transport docs; do not duplicate client snippets.
- Root README is a landing page: no benchmark tables or capability internals.
- Execution location is internal; never expose it as an agent choice.
- Capability metadata changes start in `capabilities/catalog.yaml`; MCP, CLI, docs, schemas, and
  skills must match it.
- Every capability declares chains, status, side effects, and coverage dimensions. Unsupported
  chains return `UNSUPPORTED`; do not add silent fallbacks.
- Capability responses use the capability envelope; workflows are catalog entries, not hidden
  execution paths.
- When adding or moving docs, update the relevant index and links.

## Agent skills

### Issue tracker

Issues live in this repo's GitHub Issues via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context: root `CONTEXT.md` + `docs/adr/`. See `docs/agents/domain.md`.

### Feature docs

Pre-PRD discussions at `docs/feature/<release>/<feature-slug>/discussion.md`. See `docs/agents/feature-docs.md`.

### Worktrees

Implementation tasks use isolated worktrees under `.worktrees/<task-slug>`. See `docs/agents/worktrees.md`.

## Repository engineering

Rust project (edition 2024). Async runtime: tokio (`#[tokio::main]`); the app owns the runtime.

Commands (from repo root):
- Build: `cargo build --workspace`
- Remote server: `cargo run -p eollo-remote -- serve --config configs/runtime.yaml`
  (needs `GRAPH_API_KEY` for real discovery)
- Client CLI: `cargo run -p eollo-client --bin eollo -- swap ldo-eth 100`
- MCP bridge: `cargo run -p eollo-client --bin eollo -- mcp`
- Unit tests: `cargo test --workspace`
- Mainnet integration tests (network, read-only): `cargo test --workspace -- --ignored`
- Lint: `cargo clippy --workspace --all-targets`
- Format: `cargo fmt`
- Run an example: `cargo run -p eollo --example <name>` (e.g. `quote_mainnet`)

Architecture and docs (source of truth, link rather than duplicate):
- `README.md` — landing page: positioning, capabilities, install, links.
- `CONTEXT.md` — ubiquitous language for the capability layer.
- `docs/adr/` — accepted architectural decisions.
- `capabilities/catalog.yaml` — machine-readable capability catalog.
- `docs/feature/v3/capability-runtime/` — catalog, envelope, chain matrix, registry, workflow,
  migration, and lifecycle specs.
- `docs/WHITEPAPER.md` — product vision and capability/execution boundaries.
- `docs/MCP.md` — MCP transport, client registration, lifecycle and troubleshooting.
- `docs/API.md` — HTTP API; `docs/CONFIGURATION.md` — config and execution routing.
- `docs/capabilities/` — capability contracts; `skills/` and `schemas/` — agent usage and JSON schemas.
- `docs/ARCHITECTURE.md`, `docs/LOCAL-STATE.md`, `docs/MULTI-POOL.md`,
  `docs/SPLIT-ROUTING.md`, `docs/QUOTE-QUALITY.md` — engine internals.
- `docs/benchmarks/` — methodology and results.

Workspace crates:
- `crates/protocol` — transport-independent capability contract and `swap.quote` DTOs.
- `crates/remote` — discovery, snapshots, routing, HTTP API, `eollo-remote` server and
  benchmark/graph tools.
- `crates/client` — CLI, MCP bridge, remote client, token registry, `eollo` binary.
- Root `src/lib.rs` is a test/compatibility facade; production binaries live in the crates.

Other surfaces:
- `site/` — public product landing page (React + Vite, GitHub Pages). Capability cards are generated
  from `capabilities/catalog.yaml` via
  `cargo run -p eollo-protocol --bin catalog-json > site/src/data/capabilities.json`; a catalog
  test fails when the checked-in JSON is stale.

Project-specific constraints:
- Chain interaction uses **raw JSON-RPC via `reqwest`** with hand-written ABI encode/decode;
  only `alloy-primitives` (U256/Address) is used from alloy. Do not reintroduce the full `alloy` stack.
- Config is YAML (`config.yaml`, see `config.example.yaml`; product profiles in `configs/`, benchmark profiles in `bench/configs/`, local test fixtures in `tests/configs/`). Secrets (`GRAPH_API_KEY`) live in a
  **gitignored `.env`** — never commit them or put them in YAML/README/docs.
- Data sources: The Graph decentralized-network subgraph IDs for V2/V3 + Curve main stableswap
  registry (`AddressProvider.get_registry()`). Crypto/factory/metapool Curve registries are not yet covered.
- A default major-token allowlist filters the graph; keep both-token (pool) filtering to avoid
  junk/thin pools.
- This is also a learning project (readable code, explanations), but development is currently
  prioritized over teaching.

