# Eollo skills

Skills teach an agent when to call an Eollo capability and how to read the result. They are
instructions, not an execution path: the agent still calls the MCP tools, and signing or broadcasting
stays behind the wallet confirmation boundary.

## Install with the skills CLI

Requires Node.js 22.20 or later. Run this from the project where you want the skills:

```bash
# Preview the seven skills without installing
npx skills add ZhangPeibin/eollo --list

# Interactive: choose skills and agents
npx skills add ZhangPeibin/eollo

# Install every skill for OpenCode, available in all projects
npx skills add ZhangPeibin/eollo -g -a opencode

# Install only the skills you need
npx skills add ZhangPeibin/eollo --skill swap --skill wallet -a opencode
```

A project install writes the skills to `.agents/skills/`. A global install with `-g -a opencode`
writes to `~/.agents/skills/`; OpenCode reads both locations as compatibility sources.

Pin a release when exact content matters:

```bash
npx skills add ZhangPeibin/eollo#v0.1.0 -g -a opencode
```

## Install with GitHub CLI

`gh skill` needs GitHub CLI v2.90 or later and does not require Node.js:

```bash
gh skill preview ZhangPeibin/eollo swap
gh skill install ZhangPeibin/eollo swap --agent opencode --scope user
```

Omit the skill name to choose interactively, or use `--scope project` to install into the current
repository.

## Fallback: point a client at a local clone

If you already have this repository checked out and your client supports a skills path, add the
absolute path to its config. For OpenCode:

```json
{
  "skills": ["/absolute/path/to/eollo/skills"]
}
```

This is an offline/development fallback. It requires a clone and does not update with the repository
unless you pull.

## Skills

| Skill | Use it for |
|---|---|
| `swap` | Exact-in Ethereum swap quotes, route shape and V2 calldata. |
| `wallet` | Balances, assets, allowances, collectibles, BSC Agent ID, WalletConnect sessions, signing and transfers. |
| `transaction` | Calldata inspection, simulation, sending and receipt verification. |
| `signature` | Pre-signing checks for EIP-712, Permit2, EIP-7702 and ERC-4337. |
| `yield` | Curated ETH staking and lending discovery, build, history and exit. |
| `position` | Curated ETH yield positions, ETH value and optional PnL. |
| `fourmeme` | Four.meme Binance Agent ID Exclusive token discovery and eligibility. |

Each skill lives in its own folder with a `SKILL.md` file. Human-facing capability contracts are in
[`docs/capabilities/`](https://github.com/ZhangPeibin/eollo/tree/master/docs/capabilities); schemas
are served from [eollo.xyz/schemas](https://eollo.xyz/schemas/).

## Safety

Skills run with your agent's full permissions. Read a `SKILL.md` before installing it, and keep the
wallet confirmation boundary intact: `transaction.inspect` and `signature.inspect` are advisory, not
a security guarantee, and no skill asks for a private key.
