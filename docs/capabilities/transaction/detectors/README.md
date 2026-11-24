# Detector Sub-Specs

| Spec | Purpose | Depends on |
|---|---|---|
| [core](core.md) | capability contract, report, block pinning, coverage | - |
| [decode](decode.md) | selector, ABI, signature and typed arguments | core |
| [metadata](metadata.md) | contract/EOA, verification, proxy and implementation | core |
| [effects](effects.md) | native/token/NFT/allowance state deltas | core, decode |
| [approval](approval.md) | ERC20 and NFT approval risk | decode, effects |
| [asset-flow](asset-flow.md) | unexpected asset outflow and recipient mismatch | decode, effects |
| [contract](contract.md) | unverified/new/proxy/upgrade/delegatecall risk | metadata |
| [batch](batch.md) | multicall, Safe batches and nested calls | decode, effects |
| [address](address.md) | zero/burn/self/session/allowlist checks | core, decode |
| [swap](swap.md) | router, minOut, deadline, recipient, price impact | decode, effects |
| [signature](signature.md) | Permit2, EIP-2612, EIP-7702, 4337 | core |
| [policy](policy.md) | optional caps and allowlists | effects and findings |
| [explain](explain.md) | optional plain-language explanation | report + findings |
| [eval](eval.md) | fixtures, false-positive/false-negative measurement | all |

Rule IDs are append-only. A new meaning requires a new ID.
