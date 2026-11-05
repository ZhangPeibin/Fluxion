# swap.quote

`swap.quote` is Fluxion's first capability: an exact-in swap quote on Ethereum mainnet. It returns
amounts, split allocations, hop paths and state provenance. It is read-only and never signs,
approves, or broadcasts.

## Execution model

`swap.quote` always uses the remote Fluxion service; `wallet` / `execute` capabilities use the
local runtime. Agents and users do not choose local/remote.

The installed binary uses the built-in remote endpoint. Development/tests may override it with
`FLUXION_REMOTE_URL` / `FLUXION_API_KEY`; this is an endpoint override, not a mode.

## Quick start

```bash
fluxion swap ldo-eth 100
fluxion swap ldo-eth 100 --json
fluxion capability describe swap.quote --json
```

Machine interface:

```bash
fluxion swap quote --in LDO --out ETH --amount 100 --json
```

MCP tool: `swap_quote` with `token_in`, `token_out`, `amount`, optional `max_hops`, `split`,
`split_max_branches`, `decimals_in`, `decimals_out`.

## MCP

Tool name: `swap_quote`.

Project-level MCP installation, client registration, execution routing and troubleshooting live in
[docs/MCP.md](../../MCP.md). This capability document only defines the tool contract.

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
- `provenance.execution`: always `remote` for `swap.quote`
- `provenance.pricing_mode`: `rpc` or `local_snapshot`
- `quote_block`, `state_age_secs`, `lag_blocks`: freshness
- `latency_ms`: runtime quote latency

Warm local-snapshot quotes are typically 5–30 ms in the current benchmark environment. State
freshness is measured separately from latency.

## Latest benchmark

90 rows on 4 Nov 2026, 209 pool/conversion records, full-range V3:

| Metric | Result |
|---|---:|
| Snapshot lag blocks p50/p95/max | **0 / 1 / 1** |
| Snapshot lag seconds p50/p95/max | **0 / 12 / 12 s** |
| Fluxion HTTP p50/p95 | **49 / 133 ms** |
| Success | **90 / 90** |
| ParaSwap mean delta / within ±50 bps | **−2.6 bps / 89** |
| 1inch mean delta / within ±50 bps | **−8.7 bps / 82** |

Split allocation includes disjoint multi-hop routes. Remaining large-order gaps are mostly venue
coverage limits (for example LDO and SHIB), not freshness or allocation. Full methodology:
[90-row report](../../benchmarks/quote-freshness-2026-11-04-route-split.md) ·
[benchmark index](../../benchmarks/README.md).

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
