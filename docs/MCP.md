# MCP

Fluxion exposes its capabilities to agents over **MCP stdio**. MCP is the project-level
transport: it is not a capability, and client registration is not capability-specific.

Capability contracts live in [docs/capabilities/](capabilities/README.md). This page covers
installation, registration, the remote service, lifecycle and troubleshooting.

## Install

```bash
cargo install --path .
```

`fluxion` must be on `PATH`. If it is not, use an absolute path in the client config instead.

## Run

```bash
fluxion mcp
```

`fluxion mcp` starts a stdio MCP server. It exposes the installed capabilities and routes each
call internally; the agent does not choose local or remote execution.

## Remote service

`swap.quote` always calls the remote Fluxion service. The installed binary uses the built-in
endpoint; development and tests can override it:

```bash
FLUXION_REMOTE_URL=https://api.example.com FLUXION_API_KEY=... fluxion mcp
```

`FLUXION_REMOTE_URL` is an endpoint override, not a local/remote mode.
`GRAPH_API_KEY` is not required for normal users.

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
need a non-default remote endpoint. Exact config keys vary by client version; check the client's
MCP documentation if a snippet is rejected.

## Lifecycle

- `fluxion mcp` keeps the MCP process alive and reuses one remote client.
- `swap.quote` does not start a local snapshot; the remote service owns quote state.
- Agents should call capabilities; they should not manage `fluxion start` or `fluxion stop`.
- `fluxion start` / `fluxion stop` are reserved for the local runtime used by future
  `wallet` / `execute` capabilities.

## Tools

Available tools depend on the installed capabilities:

| Capability | Tool | Contract |
|---|---|---|
| `swap.quote` | `swap_quote` | [capabilities/swap](capabilities/swap/README.md) |

## Troubleshooting

| Symptom | Check |
|---|---|
| `fluxion` not found | `cargo install --path .`; verify `PATH`; or use an absolute path |
| `INVALID_ARGUMENT` from a tool | Token symbol/address, amount format and optional decimals |
| `RPC_ERROR` | `FLUXION_REMOTE_URL` and `FLUXION_API_KEY` |
| First call times out | Check the remote service; raise `experimental.mcp_timeout` |
