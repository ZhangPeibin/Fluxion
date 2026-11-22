# Capability Envelope v2 Spec

**Status:** draft  
**Release:** v3  
**Applies to:** every capability in `capabilities/catalog.yaml`

## 1. Purpose

Envelope v2 gives every capability the same machine-readable result shape. An agent can inspect
`chain`, `provenance`, `coverage`, `warnings`, `unknowns`, and `error` without knowing which
capability produced the response.

## 1.1 Migration status

As of 2026-11-20, these capabilities use envelope v2: `wallet.balance`, `wallet.assets`,
`wallet.allowance`, `wallet.collectibles`, `fourmeme.agent-tokens`, `fourmeme.token`,
`transaction.simulate`, `transaction.inspect`, `signature.inspect`, `yield.discover`,
`yield.build`, `yield.history`, `yield.withdraw`, `position.analyze`, `swap.quote`, `swap.build`,
`wallet.connect`, `wallet.status`, `wallet.disconnect`, `wallet.sign`, `wallet.send`,
`wallet.agent-id`, and `transaction.send`. All 23 catalog capabilities now use envelope v2. Callers must read the catalog entry rather than assuming every
capability has migrated.

## 2. Success envelope

```json
{
  "meta": {
    "schema_version": 2,
    "capability": "swap.quote",
    "request_id": "req_01..."
  },
  "chain": {
    "name": "ethereum",
    "id": 1
  },
  "block": {
    "number": 26305057,
    "hash": "0x..."
  },
  "as_of": "2026-11-20T10:00:00Z",
  "provenance": {
    "sources": [
      {
        "kind": "rpc",
        "name": "ethereum-mainnet",
        "observed_at": "2026-11-20T10:00:00Z"
      }
    ],
    "freshness": "fresh",
    "age_secs": 3
  },
  "coverage": {
    "dimensions": {
      "data": "complete",
      "simulation": "not_run"
    }
  },
  "warnings": [],
  "unknowns": [],
  "data": {}
}
```

## 3. Error envelope

```json
{
  "meta": {
    "schema_version": 2,
    "capability": "swap.quote",
    "request_id": "req_01..."
  },
  "chain": {
    "name": "ethereum",
    "id": 1
  },
  "error": {
    "code": "UNSUPPORTED",
    "message": "swap.quote is not available on bsc",
    "retryable": false,
    "details": {
      "supported_chains": ["ethereum"]
    }
  }
}
```

Rules:

- `error` and `data` are mutually exclusive.
- `error.code` is a stable enum, not free text.
- `error.message` is human-readable and may change.
- `error.details` is optional but should carry actionable context.
- `retryable` tells the caller whether the same request may succeed later.

## 4. `meta`

| Field | Type | Required | Meaning |
|---|---|---|---|
| `schema_version` | integer | yes | `2` for this envelope. |
| `capability` | string | yes | Catalog id. |
| `request_id` | string | yes | Stable per request; echoed in logs. |

## 5. `chain`

| Field | Type | Required | Meaning |
|---|---|---|---|
| `name` | string | yes | `ethereum` or `bsc`. |
| `id` | integer | yes | `1` or `56`. |

Every success and error response must carry the chain. A capability that cannot determine a chain
must fail with `INVALID_ARGUMENT`, not omit the field.

## 6. `block`

| Field | Type | Required | Meaning |
|---|---|---|---|
| `number` | integer | yes | Pinned block number. |
| `hash` | string | yes | Block hash if available. |

Reads and simulations must pin a block. If a capability does not touch chain state, `block` is
omitted.

## 7. `as_of` and `provenance`

`as_of` is the time the result was observed. `provenance` explains where it came from.

```json
{
  "sources": [
    {"kind": "rpc", "name": "ethereum-mainnet", "observed_at": "..."},
    {"kind": "provider", "name": "defillama", "observed_at": "..."}
  ],
  "freshness": "fresh",
  "age_secs": 3
}
```

`freshness` values:

| Value | Meaning |
|---|---|
| `fresh` | Within the capability's freshness target. |
| `stale` | Older than the target but still usable with a warning. |
| `expired` | Too old; the capability must fail with `STALE_STATE`. |
| `unknown` | The capability cannot determine freshness. |

