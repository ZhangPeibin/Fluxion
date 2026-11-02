# Fixed-block two-pool split experiment

```json
{
  "block": 26325207,
  "block_hash": "0x6552f10c25fe99a05f776bc5e0dfac98907ee9af3d2de1002ffeda9b94aec40c",
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
    "token_allowlist": [],
    "top_k": 8,
    "uniswap_v2_subgraph_id": "EYCKATKGBKLWvSfwvBjzfCBmGwYNdVkduYXVivCsLRFu",
    "uniswap_v3_subgraph_id": "5zvR82QoaXYFyDEKLZ9t6v9adgnptxYpKpSbxtgVENFV"
  },
  "discovery_rpc_requests": 287,
  "edge_count": 432,
  "expected_rows": 6,
  "focus": "SHIB",
  "git_sha": "bed3642ee5e48d12ee996d7dd5f6e766416d3725",
  "pool_count": 210,
  "row_timeout_ms": 60000,
  "search_config": {
    "coarse_segments": 10,
    "concurrency": 8,
    "max_pools": 4,
    "max_quote_calls": 128,
    "refinement_rounds": 2,
    "refinement_segments": 4,
    "timeout_ms": 8000
  },
  "setup_rpc_requests": 39,
  "source_fingerprint_keccak256": "0x1952b0de58170f9854c887ee5b7d3d243aafac88d4c8bd7f8f80ca705280bb97",
  "timestamp_unix": 1793600024,
  "working_tree_dirty": true
}
```

Full pool inventory and corpus are in the sibling JSON report.

## Summary

| cohort | rows | success | split | baseline | median gain bps | mean gain bps | max gain bps | baseline p50/p95 ms | extra search p50/p95 ms | extra adapter calls | extra RPC |
|---|---|---|---|---|---|---|---|---|---|---|---|
| all | 6 | 6 | 6 | 0 | 8.032050581991246 | 29.018035263334443 | 107.23469947338289 | 1129/1746 | 1859/2368 | 543 | 266 |
| shib | 6 | 6 | 6 | 0 | 8.032050581991246 | 29.018035263334443 | 107.23469947338289 | 1129/1746 | 1859/2368 | 543 | 266 |
| steth | 0 | 0 | 0 | 0 | null | null | null | null/null | null/null | 0 | 0 |
| other | 0 | 0 | 0 | 0 | null | null | null | null/null | null/null | 0 | 0 |

## Per-row comparison

| pair | amount in | baseline out | selected out | kind | gain bps | search stop | baseline/search ms | extra quotes/RPC | allocations |
|---|---|---|---|---|---|---|---|---|---|
| WETH → SHIB | 333333333333333333 | 162697569490260873338699714 | 162718921501824292900896587 | split | 1.3124 | completed | 1746/2368 | 101/47 | 80.000% shibaswap-v2 + 20.000% uniswap-v2 |
| WETH → SHIB | 3333333333333333333 | 1618425591318847060037567840 | 1620101088046072130532258603 | split | 10.3526 | completed | 1129/2012 | 99/47 | 82.500% shibaswap-v2 + 17.500% uniswap-v2 |
| WETH → SHIB | 33333333333333333333 | 15376203681003950732941386791 | 15541089939081348977611573662 | split | 107.2347 | completed | 995/2121 | 109/57 | 82.500% shibaswap-v2 + 17.500% uniswap-v3 |
| SHIB → WETH | 80000000000000000000000000 | 162778867178814908 | 162801218150048808 | split | 1.3731 | completed | 1073/1780 | 78/37 | 70.000% shibaswap-v2 + 30.000% uniswap-v3 |
| SHIB → WETH | 800000000000000000000000000 | 1623584961459120415 | 1624512266731911307 | split | 5.7115 | completed | 1176/1859 | 78/38 | 80.000% shibaswap-v2 + 20.000% uniswap-v3 |
| SHIB → WETH | 8000000000000000000000000000 | 15827119495892400068 | 15903285844373993965 | split | 48.1239 | completed | 1160/1756 | 78/40 | 82.500% shibaswap-v2 + 17.500% uniswap-v2 |

## Interpretation

Gain is (selected − baseline) / baseline × 10000, using integer adapter outputs for selection. Baseline includes the existing multi-hop/conversion Router. Candidate pools are capped and the search is approximate, not globally optimal. A quote/time-budget stop retains the best complete plan found, not a claim that the search finished. All quotes use the recorded block; discovery is a live filtered inventory, saved in JSON. Block hash is verified at the end. RPC counts are actual HTTP attempts including retries; adapter calls are separately counted. Setup is reported separately; pinned V2/coarse caches are shared across rows, exact-amount memoization is request-local. Errors and whole-row timeouts are included as failed rows. Incomplete runs have completed=false. Gross outputs exclude gas, MEV, execution and external aggregators. No HTTP/Viewer contract changes. Percentages are display-only; raw integer branch inputs are authoritative.
