# Multi-pool allocation, HTTP contract and reference diagnostics

## Scope and objective

The allocator extends the two-pool experiment with a sampled multi-pool allocation stage. It
retains the full existing Router baseline and the best evaluated two-pool plan. Selected gross
output only changes on strict improvement. The candidate set still uses independent direct
Uniswap V2/V3 and ShibaSwap V2 pools; multi-hop and conversion routes remain available as the
baseline. This is not arbitrary multi-hop branch optimization or gas-adjusted execution routing.

## Discrete resource allocation

For each searched pool, sample exact integer quotes at `allocation_segments` fractions of the
total input. Dynamic programming distributes grid units across pools, bounded by `max_branches`.
The state records consumed units, active branches and sampled output; missing/failed quotes do
not supply valid transitions. This additive model requires independent physical pools.

Flooring each grid amount can leave a small integer remainder. For each retained final branch
count, assign the remainder to an active branch, re-quote that exact amount, and only accept
complete allocations whose inputs sum to the request. Alternatives for remainder placement are
checked within the same budget. Checked output addition prevents overflow. No floating-point
percentage participates in selection.

This optimizes a sampled model, not every continuous or integer allocation. Only the best sampled
state per branch count is refined for rounding; omitted pools and other allocations can still
win in a broader search. The original two-pool local refinement is preserved, so a coarse
multi-pool grid cannot erase an already better evaluated two-pool result.

The additional stage shares quote memoization, call quota, concurrency and deadline with the
two-pool stage. Budget exhaustion retains a complete known plan; `multi_pool_search` and
`multi_pool_completed` distinguish whether this stage actually ran/finished. The standalone
library/CLI defaults remain two-pool; `SplitConfig::multi_pool()` and service defaults allow four
branches, twenty allocation segments, four candidate pools and 256 additional adapter calls.

## HTTP opt-in

Existing requests retain their single-route wire response. `split: true` enables the allocator;
optional `split_max_branches` can lower the configured cap (for example, to 2) but cannot exceed
it. Supplying that field without `split: true` is an invalid request. Service YAML uses a typed
`split` mapping; partial settings preserve the service's multi-pool defaults and unknown keys fail.

Opted-in responses expose:

- `amount_out`: total selected output, decimal string;
- `plan_kind`: `baseline`, `direct` or `split`;
- `baseline_amount_out`: complete single-route baseline output;
- `allocations`: per-branch integer inputs, outputs, display-only percentages and sequential paths;
- `search`: budget, failure and completed-stage statistics;
- `quote_block`: the block used for real chain pricing.

For multiple branches, top-level `path` is empty; allocations must not be concatenated into a
fake sequential path. A single selected allocation keeps its sequential top-level path. Price
impact is the existing value when the baseline wins; it is `null` for newly selected allocations
instead of inventing an uncomputed aggregate impact.

Each real split request reads a block and builds fresh pricing-source caches pinned to it. It
reuses the server's filtered topology and coarse Router snapshot; the coarse snapshot may be
older, while every baseline/refinement amount quote in that request uses the recorded block.
The server explicitly keeps mock pricing separate from the RPC used by readiness probes, so
offline split quotes keep using mock data. In-memory contexts omit `quote_block`.
Same-token requests need no chain pricing. Minimum output is checked against the selected total,
so a split can satisfy a minimum that the baseline alone does not meet.

The entire request stays within the configured HTTP timeout. Extra search has an earlier hard
deadline, reserving a response margin, so stalled optional search can return the known baseline
rather than discarding it with a request timeout. If baseline pricing itself cannot complete,
the existing error/timeout semantics remain. Viewer enables split mode by default with a toggle,
shows percentages and each branch, and highlights the union of the selected graph edges.
The default HTTP timeout is now 10000 ms to accommodate the 8000 ms extra-search budget and
baseline pricing; a smaller configured HTTP timeout still takes precedence.

## Same-run external comparison

`split_bench --paraswap --max-branches 4 --max-quotes 256` compares single, two-pool, multi-pool
and ParaSwap outputs. It requests ParaSwap first, chooses its reported block when usable (else
head or explicit override), and constructs one pinned local source/snapshot per row. Two/multi
call order alternates. Local baselines must match for a valid paired row. All used block hashes
are checked at the end. External failed quotes remain recorded, not converted into zero outputs.

Raw external quotes may still contain cached, mixed-block pool state. The reported external
output and its received allocation re-quoted at a single block are different comparison objects.
Candidate timing excludes the extra comparison queries and diagnostic setup; their separate
cost and elapsed time remain in JSON. This is not a production HTTP latency microbenchmark.

## Reference replay and coverage

For SHIB rows, `reference` re-quotes supported received allocations at the same local block:
Uniswap V2/V3, ShibaSwap V2 and configured wstETH conversions. It checks pool factories, token
continuity and actual V3 fees, preserves input sums, and refuses a full total if any necessary
branch is unsupported or fails. Shared AMM pools across paths require stateful simulation and
are explicitly unsupported. Diagnostic calls do not add pools or tokens to the production graph.

A second diagnostic removes exchanges containing non-allowlisted tokens and renormalizes the
remaining reference weights. Its output is an allowlist-constrained reference allocation, not
ParaSwap's reported quote or a newly optimized external plan. It may still use pools outside our
discovered/capped inventory. Comparing these figures helps separate allocation, coverage and
state effects without claiming a unique causal decomposition or global parity.

Gas/MEV and actual execution remain outside the gross-output objective. Raw routes, allocation
weights, omitted-token identities, unsupported cases, configs and source fingerprints are saved
with the benchmark results.

Latest evidence: [90-row four-way comparison](benchmarks/2026-11-02-multi-pool-vs-paraswap.md),
[raw data](benchmarks/2026-11-02-multi-pool-vs-paraswap.json),
[Chinese analysis](benchmarks/2026-11-02-multi-pool-analysis.md).
