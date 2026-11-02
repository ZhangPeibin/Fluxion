# Single / two-pool / multi-pool / ParaSwap comparison

```json
{
  "block": 26325556,
  "block_hash": "0xff99bf7d73b9f03bf788a3ca3a99857cfe7a005d17649004d60b9c5f6fddb164",
  "block_hash_unchanged": true,
  "completed": true,
  "config": {
    "curve_address_provider": "0x0000000022D53366457F9d5E68Ec105046FC4383",
    "curve_registry_ids": [
      0,
      5
    ],
    "enabled_platforms": [
      "uniswap-v2",
      "uniswap-v3",
      "curve",
      "shibaswap-v2"
    ],
    "host": "127.0.0.1",
    "max_hops": 3,
    "max_pools": 10000,
    "min_liquidity": 50000,
    "min_pool_age_days": 0,
    "min_volume_24h": 0,
    "port": 8081,
    "protocol_conversions": [
      {
        "contract": "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0",
        "enabled": true,
        "kind": "wsteth",
        "underlying": "0xae7ab96520de3a18e5e111b5eaab095312d7fe84",
        "unwrap": true,
        "wrap": true,
        "wrapped": "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"
      }
    ],
    "refresh_interval_secs": 30,
    "request_timeout_ms": 10000,
    "reserve_ratio_k": 0,
    "shibaswap_v2_factory": "0x115934131916c8b277dd010ee02de363c09d037c",
    "slippage_penalty_lambda": 0.0,
    "snapshot_refresh_secs": 120,
    "split": {
      "allocation_segments": 20,
      "coarse_segments": 10,
      "concurrency": 8,
      "max_branches": 4,
      "max_pools": 4,
      "max_quote_calls": 256,
      "refinement_rounds": 2,
      "refinement_segments": 4,
      "timeout_ms": 8000
    },
    "token_allowlist": [],
    "top_k": 8,
    "uniswap_v2_subgraph_id": "EYCKATKGBKLWvSfwvBjzfCBmGwYNdVkduYXVivCsLRFu",
    "uniswap_v3_subgraph_id": "5zvR82QoaXYFyDEKLZ9t6v9adgnptxYpKpSbxtgVENFV"
  },
  "discovery_rpc_requests": 287,
  "edge_count": 432,
  "expected_rows": 6,
  "focus": "SHIB",
  "git_sha": "e98cd0f6166cbdbdd90b8207a9f1fe825dbdca19",
  "paraswap_comparison": true,
  "pool_count": 210,
  "pricing_state_mode": "per_row_reference_block_if_available_else_head",
  "reference_replay_scope": "SHIB only; supported interfaces, no shared-pool simulation",
  "row_timeout_ms": 60000,
  "search_config": {
    "allocation_segments": 20,
    "coarse_segments": 10,
    "concurrency": 8,
    "max_branches": 4,
    "max_pools": 4,
    "max_quote_calls": 256,
    "refinement_rounds": 2,
    "refinement_segments": 4,
    "timeout_ms": 8000
  },
  "setup_rpc_requests": 40,
  "source_fingerprint_keccak256": "0x973d0be5bfca8e211b435a3e6c77a89f78b7a15da3e1f050e95c0abf23a96616",
  "timestamp_unix": 1793603980,
  "two_pool_search_config": {
    "allocation_segments": 20,
    "coarse_segments": 10,
    "concurrency": 8,
    "max_branches": 2,
    "max_pools": 4,
    "max_quote_calls": 256,
    "refinement_rounds": 2,
    "refinement_segments": 4,
    "timeout_ms": 8000
  },
  "verified_row_blocks": {
    "26325556": {
      "hash": "0xff99bf7d73b9f03bf788a3ca3a99857cfe7a005d17649004d60b9c5f6fddb164",
      "unchanged": true
    },
    "26325557": {
      "hash": "0x9226ce92a62e0bb5ff10865987bdb9aa16a08444dcc59b5ffa5ee9944d5f0456",
      "unchanged": true
    },
    "26325558": {
      "hash": "0x8269fd5a11e2805a445a404bdcdc263dd8838c5057fd6bf8b34c8f94f2addc2c",
      "unchanged": true
    },
    "26325559": {
      "hash": "0x042a825dfe82e9704322f4425ddf1a34686d215026e03a5503d7e66d520dcc7e",
      "unchanged": true
    },
    "26325560": {
      "hash": "0xd08f5f5827fb0673719f3196b3fd9818cd725fc011b6c4f1c63c6571db0c1b78",
      "unchanged": true
    },
    "26325561": {
      "hash": "0xe4e55e4385d577bb0a370565b54c7277bfd0d3d2237670386834b9e0d41ac104",
      "unchanged": true
    }
  },
  "working_tree_dirty": true
}
```

Full pool inventory and corpus are in the sibling JSON report.

## Summary

| cohort | rows | success | split | baseline | median gain bps | mean gain bps | max gain bps | baseline p50/p95 ms | extra search p50/p95 ms | extra adapter calls | extra RPC |
|---|---|---|---|---|---|---|---|---|---|---|---|
| all | 6 | 6 | 6 | 0 | 12.906312635662953 | 46.23391642445868 | 174.33173349313935 | 1343/1671 | 2523/3631 | 756 | 390 |
| shib | 6 | 6 | 6 | 0 | 12.906312635662953 | 46.23391642445868 | 174.33173349313935 | 1343/1671 | 2523/3631 | 756 | 390 |
| steth | 0 | 0 | 0 | 0 | null | null | null | null/null | null/null | 0 | 0 |
| other | 0 | 0 | 0 | 0 | null | null | null | null/null | null/null | 0 | 0 |

## Per-row comparison

