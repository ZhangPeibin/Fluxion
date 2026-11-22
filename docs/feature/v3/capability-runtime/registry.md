# Capability Registry Spec

**Status:** draft  
**Release:** v3

## 1. Purpose

The registry is the runtime counterpart of the catalog. It maps a capability id to its metadata and
handler, and gives MCP, CLI, and HTTP one dispatch path.

The registry prevents new capabilities from being added by editing several unrelated match
statements.

## 2. Components

| Component | Responsibility |
|---|---|
| `CapabilityCatalog` | Loads and validates `capabilities/catalog.yaml`. |
| `CapabilityMeta` | Typed view of a catalog entry. |
| `CapabilityRegistry` | Owns metadata and handlers; resolves ids. |
| `CapabilityHandler` | Executes one capability for a typed request. |
| `CapabilityContext` | Chain, block, wallet session, policy, RPC, and request id. |
| `CapabilityEnvelope` | Common success/error response. |

## 3. Metadata

`CapabilityMeta` must expose at least:

```rust
pub struct CapabilityMeta {
    pub id: CapabilityId,
    pub domain: Domain,
    pub action: Action,
    pub status: CapabilityStatus,
    pub execution: Execution,
    pub transports: Vec<Transport>,
    pub chains: Vec<Chain>,
    pub default_chain: Option<Chain>,
    pub chain_required: bool,
    pub side_effects: SideEffect,
    pub requires: Vec<Requirement>,
    pub coverage_dimensions: Vec<CoverageDimension>,
}
```

`execution` is internal. It must not be exposed as an agent-selectable parameter.

## 4. Dispatch

```text
MCP tools/call ─┐
CLI command ────┼──▶ CapabilityRegistry::dispatch(id, request, ctx)
HTTP route ─────┘
```

Dispatch steps:

1. Resolve the capability id.
2. Validate transport and status (`proposed` is not callable).
3. Resolve the chain and reject unsupported chains with `UNSUPPORTED`.
4. Validate request schema.
5. Run the handler.
6. Wrap the result in envelope v2.
7. Map errors to the common error envelope.

## 5. Handler contract

A handler:

- receives a typed request and `CapabilityContext`;
- performs one capability, not a workflow;
- does not choose an execution location;
- does not sign or broadcast without the side-effect policy;
- returns a payload or a `CapabilityError`;
- does not format CLI text or MCP content.

Adapters own presentation. Handlers own domain behavior.

## 6. Adapters

### MCP

- `tools/list` is generated or validated from the catalog.
- `tools/call` resolves the capability and dispatches.
- The envelope is serialized into the text content block.
- `isError` follows the envelope error.

### CLI

- Command routing resolves a capability id and dispatches.
- `--json` prints the envelope.
- Human output is a renderer over the envelope.

### HTTP

- Routes map to capability ids.
- The response body is the envelope.
- Status codes follow the API mapping, not the capability id.

## 7. Side effects and policy

| Side effect | Required checks |
|---|---|
| `read_only` | None |
| `local_session` | Wallet session operation; no chain transaction |
| `prepares_calldata` | Return calldata and approvals; do not sign |
| `signs` | Wallet session + explicit confirmation + optional policy |
| `broadcasts` | Simulation + inspection + policy + explicit confirmation |

The registry must reject a `signs` or `broadcasts` handler that lacks a wallet session.

## 8. Error handling

- Handlers return `CapabilityError`; adapters never parse error strings.
- Unsupported chain errors include `supported_chains`.
- Policy blocks use `POLICY_BLOCKED`.
- User rejection uses `USER_REJECTED`.
- Timeouts use `TIMEOUT`.
- Internal failures use `INTERNAL` and must not leak secrets.

## 9. Testing

Required tests:

- Catalog parity: `CapabilityId::all()` equals the catalog id set.
- MCP parity: every `mcp` capability appears in `tools/list`; no extra tools.
- CLI parity: every `cli` capability has a command.
- Schema parity: every capability has a schema file.
- Skill/docs parity: every capability has both.
- Chain parity: unsupported chains return `UNSUPPORTED`.
- Envelope parity: success and error responses include `meta` and `chain`.

## 10. Adding a capability

1. Add the catalog entry.
2. Add the schema, skill, doc, and tests.
3. Implement the handler.
4. Register the handler.
5. Add workflow membership if applicable.
6. Run parity tests.
7. Update the capability index.

No capability is callable until all steps are complete.

## 11. Non-goals

- A general-purpose plugin system.
- Dynamic loading of third-party handlers.
- A workflow DSL.
- Exposing execution location to agents.

## 12. References

- [Catalog spec](catalog.md)
- [Envelope v2 spec](envelope-v2.md)
- [Chain matrix spec](chain-matrix.md)
- [Lifecycle spec](lifecycle.md)
