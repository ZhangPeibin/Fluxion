---
name: fourmeme
description: Use when the user asks which Four.meme tokens are in Binance Agent ID Exclusive mode, wants a single token's detail, wonders whether a wallet can trade during the bonding curve, or uses the fourmeme_agent_tokens / fourmeme_token tools. Read-only; never buys, sells or signs.
---

# Four.meme Agent ID Exclusive

Read-only discovery, detail and optional wallet eligibility for Four.meme **Binance Agent ID
Exclusive** tokens on BSC.

- Human docs: [`docs/capabilities/fourmeme`](../../docs/capabilities/fourmeme/README.md)
- Schemas: [`agent-tokens`](../../schemas/fourmeme_agent_tokens.v2.json) ·
  [`token`](../../schemas/fourmeme_token.v2.json)

## MCP

```json
{"name": "fourmeme_agent_tokens", "arguments": {"wallet": "0x...", "limit": 20}}
{"name": "fourmeme_token", "arguments": {"address": "0x...", "wallet": "0x..."}}
```

## CLI

```bash
fluxion fourmeme agent-tokens --json
fluxion fourmeme agent-tokens --wallet 0x... --limit 20 --json
fluxion fourmeme token 0x... --wallet 0x... --json
fluxion fourmeme ui --open
```

## Reading the result

Both capabilities use envelope v2:

- `meta`: `schema_version`, `capability`, `request_id`.
- `chain`: `{name: "bsc", id: 56}`.
- `provenance` / `coverage` / `warnings` / `unknowns`: completeness and provenance.
- `data`: discovery or token detail payload.

`fourmeme.agent-tokens` data:

- `mode`, `scanned`, `lookback_blocks`, `from_block`, `to_block`, `tokens[]`.
- `template_id` must be `1778027615735`; `.ai` tag or `aiCreator` alone is not enough.
- `phase`: `bonding` requires an Agent wallet; `graduated` is open to everyone.
- `wallet_eligible`: `true`, `false`, or omitted when `AgentIdentifier` is unavailable.
- `lookback_blocks` bounds the scan; discovery is not complete history.
- Pass `before_block` to scan older windows; use `next_before_block` from the response as the next
  `before_block`. A value above the head is clamped and warned.
- Check `warnings`; partial log ranges, API rate limits and region restrictions are explicit.

`fourmeme.token` data:

- `address`, `name`, `symbol`, `phase`, template fields, market fields, creator and timestamps.
- `safety`: raw `buy_tax_raw`, `sell_tax_raw`, `min_buy_raw`, `max_buy_raw` and
  `contract_verified` (`true | false | null`), with a `source`. Units are not normalized yet; do
  not call the raw tax a percentage. `contract_verified: null` means not checked or unavailable;
  never treat it as verified or unverified.
- `socials`: optional `twitter`, `telegram`, and `website` links.
- `holder_count`: bonding-phase holder count when the API supplies it; after graduation it may be
  0 or unavailable. If it is missing, say unknown; never report zero.
- `market`: `price`, `max_price`, `market_cap`, `liquidity`, `raised`, `volume_24h`,
  `change_24h`, and `progress_percent`. `change_24h_percent` is present only when the raw value is
  unambiguous; otherwise report the raw `change_24h` and do not invent a percentage.
- `tradability.transfer_locked`: `true` while bonding; inner-market transfers are locked.
- `tradability.progress_percent`: curve completion, 0–100.
- `tradability.max_supply`: creation-time max supply; the 1% hold-cap denominator.
- `wallet_cap`: when a wallet is supplied, the token is Agent ID Exclusive, and phase is bonding,
  it reports `balance`, `cap` (1% of max supply), `remaining`, and `reached`.
- If API detail is unavailable, it may return on-chain name/symbol/supply with `phase: unknown`;
  do not infer the phase.
- If `wallet_cap` is missing, say the cap is unknown; never assume the wallet has room or is full.
- `wallet_cap.reached: true` means further buys will revert or be clamped; a small `remaining` is
  not a guarantee that a full buy will fill.

## Workflow recipe: fourmeme-discovery

Use this when the user wants to find Agent ID Exclusive tokens and check a wallet's eligibility.
The workflow is `fourmeme.agent-tokens → fourmeme.token → wallet.agent-id` and is read-only.

Checkpoints:

1. `fourmeme.agent-tokens`: keep `lookback_blocks` explicit; partial log ranges stay warnings.
2. `fourmeme.token`: read the phase and template; do not infer them when the API is unavailable.
3. `wallet.agent-id`: resolve the handle or address; an unavailable registry means `unknown`, not
   `false`.
4. When a wallet is supplied to `fourmeme.token`, also read `wallet_cap` before suggesting a buy;
   report `remaining` and `reached` explicitly.

Failure handling: API unavailable → return on-chain fields with warnings; `AgentIdentifier`
unavailable → eligibility stays unknown; a partial log range is never reported as zero tokens.

## Boundaries

- Never buy/sell; this skill has no execution path.
- Do not claim eligibility when `wallet_eligible` is missing.
- Binance Agentic Wallet login / Developer Mode / AI Pro are out of scope.
