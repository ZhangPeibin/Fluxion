# ADR-0007: eollo.xyz is the product domain

- **Status:** Accepted
- **Date:** 2026-11-25
- **Deciders:** Eollo maintainers

## Context

ADR-0006 renamed the product to Eollo and served the public site from the GitHub Pages project URL
`https://zhangpeibin.github.io/eollo/` until a product domain was acquired. `eollo.com` was premium
priced. `eollo.xyz` is now registered and the GitHub Pages custom domain is configured and verified.

The site is static, published by a GitHub Actions workflow, and also serves the checked-in JSON
schemas under `/schemas/`. Those schema `$id` values currently use the GitHub Pages host.

## Decision

Use `eollo.xyz` as the canonical product domain:

- English is the default entry at `https://eollo.xyz/`.
- Chinese is served at `https://eollo.xyz/zh.html`.
- `https://www.eollo.xyz/` redirects to the apex.
- Schemas are served at `https://eollo.xyz/schemas/...` and their `$id` values use that host.
- HTTPS is enforced on the custom domain.
- The old GitHub Pages URLs continue to work through GitHub's redirect to the custom domain.

GitHub Pages publishes this site from an Actions workflow, so the custom domain is a repository
setting; a `CNAME` file is ignored and is not added to the repository. DNS uses A records on the
apex plus a `www` CNAME to `zhangpeibin.github.io`.

## Consequences

- Schema `$id` values move from the GitHub Pages host to `https://eollo.xyz/schemas/...`; relative
  `$ref` values are unaffected.
- Site canonical and `hreflang` links use the new host, with `https://eollo.xyz/` as `x-default`.
- README, site docs, and the repository homepage point at the new domain.
- The deployed artifact must be rebuilt when `schemas/` changes, so the Pages workflow watches that
  path as well as `site/`.
- Future schema identifiers should use the product domain, not the hosting URL.

## Alternatives considered

- **Keep the GitHub Pages project URL.** Rejected: it is a hosting detail, not a product identity,
  and it weakens search and link stability.
- **Use `www.eollo.xyz` as canonical.** Rejected: the apex is shorter and GitHub redirects `www`
  automatically.
- **Serve Chinese at `/` and English at `/en.html`.** Rejected: the project's primary audience and
  documentation convention are English-first.
- **Host the schemas at a separate domain.** Rejected for now: one static origin keeps deployment and
  `$id` maintenance simple.

## References

- [ADR-0006](0006-rename-to-eollo.md)
- [Site README](../../site/README.md)
