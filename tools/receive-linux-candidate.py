#!/usr/bin/env python3
"""Verify received Linux candidate bytes using an independently trusted local CLI.

Never execute a received binary/JRE or install/register a node. External plan,
receipt and verifier pins are required. Builder claims are not remote attestation
and this offline check never admits a runtime/executor service profile.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tarfile
import tempfile
import time

sys.dont_write_bytecode = True
import linux_build_contract as contract


def decode(raw):
    def unique(pairs):
        value = {}
        for key, item in pairs:
            contract.require(key not in value, "JSON_DUPLICATE_KEY")
            value[key] = item
        return value
    return json.loads(raw, object_pairs_hook=unique,
                      parse_constant=lambda _: (_ for _ in ()).throw(contract.Failure("JSON_NONFINITE")))


def verify_cli(cli, arguments):
    with tempfile.TemporaryDirectory(prefix="bxdl-linux-receive-") as temporary:
        out, err = Path(temporary) / "stdout", Path(temporary) / "stderr"
        with out.open("xb") as stdout, err.open("xb") as stderr:
            child = subprocess.Popen([str(cli), *arguments, "--allow-unsigned-development", "--json"],
                                     stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, cwd="/",
                                     env={"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"})
            deadline = time.monotonic() + 120
            while child.poll() is None:
                if time.monotonic() >= deadline or out.stat().st_size + err.stat().st_size > 4 * 1024 * 1024:
                    child.kill()
                    child.wait()
                    raise contract.Failure("VERIFIER_LIMIT")
                time.sleep(0.02)
        contract.require(child.returncode == 0 and err.stat().st_size == 0 and out.stat().st_size <= 4 * 1024 * 1024, "ARCHIVE_VERIFICATION_FAILED")
        value = decode(out.read_bytes())
        contract.require(isinstance(value, dict) and value.get("outcome") == "SUCCEEDED" and isinstance(value.get("data"), dict), "ARCHIVE_VERIFICATION_FAILED")
        return value["data"]


def entry(manifest, name):
    matches = [item for item in manifest.get("files", []) if item.get("path") == name]
    contract.require(len(matches) == 1, "PAYLOAD_REQUIRED")
    return matches[0]


def same_pin(observed, expected):
    contract.require(isinstance(expected, dict) and all(observed.get(key) == expected.get(key) for key in ("sha256", "size")), "RECEIPT_PIN_MISMATCH")


def elf_member(archive, name):
    # The trusted Rust verifier has checked the complete streaming archive first.
    # Read only the ELF header and never extract or execute received code.
    with tarfile.open(archive, "r:gz") as tar:
        members = [item for item in tar if item.name == name]
        contract.require(len(members) == 1 and members[0].isreg() and members[0].size >= 64, "ELF_REQUIRED")
        with tar.extractfile(members[0]) as incoming:
            raw = incoming.read(64)
    contract.require(raw[:7] == b"\x7fELF\x02\x01\x01" and len(raw) == 64
                     and struct.unpack_from("<HHI", raw, 16)[0] in (2, 3)
                     and struct.unpack_from("<HHI", raw, 16)[1:] == (62, 1), "LINUX_ELF_REQUIRED")


def receive(kit, plan_sha, received, receipt_sha, cli, cli_sha):
    plan = contract.load_plan(kit, plan_sha)
    contract.require(re.fullmatch(r"[0-9a-f]{64}", receipt_sha) is not None
                     and re.fullmatch(r"[0-9a-f]{64}", cli_sha) is not None, "HASH_INVALID")
    receipt_path = received / "receipt.json"
    receipt_pin = contract.fingerprint(receipt_path)
    contract.require(receipt_pin["sha256"] == receipt_sha and receipt_pin["size"] <= 256 * 1024, "RECEIPT_PIN_MISMATCH")
    receipt = decode(receipt_path.read_bytes())
    contract.require(isinstance(receipt, dict) and type(receipt.get("schemaVersion")) is int
                     and receipt["schemaVersion"] == 1 and receipt.get("kind") == "bxdl-linux-native-build-receipt"
                     and receipt.get("status") == "NATIVE_BUILT_PACKAGE_PINNED_DEVELOPMENT"
                     and receipt.get("outcome") == "PASS"
                     and receipt.get("scope") == "LINUX_NATIVE_DEVELOPMENT_CANDIDATE_BUILD_ONLY"
                     and receipt.get("inputsUnchanged") is True and receipt.get("developmentOnly") is True
                     and isinstance(receipt.get("artifacts"), dict) and isinstance(receipt.get("binaries"), dict)
                     and receipt.get("planSha256") == plan_sha and receipt.get("sourceCommit") == plan["source"]["commit"], "RECEIPT_IDENTITY_MISMATCH")
    cli_pin = contract.fingerprint(cli)
    contract.require(cli_pin["sha256"] == cli_sha and cli_pin["mode"] & 0o111, "LOCAL_VERIFIER_PIN_MISMATCH")
    packages = {}
    observed_pins = {}
    for role, filename, arguments in [("runtime", "runtime.tar.gz", ["package", "verify"]), ("management", "management-cli.tar.gz", ["cli", "bundle", "verify"])]:
        expected = receipt.get("artifacts", {}).get(role, {})
        contract.require(isinstance(expected, dict) and expected.get("path") == filename
                         and expected.get("authenticity") == "unsigned-development", "RECEIPT_PATH_INVALID")
        path = received / filename
        before = contract.fingerprint(path)
        same_pin(before, expected)
        report = verify_cli(cli, arguments + [str(path)])
        contract.require(report.get("authenticity") == "unsigned-development"
                         and report.get("manifestSha256") == expected.get("manifestSha256"), "MANIFEST_PIN_MISMATCH")
        contract.require(contract.fingerprint(path) == before, "RECEIVED_INPUT_CHANGED")
        packages[role] = report["manifest"]
        observed_pins[role] = before
    runtime, management = packages["runtime"], packages["management"]
    runtime_lock = decode((kit / plan["supply"]["runtimeLock"]["path"]).read_bytes())
    engine_lock = decode((kit / plan["supply"]["engineLock"]["path"]).read_bytes())
    contract.require(runtime["channel"] == "development" and runtime["product"]["name"] == "BXDL"
                     and runtime["product"]["revision"] == plan["source"]["commit"]
                     and management["revision"] == plan["source"]["commit"] and management["target"] == "linux-amd64"
                     and runtime["platform"] == {"os": "linux", "arch": "amd64", "libc": "glibc", "minGlibc": "2.34", "javaMajor": 21, "backend": "rocksdb"}, "PACKAGE_IDENTITY_MISMATCH")
    contract.require(runtime["engine"]["jarSha256"] == engine_lock["jarSha256"]
                     and runtime["engine"]["revision"] == engine_lock["expected"]["source"]["commit"]
                     and runtime["engine"]["contractRevision"] == engine_lock["expected"]["contract"]["fingerprint"]
                     and runtime["runtime"]["javaSha256"] == runtime_lock["javaSha256"], "SUPPLY_IDENTITY_MISMATCH")
    expected_runtime = {"runtime/" + item["path"]: item for item in runtime_lock["files"]}
    actual_runtime = {item["path"]: item for item in runtime["files"] if item["path"].startswith("runtime/")}
    contract.require(set(actual_runtime) == set(expected_runtime), "RUNTIME_INVENTORY_MISMATCH")
    for name, expected in expected_runtime.items():
        actual = actual_runtime[name]
        same_pin(actual, expected)
        contract.require(actual["mode"] == expected["mode"], "RUNTIME_INVENTORY_MISMATCH")
    binaries = {"executor": entry(runtime, "bin/bxdl"), "management": entry(management, "bin/bxdl"),
                "java": entry(runtime, "runtime/bin/java"), "jar": entry(runtime, "engine/nigo-node.jar")}
    for role, observed in binaries.items():
        same_pin(observed, receipt.get("binaries", {}).get(role))
    same_pin(binaries["jar"], plan["supply"]["jar"])
    contract.require(receipt.get("initialExecutorAndManagementSameBytes") is True, "RECEIPT_IDENTITY_MISMATCH")
    same_pin(binaries["executor"], binaries["management"])
    for archive, member in [("runtime.tar.gz", "bin/bxdl"), ("runtime.tar.gz", "runtime/bin/java"), ("management-cli.tar.gz", "bin/bxdl")]:
        elf_member(received / archive, member)
    for role, filename in [("runtime", "runtime.tar.gz"), ("management", "management-cli.tar.gz")]:
        contract.require(contract.fingerprint(received / filename) == observed_pins[role], "RECEIVED_INPUT_CHANGED")
    contract.require(contract.fingerprint(cli) == cli_pin and contract.fingerprint(receipt_path) == receipt_pin, "RECEIVED_INPUT_CHANGED")
    contract.load_plan(kit, plan_sha)
    return {"schemaVersion": 1, "outcome": "VERIFIED", "reasonCode": "LINUX_CANDIDATE_PINS_VERIFIED_OFFLINE",
            "sourceCommit": plan["source"]["commit"], "planSha256": plan_sha, "receiptSha256": receipt_sha,
            "artifacts": {key: {k: v[k] for k in ("sha256", "size")} for key, v in observed_pins.items()},
            "binaries": {key: {k: v[k] for k in ("sha256", "size")} for key, v in binaries.items()},
            "nativeBuild": "REPORTED_BY_BUILDER_NOT_REEXECUTED", "engineNativeAcceptance": "NOT_RUN",
            "systemdAcceptance": "NOT_RUN", "runtimeProfile": "NOT_ACCEPTED", "receivedCodeExecutions": 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kit", type=Path, required=True)
    parser.add_argument("--plan-sha256", required=True)
    parser.add_argument("--received", type=Path, required=True)
    parser.add_argument("--receipt-sha256", required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--cli-sha256", required=True)
    args = parser.parse_args()
    paths = [Path(os.path.abspath(path)) for path in (args.kit, args.received, args.cli)]
    print(json.dumps(receive(paths[0], args.plan_sha256, paths[1], args.receipt_sha256, paths[2], args.cli_sha256), sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (contract.Failure, OSError, ValueError, KeyError, TypeError, AttributeError, tarfile.TarError, subprocess.SubprocessError) as error:
        code = getattr(error, "code", None) or (str(error) if isinstance(error, contract.Failure) else "LINUX_CANDIDATE_RECEIVE_FAILED")
        print(json.dumps({"schemaVersion": 1, "outcome": "FAILED", "reasonCode": code, "receivedCodeExecutions": 0}))
        raise SystemExit(3)
