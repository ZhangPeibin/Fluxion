# MCP

Eollo exposes its capabilities to agents over **MCP stdio**. MCP is the project-level
transport: it is not a capability, and client registration is not capability-specific.

Capability contracts live in [docs/capabilities/](capabilities/README.md). This page covers
installation, registration, the remote service, lifecycle and troubleshooting. Security
boundaries and the deployment checklist are in [SECURITY.md](SECURITY.md).

## Install

```bash
cargo install --path crates/client
```

`eollo` must be on `PATH`. If it is not, use an absolute path in the client config instead.

## Run

```bash
eollo mcp
```

`eollo mcp` starts a stdio MCP server. It exposes the installed capabilities and routes each
call internally; the agent does not choose local or remote execution.

## Remote service

`swap.quote`, `yield.discover`, `yield.history` and `position.analyze` always call the remote
Eollo service.
`yield.build` and `yield.withdraw` run locally and only return calldata. Set `EOLLO_REMOTE_URL` to the remote service endpoint;
development and tests use the same variable:

```bash
EOLLO_REMOTE_URL=https://api.example.com EOLLO_API_KEY=... eollo mcp
```

`EOLLO_REMOTE_URL` is an endpoint override, not a local/remote mode.
`GRAPH_API_KEY` is not required for normal users.

### Installation token

With `EOLLO_REQUIRE_AUTH=1`, `/quote`, `/yield/discover`, `/yield/history` and `/position/analyze`
require a bearer token. The local client handles this
automatically:

1. first `/quote` returns `401`
2. client calls `POST /register` with a local `install_id`
3. token is stored in `~/.eollo/auth.json` (0600)
4. the quote is retried

The remote keeps tokens in memory in this minimal version. If the remote restarts, the client
re-registers on the next `401`.

The remote service is started separately. `eollo mcp` does not start or wait for it; MCP
initialize and `tools/list` always succeed. If the remote endpoint is unavailable, `swap_quote`
returns a structured retryable error.

Build/install and start the remote server separately:

```bash
cargo install --path crates/remote
EOLLO_REQUIRE_AUTH=1 eollo-remote serve --config configs/runtime.yaml
```

For local development without installing, use
`cargo run -p eollo-remote -- serve --config configs/runtime.yaml`.

## Client registration

Claude Code (`.mcp.json`):

```json
{
  "mcpServers": {
    "eollo": {
      "command": "eollo",
      "args": ["mcp"]
    }
  }
}
```

Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.eollo]
command = "eollo"
args = ["mcp"]
```

OpenCode (`opencode.json` or `~/.config/opencode/opencode.json`):

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "eollo": {
      "type": "local",
      "command": ["eollo", "mcp"],
      "enabled": true
    }
  }
}
```

Set `EOLLO_REMOTE_URL` / `EOLLO_API_KEY` in the environment that launches the agent when you
need a non-default remote endpoint. For wallet linking, set `WALLETCONNECT_PROJECT_ID` in the same
environment. Exact config keys vary by client version; check the client's
MCP documentation if a snippet is rejected.

## Lifecycle

- `eollo mcp` keeps the MCP process alive and reuses one remote client.
- `swap.quote`, `yield.discover`, `yield.history` and `position.analyze` use the remote service; the
  remote server owns discovery and snapshot warmup. `yield.build` and `yield.withdraw` are local.
- Remote tools accept `chain: "bsc"` or `chain: "ethereum"`. BSC uses `EOLLO_BSC_REMOTE_URL` when
  set, otherwise `EOLLO_REMOTE_URL`. `yield_discover` takes `asset: "BNB"` on BSC.
- Local wallet and transaction tools default to BSC; pass `chain: "ethereum"` for Ethereum.
- `wallet_sign` uses the stored session and waits for explicit phone confirmation. A rejection
  returns `USER_REJECTED`; it does not retry automatically.
- `wallet_connect` starts a background pairing task in the MCP process and returns a 53-column
  terminal QR as the first text content block, plus an optional PNG path for fallback. It does not
  open a window by default. Reproduce the QR before polling. Keep the process alive and call
  `wallet_status` with `wait_seconds` (up to 110) while the user scans; repeat if it returns
  `pending`. `wallet_disconnect` aborts a pending pairing.
