# Security inspection benchmark (11 Nov 2026)

Scope: local `transaction.inspect` and `signature.inspect`. This is a regression benchmark for
deterministic detectors and CLI latency. It is **not** an independent security evaluation and does
not claim a real-world false-negative rate.

## Environment

| item | value |
|---|---|
| machine | Apple M2 |
| OS | macOS 27.0.1 |
| Python | 3.14.8 |
| Rust | 1.99.1 |
| transaction RPC | `https://bsc-dataseed.bnbchain.org` |
| signature RPC | none |

## Correctness corpus

Synthetic fixtures live in:

- `crates/client/tests/fixtures/transaction_inspect/cases.json` (10 cases)
- `crates/client/tests/fixtures/signature_inspect/cases.json` (6 cases)

The harness is offline and runs in `cargo test --workspace`.

| group | cases | expected findings | result |
|---|---:|---|---|
| transaction controls | 4 | native transfer, normal transfer, unknown selector: no finding | 4/4 |
| transaction positives | 6 | `AUTH-001`, `AUTH-004`, `SIG-002`, `SIG-003`, `ADDR-001`, `ADDR-002`, multicall `AUTH-001`, `SWAP-001` | 6/6 |
| signature controls | 1 | unknown typed data: no finding | 1/1 |
| signature positives | 5 | `SIG-001`, `SIG-002`, `SIG-003`, `SIG-004`, `SIG-005` | 5/5 |

**Result: 16/16 synthetic fixtures matched expected findings and risk.** There were 0 unexpected
findings on the four explicit controls. This is regression evidence, not proof of real-world recall.

## End-to-end latency

`bench/security/bench.py` runs the installed CLI 20 times per case. Transaction cases include
process startup, BSC RPC block pinning, `eth_simulateV1`, and detector evaluation. Signature cases
are local JSON inspection with no RPC.

| case | kind | risk | findings | p50 | p95 | min | max |
|---|---|---|---:|---:|---:|---:|---:|
| native transfer | transaction | unknown | none | 1136 ms | 1762 ms | 1027 ms | 2179 ms |
| unlimited approve | transaction | high | AUTH-001 | 1370 ms | 2651 ms | 1161 ms | 3917 ms |
| permit max + expired | transaction | high | SIG-002, SIG-003 | 1155 ms | 1455 ms | 1114 ms | 1456 ms |
| swap zero minOut | transaction | high | SWAP-001 | 1231 ms | 1325 ms | 1168 ms | 1446 ms |
| unknown selector | transaction | unknown | none | 1160 ms | 1281 ms | 1124 ms | 1288 ms |
| typed-data permit2 | signature | high | SIG-002, SIG-003 | 4 ms | 12 ms | 4 ms | 23 ms |
| eip-7702 authorization | signature | high | SIG-004 | 4 ms | 4 ms | 4 ms | 5 ms |

Reproduce:

```bash
python3 bench/security/bench.py --iterations 20
```

The transaction path is dominated by public-RPC round-trips and `eth_simulateV1`, not by detector
logic. A persistent local RPC, reused block tag, or warmed simulation backend should reduce p50 and
especially p95. `signature.inspect` is local and sub-10 ms at p95 in this run.

## Rule coverage

| category | rules |
|---|---|
| calldata decode | known selectors, static args, dynamic bytes, multicall |
| target metadata | EOA/contract, EIP-1967 implementation |
| effects | native value, ERC20/721 logs, approvals |
| approval | `AUTH-001`, `AUTH-004` |
| asset flow | `FLOW-001`–`FLOW-006` |
| contract | `CONTRACT-001`, `CONTRACT-003` |
| batch | multicall expansion, nested approval |
| address | `ADDR-001`, `ADDR-002`, `ADDR-005` |
| swap | `SWAP-001`, `SWAP-004`, `SWAP-007` |
| signature | calldata `SIG-002`, `SIG-003`; typed-data `SIG-001`–`SIG-006` |
| policy | `POLICY-001`–`POLICY-006` |

## Limitations

- The corpus is synthetic and intentionally small. It is not a labeled mainnet exploit corpus.
- Effects come from `eth_simulateV1` logs plus balance/allowance verification; unusual tokens can still degrade to unknown.
- Trace coverage depends on `debug_traceCall`; public BSC RPC does not expose it.
- User-operation `callData` and signatures are explicitly not decoded.
- Unknown typed-data primary types and unknown selectors remain `unknown`; they are not risk scores.
- No LLM explanation was benchmarked; explanation is deterministic and local.
- Public RPC latency is noisy; these numbers are not an SLA.

## Red-team follow-up

The local Foundry/Anvil adversarial corpus is in
[redteam-2026-11-11.md](redteam-2026-11-11.md). It adds 14 detect cases, 2 controls and one
explicit known gap. Remediation added balance/allowance verification (`EFFECT-007/009/010`) and
internal-call tracing (`CONTRACT-004`); use the red-team report for post-verification local timings.

## Decision

Keep the deterministic detector path. Do not market the report as a safety guarantee. Before any
“safe” or external security claim, build an independent malicious/benign corpus and measure false
positives and false negatives per rule.