| pair | amount in | baseline out | selected out | kind | gain bps | search stop | baseline/search ms | extra quotes/RPC | allocations |
|---|---|---|---|---|---|---|---|---|---|
| WETH → SHIB | 333333333333333333 | 163212372326627623133057468 | 163222947244072195440668313 | split | 0.6479 | completed | 1167/3607 | 149/77 | 80.000% shibaswap-v2 + 10.000% uniswap-v3 + 10.000% uniswap-v2 |
| WETH → SHIB | 3333333333333333333 | 1623533103215627208436622461 | 1626145035126106267109354951 | split | 16.0879 | completed | 1374/3618 | 146/76 | 70.000% shibaswap-v2 + 15.000% uniswap-v3 + 15.000% uniswap-v2 |
| WETH → SHIB | 33333333333333333333 | 15423520024886453084360324642 | 15692400923136913404059022769 | split | 174.3317 | completed | 1151/3631 | 159/87 | 70.000% shibaswap-v2 + 15.000% uniswap-v2 + 15.000% uniswap-v3 |
| SHIB → WETH | 80000000000000000000000000 | 162265355136063282 | 162311407753772486 | split | 2.8381 | completed | 1654/2459 | 100/49 | 40.000% shibaswap-v2 + 30.000% uniswap-v2 + 25.000% uniswap-v3 + 5.000% uniswap-v3 |
| SHIB → WETH | 800000000000000000000000000 | 1618469709633296654 | 1620043618897783186 | split | 9.7247 | completed | 1343/2523 | 101/50 | 70.000% shibaswap-v2 + 15.000% uniswap-v2 + 15.000% uniswap-v3 |
| SHIB → WETH | 8000000000000000000000000000 | 15777881008710111971 | 15894279344844009586 | split | 73.7731 | completed | 1671/2471 | 101/51 | 75.000% shibaswap-v2 + 15.000% uniswap-v2 + 10.000% uniswap-v3 |

## Same-run comparison

Local single/two/multi modes use the same pinned block per row. Raw external quotes can contain cached, mixed-block state. SHIB reference allocations are re-quoted separately at that local block; unsupported venues/shared pools produce no fabricated replay total.

| cohort | n | single median/mean vs ParaSwap bps | two median/mean | multi median/mean | multi within ±50 bps | multi median vs two bps |
|---|---|---|---|---|---|---|
| all | 6 | -15.01468194734057/-53.26043116357633 | -9.95104978408712/-24.617288834283205 | -7.509464241788383/-7.685509280835652 | 6 | 5.092789465134325 |
| shib | 6 | -15.01468194734057/-53.26043116357633 | -9.95104978408712/-24.617288834283205 | -7.509464241788383/-7.685509280835652 | 6 | 5.092789465134325 |
| steth | null | null/null | null/null | null/null | null | null |
| other | null | null/null | null/null | null/null | null | null |

| pair | input | row block | single output | two-pool output | multi output | ParaSwap output | multi vs ParaSwap bps | multi vs full replay bps | multi vs allowlisted replay bps |
|---|---|---|---|---|---|---|---|---|---|
| WETH → SHIB | 333333333333333333 | 26325556 | 163212372326627623133057468 | 163219975489078407255346938 | 163222947244072195440668313 | 163223212044319753246300245 | -0.016223197928852472 | -0.01622319792888246 | -0.01622319792888246 |
| WETH → SHIB | 3333333333333333333 | 26325557 | 1623533103215627208436622461 | 1625117489519744330018893699 | 1626145035126106267109354951 | 1626148639278687234856835504 | -0.022163733953413048 | -0.02216373395341904 | -0.02216373395341904 |
| WETH → SHIB | 33333333333333333333 | 26325558 | 15423520024886453084360324642 | 15588829273921497629708484865 | 15692400923136913404059022769 | 15717403684110987455706076441 | -15.907691547904827 | -16.248606166949685 | -2.1088717969899573 |
| SHIB → WETH | 80000000000000000000000000 | 26325559 | 162265355136063282 | 162294776816804896 | 162311407753772486 | 162491952220182554 | -11.110978971156898 | -11.111067081646505 | 1.1574967993744132 |
| SHIB → WETH | 800000000000000000000000000 | 26325560 | 1618469709633296654 | 1619418089752996519 | 1620043618897783186 | 1620676971275751904 | -3.9079495124198664 | -3.9108706967886313 | -0.3383781495311276 |
| SHIB → WETH | 8000000000000000000000000000 | 26325561 | 15777881008710111971 | 15855581108721636807 | 15894279344844009586 | 15918392603516852302 | -15.148048721650056 | -15.151419800629839 | -2.8597754350752718 |

## Interpretation

Gain is (selected − baseline) / baseline × 10000, using integer adapter outputs for selection. Baseline includes the existing multi-hop/conversion Router. Candidate pools are capped and search is approximate. A budget stop retains the best complete plan found. In fixed mode, all local quotes use the recorded block; in ParaSwap comparison mode, each row pins its local variants to the reference-reported block when usable, otherwise the current head (or explicit --block override). Discovery remains a live filtered inventory. All used block hashes are verified at the end. RPC counts include actual HTTP attempts/retries; adapter calls are separate. Comparison row RPC includes setup, both local optimizers and optional diagnostic replay, not external HTTP. Candidate latency excludes those extra comparison operations; their total is recorded separately. Replay without non-allowlisted exchanges renormalizes remaining input weights and is a diagnostic reference plan, not the aggregator reported quote. Incomplete runs have completed=false; failures and unsupported replays stay visible. Gross outputs exclude gas, MEV and actual execution. Percentages are display-only; raw integer inputs are authoritative.