- `eollo mcp` is otherwise stateless and never starts the remote server.
- Start the remote server separately (for local testing or deployment).

## Skills

Skills are agent instructions, not an execution path. The seven skills live under `skills/` with the
required frontmatter; see [`skills/README.md`](../skills/README.md) for the list.

Install them with the `skills` CLI (Node.js 22.20+) or GitHub CLI:

```bash
npx skills add ZhangPeibin/eollo -g -a opencode
# or
gh skill install ZhangPeibin/eollo swap --agent opencode --scope user
```

Use `npx skills add ZhangPeibin/eollo --list` to preview them, or
`--skill swap --skill wallet` to install a subset. The offline fallback for a local clone is the
OpenCode config path:

```json
{
  "skills": ["/absolute/path/to/eollo/skills"]
}
```

Restart the agent after changing MCP or skills.

## Tools

Available tools depend on the installed capabilities. The source of truth is
[`capabilities/catalog.yaml`](../capabilities/catalog.yaml); the table below is being migrated to
catalog-generated output. The wallet read tools (`wallet_balance`, `wallet_assets`,
`wallet_allowance`, `wallet_collectibles`), the Four.meme tools (`fourmeme_agent_tokens`,
`fourmeme_token`), and the read-only transaction/signature tools (`transaction_simulate`,
`transaction_inspect`, `transaction_verify`, `signature_inspect`), the yield tools (`yield_discover`, `yield_build`,
`yield_history`, `yield_withdraw`), `position_analyze`, the swap tools (`swap_quote`, `swap_build`),
and the wallet tools already return capability envelope v2. All catalog capabilities now use
envelope v2; read each catalog entry for its schema and chain support.

Natural-language prompt examples: [AGENT-EXAMPLES.md](AGENT-EXAMPLES.md).

| Capability | Tool | Contract |
|---|---|---|
| `swap.quote` | `swap_quote` | [capabilities/swap](capabilities/swap/README.md) |
| `swap.build` | `swap_build` | [capabilities/swap](capabilities/swap/README.md) |
| `wallet.balance` | `wallet_balance` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.assets` | `wallet_assets` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.allowance` | `wallet_allowance` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.collectibles` | `wallet_collectibles` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.connect` | `wallet_connect` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.status` | `wallet_status` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.disconnect` | `wallet_disconnect` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.agent-id` | `wallet_agent_id` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.sign` | `wallet_sign` | [capabilities/wallet](capabilities/wallet/README.md) |
| `wallet.send` | `wallet_send` | [capabilities/wallet](capabilities/wallet/README.md) |
| `transaction.inspect` | `transaction_inspect` | [capabilities/transaction](capabilities/transaction/README.md) |
| `transaction.simulate` | `transaction_simulate` | [capabilities/transaction](capabilities/transaction/README.md) |
| `transaction.send` | `transaction_send` | [capabilities/transaction](capabilities/transaction/README.md) |
| `transaction.verify` | `transaction_verify` | [capabilities/transaction](capabilities/transaction/README.md) |
| `signature.inspect` | `signature_inspect` | [capabilities/signature](capabilities/signature/README.md) |
| `fourmeme.agent-tokens` | `fourmeme_agent_tokens` | [capabilities/fourmeme](capabilities/fourmeme/README.md) |
| `fourmeme.token` | `fourmeme_token` | [capabilities/fourmeme](capabilities/fourmeme/README.md) |
| `yield.discover` | `yield_discover` | [capabilities/yield](capabilities/yield/README.md) |
| `yield.build` | `yield_build` | [capabilities/yield](capabilities/yield/README.md) |
| `yield.history` | `yield_history` | [capabilities/yield](capabilities/yield/README.md) |
| `yield.withdraw` | `yield_withdraw` | [capabilities/yield](capabilities/yield/README.md) |
| `position.analyze` | `position_analyze` | [capabilities/position](capabilities/position/README.md) |

## Troubleshooting

| Symptom | Check |
|---|---|
| `eollo` not found | `cargo install --path crates/client`; verify `PATH`; or use an absolute path |
| `INVALID_ARGUMENT` from a tool | Token symbol/address, amount format and optional decimals |
| `RPC_ERROR` | `EOLLO_REMOTE_URL` and `EOLLO_API_KEY` |
| First call times out | Check the remote service; raise `experimental.mcp_timeout` |
