# ADR-0004: Workflows are first-class compositions

- **Status:** Accepted
- **Date:** 2026-11-20
- **Deciders:** Fluxion maintainers

## Context

Agents can call individual capabilities, but they currently have to infer multi-step flows from
skills and prose. For example, completing a swap requires quote, build, allowance, simulation,
inspection, wallet confirmation, broadcast, and verification. The layer should describe these
compositions explicitly.

A workflow is not a new protocol and not a hidden execution path. It is an ordered composition of
capabilities with declared preconditions, checkpoints, and failure handling.

## Decision

Workflows are first-class catalog entries:

- A workflow declares `id`, `summary`, `status`, `chains`, `steps`, `planned_steps`, `skill`,
  `docs`, and `gaps`.
- `steps` reference capability ids that exist in the catalog.
- `planned_steps` reference intended capabilities that are not implemented yet; they must not be
  presented as available.
- Every workflow has a skill section or a dedicated skill and an end-to-end test.
- Workflows must preserve the security order: simulate and inspect before send; policy check before
  wallet confirmation; no hidden signing or broadcasting.
- The first workflows are `swap`, `yield-deposit`, `fourmeme-discovery`, and `portfolio`.

## Consequences

- The catalog validates workflow step references.
- `swap` remains experimental until `swap.build` covers V3/split and a verify step exists.
- `yield-deposit` composes existing `yield.build` with `transaction.*`.
- `fourmeme-discovery` is explicitly read-only.
- Workflow docs become the primary agent-facing explanation of multi-step use.

## Alternatives considered

- **No workflows; rely on skills prose.** Rejected: it leaves composition implicit and untestable.
- **One monolithic `swap.execute` capability.** Rejected as the only surface: it hides the steps and
  prevents reuse. A convenience wrapper may exist later, but the workflow remains explicit.
- **A general workflow DSL with a new runtime.** Rejected for now: the first version only needs
  catalog entries and skills.

## References

- [Workflows spec](../feature/v3/capability-runtime/workflows.md)
- [Catalog spec](../feature/v3/capability-runtime/catalog.md)
