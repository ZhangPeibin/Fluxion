# Four.meme token detail + local page - Spec

**Status:** implemented
**Release:** v2
**Capabilities:** `fourmeme.token`, `fourmeme.agent-tokens`

## Problem

`fourmeme.agent-tokens` can list tokens, but a user who sees a bonding token cannot click through to
inspect its name, phase, market data, creator, description, eligibility or external links. Agents
also lack a single-token read-only contract.

## Scope

- New `fourmeme.token` capability with optional wallet eligibility.
- Local read-only page: `fluxion fourmeme ui`.
- List cards link to `#/token/<address>` detail pages.
- Read-only. No wallet connection, signing, buying or selling.
- Embedded HTML/JS/CSS served by the client binary; no npm build.

## UX

```bash
fluxion fourmeme ui --open
fluxion fourmeme ui --port 8787 --lookback-blocks 100000 --wallet 0x...
```

- List: phase tabs, search, lookback, optional wallet, warnings.
- Detail: image, phase, template, raised, market cap, price, supply, creator, timestamps,
  description, eligibility, Four.meme/BscScan links.
- API detail failures fall back to `OpenFourCore.tokens(address)`, `vault.phase()` and ERC20
  name/symbol/supply; the error remains in warnings. No phase is invented.

## Data sources

- Four.meme `GET /private/token/get/v2?address=...`.
- BSC `AgentIdentifier.isAgent(address)` for bonding eligibility.
- `fourmeme.agent-tokens` BSC `TokenCreated` log scan for the list.

## Rules

- `FOURMEME-TOKEN-001`: `PUBLISH` / `PUBLISHED` -> `bonding`; `TRADE` -> `graduated`.
  On-chain vault `Created/Trading/SoldOut` -> bonding; `MigratePending/Migrated` -> graduated;
  `Terminal` or unreadable -> unknown.
- `FOURMEME-TOKEN-002`: template id `1778027615735` is the only Agent ID Exclusive preset;
  mismatch returns `agent_id_exclusive=false`.
- `FOURMEME-TOKEN-003`: wallet eligibility is `true | false | null`; null has an explicit reason.
- `FOURMEME-TOKEN-004`: the local UI binds only `127.0.0.1` and serves GET-only read routes.
- `FOURMEME-TOKEN-005`: external data is rendered with `textContent`; no remote HTML injection.
- `FOURMEME-TOKEN-006`: list cache is 30 s, detail cache 60 s; refresh bypasses both.

## Acceptance

- CLI/MCP detail works for a known token.
- Local page lists bonding/graduated tokens and click opens details.
- Wallet eligibility stays explicit.
- No execution path.
- fmt, clippy, workspace tests and live smoke.
