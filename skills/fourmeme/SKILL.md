---
name: fourmeme
description: Use when the user asks which Four.meme tokens are in Binance Agent ID Exclusive mode, wants a single token's detail, wonders whether a wallet can trade during the bonding curve, or uses the fourmeme_agent_tokens / fourmeme_token tools. Read-only; never buys, sells or signs.
---

# Four.meme Agent ID Exclusive

Read-only discovery, detail and optional wallet eligibility for Four.meme **Binance Agent ID
Exclusive** tokens on BSC.

- Human docs: [`docs/capabilities/fourmeme`](../../docs/capabilities/fourmeme/README.md)
- Schemas: [`agent-tokens`](../../schemas/fourmeme_agent_tokens.v1.json) ·
  [`token`](../../schemas/fourmeme_token.v1.json)

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

- `template_id` must be `1778027615735`; `.ai` tag or `aiCreator` alone is not enough.
- `phase`: `bonding` requires an Agent wallet; `graduated` is open to everyone.
- `wallet_eligible`: `true`, `false`, or omitted when `AgentIdentifier` is unavailable.
- `coverage`: combinations of `onchain`, `api`, and `agent-id`.
- `lookback_blocks` bounds the scan; discovery is not complete history.
- Check `warnings`; partial log ranges, API rate limits and region restrictions are explicit.
- If API detail is unavailable, `fourmeme.token` may return on-chain name/symbol/supply with
  `phase: unknown`; do not infer the phase.

## Boundaries

- Never buy/sell; this skill has no execution path.
- Do not claim eligibility when `wallet_eligible` is missing.
- Binance Agentic Wallet login / Developer Mode / AI Pro are out of scope.
