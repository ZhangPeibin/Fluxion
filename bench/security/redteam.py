#!/usr/bin/env python3
"""Local adversarial corpus for Fluxion security inspection.

Starts anvil, deploys test/malicious contracts with forge, runs the installed fluxion CLI, and
writes a detection matrix. This is a red-team regression corpus, not a certification.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

ROOT = Path(__file__).resolve().parents[2]
SECURITY = ROOT / "tests" / "security"
RPC = "http://127.0.0.1:8546"
CHAIN_ID = 56
PRIVATE_KEY = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
ACCOUNT = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
RECIPIENT = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
ATTACKER = "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"
BURN = "0x000000000000000000000000000000000000dEaD"
VERIFIER_ENV: dict[str, str] = {}


def word(value: int) -> str:
    return f"{value:064x}"


def address_word(address: str) -> str:
    return address.lower().removeprefix("0x").rjust(64, "0")


def approve_data(spender: str, amount: int) -> str:
    return "0x095ea7b3" + address_word(spender) + word(amount)


def transfer_data(to: str, amount: int) -> str:
    return "0xa9059cbb" + address_word(to) + word(amount)


def safe_transfer_data(from_address: str, to: str, token_id: int) -> str:
    return "0x42842e0e" + address_word(from_address) + address_word(to) + word(token_id)


def set_approval_for_all(operator: str, approved: bool) -> str:
    return "0xa22cb465" + address_word(operator) + word(1 if approved else 0)


def upgrade_to(implementation: str) -> str:
    return "0x3659cfe6" + address_word(implementation)


def multicall_data(calls: list[str]) -> str:
    payload = word(32) + word(len(calls))
    header = 32 + 32 * len(calls)
    offset = header
    for call in calls:
        payload += word(offset)
        raw = bytes.fromhex(call.removeprefix("0x"))
        offset += 32 + ((len(raw) + 31) // 32) * 32
    for call in calls:
        raw = bytes.fromhex(call.removeprefix("0x"))
        payload += word(len(raw)) + raw.hex() + "00" * ((-len(raw)) % 32)
    return "0xac9650d8" + payload


def swap_data(token_in: str, token_out: str, recipient: str, min_out: int) -> str:
    return (
        "0x38ed1739"
        + word(1000)
        + word(min_out)
        + word(160)
        + address_word(recipient)
        + word(9999999999)
        + word(2)
        + address_word(token_in)
        + address_word(token_out)
    )


def run(
    command: list[str],
    *,
    cwd: Path | None = None,
    timeout: int = 120,
    env: dict[str, str] | None = None,
) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        capture_output=True,
        text=True,
        timeout=timeout,
        env={**os.environ, **env} if env else None,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"command failed: {' '.join(command)}\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
        )
    return completed.stdout


def forge_create(spec: str, constructor_args: list[str] | None = None) -> str:
    source, contract = spec.split(":", 1)
    command = [
        "forge",
        "create",
        f"src/{source}.sol:{contract}",
        "--rpc-url",
        RPC,
        "--private-key",
        PRIVATE_KEY,
        "--broadcast",
        "--json",
    ]
    if constructor_args:
        command.extend(["--constructor-args", *constructor_args])
    output = run(command, cwd=SECURITY)
    return json.loads(output)["deployedTo"]


def cast_send(to: str, signature: str, *args: str) -> None:
    run(
        [
            "cast",
            "send",
            to,
            signature,
            *args,
            "--rpc-url",
            RPC,
            "--private-key",
            PRIVATE_KEY,
        ]
    )


def cast_calldata(signature: str, *args: str) -> str:
    return run(["cast", "calldata", signature, *args]).strip()


def deploy() -> dict[str, str]:
    token = forge_create("TestERC20:TestERC20", ["Test Token", "TEST"])
    fee = forge_create("TestERC20:FeeOnTransferERC20")
    fake = forge_create("TestERC20:FakeLogERC20")
    nft = forge_create("TestERC721:TestERC721")
    fake_nft = forge_create("FakeLogERC721:FakeLogERC721")
    multicall = forge_create("MulticallHelper:MulticallHelper")
    implementation = forge_create("EIP1967Proxy:ImplementationV1")
    proxy = forge_create("EIP1967Proxy:EIP1967Proxy", [implementation])
    router = forge_create("SwapRouterStub:SwapRouterStub")
    delegate = forge_create("DelegatecallAttacker:DelegatecallAttacker")
    benign = forge_create("DelegatecallAttacker:BenignTarget")

    cast_send(token, "mint(address,uint256)", ACCOUNT, str(10**24))
    cast_send(token, "mint(address,uint256)", RECIPIENT, str(10**24))
    cast_send(fee, "mint(address,uint256)", ACCOUNT, str(10**24))
    cast_send(nft, "mint(address,uint256)", ACCOUNT, "1")

    deployments = {
        "token": token,
        "fee": fee,
        "fake": fake,
        "nft": nft,
        "fake_nft": fake_nft,
        "multicall": multicall,
        "implementation": implementation,
        "proxy": proxy,
        "router": router,
        "delegate": delegate,
        "benign": benign,
    }
    (SECURITY / "deployments.json").write_text(json.dumps(deployments, indent=2) + "\n")
    return deployments


def inspect_command(
    binary: str,
    to: str,
    data: str,
    *,
    value: str | None = None,
    policy: str | None = None,
) -> list[str]:
    command = [
        binary,
        "transaction",
        "inspect",
        "--rpc-url",
        RPC,
        "--chain",
        "bsc",
        "--from",
        ACCOUNT,
        "--to",
        to,
        "--data",
        data,
    ]
    if value:
        command.extend(["--value", value])
    if policy:
        command.extend(["--policy", policy])
    command.append("--json")
    return command


PERMIT2 = {
    "domain": {"name": "Permit2", "chainId": 56},
    "primaryType": "PermitSingle",
    "message": {
        "spender": ATTACKER,
        "sigDeadline": "9999999999",
        "details": {
            "token": "0x0000000000000000000000000000000000000010",
            "amount": "1461501637330902918203684832716283019655932542975",
            "expiration": "0",
            "nonce": 0,
        },
    },
}

AUTH_LIST = [{"chainId": 1, "address": ATTACKER}]
USER_OP = {
    "sender": ACCOUNT,
    "paymaster": ATTACKER,
    "callData": "0x",
}

def cases(deployments: dict[str, str]) -> list[dict]:
    token = deployments["token"]
    fee = deployments["fee"]
    fake = deployments["fake"]
    nft = deployments["nft"]
    multicall = deployments["multicall"]
    proxy = deployments["proxy"]
    router = deployments["router"]
    delegate = deployments["delegate"]
    benign = deployments["benign"]

    nested_approve = approve_data(ATTACKER, (1 << 256) - 1)
    claim_data = cast_calldata(
        "claim(address,bytes)",
        benign,
        cast_calldata("ping()"),
    )

    return [
        {
            "name": "benign transfer",
            "mode": "control",
            "kind": "transaction",
            "to": token,
            "data": transfer_data(RECIPIENT, 1000),
            "expect": [],
        },
        {
            "name": "benign finite approve",
            "mode": "control",
            "kind": "transaction",
            "to": token,
            "data": approve_data(ATTACKER, 1000),
            "expect": [],
        },
        {
            "name": "unlimited approve",
            "mode": "detect",
            "kind": "transaction",
            "to": token,
            "data": approve_data(ATTACKER, (1 << 256) - 1),
            "expect": ["AUTH-001"],
        },
        {
            "name": "nft operator approval",
            "mode": "detect",
            "kind": "transaction",
            "to": nft,
            "data": set_approval_for_all(ATTACKER, True),
            "expect": ["AUTH-004"],
        },
        {
            "name": "multicall hidden approve",
            "mode": "detect",
            "kind": "transaction",
            "to": multicall,
            "data": multicall_data([nested_approve]),
            "expect": ["AUTH-001"],
        },
        {
            "name": "transfer to zero",
            "mode": "detect",
            "kind": "transaction",
            "to": token,
            "data": transfer_data("0x0000000000000000000000000000000000000000", 1),
            "expect": ["ADDR-001"],
        },
        {
            "name": "transfer to burn",
            "mode": "detect",
            "kind": "transaction",
            "to": token,
            "data": transfer_data(BURN, 1),
            "expect": ["ADDR-002"],
        },
        {
            "name": "swap zero minOut",
            "mode": "detect",
            "kind": "transaction",
            "to": router,
            "data": swap_data(token, token, ACCOUNT, 0),
            "expect": ["SWAP-001", "SWAP-007"],
        },
        {
            "name": "proxy upgrade",
            "mode": "detect",
            "kind": "transaction",
            "to": proxy,
            "data": upgrade_to(deployments["implementation"]),
            "expect": ["CONTRACT-003"],
        },
        {
            "name": "fee on transfer",
            "mode": "detect",
            "kind": "transaction",
            "to": fee,
            "data": transfer_data(RECIPIENT, 1000),
            "expect": ["FLOW-004", "FLOW-005"],
        },
        {
            "name": "fake transfer logs",
            "mode": "detect",
            "kind": "transaction",
            "to": fake,
            "data": transfer_data(RECIPIENT, 1000),
            "expect": ["EFFECT-007"],
        },
        {
            "name": "fake approval logs",
            "mode": "detect",
            "kind": "transaction",
            "to": fake,
            "data": approve_data(ATTACKER, 1000),
            "expect": ["EFFECT-009"],
        },
        {
            "name": "benign NFT transfer",
            "mode": "control",
            "kind": "transaction",
            "to": nft,
            "data": safe_transfer_data(ACCOUNT, RECIPIENT, 1),
            "expect": [],
        },
        {
            "name": "fake NFT transfer logs",
            "mode": "detect",
            "kind": "transaction",
            "to": deployments["fake_nft"],
            "data": safe_transfer_data(ACCOUNT, RECIPIENT, 1),
            "expect": ["EFFECT-010"],
        },
        {
            "name": "hidden delegatecall",
            "mode": "detect",
            "kind": "transaction",
            "to": delegate,
            "data": claim_data,
            "expect": ["CONTRACT-004"],
        },
        {
            "name": "unverified benign target",
            "mode": "detect",
            "kind": "transaction",
            "to": benign,
            "data": "0x5c36b186",
            "expect": ["CONTRACT-001"],
        },
        {
            "name": "policy native cap",
            "mode": "detect",
            "kind": "transaction",
            "to": token,
            "data": "0x",
            "value": "1",
            "policy": {"mode": "enforce", "max_native_spend": "0.5"},
            "expect": ["POLICY-002"],
            "expect_decision": "block",
        },
        {
            "name": "permit2 unlimited + no expiration",
            "mode": "detect",
            "kind": "signature",
            "input": PERMIT2,
            "expect": ["SIG-002", "SIG-003"],
        },
        {
            "name": "eip7702 authorization",
            "mode": "detect",
            "kind": "signature",
            "input_type": "authorization_list",
            "input": AUTH_LIST,
            "expect": ["SIG-004"],
        },
        {
            "name": "erc4337 paymaster",
            "mode": "detect",
            "kind": "signature",
            "input_type": "user_operation",
            "input": USER_OP,
            "expect": ["SIG-005"],
        },
    ]


def run_case(binary: str, case: dict, temp: Path) -> dict:
    if case["kind"] == "transaction":
        policy_path = None
        if case.get("policy"):
            policy_path = temp / f"{case['name'].replace(' ', '-')}-policy.json"
            policy_path.write_text(json.dumps(case["policy"]))
        command = inspect_command(
            binary,
            case["to"],
            case["data"],
            value=case.get("value"),
            policy=str(policy_path) if policy_path else None,
        )
    else:
        path = temp / f"{case['name'].replace(' ', '-')}.json"
        path.write_text(json.dumps(case["input"]))
        flag = {
            "typed_data": "--typed-data",
            "authorization_list": "--authorization-list",
            "user_operation": "--user-operation",
        }[case.get("input_type", "typed_data")]
        command = [binary, "signature", "inspect", flag, str(path), "--json"]

    started = time.perf_counter()
    output = run(command, env=VERIFIER_ENV)
    elapsed_ms = (time.perf_counter() - started) * 1000
    response = json.loads(output)
    actual = sorted({finding["id"] for finding in response["findings"]})
    expected = case["expect"]
    actual_decision = response.get("policy", {}).get("decision")
    if case["mode"] == "detect":
        passed = set(expected).issubset(set(actual))
        if case.get("expect_decision"):
            passed = passed and actual_decision == case["expect_decision"]
    elif case["mode"] == "control":
        passed = not actual
    else:
        passed = True
    return {
        **case,
        "actual": actual,
        "risk": response["risk"],
        "decision": actual_decision,
        "coverage": response.get("coverage"),
        "unknowns": response.get("unknowns", []),
        "warnings": response.get("warnings", []),
        "passed": passed,
        "latency_ms": elapsed_ms,
    }


def markdown(results: list[dict]) -> str:
    lines = [
        "Local red-team corpus against anvil (chain-id 56).",
        "",
        "| case | mode | risk | policy | expected | actual | result | latency |",
        "|---|---|---|---|---|---|---:|---:|",
    ]
    for result in results:
        result_label = "pass" if result["passed"] else "FAIL"
        if result["mode"] == "known_gap":
            result_label = "known-gap"
        lines.append(
            f"| {result['name']} | {result['mode']} | {result['risk']} | "
            f"{result.get('decision') or '-'} | "
            f"{', '.join(result['expect']) or 'none'} | "
            f"{', '.join(result['actual']) or 'none'} | {result_label} | "
            f"{result['latency_ms']:.0f} ms |"
        )
    return "\n".join(lines)


class VerificationHandler(BaseHTTPRequestHandler):
    benign = ""

    def do_GET(self) -> None:
        query = parse_qs(urlparse(self.path).query)
        address = query.get("address", [""])[0].lower()
        verified = address != self.benign.lower()
        payload = {
            "status": "1",
            "message": "OK",
            "result": [
                {
                    "SourceCode": "contract Mock {}" if verified else "",
                    "ABI": "[]",
                    "Proxy": "0",
                    "Implementation": "",
                }
            ],
        }
        body = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args: object) -> None:
        pass


def start_verifier(benign: str) -> tuple[ThreadingHTTPServer, threading.Thread]:
    VerificationHandler.benign = benign
    server = ThreadingHTTPServer(("127.0.0.1", 0), VerificationHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, thread


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default="fluxion")
    parser.add_argument("--out", type=Path)
    parser.add_argument("--json-out", type=Path)
    args = parser.parse_args()

    missing = [tool for tool in ("forge", "anvil", "cast", args.bin) if shutil.which(tool) is None]
    if missing:
        raise SystemExit(f"missing required tools: {', '.join(missing)}")

    anvil = None
    started_here = False
    try:
        try:
            run(["cast", "chain-id", "--rpc-url", RPC], timeout=5)
        except Exception:
            anvil = subprocess.Popen(
                [
                    "anvil",
                    "--chain-id",
                    str(CHAIN_ID),
                    "--host",
                    "127.0.0.1",
                    "--port",
                    "8546",
                    "--silent",
                ],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            started_here = True
            for _ in range(50):
                try:
                    if run(["cast", "chain-id", "--rpc-url", RPC], timeout=2).strip() == str(CHAIN_ID):
                        break
                except Exception:
                    time.sleep(0.2)
            else:
                raise RuntimeError("anvil did not start")

        deployments = deploy()
        verifier, verifier_thread = start_verifier(deployments["benign"])
        global VERIFIER_ENV
        VERIFIER_ENV = {
            "BSCSCAN_API_KEY": "test",
            "FLUXION_BSCSCAN_API_URL": f"http://127.0.0.1:{verifier.server_port}/api",
        }
        # Warm the local node/process path once before recording per-case latency.
        run(inspect_command(args.bin, deployments["token"], transfer_data(RECIPIENT, 1)))
        results = []
        with tempfile_dir() as temp:
            for case in cases(deployments):
                results.append(run_case(args.bin, case, temp))

        report = markdown(results)
        print(report)
        controls = [result for result in results if result["mode"] == "control"]
        detects = [result for result in results if result["mode"] == "detect"]
        gaps = [result for result in results if result["mode"] == "known_gap"]
        print(
            f"\nsummary: controls {sum(result['passed'] for result in controls)}/{len(controls)}, "
            f"detect {sum(result['passed'] for result in detects)}/{len(detects)}, "
            f"known gaps {len(gaps)}"
        )
        if args.out:
            args.out.write_text(report + "\n")
        if args.json_out:
            args.json_out.write_text(json.dumps(results, indent=2) + "\n")
        return 0 if all(result["passed"] for result in results) else 1
    finally:
        if "verifier" in locals():
            verifier.shutdown()
            verifier.server_close()
        if started_here and anvil is not None:
            anvil.terminate()
            anvil.wait(timeout=10)


class tempfile_dir:
    def __enter__(self) -> Path:
        import tempfile

        self._temp = tempfile.TemporaryDirectory(prefix="fluxion-redteam-")
        return Path(self._temp.name)

    def __exit__(self, *args: object) -> None:
        self._temp.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
