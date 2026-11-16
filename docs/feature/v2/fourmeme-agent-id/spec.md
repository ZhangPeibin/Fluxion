# Four.meme Agent ID Exclusive tokens - Spec

**Status:** implemented
**Release:** v2
**Capability:** `fourmeme.agent-tokens`

## Problem

Four.meme has a Binance Agent ID Exclusive launch mode. During the bonding-curve phase only
Binance Agentic Wallets with an Agent ID can trade; each wallet is capped at 1% of supply, transfers
are locked, and trading opens after graduation.

The mode is not the same as `aiCreator` or the `.ai` tag. Users need a read-only way to answer:

- which recent tokens are in Binance Agent ID Exclusive mode;
- whether a token is still on the bonding curve or graduated;
- whether a wallet currently satisfies the Agent wallet gate.

## Scope

### In scope

- BSC only.
- Read-only discovery of recent Four.meme Agent ID Exclusive tokens.
- Token detail: name, symbol, address, template id, phase, progress, raised amount.
- Optional wallet eligibility:
  - Four.meme `AgentIdentifier.isAgent(wallet)` on BSC;
  - token must use template `1778027615735`;
  - phase must be `bonding` or `graduated`;
  - verdict is `true | false | unknown`, never inferred.
- On-chain log discovery plus optional API detail, with `coverage` / `sources` / `warnings`.

### Out of scope

- Buying or selling tokens.
- Binance Agentic Wallet login, AI Pro, or Developer Mode automation.
- Complete historical indexing; discovery is bounded by `lookback-blocks`.
- Price, PnL, or token quality claims.

## Data sources

- BSC `eth_getLogs` on OpenFour Core `0xebe7b6C1089D9F72aD07f34E36d898e44E5e27f3`;
  `TokenCreated` topic filtered by `presetId=1778027615735`.
- Optional Four.meme REST detail: `GET /private/token/get/v2?address=...`.
- BSC: `AgentIdentifier 0x09B44A633de9F9EBF6FB9Bdd5b5629d3DD2cef13`, `isAgent(address)`.
- Env: `FOURMEME_BSC_RPC_URL` (must support `eth_getLogs`), `FOURMEME_API_URL`.
- Token template id / preset: `1778027615735` (`Binance Agent ID Exclusive`).

## Output

```json
{
  "chain": "bsc",
  "mode": "binance_agent_id_exclusive",
  "scanned": 1,
  "lookback_blocks": 100000,
  "from_block": 133750000,
  "to_block": 133850000,
  "tokens": [
    {
      "address": "0x...",
      "name": "4AGENT",
      "symbol": "BNB",
      "template_id": "1778027615735",
      "phase": "graduated",
      "progress": "1",
      "raised": "13.4",
      "agent_id_exclusive": true,
      "wallet_eligible": true,
      "eligibility_reason": "agent_wallet"
    }
  ],
  "coverage": "onchain+api",
  "sources": [],
  "warnings": []
}
```

## Rules

- `FOURMEME-001`: template id must equal `1778027615735`; `.ai` tag or `aiCreator` alone is not
  sufficient.
- `FOURMEME-002`: discovery uses `TokenCreated.presetId`; `status=PUBLISH` / `PUBLISHED` is bonding and
  `status=TRADE` is graduated.
- `FOURMEME-003`: if `AgentIdentifier` is unavailable, `wallet_eligible=null` + warning.
- `FOURMEME-004`: if API detail is unavailable, keep the log-derived token with `phase=unknown`
  and report the failure; partial log ranges are warnings, never silent zeros.
- `FOURMEME-005`: discovery is bounded by `lookback-blocks`, not a complete historical index.
- `FOURMEME-006`: no buy/sell calldata is built.

## Acceptance

- CLI/MCP can list exclusive tokens created in the lookback window.
- Optional `--wallet` reports `true/false/null` eligibility.
- No false claim based on `aiCreator` or `.ai` tag.
- Templates, API/RPC failures and out-of-scope phases are explicit.
- Tests cover event decoding, template matching and phase mapping; one live read-only smoke check.
- fmt/clippy/workspace tests and docs/schema/skill updates.
