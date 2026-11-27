# Security boundaries

Eollo is a Web3 capability layer for agents. This document states what the project protects, what
it assumes, and how to deploy it. It is not an audit and it does not guarantee safety.

For transaction and signature inspection rules, see
[ASSURANCE](capabilities/transaction/ASSURANCE.md). For signed threat-intel feeds, see
[THREAT-INTEL](capabilities/transaction/THREAT-INTEL.md).

## What Eollo guarantees

- It does not hold private keys, sign, or broadcast. Signing happens in the user's wallet.
- `transaction.inspect` and `signature.inspect` are deterministic for the same request, block and
  ruleset. LLMs do not participate in detectors, risk or coverage.
- Missing checks are reported as `unknown` or as a coverage dimension, never as a pass.
- A clean report is not a safety guarantee.
- Capability responses use stable error codes and a pinned block.

## Trust boundaries

| Component | Assumption | Failure and limit |
|---|---|---|
| Caller (agent or user) | Untrusted planner. It can construct arbitrary calldata and bypass MCP by calling the CLI or RPC directly. | Not treated as a security boundary. |
| Phone wallet and WalletConnect | Private keys and final confirmation stay in the wallet. | Rejection returns `USER_REJECTED`; timeout returns `TIMEOUT`. |
| Local runtime | No keys. Read-only RPC. The process and config are controlled by the local machine. | Does not defend against a malicious local process or a tampered binary. |
| RPC | Used for block, code, simulation and trace. No light-client proof. | Failure returns `RPC_ERROR` or an explicit `unavailable` coverage value. |
| Remote service | Returns quotes, yield and positions over a pinned snapshot. No keys. | Failure returns `RPC_ERROR`. There is no local quote fallback. |
| Source verifier | Optional BscScan or Etherscan. | Missing key or request failure returns `verified: null`, never inferred. |
| Threat intel | Operator-provided registry. Optional Ed25519 signature. | Unsigned or stale feeds can be incomplete or wrong. |
| Skills | Instructions for the agent. | Not an enforcement boundary. Policy is enforced by the runtime. |

## Threat model

Covered:

- Malicious or malformed calldata: unknown selectors, wrong argument layout, hidden multicall,
  proxy upgrades.
- Assets and approvals: unintended transfer, forged logs, unlimited approval, NFT operator
  approval.
- Contract execution: unexpected internal `DELEGATECALL`, unverified source, EIP-1967 proxy.
- Addresses and policy: zero address, burn address, recipient mismatch, policy caps and allowlists.
- Prompt injection is treated as an input-trust problem. The agent is the planner, and Eollo only
  inspects the transaction it receives.

Not covered:

- Private-key compromise, a malicious wallet app, or a controlled signing device.
- Protocol economics, oracle manipulation, MEV, liquidations, and bridge risk.
- Off-chain signatures not supplied to the inspector, or every real-world attack pattern.
- A malicious local process, a tampered binary, or a compromised operating system.

## Deployment checklist

- Bind to loopback for local use. The default host is `127.0.0.1`.
- For any non-loopback bind, set `EOLLO_REQUIRE_AUTH=1`. The server logs a warning when a
  non-loopback host runs without auth.
- Put TLS and per-IP rate limiting in front of the service. `EOLLO_RATE_LIMIT_PER_SEC` defaults to
  1000 requests per second per process, not per client.
- Keep `.env` out of git. It is gitignored. `GRAPH_API_KEY` and `ONEINCH_API_KEY` are secrets.
  `WALLETCONNECT_PROJECT_ID` is public.
- Rotate any key that was pasted into a chat, a log, or a config backup. Check
  `~/.config/opencode/opencode.json` and its backups for plaintext provider keys. Move secrets to
  environment variables or the OS keychain.
- Use a dedicated RPC. Set `EOLLO_VERIFY_RPC_URL` to cross-check block hashes and target code.
- Set `BSCSCAN_API_KEY` / `ETHERSCAN_API_KEY` for source verification.
- Set `EOLLO_THREAT_INTEL_PUBKEY` to require signed threat-intel feeds.
- Log request IDs, not secrets. Do not log permit or signature payloads unless the operator
  redacts them.
- Pin dependencies with `Cargo.lock` and `site/package-lock.json`. Install with
  `cargo install --locked` and `npm ci`.
- Use a dedicated OS user or machine for the local runtime. Protect
  `~/.eollo/walletconnect/session.json`.
- Enable `EOLLO_REQUIRE_AUTH=1` on any shared or public deployment, including the BSC profile.

## Agent and prompt-injection boundary

- The agent decides what to do. Eollo inspects the calldata or typed data it is given.
- Treat webpages, token metadata, social posts, and tool output as untrusted input.
- A clean inspection must not override the user's explicit confirmation.
- Skills tell the agent what to call and how to read the result. They do not enforce policy.

## Incident response

1. Stop the remote service.
2. Rotate `GRAPH_API_KEY`, `ONEINCH_API_KEY`, and any provider key that may have leaked.
3. Disconnect the WalletConnect session with `eollo wallet disconnect` and revoke it in the wallet
   if needed.
4. Inspect `~/.eollo/walletconnect/session.json` and remove it on a shared machine.
5. Review logs for the affected request IDs and block range.

## Current gaps

- RPC responses are trusted. There is no light-client proof, and `EOLLO_VERIFY_RPC_URL` does not
  cover every state read.
- Local binary integrity is not verified. A tampered `eollo` binary can change the result.
- Rate limiting is per process, not per client. Use a reverse proxy for per-IP limits.
- `transaction.inspect` decodes Uniswap/Pancake V2 swaps and V3 `exactInput`, `exactInputSingle`,
  and `exactOutput`. Mixed V2/V3 routes and other aggregators are not decoded yet.
- BSC Venus calldata uses standard Compound v2 ABIs, with contract code, symbol, decimals and
  exchange rate checked on-chain. It has not been exercised with real funds.
- Firmware, OS, and browser wallet security are outside the project's control.
