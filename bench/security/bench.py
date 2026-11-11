#!/usr/bin/env python3
"""End-to-end latency and expectation benchmark for Fluxion security inspection.

This measures the installed CLI, so process startup and live BSC RPC round-trips are included.
It is a regression benchmark, not an independent security evaluation.
"""

from __future__ import annotations

import argparse
import json
import math
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

BSC = "https://bsc-dataseed.bnbchain.org"
USDT = "0x55d398326f99059fF775485246999027B3197955"
PANCAKE_V2 = "0x10ED43C718714eb63d5aA57B78B54704E256024E"
ACCOUNT = "0x5ED10018795d667050e996833C60fF02b1388058"
SPENDER = "0xA506ac7069937571DC56C5fFDa53c4A729456c6"


def word(value: int) -> str:
    return f"{value:064x}"


def address_word(address: str) -> str:
    return address.lower().removeprefix("0x").rjust(64, "0")


def approve_max() -> str:
    return "0x095ea7b3" + address_word(SPENDER) + "f" * 64


def permit_max() -> str:
    return (
        "0xd505accf"
        + address_word(ACCOUNT)
        + address_word(SPENDER)
        + "f" * 64
        + word(0)
        + word(27)
        + "11" * 32
        + "22" * 32
    )


def swap_zero_min_out() -> str:
    return (
        "0x38ed1739"
        + word(1000)
        + word(0)
        + word(160)
        + address_word(ACCOUNT)
        + word(9999999999)
        + word(2)
        + address_word(USDT)
        + address_word("0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c")
    )


PERMIT2 = {
    "domain": {"name": "Permit2", "chainId": 56},
    "primaryType": "PermitSingle",
    "message": {
        "spender": SPENDER,
        "sigDeadline": "9999999999",
        "details": {
            "token": USDT,
            "amount": "1461501637330902918203684832716283019655932542975",
            "expiration": "0",
            "nonce": 0,
        },
    },
}

AUTH_LIST = [{"chainId": 1, "address": "0x0000000000000000000000000000000000000002"}]


def transaction_cases() -> list[dict]:
    return [
        {
            "name": "native transfer",
            "kind": "transaction",
            "args": ["--to", ACCOUNT, "--data", "0x"],
            "expect": [],
        },
        {
            "name": "unlimited approve",
            "kind": "transaction",
            "args": [
                "--to",
                USDT,
                "--from",
                ACCOUNT,
                "--data",
                approve_max(),
            ],
            "expect": ["AUTH-001"],
        },
        {
            "name": "permit max + expired",
            "kind": "transaction",
            "args": [
                "--to",
                USDT,
                "--from",
                ACCOUNT,
                "--data",
                permit_max(),
            ],
            "expect": ["SIG-002", "SIG-003"],
        },
        {
            "name": "swap zero minOut",
            "kind": "transaction",
            "args": [
                "--to",
                PANCAKE_V2,
                "--from",
                ACCOUNT,
                "--data",
                swap_zero_min_out(),
            ],
            "expect": ["SWAP-001"],
        },
        {
            "name": "unknown selector",
            "kind": "transaction",
            "args": ["--to", USDT, "--data", "0xdeadbeef"],
            "expect": [],
        },
    ]


def signature_cases(tmp: Path) -> list[dict]:
    typed = tmp / "permit2.json"
    typed.write_text(json.dumps(PERMIT2))
    auth = tmp / "auth.json"
    auth.write_text(json.dumps(AUTH_LIST))
    return [
        {
            "name": "typed-data permit2",
            "kind": "signature",
            "args": ["--typed-data", str(typed)],
            "expect": ["SIG-002", "SIG-003"],
        },
        {
            "name": "eip-7702 authorization",
            "kind": "signature",
            "args": ["--authorization-list", str(auth)],
            "expect": ["SIG-004"],
        },
    ]


def percentile(values: list[float], p: float) -> float:
    ordered = sorted(values)
    index = max(0, math.ceil(p * len(ordered)) - 1)
    return ordered[index]


def run_case(binary: str, case: dict, iterations: int) -> dict:
    if case["kind"] == "transaction":
        command = [binary, "transaction", "inspect", "--json", *case["args"]]
    else:
        command = [binary, "signature", "inspect", "--json", *case["args"]]

    latencies = []
    last = None
    for _ in range(iterations):
        started = time.perf_counter()
        completed = subprocess.run(command, capture_output=True, text=True, timeout=60)
        elapsed_ms = (time.perf_counter() - started) * 1000
        if completed.returncode != 0:
            raise RuntimeError(f"{case['name']} failed: {completed.stderr}")
        last = json.loads(completed.stdout)
        latencies.append(elapsed_ms)

    actual = sorted(finding["id"] for finding in last.get("findings", []))
    expected = sorted(case["expect"])
    return {
        "name": case["name"],
        "kind": case["kind"],
        "risk": last["risk"],
        "findings": actual,
        "expected": expected,
        "passed": actual == expected,
        "p50_ms": percentile(latencies, 0.50),
        "p95_ms": percentile(latencies, 0.95),
        "min_ms": min(latencies),
        "max_ms": max(latencies),
    }


def markdown(results: list[dict], iterations: int) -> str:
    lines = [
        f"End-to-end CLI latency, {iterations} runs per case, BSC public RPC.",
        "",
        "| case | kind | risk | findings | p50 | p95 | min | max | check |",
        "|---|---|---|---:|---:|---:|---:|---:|---|",
    ]
    for result in results:
        findings = ", ".join(result["findings"]) or "none"
        passed = "pass" if result["passed"] else "FAIL"
        lines.append(
            f"| {result['name']} | {result['kind']} | {result['risk']} | {findings} | "
            f"{result['p50_ms']:.0f} ms | {result['p95_ms']:.0f} ms | "
            f"{result['min_ms']:.0f} ms | {result['max_ms']:.0f} ms | {passed} |"
        )
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default="fluxion")
    parser.add_argument("--iterations", type=int, default=20)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()

    with tempfile.TemporaryDirectory(prefix="fluxion-security-bench-") as temp:
        cases = transaction_cases() + signature_cases(Path(temp))
        results = [run_case(args.bin, case, args.iterations) for case in cases]

    output = markdown(results, args.iterations)
    print(output)
    if args.out:
        args.out.write_text(output + "\n")
    return 0 if all(result["passed"] for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
