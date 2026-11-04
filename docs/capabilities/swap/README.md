# swap.quote

`swap.quote` is Fluxion's first capability: an exact-in swap quote on Ethereum mainnet. It returns
amounts, split allocations, hop paths and state provenance. It is read-only and never signs,
approves, or broadcasts.

## Execution model

Execution location is an internal Fluxion decision; agents do not choose local/remote.

| Mode | `swap.quote` execution | Requirements |
|---|---|---|
| `managed` (default when `FLUXION_REMOTE_URL` is set) | Fluxion hosted service | `FLUXION_API_KEY` |
| `self_hosted` | local runtime | `GRAPH_API_KEY` + RPC |

`FLUXION_MODE=auto` is the default: remote when `FLUXION_REMOTE_URL` is configured, otherwise
local. `FLUXION_MODE` is operator configuration, not an agent input.

## Quick start

Managed mode:

```bash
fluxion swap ldo-eth 100
fluxion swap ldo-eth 100 --json
fluxion capability describe swap.quote --json
```

Development / self-hosted:

```bash
export GRAPH_API_KEY=...
FLUXION_MODE=self_hosted fluxion start   # warm runtime; first start takes about a minute
FLUXION_MODE=self_hosted fluxion swap ldo-eth 100
fluxion stop
```

Machine interface:

```bash
fluxion swap quote --in LDO --out ETH --amount 100 --json
```

MCP tool: `swap_quote` with `token_in`, `token_out`, `amount`, optional `max_hops`, `split`,
`split_max_branches`, `decimals_in`, `decimals_out`.

## MCP client setup

`fluxion mcp` speaks MCP over stdio. Register the binary as a local MCP server in your agent
client. `fluxion` must be on `PATH` (`cargo install --path .`). Managed mode needs no
`GRAPH_API_KEY`. For `FLUXION_MODE=self_hosted`, export `GRAPH_API_KEY` in the agent's environment;
`fluxion mcp` will auto-start the warm runtime if it is not already running.

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

OpenCode (`opencode.json`):

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

For a self-hosted runtime managed separately, use
`FLUXION_MODE=self_hosted fluxion mcp --url http://127.0.0.1:8080 --no-auto-start`. The agent should call the `swap_quote` tool; it should not manage `fluxion start` or
`fluxion stop` itself. Exact config keys vary by client version; check the client's MCP
documentation if the snippet is rejected.

## Inputs

| Field | Required | Description |
|---|---|---|
| `token_in` / `token_out` | yes | Symbol (`LDO`, `ETH`, `USDC`) or `0x` address |
| `amount` | yes | Human units, e.g. `100` or `0.5` |
| `decimals_in` / `decimals_out` | no | Required for unknown `0x` addresses |
| `max_hops` | no | Maximum route hops |
| `split` | no | Default true; allocate across independent pools/routes |
| `split_max_branches` | no | Maximum split branches |

## Output

The response follows [`schemas/swap_quote.v1.json`](../../../schemas/swap_quote.v1.json).
Important fields:

- `amount_in`, `amount_out`: exact `raw` integer and display `decimal`
- `plan_kind`: `baseline`, `direct`, or `split`
- `allocations`: exact per-branch inputs, outputs and hop paths
- `provenance.provider`: diagnostic execution label (for example `local-mcp` or `fluxion-remote`)
- `provenance.pricing_mode`: `rpc` or `local_snapshot`
- `quote_block`, `state_age_secs`, `lag_blocks`: freshness
- `latency_ms`: runtime quote latency

Warm local-snapshot quotes are typically 5–30 ms in the current benchmark environment. State
freshness is measured separately from latency.

## Errors

Errors use stable codes rather than free-form text:

`INVALID_ARGUMENT`, `NO_PATH`, `INSUFFICIENT_LIQUIDITY`, `AMOUNT_OUT_BELOW_MIN`, `RPC_ERROR`,
`STALE_STATE`, `STATE_INCOMPLETE`, `UNSUPPORTED`, `TIMEOUT`, `INTERNAL`.

CLI exit codes: `2` invalid argument, `3` no path/liquidity, `4` RPC/unsupported, `5` stale state,
`6` timeout, `1` internal.

## Boundaries

- Read-only: this capability does not construct, simulate, sign, or execute transactions.
- `swap.build`, `swap.simulate`, and `swap.execute` are future local-only capabilities.
- A successful quote is not a guarantee of execution; re-check freshness, slippage, gas, MEV and
  user authorization before acting.
- Skills are documentation, not a security boundary.

## Related

- [Capability index](../README.md)
- [Agent skill](../../../skills/swap/SKILL.md)
- [JSON schema](../../../schemas/swap_quote.v1.json)
- [Project whitepaper](../../WHITEPAPER.md)
- [Latest benchmark](../../benchmarks/quote-freshness-2026-11-04-route-split.md)
- [Root README](../../../README.md)
