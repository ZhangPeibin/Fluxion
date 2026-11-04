# Protocol conversion and ShibaSwap quote coverage

## Purpose

The original benchmark already quoted wstETH/WETH correctly, but could not reach that liquidity
from stETH through its protocol wrapper. It also omitted ShibaSwap liquidity used by the external
aggregator for SHIB. This change adds those capabilities while retaining single-path exact-in quotes.

## Typed conversions, configured deployments

`ConversionKind` describes supported interfaces; `ProtocolConversionConfig` describes each
deployment's contract, underlying and wrapped token, enabled state, and allowed directions.
Currently the implemented interface is `wsteth`. Reusing that interface requires configuration;
supporting a different protocol interface requires a new adapter, not arbitrary ABI in YAML.

The default instance is Ethereum Lido's stETH/wstETH wrapper. Startup validates its `stETH()` getter
against the configured underlying and probes each enabled direction. Invalid config relationships
fail configuration loading; an unavailable on-chain deployment is logged and omitted. Both tokens
must pass the same allowlist as ordinary pools. Explicit `[]` disables the default instance.

The graph stores the wrapper as a two-token link with only enabled directed edges. It currently
reuses the `Pool` container, but `PoolKind::Conversion` distinguishes the semantics. The dispatcher
selects `ProtocolConversions`, which calls `getWstETHByStETH(amount)` or
`getStETHByWstETH(amount)` for the actual input amount. These mirror Lido's integer conversion
math; no floating-point approximate quote or user transaction is sent. Very small inputs rounding
to zero are rejected. The spot-rate probe uses one 18-decimal unit for coarse routing.

Conversions have no AMM fee penalty or finite AMM reserve; the internal `U256::MAX` sentinel
prevents a reserve-ratio filter from excluding the edge, but is not exposed as a graph reserve.
This does not generalize to arbitrary protocols' caps, fees, asynchronous withdrawals, or user
balances/approvals. Each conversion consumes one `max_hops` edge, just like a swap. The existing
no-repeated-token rule prevents wrap/unwrap cycles in one route.

`operation` in quote and graph JSON explicitly labels `swap`, `wrap`, or `unwrap`; the `pool`
field remains backward-compatible and contains the wrapper address for conversions. The viewer
adds both new venue labels and shows conversion links dashed, exempting them from reserve filters.

## ShibaSwap V2

The distinct `ShibaSwapV2` venue reuses the V2 reserve adapter, 997/1000 fee calculation, cache and
Multicall prefetch. Mainnet factory: `0x115934131916C8b277DD010Ee02de363c09d037c`; default
`enabled_platforms` includes `shibaswap-v2` independently of the conversion configuration.

Factory discovery generates sorted unique unordered pairs from the allowlist and queries `getPair`
in Multicall batches of 200. This is quadratic in allowlist size, intentionally bounded by the
major-token universe rather than scanning all historical factory pairs. Discovered contracts must
return matching `token0`/`token1` and nonzero `getReserves` values. A bad subcall does not revert
other calls. Requests use raw JSON-RPC and hand-written ABI.

USD-like liquidity estimates start with USDC/USDT/DAI at $1 and WETH at the canonical Uniswap V2
USDC/WETH reserve ratio, then propagate raw-unit prices across discovered reserve ratios. These
are approximate discovery heuristics, not manipulation-resistant oracles. At a positive
`min_liquidity`, unpriced/below-floor pools are excluded; qualifying pools are ranked by estimated
liquidity before `max_pools`. With a zero floor, unpriced pools can be admitted explicitly.
Factory reads cannot evaluate volume or deployment age, so nonzero global volume/age thresholds
skip ShibaSwap discovery with a clear warning instead of claiming to enforce unavailable data.

## Evidence and verification

- Behavioral tests cover integer rounding, zero input, disabled direction, incorrect underlying,
  configuration validation, conversion+swap path composition, and public operation/reserve fields.
- Discovery tests cover both-token pairing, liquidity-unit normalization, and retaining the deepest
  qualifying pool at a small pool limit.
- The ignored mainnet test pins all `eth_call`s to one block. It validates wrapper round-trip
  rounding, discovers the real SHIB/WETH ShibaSwap pair, and compares the reserve adapter against
  the deployed ShibaSwap Router's `getAmountsOut` exactly.
- Observed SHIB/WETH pair: `0xCF6dAAB95c476106ECa715D48DE4b13287ffDEAa`. The first diagnostic at
  block 26324563 matched Router output `485582333527276483260738248` raw SHIB for 1 WETH.

Protocol references:
- [Lido WstETH source](https://github.com/lidofinance/core/blob/master/contracts/0.6.12/WstETH.sol)
- [ParaSwap V2 venue config (ShibaSwap factory, 30 bps)](https://github.com/paraswap/paraswap-dex-lib/blob/master/src/dex/uniswap-v2/config.ts)

## Paired benchmark

### Candidate diversity is part of composing the new edges

The initial integration made stETH↔WETH competitive but still missed conversion-prefixed routes
to USDC/LDO. A live diagnostic found stETH→USDC returned `74754414248` raw USDC, while wrapping
the identical input and separately quoting the remaining two hops returned `76676703531`.
The better three-edge route existed; spot-only top-K pruning discarded it before refinement.
The [initial benchmark](benchmarks/2026-11-02-quote-quality-initial.md) is retained, including
its large unresolved USDC→stETH loss, rather than presented as a finished fix.

Coarse candidates now carry two bounded capability flags: contains a conversion, contains a
ShibaSwap pool. Each `(token, hop)` retains up to `top_k` candidates **per family**, at most four
families. This keeps the original-venue beam as well as new-capability beams; misleading spot
prices in one family cannot consume every candidate slot in another. Exact refinement still
picks the maximum output across all retained families and hop lengths, with concurrency 16.
Cost ties are ordered by edge IDs for deterministic pruning within one graph. This is still a
heuristic, not exhaustive global routing, and the extra candidates can increase latency/RPC cost.

Regression tests use `top_k=1`: one proves a deep conversion+two-swap route survives a much
better-looking thin AMM route; another proves a newly added thin ShibaSwap pool does not crowd
out the deeper original-venue route. Both test public router outcomes, not just internal buckets.

`bench/configs/threeway.yaml` enables conversions + ShibaSwap; `bench/configs/control.yaml` explicitly
uses the original three venues with no conversions. Both use the same defaults, thresholds and
corpus otherwise. `bench --control-service URL` alternates candidate/control call order, then asks
ParaSwap. The JSON retains both local paths and the external `bestRoute`, including percentages
and venue/pool details when provided. Venue names are read from `swapExchanges`, not unrelated
address-valued `exchange` keys nested in adapter data.

Results separate all rows, stETH-related rows, SHIB-related rows, and other rows. Live-state skew,
different discovery/startup snapshots, gas, actual execution and external split routing remain
limitations. The control is a feature-disabled run of the same binary, not a historical binary
or exact pinned-state ablation. Fixed raw input amounts are unchanged from the original corpus.
