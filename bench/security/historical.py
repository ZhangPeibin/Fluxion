#!/usr/bin/env python3
"""Replay real historical BSC transaction calldata through transaction.inspect.

The state is inspected at the current block; this is calldata-level historical replay, not a
historical-state fork. Results are labeled detect or coverage_gap, not a recall claim.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import urllib.request
from pathlib import Path

BSC_RPC = "https://bsc-dataseed.bnbchain.org"
ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "tests" / "security" / "fixtures" / "historical.json"
THREAT_INTEL = ROOT / "tests" / "security" / "fixtures" / "threat-intel.json"


def rpc(method: str, params: list) -> dict:
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    request = urllib.request.Request(
        BSC_RPC,
        data=body,
        headers={"content-type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        payload = json.loads(response.read())
    if "error" in payload:
        raise RuntimeError(payload["error"])
    return payload["result"]


def fetch_tx(tx_hash: str) -> dict:
    result = rpc("eth_getTransactionByHash", [tx_hash])
    if result is None:
        raise RuntimeError(f"transaction not found: {tx_hash}")
    return result


def inspect(binary: str, tx: dict, threat_intel: Path | None) -> dict:
    command = [
        binary,
        "transaction",
        "inspect",
        "--rpc-url",
        BSC_RPC,
        "--chain",
        "bsc",
        "--from",
        tx["from"],
        "--to",
        tx["to"],
        "--data",
        tx["input"],
        "--json",
    ]
    env = None
    if threat_intel is not None:
        env = {**os.environ, "FLUXION_THREAT_INTEL_FILE": str(threat_intel)}
    completed = subprocess.run(
        command,
        capture_output=True,
        text=True,
        timeout=60,
        env=env,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"inspect failed for {tx['hash']}:\n{completed.stderr}"
        )
    return json.loads(completed.stdout)


def run_case(binary: str, case: dict, threat_intel: Path | None) -> dict:
    tx = fetch_tx(case["tx"])
    response = inspect(binary, tx, threat_intel)
    findings = sorted({finding["id"] for finding in response["findings"]})
    decode = response["coverage"]["decode"]
    if case["mode"] == "detect":
        passed = decode == case["expect_decode"] and set(case["expect"]).issubset(findings)
    else:
        passed = decode == case["expect_decode"] and not findings
    return {
        **case,
        "to": tx["to"],
        "selector": response["intent"]["selector"],
        "risk": response["risk"],
        "decode": decode,
        "findings": findings,
        "passed": passed,
    }


def markdown(results: list[dict]) -> str:
    lines = [
        "Real historical BSC calldata replay at the current block.",
        "",
        "| id | mode | selector | risk | decode | expected | actual | result |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for result in results:
        expected = ", ".join(result["expect"]) or "none"
        actual = ", ".join(result["findings"]) or "none"
        lines.append(
            f"| {result['id']} | {result['mode']} | {result['selector']} | "
            f"{result['risk']} | {result['decode']} | {expected} | {actual} | "
            f"{'pass' if result['passed'] else 'FAIL'} |"
        )
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default="fluxion")
    parser.add_argument("--corpus", type=Path, default=CORPUS)
    parser.add_argument("--threat-intel", type=Path, default=THREAT_INTEL)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--json-out", type=Path)
    args = parser.parse_args()

    cases = json.loads(args.corpus.read_text())
    threat_intel = args.threat_intel if args.threat_intel.exists() else None
    results = [run_case(args.bin, case, threat_intel) for case in cases]
    output = markdown(results)
    print(output)
    if args.out:
        args.out.write_text(output + "\n")
    if args.json_out:
        args.json_out.write_text(json.dumps(results, indent=2) + "\n")
    return 0 if all(result["passed"] for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
