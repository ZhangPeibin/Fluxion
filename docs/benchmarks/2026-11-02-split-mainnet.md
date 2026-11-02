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
  "expected_rows": 90,
  "focus": null,
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
  "timestamp_unix": 1793600137,
  "working_tree_dirty": true
}
```

Full pool inventory and corpus are in the sibling JSON report.

## Summary

| cohort | rows | success | split | baseline | median gain bps | mean gain bps | max gain bps | baseline p50/p95 ms | extra search p50/p95 ms | extra adapter calls | extra RPC |
|---|---|---|---|---|---|---|---|---|---|---|---|
| all | 90 | 90 | 35 | 55 | 0.0 | 4.578051705178422 | 107.23469947338289 | 1315/1862 | 2171/2921 | 5152 | 3727 |
| shib | 6 | 6 | 6 | 0 | 8.032050581991246 | 29.018035263334443 | 107.23469947338289 | 1142/1279 | 1959/2633 | 543 | 266 |
| steth | 15 | 15 | 0 | 15 | 0.0 | 0.0 | 0.0 | 1326/1642 | 0/1247 | 160 | 80 |
| other | 69 | 69 | 29 | 40 | 0.0 | 3.448064375160164 | 88.5402011187744 | 1361/1925 | 2341/3272 | 4449 | 3381 |

## Per-row comparison

| pair | amount in | baseline out | selected out | kind | gain bps | search stop | baseline/search ms | extra quotes/RPC | allocations |
|---|---|---|---|---|---|---|---|---|---|
| WETH → USDC | 333333333333333333 | 762647297 | 762648297 | split | 0.0131 | completed | 2109/2587 | 92/62 | 75.000% uniswap-v3 + 25.000% uniswap-v3 |
| WETH → USDC | 3333333333333333333 | 7625718832 | 7626081407 | split | 0.4755 | completed | 1738/2844 | 96/76 | 55.000% uniswap-v3 + 45.000% uniswap-v3 |
| WETH → USDC | 33333333333333333333 | 76196049584 | 76222699533 | split | 3.4975 | completed | 1839/3445 | 100/84 | 60.000% uniswap-v3 + 40.000% uniswap-v3 |
| USDC → WETH | 1000000000 | 436973246566583849 | 436973246566583849 | baseline | 0.0000 | completed | 1973/2414 | 72/57 | 100.000% uniswap-v3 |
| USDC → WETH | 10000000000 | 4369049407424724971 | 4369049407424724971 | baseline | 0.0000 | completed | 1781/2382 | 72/57 | 100.000% uniswap-v3 |
| USDC → WETH | 100000000000 | 43609539284890293261 | 43646198779303218218 | split | 8.4063 | completed | 1719/2341 | 73/57 | 50.000% uniswap-v3 + 50.000% uniswap-v3 |
| WETH → USDT | 333333333333333333 | 763436389 | 763436389 | baseline | 0.0000 | completed | 1669/2887 | 92/75 | 100.000% curve |
| WETH → USDT | 3333333333333333333 | 7631770247 | 7632033336 | split | 0.3447 | completed | 1925/3295 | 102/82 | 87.500% uniswap-v3 + 12.500% uniswap-v3 |
| WETH → USDT | 33333333333333333333 | 76231692901 | 76250535238 | split | 2.4717 | completed | 1644/2796 | 98/69 | 67.500% uniswap-v3 + 32.500% uniswap-v3 |
| USDT → WETH | 1000000000 | 436419763414627705 | 436419763414627705 | baseline | 0.0000 | completed | 1541/2302 | 72/57 | 100.000% uniswap-v3 |
| USDT → WETH | 10000000000 | 4361236488328235289 | 4361803724852276474 | split | 1.3006 | completed | 1404/2648 | 79/62 | 77.500% uniswap-v3 + 22.500% uniswap-v3 |
| USDT → WETH | 100000000000 | 43505379531828153711 | 43520135658983924016 | split | 3.3918 | completed | 1435/2342 | 76/57 | 87.500% uniswap-v3 + 12.500% uniswap-v3 |
| WETH → DAI | 333333333333333333 | 762543436802109914332 | 762543436802109914332 | baseline | 0.0000 | completed | 1759/2921 | 103/69 | 100.000% uniswap-v3→curve |
| WETH → DAI | 3333333333333333333 | 7624522437966032785535 | 7624522437966032785535 | baseline | 0.0000 | completed | 1772/2919 | 103/69 | 100.000% uniswap-v3→curve |
| WETH → DAI | 33333333333333333333 | 76179474655589179791175 | 76179474655589179791175 | baseline | 0.0000 | completed | 1671/2674 | 96/65 | 100.000% uniswap-v3→uniswap-v3 |
| DAI → WETH | 1000000000000000000000 | 435973985898676870 | 435973985898676870 | baseline | 0.0000 | completed | 1369/2639 | 80/55 | 100.000% uniswap-v3 |
| DAI → WETH | 10000000000000000000000 | 4342089574596406304 | 4345810413948815310 | split | 8.5692 | completed | 1500/2341 | 81/58 | 77.500% uniswap-v3 + 22.500% uniswap-v3 |
| DAI → WETH | 100000000000000000000000 | 42669294308092366786 | 43047089098055834155 | split | 88.5402 | completed | 1365/2362 | 80/57 | 55.000% uniswap-v3 + 45.000% uniswap-v2 |
| WETH → WBTC | 333333333333333333 | 975007 | 975007 | baseline | 0.0000 | completed | 1544/4036 | 90/75 | 100.000% uniswap-v3 |
| WETH → WBTC | 3333333333333333333 | 9747486 | 9747815 | split | 0.3375 | completed | 1510/3272 | 107/85 | 85.000% uniswap-v3 + 15.000% uniswap-v3 |
| WETH → WBTC | 33333333333333333333 | 97439770 | 97444298 | split | 0.4647 | completed | 1484/2794 | 100/70 | 85.000% uniswap-v3 + 15.000% uniswap-v3 |
| WBTC → WETH | 1670000 | 570590736025700381 | 570594366190004975 | split | 0.0636 | completed | 2044/2267 | 76/57 | 85.000% uniswap-v3 + 15.000% uniswap-v3 |
| WBTC → WETH | 16700000 | 5704545634885452742 | 5704716982945374530 | split | 0.3004 | completed | 1570/2180 | 73/55 | 90.000% uniswap-v3 + 10.000% uniswap-v3 |
| WBTC → WETH | 167000000 | 57010302743448706330 | 57012040663219730140 | split | 0.3048 | completed | 1512/2511 | 81/60 | 96.250% uniswap-v3 + 3.750% uniswap-v3 |
| USDC → USDT | 1000000000 | 1000861567 | 1000861567 | baseline | 0.0000 | completed | 1327/2452 | 72/51 | 100.000% curve |
| USDC → USDT | 10000000000 | 10008611698 | 10008611698 | baseline | 0.0000 | completed | 1249/2184 | 74/55 | 100.000% curve |
| USDC → USDT | 100000000000 | 100085721591 | 100085721591 | baseline | 0.0000 | completed | 1182/2374 | 72/53 | 100.000% curve |
| USDT → USDC | 1000000000 | 999022505 | 999022505 | baseline | 0.0000 | completed | 1294/2250 | 72/57 | 100.000% uniswap-v3 |
| USDT → USDC | 10000000000 | 9990217781 | 9990217781 | baseline | 0.0000 | completed | 1568/2365 | 74/55 | 100.000% uniswap-v3 |
| USDT → USDC | 100000000000 | 99901450375 | 99901450375 | baseline | 0.0000 | completed | 1230/2220 | 75/54 | 100.000% uniswap-v3 |
| USDC → DAI | 1000000000 | 999863803753647327393 | 999863803753647327393 | baseline | 0.0000 | completed | 1598/2620 | 51/36 | 100.000% curve |
| USDC → DAI | 10000000000 | 9998633313624834664979 | 9998633313624834664979 | baseline | 0.0000 | completed | 1263/2049 | 51/36 | 100.000% curve |
| USDC → DAI | 100000000000 | 99985860780195041997423 | 99985860780195041997423 | baseline | 0.0000 | completed | 1167/2055 | 51/43 | 100.000% curve |
| DAI → USDC | 1000000000000000000000 | 999996715 | 999996715 | baseline | 0.0000 | completed | 1372/1775 | 51/36 | 100.000% uniswap-v3 |
| DAI → USDC | 10000000000000000000000 | 9999950938 | 9999950938 | baseline | 0.0000 | completed | 1141/1925 | 51/36 | 100.000% uniswap-v3 |
| DAI → USDC | 100000000000000000000000 | 99997888008 | 99997888008 | baseline | 0.0000 | completed | 1093/1883 | 51/36 | 100.000% uniswap-v3 |
| stETH → WETH | 333333333333333333 | 333214520743726338 | 333214520743726338 | baseline | 0.0000 | completed | 1607/1228 | 26/13 | 100.000% wsteth→uniswap-v3 |
| stETH → WETH | 3333333333333333333 | 3332143846260596080 | 3332143846260596080 | baseline | 0.0000 | completed | 845/1090 | 26/13 | 100.000% wsteth→uniswap-v3 |
| stETH → WETH | 33333333333333333333 | 33321302345550868744 | 33321302345550868744 | baseline | 0.0000 | completed | 963/1146 | 28/14 | 100.000% wsteth→uniswap-v3 |
| WETH → stETH | 333333333333333333 | 333385470916415025 | 333385470916415025 | baseline | 0.0000 | completed | 1339/1093 | 26/13 | 100.000% uniswap-v3→wsteth |
| WETH → stETH | 3333333333333333333 | 3333853346939855154 | 3333853346939855154 | baseline | 0.0000 | completed | 905/1118 | 28/14 | 100.000% uniswap-v3→wsteth |
| WETH → stETH | 33333333333333333333 | 33338397247581307976 | 33338397247581307976 | baseline | 0.0000 | completed | 862/1247 | 26/13 | 100.000% uniswap-v3→wsteth |
| wstETH → WETH | 285714285714285714 | 356497511069036653 | 356497511069036653 | baseline | 0.0000 | completed | 1095/2488 | 59/59 | 100.000% uniswap-v3 |
| wstETH → WETH | 2857142857142857142 | 3564973552646609890 | 3564973552646609890 | baseline | 0.0000 | completed | 1005/2518 | 61/61 | 100.000% uniswap-v3 |
| wstETH → WETH | 28571428571428571428 | 35649579722839453671 | 35649579722839453671 | baseline | 0.0000 | completed | 1091/2504 | 61/61 | 100.000% uniswap-v3 |
| WETH → wstETH | 333333333333333333 | 267095945043350295 | 267095945043350295 | baseline | 0.0000 | completed | 1131/2481 | 60/60 | 100.000% uniswap-v3 |
| WETH → wstETH | 3333333333333333333 | 2670958359070449661 | 2670958359070449661 | baseline | 0.0000 | completed | 1209/2531 | 60/60 | 100.000% uniswap-v3 |
| WETH → wstETH | 33333333333333333333 | 26709474454889692863 | 26709474454889692863 | baseline | 0.0000 | completed | 1318/2332 | 60/60 | 100.000% uniswap-v3 |
| stETH → USDC | 333333333333333333 | 762375464 | 762375464 | baseline | 0.0000 | no_pairs | 1326/0 | 0/0 | 100.000% wsteth→uniswap-v3→uniswap-v3 |
| stETH → USDC | 3333333333333333333 | 7622997887 | 7622997887 | baseline | 0.0000 | no_pairs | 1430/0 | 0/0 | 100.000% wsteth→uniswap-v3→uniswap-v3 |
| stETH → USDC | 33333333333333333333 | 76168569717 | 76168569717 | baseline | 0.0000 | no_pairs | 904/0 | 0/0 | 100.000% wsteth→uniswap-v3→uniswap-v3 |
| USDC → stETH | 1000000000 | 437041588584199851 | 437041588584199851 | baseline | 0.0000 | no_pairs | 1642/0 | 0/0 | 100.000% uniswap-v3→uniswap-v3→wsteth |
| USDC → stETH | 10000000000 | 4369730380547498114 | 4369730380547498114 | baseline | 0.0000 | no_pairs | 1315/0 | 0/0 | 100.000% uniswap-v3→uniswap-v3→wsteth |
| USDC → stETH | 100000000000 | 43616103287717257372 | 43616103287717257372 | baseline | 0.0000 | no_pairs | 1496/0 | 0/0 | 100.000% uniswap-v3→uniswap-v3→wsteth |
| stETH → LDO | 333333333333333333 | 2245438910667845989252 | 2245438910667845989252 | baseline | 0.0000 | no_pairs | 1388/0 | 0/0 | 100.000% wsteth→uniswap-v3→uniswap-v3 |
| stETH → LDO | 3333333333333333333 | 22271911196660573312222 | 22271911196660573312222 | baseline | 0.0000 | no_pairs | 1563/0 | 0/0 | 100.000% wsteth→uniswap-v3→uniswap-v3 |
| stETH → LDO | 33333333333333333333 | 207477969417910459293974 | 207477969417910459293974 | baseline | 0.0000 | no_pairs | 1152/0 | 0/0 | 100.000% wsteth→uniswap-v3→uniswap-v3 |
| LDO → WETH | 1600000000000000000000 | 235822553946911196 | 235859631257012010 | split | 1.5723 | completed | 1085/2299 | 53/53 | 82.500% uniswap-v3 + 17.500% uniswap-v3 |
| LDO → WETH | 16000000000000000000000 | 2345366665409291416 | 2347365274046929159 | split | 8.5215 | completed | 1013/2116 | 52/52 | 87.500% uniswap-v3 + 12.500% uniswap-v3 |
| LDO → WETH | 160000000000000000000000 | 22168919474170286330 | 22342769018202788234 | split | 78.4204 | completed | 899/2082 | 51/51 | 87.500% uniswap-v3 + 12.500% uniswap-v3 |
| WETH → PEPE | 333333333333333333 | 156353589676468667042911015 | 156353589676468667042911015 | baseline | 0.0000 | completed | 1099/2072 | 60/45 | 100.000% uniswap-v2 |
| WETH → PEPE | 3333333333333333333 | 1562857042514657517037095679 | 1562857042514657517037095679 | baseline | 0.0000 | completed | 1158/2041 | 60/45 | 100.000% uniswap-v2 |
| WETH → PEPE | 33333333333333333333 | 15561007817129363734231735635 | 15561514272851012320584862774 | split | 0.3255 | completed | 1066/2122 | 62/47 | 99.375% uniswap-v2 + 0.625% uniswap-v3 |
| PEPE → WETH | 125000000000000000000000000 | 264870475319209290 | 264898031861656420 | split | 1.0404 | completed | 978/1499 | 51/33 | 85.000% uniswap-v2 + 15.000% uniswap-v3 |
| PEPE → WETH | 1250000000000000000000000000 | 2647788066100598172 | 2647852864826961757 | split | 0.2447 | completed | 871/1843 | 51/35 | 97.500% uniswap-v2 + 2.500% uniswap-v3 |
| PEPE → WETH | 12500000000000000000000000000 | 26386559728904015780 | 26387820294124378443 | split | 0.4777 | completed | 914/1798 | 51/36 | 98.750% uniswap-v2 + 1.250% uniswap-v3 |
| WETH → LINK | 333333333333333333 | 67307028999344731327 | 67307028999344731327 | baseline | 0.0000 | completed | 1723/2278 | 60/45 | 100.000% uniswap-v3 |
| WETH → LINK | 3333333333333333333 | 670966395715411766145 | 671476481384986623659 | split | 7.6023 | completed | 1545/2101 | 62/45 | 55.000% uniswap-v3 + 45.000% uniswap-v3 |
| WETH → LINK | 33333333333333333333 | 6699694375102204831572 | 6701493834778011262265 | split | 2.6859 | completed | 1862/1848 | 64/44 | 92.500% uniswap-v3 + 7.500% uniswap-v3 |
| LINK → USDC | 66000000000000000000 | 746616904 | 746616904 | baseline | 0.0000 | no_pairs | 1254/0 | 0/0 | 100.000% uniswap-v3 |
| LINK → USDC | 666000000000000000000 | 7510918665 | 7510918665 | baseline | 0.0000 | no_pairs | 847/0 | 0/0 | 100.000% uniswap-v3 |
| LINK → USDC | 6666000000000000000000 | 72907430791 | 72907430791 | baseline | 0.0000 | no_pairs | 771/0 | 0/0 | 100.000% uniswap-v3 |
| WETH → UNI | 333333333333333333 | 105464151610903455213 | 105499637693874720480 | split | 3.3648 | completed | 1391/2171 | 100/52 | 67.500% uniswap-v3 + 32.500% uniswap-v3 |
| WETH → UNI | 3333333333333333333 | 1053331131701664442307 | 1053509269702373749886 | split | 1.6912 | completed | 1283/2493 | 101/66 | 92.500% uniswap-v3 + 7.500% uniswap-v3 |
| WETH → UNI | 33333333333333333333 | 10505549382019154718483 | 10506886292226859425274 | split | 1.2726 | completed | 1100/2693 | 98/68 | 95.000% uniswap-v3 + 5.000% uniswap-v2 |
| UNI → USDC | 140000000000000000000 | 1007867190 | 1007867190 | baseline | 0.0000 | no_pairs | 1072/0 | 0/0 | 100.000% uniswap-v2→uniswap-v3 |
| UNI → USDC | 1400000000000000000000 | 10067241431 | 10067241431 | baseline | 0.0000 | no_pairs | 1385/0 | 0/0 | 100.000% uniswap-v3→uniswap-v3 |
| UNI → USDC | 14000000000000000000000 | 100165087662 | 100165087662 | baseline | 0.0000 | no_pairs | 1093/0 | 0/0 | 100.000% uniswap-v3→uniswap-v3 |
| WETH → AAVE | 333333333333333333 | 5710303818449768015 | 5710303818449768015 | baseline | 0.0000 | completed | 1418/2455 | 90/60 | 100.000% uniswap-v3 |
| WETH → AAVE | 3333333333333333333 | 56986606096833682254 | 57020709288062410221 | split | 5.9844 | completed | 1473/2489 | 92/63 | 55.000% uniswap-v3 + 45.000% uniswap-v3 |
| WETH → AAVE | 33333333333333333333 | 568206426676557082237 | 568560481930320830651 | split | 6.2311 | completed | 1211/2476 | 91/62 | 85.000% uniswap-v3 + 15.000% uniswap-v3 |
| AAVE → USDC | 4000000000000000000 | 533462522 | 533462522 | baseline | 0.0000 | no_pairs | 1361/0 | 0/0 | 100.000% uniswap-v3→uniswap-v3 |
| AAVE → USDC | 40000000000000000000 | 5325692670 | 5325692670 | baseline | 0.0000 | no_pairs | 1027/0 | 0/0 | 100.000% uniswap-v3→uniswap-v3 |
| AAVE → USDC | 400000000000000000000 | 52385094778 | 52385094778 | baseline | 0.0000 | no_pairs | 1198/0 | 0/0 | 100.000% uniswap-v3→uniswap-v3 |
| WETH → SHIB | 333333333333333333 | 162697569490260873338699714 | 162718921501824292900896587 | split | 1.3124 | completed | 1069/2164 | 101/47 | 80.000% shibaswap-v2 + 20.000% uniswap-v2 |
| WETH → SHIB | 3333333333333333333 | 1618425591318847060037567840 | 1620101088046072130532258603 | split | 10.3526 | completed | 1156/2033 | 99/47 | 82.500% shibaswap-v2 + 17.500% uniswap-v2 |
| WETH → SHIB | 33333333333333333333 | 15376203681003950732941386791 | 15541089939081348977611573662 | split | 107.2347 | completed | 1099/2633 | 109/57 | 82.500% shibaswap-v2 + 17.500% uniswap-v3 |
| SHIB → WETH | 80000000000000000000000000 | 162778867178814908 | 162801218150048808 | split | 1.3731 | completed | 1142/1793 | 78/37 | 70.000% shibaswap-v2 + 30.000% uniswap-v3 |
| SHIB → WETH | 800000000000000000000000000 | 1623584961459120415 | 1624512266731911307 | split | 5.7115 | completed | 1279/1959 | 78/38 | 80.000% shibaswap-v2 + 20.000% uniswap-v3 |
| SHIB → WETH | 8000000000000000000000000000 | 15827119495892400068 | 15903285844373993965 | split | 48.1239 | completed | 1159/1886 | 78/40 | 82.500% shibaswap-v2 + 17.500% uniswap-v2 |

## Interpretation

Gain is (selected − baseline) / baseline × 10000, using integer adapter outputs for selection. Baseline includes the existing multi-hop/conversion Router. Candidate pools are capped and the search is approximate, not globally optimal. A quote/time-budget stop retains the best complete plan found, not a claim that the search finished. All quotes use the recorded block; discovery is a live filtered inventory, saved in JSON. Block hash is verified at the end. RPC counts are actual HTTP attempts including retries; adapter calls are separately counted. Setup is reported separately; pinned V2/coarse caches are shared across rows, exact-amount memoization is request-local. Errors and whole-row timeouts are included as failed rows. Incomplete runs have completed=false. Gross outputs exclude gas, MEV, execution and external aggregators. No HTTP/Viewer contract changes. Percentages are display-only; raw integer branch inputs are authoritative.
