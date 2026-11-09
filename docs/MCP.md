# MCP

Fluxion exposes its capabilities to agents over **MCP stdio**. MCP is the project-level
transport: it is not a capability, and client registration is not capability-specific.

Capability contracts live in [docs/capabilities/](capabilities/README.md). This page covers
installation, registration, the remote service, lifecycle and troubleshooting.

## Install

```bash
cargo install --path crates/client
```

`fluxion` must be on `PATH`. If it is not, use an absolute path in the client config instead.

## Run

```bash
fluxion mcp
```

`fluxion mcp` starts a stdio MCP server. It exposes the installed capabilities and routes each
call internally; the agent does not choose local or remote execution.

## Remote service

`swap.quote` always calls the remote Fluxion service. Set `FLUXION_REMOTE_URL` to its endpoint;
development and tests use the same variable:

```bash
FLUXION_REMOTE_URL=https://api.example.com FLUXION_API_KEY=... fluxion mcp
```

`FLUXION_REMOTE_URL` is an endpoint override, not a local/remote mode.
`GRAPH_API_KEY` is not required for normal users.

### Installation token

With `FLUXION_REQUIRE_AUTH=1`, `/quote` requires a bearer token. The local client handles this
automatically:

1. first `/quote` returns `401`
2. client calls `POST /register` with a local `install_id`
3. token is stored in `~/.fluxion/auth.json` (0600)
4. the quote is retried

The remote keeps tokens in memory in this minimal version. If the remote restarts, the client
re-registers on the next `401`.

The remote service is started separately. `fluxion mcp` does not start or wait for it; MCP
initialize and `tools/list` always succeed. If the remote endpoint is unavailable, `swap_quote`
returns a structured retryable error.

Build/install and start the remote server separately:

```bash
cargo install --path crates/remote
FLUXION_REQUIRE_AUTH=1 fluxion-remote serve --config configs/runtime.yaml
```

For local development without installing, use
`cargo run -p fluxion-remote -- serve --config configs/runtime.yaml`.

## Client registration

Claude Code (`.mcp.json`):

```json
{
  "mcpServers": {
    "fluxion": {
      "command": "fluxion",
      "args": ["mcp"]
    }
  }
}
```

Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.fluxion]
command = "fluxion"
args = ["mcp"]
```

OpenCode (`opencode.json` or `~/.config/opencode/opencode.json`):

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "fluxion": {
      "type": "local",
      "command": ["fluxion", "mcp"],
      "enabled": true
    }
  }
}
```

Set `FLUXION_REMOTE_URL` / `FLUXION_API_KEY` in the environment that launches the agent when you
need a non-default remote endpoint. For wallet linking, set `WALLETCONNECT_PROJECT_ID` in the same
environment. Exact config keys vary by client version; check the client's
MCP documentation if a snippet is rejected.

## Lifecycle

- `fluxion mcp` keeps the MCP process alive and reuses one remote client.
- `swap.quote` uses the remote service; the remote server owns discovery and snapshot warmup.
- `wallet_connect` starts a background pairing task in the MCP process. Keep the process alive and
  call `wallet_status` with `wait_seconds` (up to 110) while the user scans; repeat if it returns
  `pending`. `wallet_disconnect` aborts a pending pairing.
- `fluxion mcp` is otherwise stateless and never starts the remote server.
- Start the remote server separately (for local testing or deployment).

## Skills

Skills are agent instructions, not an execution path. Fluxion's swap skill lives in
`skills/swap/SKILL.md` with required frontmatter.

For opencode, add the repository skills directory to your config:

```json
{
  "skills": {
    "paths": ["/Users/peibin/code/quoter/skills"]
  }
}
```

Restart the agent after changing MCP or Skills config.

## Tools

Available tools depend on the installed capabilities:

| Capability | Tool | Contract |
|---|---|---|
| `swap.quote` | `swap_quote` | [capabilities/swap](capabilities/swap/README.md) |
| `wallet.balance` | `wallet_balance` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.assets` | `wallet_assets` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.allowance` | `wallet_allowance` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.collectibles` | `wallet_collectibles` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.connect` | `wallet_connect` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.status` | `wallet_status` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.disconnect` | `wallet_disconnect` | [capabilities/wallet](capabilities/wallet/README.md) |
| `transaction.simulate` | `transaction_simulate` | [capabilities/transaction](capabilities/transaction/README.md) |

## Troubleshooting

| Symptom | Check |
|---|---|
| `fluxion` not found | `cargo install --path crates/client`; verify `PATH`; or use an absolute path |
| `INVALID_ARGUMENT` from a tool | Token symbol/address, amount format and optional decimals |
| `RPC_ERROR` | `FLUXION_REMOTE_URL` and `FLUXION_API_KEY` |
| First call times out | Check the remote service; raise `experimental.mcp_timeout` |
