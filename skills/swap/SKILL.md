---
name: swap
description: Use when the user asks for an exact-in swap quote on Ethereum (for example LDO to ETH), needs amount_out, route allocations, price impact or quote freshness, wants to build V2 swap calldata from a quote, or uses the Fluxion swap_quote or swap_build MCP tools. Do not use for signing, wallet or transaction execution.
---

# swap.quote

Use this capability when an agent needs an **exact-in swap quote** on Ethereum before deciding
whether to build or execute a transaction. It is read-only: it returns amounts, routes and state
provenance, and never signs, approves, or broadcasts.

Human-facing capability documentation is in
[docs/capabilities/swap/README.md](../../docs/capabilities/swap/README.md); the JSON contract is
[schemas/swap_quote.v2.json](../../schemas/swap_quote.v2.json).

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

Do not choose local/remote. `swap.quote` always uses the remote Fluxion service. `wallet.*` and
`transaction.simulate` use the local runtime. The agent does not start or manage any server
process.

## swap.build

Build local Uniswap V2 calldata from a saved quote:

```bash
fluxion swap build --quote quote.json --recipient 0x... --slippage-bps 50 --json
```

- Ethereum only; one `uniswap-v2` allocation; no split/nested/V3/Curve routes.
- Returns `router`, `data`, `value`, `amount_out_min`, `deadline`, `path`, `hops`.
- Never signs or broadcasts.

## Workflow recipe: swap

Use this recipe only when the user wants to execute a swap, not merely quote it. The workflow is
`swap.quote → swap.build → wallet.allowance → transaction.simulate → transaction.inspect →
transaction.send → transaction.verify`.

Required inputs: `token_in`, `token_out`, `amount`, `recipient`, `slippage_bps`, and an optional
policy. The chain is Ethereum.

Checkpoints:

1. `swap.quote`: reject stale or incomplete state; confirm the route and amount before building.
2. `swap.build`: stop on `UNSUPPORTED`; never ask for a signature if build failed.
3. `wallet.allowance`: if the allowance is insufficient, request an explicit approval first; do
   not hide the extra transaction.
4. `transaction.simulate`: stop on revert before any wallet request.
5. `transaction.inspect`: stop on a policy block and surface high findings to the user.
6. `transaction.send`: requires a connected WalletConnect session; the user confirms on the phone.
7. `transaction.verify`: require `status: success`; report `mismatch` against the inspect report.

Failure handling: no route, unsupported build, insufficient allowance, simulation failure, policy
block, or user rejection stops the workflow. Never skip a failed step or sign silently.

## CLI

```bash
fluxion capability describe swap.quote --json
fluxion capability describe swap.build --json
fluxion swap quote --in LDO --out ETH --amount 100 --json
fluxion swap build --quote quote.json --recipient 0x... --json
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

`swap.quote` uses envelope v2: `meta`, `chain`, `block`, `as_of`, `provenance`, `coverage`,
`warnings`, `unknowns`, and `data`. Quote fields are under `data`.

- `amount_out.raw` / `amount_out.decimal`: exact integer and display amount
- `plan_kind`: `baseline`, `direct`, or `split`
- `allocations`: per-branch exact inputs, outputs and hop paths. Each entry may also carry
  `children`; see below.
- `provenance.pricing_mode`: `rpc` or `local_snapshot`
- `block` and `provenance.age_secs`: pinned block and state freshness
- `latency_ms`: runtime quote latency

### Shared-prefix allocations

`allocations` is a tree when routes share an intermediate node:

```text
allocation.hops        = simple prefix path, if any
allocation.sources[]   = multiple prefix branches feeding the same intermediate node
allocation.children[]  = suffix branches leaving that intermediate node
```

- A regular path has `hops` and no `sources`/`children`.
- A shared-prefix fan-out has `hops` plus `children`: `hops` is only the prefix.
- A fan-in node has `sources`; `hops` is empty. Fan-in plus fan-out has both `sources` and
  `children`.
- `allocation.amount_in` is in the request's input token; child `amount_in` is in the
  intermediate token.
- To report leaf routes, flatten `source.hops + allocation.hops + child.hops` as applicable.
- Root-level `amount_in` values sum to the requested amount. Sources' `amount_out` sum to the
  intermediate amount; children's `amount_in` sum to that same intermediate amount.

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
- `transaction.simulate` is available for read-only calldata simulation before signing.
- `swap.build` is a separate local capability; `swap.execute` remains a future convenience wrapper
  and is not part of this skill.
- Related skills: [wallet](../wallet/SKILL.md) · [transaction](../transaction/SKILL.md).
