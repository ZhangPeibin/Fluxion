# Security red-team lab

Local Foundry project used by `bench/security/redteam.py`.

- `src/` contains benign controls and deliberately malicious/test contracts.
- The harness starts Anvil with `--chain-id 56`, deploys via `forge create`, runs the installed
  `fluxion` CLI, and reports detected/missed/known-gap cases.
- `deployments.json`, `out/`, `cache/` and `broadcast/` are generated and gitignored.
- These contracts are test fixtures only. Do not deploy them on a public network.

Run from the repository root:

```bash
python3 bench/security/redteam.py --out /tmp/redteam.md --json-out /tmp/redteam.json
```
