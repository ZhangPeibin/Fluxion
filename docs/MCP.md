# MCP

Fluxion exposes its capabilities to agents over **MCP stdio**. MCP is the project-level
transport: it is not a capability, and client registration is not capability-specific.

Capability contracts live in [docs/capabilities/](capabilities/README.md). This page covers
installation, registration, execution modes, lifecycle and troubleshooting.

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

## Execution modes

| Mode | Behavior | Requirements |
|---|---|---|
| `managed` | capabilities use the hosted Fluxion service | `FLUXION_REMOTE_URL`, `FLUXION_API_KEY` |
| `self_hosted` | capabilities use the local runtime | `GRAPH_API_KEY` + RPC |
| `auto` (default) | remote when `FLUXION_REMOTE_URL` is set, otherwise local | depends on resolved mode |

`FLUXION_MODE` is operator configuration, not an agent input. Managed mode does not need
`GRAPH_API_KEY`; self-hosted/development mode does. See
[execution routing](CONFIGURATION.md#execution-routing).

For a self-hosted runtime managed separately:

```bash
FLUXION_MODE=self_hosted fluxion mcp --url http://127.0.0.1:8080 --no-auto-start
```

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

For self-hosted mode, set `FLUXION_MODE=self_hosted` and `GRAPH_API_KEY` in the environment that
launches the agent, or use the client's local MCP `environment` field. Exact config keys vary by
client version; check the client's MCP documentation if a snippet is rejected.

## Lifecycle and warmup

- `fluxion mcp` keeps the MCP process alive; quotes reuse the same warm runtime.
- The first quote can take 40–90 seconds while the local snapshot warms. Pre-warm with
  `fluxion start` for self-hosted mode, or raise the client's MCP timeout.
- `--no-auto-start` prevents the MCP server from starting a local runtime.
- Local runtime logs: `~/.fluxion/runtime.log`.
- Agents should call capabilities; they should not manage `fluxion start` or `fluxion stop`.

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
| `RPC_ERROR` in managed mode | `FLUXION_REMOTE_URL` and `FLUXION_API_KEY` |
| `RPC_ERROR` in self-hosted mode | `GRAPH_API_KEY`, RPC reachability, `~/.fluxion/runtime.log` |
| First call times out | Pre-warm with `fluxion start`, or raise `experimental.mcp_timeout` |
