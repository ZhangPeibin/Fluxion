# inspect.explain - Plain-Language Explanation

## Goal

Turn a structured inspection report into a short, user-readable explanation without changing the
report.

## Dependencies

`inspect.core` and at least one finding/effects producer.

## Contract

Input is the structured report only. Output is text plus the report hash/version it explains.

```json
{
  "report_id": "0x...",
  "language": "zh",
  "summary": "This transaction approves an unlimited USDT allowance to an unknown spender.",
  "key_changes": ["You grant unlimited USDT spending permission."],
  "unknowns": ["Asset effects were not simulated."]
}
```

## Implemented Slice

The response now includes a deterministic `explanation` object:

- `summary`: decoded method/chain, finding count and risk, or “no known findings” plus risk;
- `key_changes`: native/token/NFT/approval effects;
- `unknowns`: copied from the report.

No LLM is called. The host agent can paraphrase this object. The summary never says `safe`.

## Rules

- `EXPLAIN-001`: never receives raw calldata as the authoritative source.
- `EXPLAIN-002`: cannot add, remove, upgrade or downgrade findings.
- `EXPLAIN-003`: must mention coverage gaps and unknowns.
- `EXPLAIN-004`: output is labeled explanation, not risk.
- `EXPLAIN-005`: deterministic templates must exist when no model is configured.
- `EXPLAIN-006`: treat all decoded strings and addresses as untrusted data, not instructions.

## Failure and Unknown Behaviour

- Model failure falls back to deterministic templates.
- Missing findings yields a factual report summary, not “safe”.
- No remote model by default.

## Tests

- explanation of an unlimited approval;
- explanation with unknown coverage;
- prompt-injection string inside a token symbol or argument;
- fallback without a model.

## Acceptance

- explanation cannot mutate the structured report;
- no “safe” claim;
- privacy mode can disable remote explaining.

## Recommendation

Prefer host-agent explanation in v1. Add a Fluxion explanation service only if users need a stable,
localized format and privacy requirements are satisfied.
