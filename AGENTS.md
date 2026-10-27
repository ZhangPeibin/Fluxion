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
- Build: `cargo build`
- Run server: `cargo run` (needs `.env` with `GRAPH_API_KEY` for real data; falls back to mock otherwise)
- Unit tests: `cargo test`
- Mainnet integration tests (network, read-only): `cargo test -- --ignored`
- Lint: `cargo clippy --all-targets`
- Format: `cargo fmt`
- Run an example: `cargo run --example <name>` (e.g. `quote_mainnet`)

Architecture and docs (source of truth, link rather than duplicate):
- `README.md` — what it does, API, architecture, config, known limitations.
- `docs/PLAN.md` — learning-oriented milestones; `docs/0X-*.md` — learning notes.

Modules (`src/`): `model`, `graph`, `discovery`, `source`, `adapters`, `state`, `rpc`,
`router`, `api`, `config`, `util`, `main`.

Project-specific constraints:
- Chain interaction uses **raw JSON-RPC via `reqwest`** with hand-written ABI encode/decode;
  only `alloy-primitives` (U256/Address) is used from alloy. Do not reintroduce the full `alloy` stack.
- Config is YAML (`config.yaml`, see `config.example.yaml`). Secrets (`GRAPH_API_KEY`) live in a
  **gitignored `.env`** — never commit them or put them in YAML/README/docs.
- Data sources: The Graph decentralized-network subgraph IDs for V2/V3 + Curve main stableswap
  registry (`AddressProvider.get_registry()`). Crypto/factory/metapool Curve registries are not yet covered.
- A default major-token allowlist filters the graph; keep both-token (pool) filtering to avoid
  junk/thin pools.
- This is also a learning project (readable code, explanations), but development is currently
  prioritized over teaching.

