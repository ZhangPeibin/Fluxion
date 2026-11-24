# ADR-0002: Ethereum and BSC are declared, first-class chains

- **Status:** Accepted
- **Date:** 2026-11-20
- **Deciders:** Eollo maintainers

## Context

The codebase currently mixes three chain assumptions:

- `swap.quote`, `yield.*`, and `position.analyze` are Ethereum-only.
- `wallet.*`, `transaction.*`, and `signature.inspect` default to BSC and also accept Ethereum.
- `fourmeme.*` is BSC-only.
- The whitepaper still names Base as the initial chain.

This produces inconsistent defaults, documentation contradictions, and an agent experience where
the same layer behaves differently per tool without a declared contract.

## Decision

Ethereum and BSC are both first-class chains **at the layer level**, with a **declarative
capability matrix**:

- Every capability declares `chains` and, when applicable, `default_chain`.
- The runtime must never silently fall back to another chain.
- An unsupported chain returns `UNSUPPORTED` with `details.supported_chains`.
- Every response declares the chain that produced it.
- Chain-specific capabilities are allowed: `fourmeme.*` is BSC-only by design; `yield.*` and
  `position.analyze` are Ethereum-only until adapters exist.
- Gaps are explicit in the catalog and in `UNSUPPORTED` responses, not hidden in defaults.

The first supported chain ids are Ethereum (`eip155:1`) and BSC (`eip155:56`). Base is no longer the
implementation target; the whitepaper must be updated to match this decision.

## Consequences

- The global `DEFAULT_CHAIN = Bsc` is removed.
- Chain defaults move into the capability catalog and are documented per capability.
- BSC swap, BSC yield, and BSC position are tracked gaps, not implicit promises.
- Adding a chain requires updating the chain abstraction, RPC configuration, token registry,
  catalog matrix, tests, and docs.

## Alternatives considered

- **Ethereum-only.** Rejected: it drops the BSC wallet, security, and Four.meme capabilities.
- **BSC-only.** Rejected: it drops the Ethereum swap, yield, and position engine.
- **Full parity immediately.** Rejected for the first slice: it requires new BSC DEX and yield
  adapters before the layer contract is stable.
- **Keep implicit defaults.** Rejected: this is the current source of drift.

## References

- [Chain matrix spec](../feature/v3/capability-runtime/chain-matrix.md)
- [Catalog spec](../feature/v3/capability-runtime/catalog.md)
- [CONTEXT.md](../../CONTEXT.md)
