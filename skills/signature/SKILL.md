---
name: signature
description: Use when the user wants to inspect EIP-712 typed data, a Permit2 signature, an EIP-7702 authorization list or an ERC-4337 user operation before signing. Advisory only; never signs, broadcasts or asks for a private key.
---

# signature.inspect

Inspect non-calldata authorization before the user signs. Default chain is BSC; pass
`chain: "ethereum"` for Ethereum.

- Human docs: [`docs/capabilities/signature`](../../docs/capabilities/signature/README.md)
- Schema: [`signature_inspect`](../../schemas/signature_inspect.v1.json)

## MCP tool

Exactly one input:

```json
{"name": "signature_inspect", "arguments": {"chain": "bsc", "typed_data": {}}}
```

```json
{"name": "signature_inspect", "arguments": {"chain": "bsc", "authorization_list": []}}
```

```json
{"name": "signature_inspect", "arguments": {"chain": "bsc", "user_operation": {}}}
```

## CLI

```bash
fluxion signature inspect --typed-data permit.json --json
fluxion signature inspect --authorization-list auth.json --json
fluxion signature inspect --user-operation userop.json --json
```

## Reading the result

- `SIG-001`: typed-data domain chain mismatch.
- `SIG-002`: permit deadline/expiration is zero or expired.
- `SIG-003`: unlimited Permit2 amount or zero spender.
- `SIG-004`: EIP-7702 authorization present, wrong chain, or zero address.
- `SIG-005`: ERC-4337 paymaster or account factory present.
- `SIG-006`: authorization cannot be decoded or sender is invalid.
- `risk`: highest deterministic finding, otherwise `unknown`; never treat `low` as approval.
- `unknowns`: callData/signature or unsupported typed-data semantics that were not inspected.
- `explanation`: deterministic summary; the host agent may paraphrase it.

## Boundaries

- No signing, no private keys, no broadcast.
- Unknown typed-data types and userOp calldata remain unknown.
- A successful inspection is not a guarantee of safety.
