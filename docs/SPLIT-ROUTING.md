# Two-pool split quote experiment (#30)

[Issue #30](https://github.com/ZhangPeibin/Fluxion/issues/30) delivers a public library optimizer and
a runnable fixed-block experiment. The HTTP `/quote` contract and Viewer remain single-path;
parallel branches are not flattened into a fake sequential route. This phase optimizes gross
output, excluding additional execution gas, MEV, transaction construction and actual execution.

## Public behavior

`SplitOptimizer::quote` first invokes the existing `Router` against one request-local observed
source. The baseline retains all existing multi-hop and protocol-conversion capabilities. A
baseline error propagates as the existing `QuoteError`; no fallback result is fabricated.

The result contains the complete baseline, selected plan kind (`baseline`, `direct`, `split`),
selected integer output, allocations and search statistics. An allocation has an exact input,
output and hop sequence. A baseline allocation can have multiple hops or conversions; split
allocations are two independent direct AMM pools. Library callers must supply an immutable
pricing context for meaningful comparisons; the experiment supplies pinned RPC state.

Selection only replaces the current best on a strict output increase. Therefore the original
baseline wins ties and the returned gross output cannot fall below the successful baseline.
Selection and branch sums use U256 integers; percentages in reports are display-only.

## Algorithm

1. Enumerate direct edges for the requested tokens, restricted to Uniswap V2/V3 and ShibaSwap V2.
   Deduplicate by physical pool address so the same pool cannot be both independent branches.
   Discovery and its both-token allowlist remain unchanged. Curve/conversion paths can still win
   as the baseline, but are not split branches in this phase.
2. Rank pools by the already-computed full-input baseline quotes and apply the explicit pool cap.
   Report both eligible and searched counts; unsearched pools are not claimed to be optimized.
3. For each pair A/B and total input Q, evaluate `quote_A(x) + quote_B(Q-x)` on a coarse grid,
   including both all-A/all-B endpoints. Zero branches have zero output without an adapter call.
4. Refine the interval around each pair's best evaluated point. Keep that point and sample the
   neighboring interval again. No concavity assumption, ternary-search proof or global-optimality
   claim is required. The number of refinement rounds is bounded.
5. Compare all completed valid plans against the retained baseline and return the best.

Integer grid points use quotient/remainder decomposition rather than multiplying Q by a ratio
first, so even U256::MAX inputs do not overflow in allocation math. Assigning `Q-x` to the second
branch preserves every input unit, including odd amounts. Output addition is checked for overflow.

## Budgets, caching and failures

Budgets start after the baseline succeeds. Missing positive-amount quote keys are deduplicated
per stage, and completed `(pool, direction, amount)` outcomes are memoized across baseline and
search within a request. Errors are also cached for that pricing context. Baseline adapter calls
are reported separately and do not consume the additional-search budget.

Search has a cap on adapter calls, concurrency and elapsed time. Calls are admitted before
execution; work not admitted or still in flight at timeout is not treated as a completed plan.
Completed cache entries survive cancellation, allowing the optimizer to keep an improvement
already evaluated before a later candidate stalls. One failed branch invalidates the complete
two-branch plan, rather than returning the other branch as if it consumed all Q.

Stop reasons distinguish configured-stage completion, no eligible pair, quote-budget exhaustion,
and time-budget exhaustion. Completion means the configured sampled search finished, not that
all possible integer allocations or market pools were enumerated.

Default parameters, selected after the recorded SHIB calibration:

| Parameter | Default |
|---|---:|
| Coarse segments | 10 |
| Refinement segments / rounds | 4 / 2 |
| Maximum searched direct pools | 4 |
| Additional adapter calls | 128 |
| Search concurrency | 8 |
| Search time budget | 8000 ms |

These are `SplitConfig` / experiment CLI options, not new YAML service settings. A zero call/time
budget returns the baseline. Invalid segment/concurrency/pool bounds fail configuration validation.

## Fixed-state experiment and RPC metrics

The shared `universe` assembly extracts discovery, pricing-source construction and coarse warming
from the server without introducing a second, divergent venue list. The server retains its
existing mock fallback. Experiments refuse missing/empty real discovery instead of benchmarking mock data.

The CLI fixes the discovered inventory, then chooses a block (or accepts `--block`). It creates
fresh pricing caches, warms and builds a snapshot using that pinned client, and uses the same
source for baseline and split. Background refresh tasks are not started. The live filtered
inventory is not a claim of historical indexer replay; the full pool list is saved in JSON.

`RpcClient::request_count` is shared across clones and counts each JSON-RPC HTTP attempt, including
retries and failed connections. The optimizer optionally samples that monotonic meter around the
baseline and search phases. Adapter quote counts are separate: V2 math can be local, whereas a
V3 quote usually incurs RPC. Setup/discovery RPC is separated from per-row quote cost; discovery
RPC counts do not include GraphQL HTTP requests. Caches may warm/expire across rows at the same
fixed block, affecting cost but not the pinned quote state.

Reports include source fingerprint, git/dirty state, effective public config (without RPC URL),
pool inventory, corpus, exact allocations, failure/stop states and timing. They checkpoint after
each attempted row with `completed=false`; the final report verifies the block hash again.
Whole-row timeouts are reported as failures, not invented successful baseline results.

## Verification

- Public-result tests cover split wins, multi-hop baseline preservation, ties, odd/tiny/maximum
  inputs, zero allocations, duplicate physical pools, branch failures, quote-budget and timeout
  behavior, and original baseline errors.
- A local mock RPC confirms pinned-clone parameters and retry-inclusive shared request counts.
- A pinned-block mainnet test compares selected independent V2/ShibaSwap branches against fresh
  adapter reads and verifies exact input/output sums.
- Full result and calibration evidence: [benchmark analysis](benchmarks/2026-11-02-split-analysis.md).
