# ADR-0001: Eollo is a composable Web3 capability runtime

- **Status:** Accepted
- **Date:** 2026-11-20
- **Deciders:** Eollo maintainers

## Context

Eollo started as an Ethereum swap-quote engine. It now exposes swap, wallet, transaction,
signature, yield, position, and Four.meme capabilities. The growth raised a legitimate question:
is the project drifting by adding Web3 domains, or is breadth the product?

The answer is that **breadth is intentional**. The product is the layer that lets an agent operate
across Web3 actions without rebuilding each integration. The drift is not the number of
capabilities; it is the absence of a platform contract that makes them one layer instead of a
collection of unrelated tools.

## Decision

Eollo is a **composable Web3 capability runtime**. A capability is an action-level contract, not a
protocol integration. Breadth is admitted only when the capability satisfies the admission checklist
in the lifecycle spec:

1. It serves a clear agent/user outcome.
2. It declares its domain, action, chains, execution location, and side effects.
3. It has a stable input/output schema and a v2 envelope.
4. It has a skill, capability doc, tests, and a catalog entry.
5. Capabilities that sign or broadcast have a threat model and a policy checkpoint.

The project will not optimize for capability count. It will optimize for **composability,
consistency, and verifiability**.

## Consequences

- A capability catalog and registry become mandatory infrastructure.
- Every new capability must pay the contract cost; ad hoc tools are rejected.
- Domain breadth is preserved, but capability semantics are standardized.
- The project roadmap is organized by workflows and domains, not by adding endpoints.

## Alternatives considered

- **Narrow to swap execution only.** Rejected: it discards working wallet, security, yield, and
  Four.meme capabilities and contradicts the stated layer goal.
- **Narrow to pre-sign security only.** Rejected: security is a differentiator and a guardrail, not
  the whole layer.
- **Keep adding capabilities without a catalog.** Rejected: this is the current drift.
- **Split every domain into its own repository.** Rejected for now: it increases coordination cost
  before the shared runtime contract exists.

## References

- [CONTEXT.md](../../CONTEXT.md)
- [Capability runtime spec](../feature/v3/capability-runtime/spec.md)
- [Lifecycle spec](../feature/v3/capability-runtime/lifecycle.md)
