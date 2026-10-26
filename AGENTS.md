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
- Run: `cargo run`
- Test: `cargo test`
- Lint: `cargo clippy`
- Format: `cargo fmt`
- Run a teaching example: `cargo run --example <name>` (see `examples/`)

Architecture and plan (source of truth, link rather than duplicate):
- `README.md` — full v1 technical design: module layout, data model, routing, API.
- `docs/PLAN.md` — learning-oriented milestones (P0–P7) with checkpoints.
- `docs/0X-*.md` — per-stage learning notes.

Project-specific constraints:
- This is also a learning project. Prefer explaining concepts and keeping code readable; each stage has a checkpoint in `docs/PLAN.md`. The learner must understand code before moving on.
- Dependencies are added incrementally per phase (alloy at P3, axum at P6); do not pre-add future-phase dependencies.
- Intended modules (`model`, `graph`, `discovery`, `source`, `state`, `router`, `api`) are described in `README.md`; only `src/main.rs` exists today — do not assume the others exist.
