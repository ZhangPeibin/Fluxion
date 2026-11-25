# ADR-0006: Rename Fluxion to Eollo

- **Status:** Accepted
- **Date:** 2026-11-24
- **Deciders:** Eollo maintainers
- **Domain note:** the GitHub Pages domain choice is superseded by
  [ADR-0007](0007-eollo-xyz-domain.md) (2026-11-25).

## Context

The project was named Fluxion. In the same category there are already at least two DEXes using that
name: Fluxion Network on Mantle and Fluxion on Robinhood Chain. Sharing the name with a swap venue
harms search, trust, and trademark position for a capability layer that agents and wallets embed.

`eollo.com` was registered and listed as a premium domain, so it was not a viable purchase. No
product domain has been purchased yet. Until one is acquired, the public site uses the GitHub Pages
project URL.

The project is pre-1.0 and has no production users. A clean break is cheaper now than maintaining
aliases forever.

## Decision

Rename the product to **Eollo** with one canonical spelling for every surface:

- Product and prose: `Eollo`
- Public site: `https://zhangpeibin.github.io/eollo/` until a product domain is acquired
- CLI and binary: `eollo`
- Crates: `eollo`, `eollo-protocol`, `eollo-remote`, `eollo-client`
- Environment variables: `EOLLO_*`
- Local state: `~/.eollo`
- GitHub repository: `ZhangPeibin/eollo`
- MCP server key in examples: `eollo`

This is a deliberate clean break:

- No `fluxion` binary alias and no `FLUXION_*` fallback.
- No compatibility shim for `~/.fluxion`.
- Schema `$id` values move to `https://zhangpeibin.github.io/eollo/schemas/...`.
- Git history is not rewritten; old commits keep the old name.
- ADR content is updated for the current brand only; historical decisions are unchanged.

## Consequences

- Pre-1.0 users must update the binary name, environment variables, MCP config, and local state
  directory.
- README, capability docs, skills, site copy, schemas, badges, and Pages URLs change together.
- The site and schema `$id` values use the GitHub Pages project URL until a product domain is
  purchased; switching later is a small follow-up.
- Search results and external links for the old name are intentionally not preserved.

## Alternatives considered

- **Keep Fluxion.** Rejected: direct collision with established DEXes in the same category.
- **Buy `fluxion.com`.** Rejected: unavailable or premium-priced, and the name collision remains.
- **Dual brand or one-release aliases.** Rejected: the project is pre-1.0 and aliases would keep the
  old name in the codebase indefinitely.
