# swap.quote

`swap.quote` is Fluxion's first capability: an exact-in swap quote on Ethereum mainnet. It returns
amounts, split allocations, hop paths and state provenance. It is read-only and never signs,
approves, or broadcasts.

## Status

| Provider | Status | Invocation |
|---|---|---|
| `local-cli` | available | `fluxion swap quote ...` / `fluxion ldo-eth 100` |
| `local-mcp` | available | MCP tool `swap_quote` |
| `remote-mcp` | planned | returns `UNSUPPORTED` until implemented |

Local providers require a warm Fluxion runtime. The runtime performs discovery and snapshot sync;
`fluxion start` (or CLI auto-start) manages it. An agent should call the capability, not manage the
process.

## Quick start

```bash
export GRAPH_API_KEY=...        # local providers use the operator's Graph/RPC credentials
fluxion start                   # warm runtime; first start takes roughly 1 minute
fluxion swap ldo-eth 100
fluxion swap ldo-eth 100 --json
fluxion capability describe swap.quote --json
fluxion stop
```

Machine interface:

```bash
fluxion swap quote --in LDO --out ETH --amount 100 --json
```

MCP tool: `swap_quote` with `token_in`, `token_out`, `amount`, optional `provider`,
`max_hops`, `split`, `split_max_branches`, `decimals_in`, `decimals_out`.

## Inputs

| Field | Required | Description |
|---|---|---|
| `token_in` / `token_out` | yes | Symbol (`LDO`, `ETH`, `USDC`) or `0x` address |
| `amount` | yes | Human units, e.g. `100` or `0.5` |
| `decimals_in` / `decimals_out` | no | Required for unknown `0x` addresses |
| `provider` | no | `local` (default); `remote` is not implemented |
| `max_hops` | no | Maximum route hops |
| `split` | no | Default true; allocate across independent pools/routes |
| `split_max_branches` | no | Maximum split branches |

## Output

The response follows [`schemas/swap_quote.v1.json`](../../../schemas/swap_quote.v1.json).
Important fields:

- `amount_in`, `amount_out`: exact `raw` integer and display `decimal`
- `plan_kind`: `baseline`, `direct`, or `split`
- `allocations`: exact per-branch inputs, outputs and hop paths
- `provenance.provider`: actual provider (`local-cli`, `local-mcp`)
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
