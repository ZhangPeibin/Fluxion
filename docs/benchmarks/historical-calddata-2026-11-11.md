# Historical BSC calldata replay (11 Nov 2026)

This benchmark replays **real historical BSC transaction calldata** through
`transaction.inspect`. It is a calldata-level replay at the current block, not a historical-state
fork, and it is not a recall claim.

## Corpus

`tests/security/fixtures/historical.json`

| case | source | expected |
|---|---|---|
| unlimited BUSD approval, 2022-07-09 | BscScan tx `0x3bfc8e4c...2cf4f` | `AUTH-001` |
| unlimited BUSD approval, 2022-09-28 | BscScan tx `0xdbbb008e...b60f8` | `AUTH-001` |
| Orion exploit entrypoint, 2024-05-28 | Verichains incident analysis | opaque selector, `decode=partial` |
| NFTG exploit entrypoint, 2024-11 | BscScan tx `0xbd330fd1...28f6` | opaque selector, `decode=partial` |

The two approval transactions are real phishing approvals with `approve(address,uint256)` and
`uint256::MAX`. The two exploit entrypoints are real attack transactions but use opaque flash-loan
attack contracts; the inspector correctly leaves them `unknown` rather than guessing.

## Results

| id | mode | selector | risk | decode | expected | actual | result |
|---|---|---|---|---|---|---|---|
| phishing-approval-2022-07-09 | detect | `0x095ea7b3` | high | complete | AUTH-001 | AUTH-001 | pass |
| phishing-approval-2022-09-28 | detect | `0x095ea7b3` | high | complete | AUTH-001 | AUTH-001 | pass |
| orion-exploit-2024-05-28 | coverage_gap | `0xd79eee59` | unknown | partial | none | none | pass |
| nftg-exploit-2024-11 | coverage_gap | `0x1a695230` | unknown | partial | none | none | pass |

Interpretation:

- Known malicious approval calldata is detected on real historical transactions.
- Opaque exploit entrypoints are not falsely labeled; they remain `unknown` with partial decode.
- A real exploit transaction is not the same as a malicious *approval* transaction. The inspector is
  a pre-sign calldata checker, not a protocol exploit detector.

## Reproduce

```bash
python3 bench/security/historical.py --out /tmp/historical.md --json-out /tmp/historical.json
```

This uses the public BSC RPC only to fetch transaction calldata and current code. It does not replay
historical state, simulate the original exploit, or claim mainnet recall.
