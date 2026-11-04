# swap.quote

Use this capability when an agent needs an **exact-in swap quote** on Ethereum before deciding
whether to build or execute a transaction. It is read-only: it returns amounts, routes and state
provenance, and never signs, approves, or broadcasts.

## Capability

```text
swap.quote
```

Inputs:

| Field | Required | Description |
|---|---|---|
| `token_in` | yes | Symbol (`LDO`, `ETH`, `USDC`) or `0x` address |
| `token_out` | yes | Symbol or `0x` address |
| `amount` | yes | Human units, e.g. `100` or `0.5` |
| `decimals_in` / `decimals_out` | no | Required only for unknown `0x` addresses |
| `max_hops` | no | Maximum hops |
| `split` | no | Default `true`; allocate across independent pools/routes |
| `split_max_branches` | no | Maximum split branches |
| `provider` | no | `local` (default) or `remote` |

## Providers

| Provider | Status | Invocation |
|---|---|---|
| `local-cli` | available | `fluxion swap quote ... --json` |
| `local-mcp` | available | MCP tool `swap_quote` |
| `remote-mcp` | not implemented | returns `UNSUPPORTED` |

`local-cli` and `local-mcp` require a warm local runtime. The runtime is managed by the host or
by `fluxion start`; an agent must not treat `fluxion start` as a capability. `fluxion mcp`
auto-starts the local runtime when needed.

Local providers require the operator to supply `RPC`/`GRAPH_API_KEY`. A future remote provider
will hold those keys server-side and require a Fluxion API key instead.

## CLI

```bash
fluxion capability describe swap.quote --json
fluxion swap quote --in LDO --out ETH --amount 100 --json
fluxion ldo-eth 100 --json
```

## MCP

Tool name: `swap_quote`.

```json
{
  "name": "swap_quote",
  "arguments": {
    "token_in": "LDO",
    "token_out": "ETH",
    "amount": "100",
    "provider": "local"
  }
}
```

The tool result contains a JSON document in `content[0].text`; `isError` is `true` for tool
failures. Prefer parsing the JSON, not the human text.

## Reading the result

- `amount_out.raw` / `amount_out.decimal`: exact integer and display amount
- `plan_kind`: `baseline`, `direct`, or `split`
- `allocations`: per-branch exact inputs, outputs and hop paths
- `provenance.provider`: actual provider (`local-cli`, `local-mcp`, ...)
- `provenance.pricing_mode`: `rpc` or `local_snapshot`
- `quote_block`, `state_age_secs`, `lag_blocks`: state freshness
- `latency_ms`: runtime quote latency

Do not assume that a successful quote is executable. Re-check freshness, liquidity, slippage,
gas and user authorization before building or executing a transaction.

## Errors

| Code | Meaning | Agent action |
|---|---|---|
| `INVALID_ARGUMENT` | Bad token, amount or provider | Fix the request |
| `NO_PATH` / `INSUFFICIENT_LIQUIDITY` | No usable route | Try another amount/pair |
| `RPC_ERROR` | Runtime or chain data unavailable | Retry or switch provider |
| `STALE_STATE` / `STATE_INCOMPLETE` | Snapshot not ready or too old | Wait/retry; do not execute |
| `UNSUPPORTED` | Provider or capability unavailable | Use a supported provider |
| `TIMEOUT` | Quote took too long | Retry with a smaller amount or later |

## Boundaries

- This skill is documentation, not a security boundary.
- Signing and authorization belong to the Local Runtime; never let a remote provider sign.
- `swap.build`, `swap.simulate` and `swap.execute` are separate local-only capabilities and are
  not part of this skill.