## 8. `coverage`

Coverage reports what actually ran.

```json
{
  "dimensions": {
    "data": "complete",
    "decode": "partial",
    "metadata": "complete",
    "simulation": "unavailable",
    "trace": "not_configured",
    "policy": "not_run",
    "freshness": "complete",
    "eligibility": "complete"
  }
}
```

Standard dimensions:

| Dimension | Meaning |
|---|---|
| `data` | Required chain/provider data was read. |
| `decode` | Calldata or typed data was decoded. |
| `metadata` | Contract, proxy, source, or token metadata was checked. |
| `simulation` | State simulation ran. |
| `trace` | Internal calls or delegatecall tracing ran. |
| `policy` | A caller policy was evaluated. |
| `freshness` | Freshness was determined. |
| `eligibility` | A wallet or token eligibility rule was checked. |

Statuses:

| Status | Meaning |
|---|---|
| `complete` | The dimension ran and produced a usable result. |
| `partial` | Some checks ran; the result is incomplete. |
| `not_run` | The caller did not request or trigger the dimension. |
| `unavailable` | The dimension was requested but the backend could not provide it. |
| `not_configured` | The capability has no provider or rule configured. |

Rules:

- A missing dimension is never treated as a pass.
- `unknowns` must explain why coverage is partial, unavailable, or not configured.
- Capabilities declare their supported dimensions in the catalog.

## 9. `warnings` and `unknowns`

Both are arrays of structured entries:

```json
{
  "code": "FOURMEME-API-UNAVAILABLE",
  "message": "Four.meme API detail unavailable; on-chain fields returned.",
  "source": "provider",
  "details": {}
}
```

| Field | Required | Meaning |
|---|---|---|
| `code` | yes | Stable code for the warning or unknown. |
| `message` | yes | Human-readable explanation. |
| `source` | no | `rpc`, `provider`, `wallet`, `registry`, or `capability`. |
| `details` | no | Structured context. |

Warnings are non-blocking. Unknowns are material gaps in the result.

## 10. `data`

`data` is capability-specific but must not duplicate common envelope fields. For example,
`swap.quote` puts the quote payload under `data`; it does not add a second `chain` or `block`.

## 11. Error codes

The stable set is defined in `crates/protocol/src/capability/error.rs`:

`INVALID_ARGUMENT`, `NO_PATH`, `INSUFFICIENT_LIQUIDITY`, `AMOUNT_OUT_BELOW_MIN`, `RPC_ERROR`,
`STALE_STATE`, `STATE_INCOMPLETE`, `UNSUPPORTED`, `USER_REJECTED`, `SIMULATION_FAILED`,
`POLICY_BLOCKED`, `TIMEOUT`, `INTERNAL`.

New capabilities must reuse these codes unless a new code is justified and added to the shared
enum.

## 12. Transport mapping

### MCP

- Success: `content` contains a text block with the envelope JSON; `isError` is `false`.
  Where the client supports it, `structuredContent` carries the same object.
- Error: `content` contains a text block with the error envelope JSON; `isError` is `true`.
- QR and other human artifacts may add extra content blocks, but the envelope remains the canonical
  text block.

### CLI

- `--json` prints the envelope.
- Human output is a rendering of the envelope, not a separate data model.

### HTTP

- The response body is the envelope.
- Status codes follow the existing API mapping; the envelope is always present for capability
  responses.

## 13. Versioning

- `meta.schema_version` is `2`.
- v1 schemas are deprecated for one release and then removed.
- The runtime does not keep two response code paths.
- Any future breaking change increments `schema_version` and adds a migration spec.

## 14. Acceptance

- Every capability returns the success or error envelope.
- Every capability declares its coverage dimensions.
- Every `warnings` and `unknowns` entry has a code.
- No capability returns a bare domain payload without `meta` and `chain`.
- MCP, CLI, and HTTP expose the same envelope.

## 15. References

- [Master spec](spec.md)
- [Catalog spec](catalog.md)
- [Migration spec](migration.md)
