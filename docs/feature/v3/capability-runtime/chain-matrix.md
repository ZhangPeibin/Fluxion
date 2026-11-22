# Chain Matrix Spec

**Status:** draft  
**Release:** v3  
**Chains:** Ethereum (`eip155:1`), BSC (`eip155:56`)

## 1. Purpose

This spec defines how Fluxion declares and enforces chain support. Chain support is a capability
property, not a global mode and not an implicit default.

## 2. Supported chains

| Chain | Id | Native symbol | Default RPC env | Token registry |
|---|---|---|---|---|
| Ethereum | 1 | ETH | `FLUXION_RPC_URL` / `RPC_URL` | built-in Ethereum registry |
| BSC | 56 | BNB | `FLUXION_BSC_RPC_URL` | built-in BSC registry |

Ethereum and BSC are first-class. Base is not an implementation target in v3.

## 3. Capability matrix

| Capability | Ethereum | BSC | Notes |
|---|---|---|---|
| `swap.quote` | yes | no | Ethereum routing engine |
| `swap.build` | yes | no | V2 calldata |
| `wallet.balance` | yes | yes | BSC default |
| `wallet.assets` | yes | yes | BSC default; discovery depends on provider |
| `wallet.allowance` | yes | yes | BSC default |
| `wallet.collectibles` | yes | yes | BSC default; discovery depends on indexer |
| `wallet.connect` | yes | yes | BSC default |
| `wallet.status` | yes | yes | Reads local session |
| `wallet.disconnect` | yes | yes | Reads local session |
| `wallet.agent-id` | no | yes | BSC `.ai` Agent ID |
| `wallet.sign` | yes | yes | BSC default |
| `wallet.send` | yes | yes | BSC default |
| `transaction.simulate` | yes | yes | BSC default |
| `transaction.inspect` | yes | yes | BSC default |
| `transaction.send` | yes | yes | BSC default |
| `signature.inspect` | yes | yes | BSC default |
| `fourmeme.agent-tokens` | no | yes | Four.meme is BSC-only |
| `fourmeme.token` | no | yes | Four.meme is BSC-only |
| `yield.discover` | yes | no | Ethereum curated universe |
| `yield.build` | yes | no | Ethereum ABI encoding |
| `yield.history` | yes | no | Ethereum provider data |
| `yield.withdraw` | yes | no | Ethereum ABI encoding |
| `position.analyze` | yes | no | Ethereum yield positions |

## 4. Defaults

- There is no global default chain.
- Each capability may declare `default_chain` in the catalog.
- If `chain_required` is true, the caller must supply `chain`; no default is applied.
- The response always includes `chain`.
- CLI/MCP may apply the catalog default for ergonomics, but must show the resolved chain in output.

Recommended defaults:

| Capability group | Default |
|---|---|
| `swap.*` | Ethereum |
| `yield.*` | Ethereum |
| `position.analyze` | Ethereum |
| `fourmeme.*` | BSC |
| `wallet.*` | BSC (backward-compatible), explicit chain recommended |
| `transaction.*` | BSC (backward-compatible), explicit chain recommended |
| `signature.inspect` | BSC (backward-compatible), explicit chain recommended |

## 5. Unsupported chains

When a capability is called on an unsupported chain:

```json
{
  "meta": {"schema_version": 2, "capability": "swap.quote", "request_id": "..."},
  "chain": {"name": "bsc", "id": 56},
  "error": {
    "code": "UNSUPPORTED",
    "message": "swap.quote is not available on bsc",
    "retryable": false,
    "details": {"supported_chains": ["ethereum"]}
  }
}
```

Rules:

- Do not fall back to another chain.
- Do not silently ignore the requested chain.
- Do not return empty data with a success status.

## 6. Chain-specific configuration

| Concern | Ethereum | BSC |
|---|---|---|
| RPC | `FLUXION_RPC_URL` / `RPC_URL` | `FLUXION_BSC_RPC_URL` |
| Source verification | `ETHERSCAN_API_KEY` | `BSCSCAN_API_KEY` |
| Explorer | `etherscan.io` | `bscscan.com` |
| WalletConnect chain id | 1 | 56 |
| Native symbol | ETH | BNB |

Four.meme has an additional RPC override: `FOURMEME_BSC_RPC_URL`.

## 7. Adding a chain

To add a chain:

1. Add it to the chain abstraction and id mapping.
2. Add RPC configuration and token registry.
3. Add explorer and source-verification configuration.
4. Update each capability's `chains` and `default_chain` in the catalog.
5. Add chain-specific integration tests.
6. Update the chain matrix and capability docs.
7. Ensure unsupported-chain behavior still returns `UNSUPPORTED`.

## 8. Current gaps

These are explicit gaps, not errors:

- BSC swap: no BSC DEX discovery/adapters yet.
- BSC yield: no BSC curated yield universe yet.
- BSC position: no BSC position analyzer yet.

Until implemented, calls must return `UNSUPPORTED`. Do not add BSC to a capability's `chains` before
the adapter and tests exist.

## 9. References

- [ADR-0002](../../../adr/0002-ethereum-bsc-chain-matrix.md)
- [Catalog spec](catalog.md)
- [Envelope v2 spec](envelope-v2.md)
