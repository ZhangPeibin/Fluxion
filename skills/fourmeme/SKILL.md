---
name: fourmeme
description: Use when the user asks which Four.meme tokens are in Binance Agent ID Exclusive mode, whether a wallet can trade one during the bonding curve, or uses the Fluxion fourmeme_agent_tokens MCP tool. Read-only; never buys, sells or signs.
---

# fourmeme.agent-tokens

Read-only discovery for Four.meme **Binance Agent ID Exclusive** tokens on BSC.

- Human docs: [`docs/capabilities/fourmeme`](../../docs/capabilities/fourmeme/README.md)
- Schema: [`fourmeme_agent_tokens`](../../schemas/fourmeme_agent_tokens.v1.json)

## MCP

```json
{"name": "fourmeme_agent_tokens", "arguments": {"wallet": "0x...", "limit": 20}}
```

## CLI

```bash
fluxion fourmeme agent-tokens --json
fluxion fourmeme agent-tokens --wallet 0x... --limit 20 --json
fluxion fourmeme agent-tokens --lookback-blocks 100000 --json
```

## Reading the result

- `template_id` must be `1778027615735`; `.ai` tag or `aiCreator` alone is not enough.
- `phase`: `bonding` requires an Agent wallet; `graduated` is open to everyone.
- `wallet_eligible`: `true`, `false`, or omitted when `AgentIdentifier` is unavailable.
- `coverage`: combinations of `onchain`, `api`, and `agent-id`.
- `lookback_blocks` bounds the scan; discovery is not complete history.
- Check `warnings`; partial log ranges or API detail failures are explicit.

## Boundaries

- Never buy/sell; this skill has no execution path.
- Do not claim eligibility when `wallet_eligible` is missing.
- Binance Agentic Wallet login / Developer Mode / AI Pro are out of scope.
