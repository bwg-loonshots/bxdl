#!/usr/bin/env python3
"""Bridge a locally built Linux development candidate to installation inputs.

The operator supplies the independently retained build-plan, receipt and local
builder CLI hashes. Neither a downloaded executable nor a receipt alone is an
execution trust anchor. Verified payloads are copied into a new private folder;
this tool never invokes sudo, Java, setup, account creation or systemd.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shlex
import sys
import tarfile

sys.dont_write_bytecode = True
import linux_build_contract as contract


def sibling(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).resolve().with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


builder = sibling("operations_builder", "build-linux-candidate.py")
receiver = sibling("operations_receiver", "receive-linux-candidate.py")
MAX_PAYLOAD = 1024 * 1024 * 1024
MAX_FILES = 20000
HELPER = "/usr/local/libexec/bxdl/bxdl"


def new_output(path):
    path = contract._absolute(path)
    contract.require(not os.path.lexists(path), "NEW_OUTPUT_REQUIRED")
    return path


def pin(path):
    value = contract.fingerprint(path)
    return {key: value[key] for key in ("sha256", "size")}


def write_new(path, raw, mode=0o600):
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, mode)
    with os.fdopen(fd, "wb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    path.chmod(mode)


def extract_verified(archive, destination, manifest, manifest_sha, archive_pin):
    """Copy exact verified regular members; never tar-extract paths or metadata."""
    contract.require(pin(archive) == archive_pin, "PACKAGE_CHANGED")
    rows = manifest.get("files")
    contract.require(isinstance(rows, list) and 0 < len(rows) <= MAX_FILES, "PAYLOAD_INVALID")
    expected = {}
    total = 0
    for row in rows:
        contract.require(isinstance(row, dict), "PAYLOAD_INVALID")
        name = str(contract._relative(row.get("path")))
        contract.require(name not in ("manifest.json", "manifest.sig") and name not in expected
                         and type(row.get("size")) is int and 0 <= row["size"] <= MAX_PAYLOAD
                         and type(row.get("mode")) is int and row["mode"] in (0o644, 0o755)
                         and contract._hash(row.get("sha256")), "PAYLOAD_INVALID")
        total += row["size"]
        contract.require(total <= MAX_PAYLOAD, "PAYLOAD_LIMIT")
        expected[name] = row
    contract.require(not os.path.lexists(destination), "NEW_OUTPUT_REQUIRED")
    destination.mkdir(mode=0o700)
    seen = set()
    with tarfile.open(archive, "r:gz") as incoming:
        for member in incoming:
            name = str(contract._relative(member.name))
            contract.require(len(seen) <= len(expected) and name not in seen and member.isreg()
                             and not member.pax_headers and member.linkname == ""
                             and member.uid == 0 and member.gid == 0,
                             "ARCHIVE_MEMBER_INVALID")
            if not seen:
                contract.require(name == "manifest.json", "MANIFEST_REQUIRED")
            if name == "manifest.json":
                contract.require(0 < member.size <= contract.MAX_JSON and member.mode == 0o644,
                                 "MANIFEST_INVALID")
                with incoming.extractfile(member) as source:
                    raw = source.read(contract.MAX_JSON + 1)
                contract.require(len(raw) == member.size and hashlib.sha256(raw).hexdigest() == manifest_sha
                                 and receiver.decode(raw) == manifest, "MANIFEST_PIN_MISMATCH")
                write_new(destination / name, raw, 0o644)
            else:
                row = expected.get(name)
                contract.require(row is not None and member.size == row["size"] and member.mode == row["mode"],
                                 "ARCHIVE_MEMBER_INVALID")
                target = destination / name
                target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
                fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, row["mode"])
                count, digest = 0, hashlib.sha256()
                with incoming.extractfile(member) as source, os.fdopen(fd, "wb") as output:
                    while raw := source.read(contract.CHUNK):
                        count += len(raw)
                        contract.require(count <= row["size"], "PAYLOAD_CHANGED")
                        digest.update(raw)
                        output.write(raw)
                    output.flush()
                    os.fsync(output.fileno())
                target.chmod(row["mode"])
                contract.require(count == row["size"] and digest.hexdigest() == row["sha256"], "PAYLOAD_CHANGED")
            seen.add(name)
    contract.require(seen == set(expected) | {"manifest.json"} and pin(archive) == archive_pin,
                     "PACKAGE_CHANGED")
    return {name: {"sha256": row["sha256"], "size": row["size"], "mode": row["mode"]}
            for name, row in expected.items()}


def bootstrap_steps(worker, worker_sha):
    # The user must inspect existing root-owned ancestors first. Existing helper
    # files, including identical bytes and dangling symlinks, are never replaced.
    source = shlex.quote(str(worker))
    return [
        f"printf '%s  %s\\n' {worker_sha} {source} | sha256sum --check --strict",
        f"namei -l {HELPER}",
        "sudo /bin/sh -eu -c '\n"
        f"  test ! -e {HELPER}\n  test ! -L {HELPER}\n"
        "  check_root_dir() {\n"
        "    test -d \"$1\"\n    test ! -L \"$1\"\n"
        "    test \"$(/usr/bin/stat -c %u -- \"$1\")\" = 0\n"
        "    bxdl_dir_mode=$(/usr/bin/stat -c %a -- \"$1\")\n"
        "    test \"$((0$bxdl_dir_mode & 0022))\" = 0\n"
        "    test \"$((0$bxdl_dir_mode & 0001))\" = 1\n  }\n"
        "  for bxdl_dir in / /usr /usr/local; do check_root_dir \"$bxdl_dir\"; done\n"
        "  for bxdl_dir in /usr/local/libexec /usr/local/libexec/bxdl; do\n"
        "    if test ! -e \"$bxdl_dir\" && test ! -L \"$bxdl_dir\"; then\n"
        "      /usr/bin/mkdir -m 0755 -- \"$bxdl_dir\"\n    fi\n"
        "    check_root_dir \"$bxdl_dir\"\n  done\n'",
        "sudo /bin/sh -eu -c '\n"
        f"  test ! -e {HELPER}\n  test ! -L {HELPER}\n"
        f"  /usr/bin/install -o root -g root -m 0755 \"$1\" {HELPER}\n"
        "' bxdl-bootstrap " + source,
        f"printf '%s  %s\\n' {worker_sha} {HELPER} | sha256sum --check --strict",
    ]


def prepare(args):
    host = builder.native_host()  # Refuse foreign hosts/root before any child or write.
    contract.require(args.allow_unsigned_development, "EXPLICIT_DEVELOPMENT_REQUIRED")
    kit, built, cli = [contract._absolute(path) for path in (args.kit, args.built, args.cli)]
    output = new_output(args.output)
    for path in (kit, built):
        contract.require(not output.is_relative_to(path) and not path.is_relative_to(output), "OUTPUT_OVERLAP")
    contract.require(cli == built / "work/target/x86_64-unknown-linux-gnu/release/bxdl",
                     "LOCAL_BUILD_CLI_REQUIRED")
    # The supplied CLI was built locally by the operator. Its external hash is
    # checked before the receiver executes only package-verification commands.
    report = receiver.receive(kit, args.plan_sha256, built, args.receipt_sha256, cli, args.cli_sha256)
    contract.require(pin(cli) == report["binaries"]["executor"], "LOCAL_BUILD_CLI_MISMATCH")
    receipt_path = built / "receipt.json"
    receipt = receiver.decode(contract._read(receipt_path, 256 * 1024))
    contract.require(contract.digest(receipt_path) == args.receipt_sha256, "RECEIPT_PIN_MISMATCH")
    plans = {}
    for role, filename, command in (
        ("runtime", "runtime.tar.gz", ["package", "verify"]),
        ("management", "management-cli.tar.gz", ["cli", "bundle", "verify"]),
    ):
        contract.require(pin(built / filename) == report["artifacts"][role], "PACKAGE_CHANGED")
        manifest_report = receiver.verify_cli(cli, command + [str(built / filename)])
        contract.require(manifest_report.get("manifestSha256") == receipt["artifacts"][role]["manifestSha256"]
                         and manifest_report.get("authenticity") == "unsigned-development", "MANIFEST_PIN_MISMATCH")
        plans[role] = manifest_report["manifest"]
    contract.require(pin(cli) == report["binaries"]["executor"], "LOCAL_BUILD_CLI_MISMATCH")
    output.mkdir(mode=0o700)
    for role, filename, directory in (("runtime", "runtime.tar.gz", "runtime-package"),
                                       ("management", "management-cli.tar.gz", "management-cli")):
        extract_verified(built / filename, output / directory, plans[role],
                         receipt["artifacts"][role]["manifestSha256"], report["artifacts"][role])
    worker = output / "runtime-package/bin/bxdl"
    manager = output / "management-cli/bin/bxdl"
    contract.require(pin(worker) == report["binaries"]["executor"] and pin(manager) == report["binaries"]["management"],
                     "PREPARED_BINARY_MISMATCH")
    contract.load_plan(kit, args.plan_sha256)
    for role, filename in (("runtime", "runtime.tar.gz"), ("management", "management-cli.tar.gz")):
        contract.require(pin(built / filename) == report["artifacts"][role], "PREPARATION_INPUT_CHANGED")
    contract.require(contract.digest(receipt_path) == args.receipt_sha256 and pin(cli) == report["binaries"]["executor"],
                     "PREPARATION_INPUT_CHANGED")
    record = {
        "schemaVersion": 1, "kind": "bxdl-linux-operations-inputs", "outcome": "PREPARED",
        "scope": "PRIVATE_LINUX_DEVELOPMENT_INSTALL_INIT_SERVICE_TEST_INPUTS",
        "sourceCommit": report["sourceCommit"], "planSha256": args.plan_sha256,
        "buildReceiptSha256": args.receipt_sha256, "host": host,
        "artifacts": report["artifacts"], "binaries": report["binaries"],
        "manifestSha256": {role: receipt["artifacts"][role]["manifestSha256"] for role in ("runtime", "management")},
        "locations": {"runtime": str(output / "runtime-package/runtime"),
                      "jar": str(output / "runtime-package/engine/nigo-node.jar"),
                      "worker": str(worker), "managementCli": str(manager), "installer": HELPER},
        "commands": {"managementInstall": [str(manager), "cli", "install", "--from", str(output / "management-cli"), "--allow-unsigned-development"],
                     "hostCheck": [str(manager), "host", "check", "--json"],
                     "bootstrapManualSteps": bootstrap_steps(worker, report["binaries"]["executor"]["sha256"])},
        "bootstrapRequirements": {"rootOwnedAncestors": True, "noSymlinkAncestors": True,
                                  "noGroupOtherWritableAncestors": True, "otherSearchableAncestors": True,
                                  "refuseExistingHelper": True,
                                  "runStepsSeparatelyAfterInspection": True},
        "nodeInstallation": "NOT_RUN", "initialization": "NOT_RUN", "systemdAcceptance": "NOT_RUN",
        "engineNativeAcceptance": "NOT_RUN", "runtimeProfile": "NOT_ACCEPTED",
        "serviceMutations": 0, "jvmInvocations": 0,
        "resultMeaning": "VERIFIED_ARCHIVES_EXTRACTED_NOT_INSTALLED_OR_ACCEPTED",
    }
    write_new(output / "operations-plan.json", (json.dumps(record, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode())
    return record


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("kit", "built", "cli", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("plan-sha256", "receipt-sha256", "cli-sha256"):
        parser.add_argument("--" + name, required=True)
    parser.add_argument("--allow-unsigned-development", action="store_true")
    args = parser.parse_args(argv)
    os.umask(0o077)
    try:
        record = prepare(args)
        result = {key: record[key] for key in ("schemaVersion", "outcome", "scope", "nodeInstallation", "initialization", "systemdAcceptance", "serviceMutations")}
        result["reasonCode"] = "LINUX_OPERATION_INPUTS_PREPARED"
        exit_code = 0
    except (contract.Failure, OSError, ValueError, KeyError, TypeError, AttributeError, tarfile.TarError) as error:
        result = {"schemaVersion": 1, "outcome": "FAILED", "reasonCode": str(error) if isinstance(error, contract.Failure)
                  else "LINUX_OPERATION_PREPARATION_FAILED", "serviceMutations": 0}
        exit_code = 3
    print(json.dumps(result, sort_keys=True))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
