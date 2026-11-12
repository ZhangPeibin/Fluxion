# signature.inspect

Deterministic pre-sign inspection for authorization that is not ordinary calldata. Local, advisory,
BSC by default, Ethereum explicit `--chain ethereum`. It never signs or broadcasts and never claims
safety.

## Inputs

Exactly one of:

- EIP-712 typed data (`typed_data`)
- EIP-7702 authorization list (`authorization_list`)
- ERC-4337 user operation (`user_operation`)

```bash
fluxion signature inspect --typed-data permit.json --json
fluxion signature inspect --authorization-list auth.json --chain ethereum --json
fluxion signature inspect --user-operation userop.json --json
fluxion signature inspect --typed-data permit.json --policy policy.json --json
```

## Typed Data

Checks the EIP-712 domain chain against the selected chain and inspects known Permit2 primary types.

```json
{
  "domain": {"name": "Permit2", "chainId": 56, "verifyingContract": "0x..."},
  "primaryType": "PermitSingle",
  "message": {
    "spender": "0x...",
    "sigDeadline": "9999999999",
    "details": {
      "token": "0x...",
      "amount": "1461501637330902918203684832716283019655932542975",
      "expiration": "0",
      "nonce": 0
    }
  }
}
```

Findings:

- `SIG-001`: domain chain ID differs from selected chain.
- `SIG-002`: permit expiration/deadline is zero or in the past.
- `SIG-003`: Permit2 amount is `uint160::MAX` or spender is zero.
- `SIG-006`: typed data cannot be decoded.

Unknown primary types are reported as `unknown`; only the domain check is applied.

## EIP-7702

Any non-empty authorization list is `SIG-004` high because it can delegate account code. Chain
mismatch and zero delegation address are also high.

## ERC-4337

- `SIG-005`: paymaster or account-factory deployment is present; verify the policy and trust.
- `SIG-006`: missing/invalid sender.
- SimpleAccount `execute(address,uint256,bytes)` callData is decoded into `calls[]`; nested
  approval/address/swap/permit checks reuse the transaction decoders.
- Unsupported userOp callData selectors remain explicit unknowns.
- The userOp signature is explicitly reported as not inspected.

## Output

See [`schemas/signature_inspect.v1.json`](../../../schemas/signature_inspect.v1.json).

- `kind`: `typed_data`, `authorization_list`, or `user_operation`
- `domain`: normalized EIP-712 domain when present
- `calls`: decoded nested calls for supported SimpleAccount `execute` userOps
- `findings`: rule ID, severity, evidence and confidence
- `risk`: highest deterministic finding, otherwise `unknown`; never `safe`
- `coverage`: `complete`, `partial`, `unavailable` or `not_configured`
- `unknowns`: authorization that was not inspected
- `policy`: policy `mode`, `decision` (`allow`/`warn`/`block`), `blocked`, and reasons
- `assurance`: ruleset version, stable report ID, deterministic flag, limitations
- `explanation`: deterministic summary for the host agent

## Policy

Optional `--policy` / MCP `policy` reuses the same policy shape as `transaction.inspect`. Supported
signature checks are chain, verifying-contract allowlist, and spender allowlist. `mode=enforce`
blocks high-risk signature findings; `advisory` returns `decision: warn`.

`wallet.sign --policy FILE` applies the same gate before the phone request. Policy on a plain message
signature is rejected because there is no structured authorization to evaluate.

## Boundaries

- Advisory only; no signing, private keys or broadcast.
- Not a substitute for wallet confirmation or audit.
- Unknown typed-data types, userOp calldata and signatures remain explicitly unknown.
- A successful inspection is not an execution guarantee.
