# Threat-intel feed

`transaction.inspect` can load an optional local address registry through
`FLUXION_THREAT_INTEL_FILE`. Matches produce `ADDR-007` with the configured label and source.

## File format

```json
{
  "version": "2026-11-11.1",
  "issued_at": 1762800000,
  "expires_at": 1765392000,
  "signature": "0x...",
  "addresses": {
    "0x75117c9158f53998ce8689b64509eff4fa10ad80": {
      "label": "phishing spender",
      "source": "https://bscscan.com/tx/0x..."
    }
  }
}
```

- `version`, `issued_at`, `expires_at`, `signature` are optional for unsigned local registries.
- `expires_at` is Unix seconds. An expired feed is rejected with an explicit unknown.
- Address keys are serialized as checksummed addresses; the registry is a `BTreeMap`, so signing
  payloads are deterministic.

## Signed feeds

Set `FLUXION_THREAT_INTEL_PUBKEY` to a 32-byte Ed25519 public key in hex. When a public key is
configured:

1. The file must include a 64-byte Ed25519 `signature` in hex.
2. The signed payload is the UTF-8 compact JSON of:
   ```json
   {"version":..., "issued_at":..., "expires_at":..., "addresses":{...}}
   ```
   without the `signature` field.
3. Missing/invalid signatures and expired feeds are rejected; entries are not used.
4. `sources[].scope` becomes `address-registry-signed`.

Without a public key, unsigned feeds are accepted with a warning and
`address-registry-unsigned`. This keeps local development usable but makes production signing
explicit.

## Boundaries

- The registry is operator-provided; Fluxion does not ship a universal malicious-address list.
- Address labels and sources are not independently verified by Fluxion.
- A miss is not proof of safety; the feed may be incomplete or stale.
- Threat intel is an additional signal, not a replacement for simulation, trace, source verification,
  or wallet confirmation.
