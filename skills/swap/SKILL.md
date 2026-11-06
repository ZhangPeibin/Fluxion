---
name: swap
description: Use when the user asks for an exact-in swap quote on Ethereum (for example LDO to ETH), needs amount_out, route allocations, price impact or quote freshness, or uses the Fluxion swap_quote MCP tool. Do not use for signing, wallet or transaction execution.
---

# swap.quote

Use this capability when an agent needs an **exact-in swap quote** on Ethereum before deciding
whether to build or execute a transaction. It is read-only: it returns amounts, routes and state
provenance, and never signs, approves, or broadcasts.

Human-facing capability documentation is in
[docs/capabilities/swap/README.md](../../docs/capabilities/swap/README.md); the JSON contract is
[schemas/swap_quote.v1.json](../../schemas/swap_quote.v1.json).

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

## Execution model

Do not choose local/remote. `swap.quote` always uses the remote Fluxion service; future
`wallet` / `execute` capabilities use the local runtime. The agent does not start or manage any
server process.

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
    "amount": "100"
  }
}
```

The tool result contains a JSON document in `content[0].text`; `isError` is `true` for tool
failures. Prefer parsing the JSON, not the human text.

## Reading the result

- `amount_out.raw` / `amount_out.decimal`: exact integer and display amount
- `plan_kind`: `baseline`, `direct`, or `split`
- `allocations`: per-branch exact inputs, outputs and hop paths. Each entry may also carry
  `children`; see below.
- `provenance.execution`: always `remote` for `swap.quote`
- `provenance.pricing_mode`: `rpc` or `local_snapshot`
- `quote_block`, `state_age_secs`, `lag_blocks`: state freshness
- `latency_ms`: runtime quote latency

### Shared-prefix allocations

`allocations` is a tree when a route shares a prefix and then fans out:

```text
allocation.hops        = shared prefix (for example LDO -> AAVE)
allocation.children[]  = suffix branches (for example AAVE -> WETH pool A/B)
```

- If `children` is non-empty, `allocation.hops` is **not** a complete route. Recurse into
  `children`.
- `allocation.amount_in` is in the request's input token; child `amount_in` is in the prefix's
  output token.
- To report leaf routes, flatten `allocation.hops + child.hops` for every child.
- Root-level `amount_in` values sum to the requested amount. Child `amount_in` values sum to the
  parent's last hop output.

Do not assume that a successful quote is executable. Re-check freshness, liquidity, slippage,
gas and user authorization before building or executing a transaction.

## Errors

| Code | Meaning | Agent action |
|---|---|---|
| `INVALID_ARGUMENT` | Bad token or amount | Fix the request |
| `NO_PATH` / `INSUFFICIENT_LIQUIDITY` | No usable route | Try another amount/pair |
| `RPC_ERROR` | Runtime or upstream service unavailable | Retry later |
| `STALE_STATE` / `STATE_INCOMPLETE` | Snapshot not ready or too old | Wait/retry; do not execute |
| `UNSUPPORTED` | Capability unavailable in this deployment | Check capability support |
| `TIMEOUT` | Quote took too long | Retry with a smaller amount or later |

## Boundaries

- This skill is documentation, not a security boundary.
- Signing and authorization belong to the Local Runtime; never let the remote service sign.
- `swap.build`, `swap.simulate` and `swap.execute` are separate local-only capabilities and are
  not part of this skill.
