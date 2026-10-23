#!/usr/bin/env python3
"""Opt-in installed-management-CLI / unchanged RC.3 Mac service acceptance.

Only new output roots are used. The prior fixture supplies read-only inputs, never
its control or database. All operations run through the installed stable entry;
resolving that entry to a payload executable would bypass CLI admission.
Failures preserve their original outcome and state, without retry or forced kill.
This module does not execute commands or create files when imported.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import stat
import subprocess

import support as h

PINS = {
    "archive": "8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017",
    "publicKey": "0f03b13417dbb9c362a8762afc7e45bedbf20a983024105b2dcbf55b4dc40f17",
    "engineLock": "2d6ebff73f8f9c741922b3324fc1d5fc162be64e53ce830cb546ee6fb5d8c344",
    "manifest": "2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123",
    "executor": "b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef",
    "jar": "05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f",
    "java": "9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c",
}
SUPPLY = {"package.tar.gz": "archive", "public-key.pem": "publicKey", "engine-lock.json": "engineLock"}
INPUT_NAMES = {
    "instance.json", "node.json", "chain.json", "validator.keystore", "validator.password",
    "peer-identity.p12", "peer-identity.password", "peer-trust.p12", "peer-trust.password",
}
APPLY_PROMPT = "위 준비 버전으로 설치를 적용하거나 완료된 결과를 확인하고 이름을 연결할까요?"
APPLY_FINISH = "설치·검사·등록·이름 연결을 확인했습니다."
DECLINE_FINISH = "설치 적용을 승인하지 않았습니다. 기존 자료를 보존했습니다."


def absolute_entry(path):
    """Preserve the caller's stable symlink path, including across activations."""
    return Path(os.path.abspath(path))


def snapshot(path):
    if not path.exists():
        return {}
    result = {}
    for p in [path, *sorted(path.rglob("*"))]:
        s = p.lstat()
        h.require(not stat.S_ISLNK(s.st_mode), "SNAPSHOT_SYMLINK")
        h.require(stat.S_ISREG(s.st_mode) or stat.S_ISDIR(s.st_mode), "SNAPSHOT_SPECIAL_FILE")
        result[str(p.relative_to(path))] = {
            "device": s.st_dev, "inode": s.st_ino, "mode": stat.S_IMODE(s.st_mode),
            "size": s.st_size, "mtimeNs": s.st_mtime_ns, "ctimeNs": s.st_ctime_ns,
            "sha256": h.sha(p) if p.is_file() else None,
        }
    return result


def remap_inputs(value, original, copied):
    if isinstance(value, dict):
        return {k: remap_inputs(v, original, copied) for k, v in value.items()}
    if isinstance(value, list):
        return [remap_inputs(v, original, copied) for v in value]
    if isinstance(value, str) and Path(value).is_absolute():
        p = Path(value)
        try:
            relative = p.relative_to(original)
        except ValueError:
            return value
        h.require(len(relative.parts) == 1 and relative.name in INPUT_NAMES, "FIXTURE_REFERENCE_OUTSIDE_INPUTS")
        return str(copied / relative)
    return value


def prepare(args, root):
    supply_source = (args.fixture / "supply" if args.fixture else args.supply).resolve()
    input_source = (args.fixture / "operational inputs" if args.fixture else args.inputs).resolve()
    h.require(supply_source.is_dir() and input_source.is_dir(), "FIXTURE_DIRECTORIES_REQUIRED")
    for source in (supply_source, input_source):
        h.require(not root.is_relative_to(source) and not source.is_relative_to(root), "OUTPUT_OVERLAPS_FIXTURE")
    h.require({p.name for p in input_source.iterdir()} == INPUT_NAMES, "FIXTURE_INPUT_MEMBERSHIP")
    source_hashes = {}
    for name, pin in SUPPLY.items():
        p = supply_source / name
        h.regular(p)
        source_hashes[str(p)] = h.sha(p)
        h.require(source_hashes[str(p)] == PINS[pin], "RC3_SUPPLY_PIN_" + pin.upper())
    for name in INPUT_NAMES:
        p = input_source / name
        h.regular(p)
        source_hashes[str(p)] = h.sha(p)
    product = h.document(input_source / "instance.json")
    native = h.document(input_source / "node.json")
    node = native["node"]
    h.require(product["role"] == "validator" and product["storage"]["backend"] == "rocksdb", "FIXTURE_ROLE_BACKEND")
    h.require(product["http"]["address"] == product["p2p"]["address"] == "127.0.0.1", "FIXTURE_NOT_LOOPBACK")
    h.require(node["nigo.protocol.consensus.auto-start"] is False and
              node["nigo.protocol.consensus.qbft.sync.enabled"] is False, "MANUAL_SINGLE_VALIDATOR_FIXTURE_REQUIRED")
    h.require(not any(".peers[" in k for k in node), "PEERLESS_FIXTURE_REQUIRED")
    h.require(args.instance_id != product["instanceId"], "FRESH_INSTANCE_ID_REQUIRED")
    for value in [product["chainDescription"], *product["secrets"].values(), native["chainFile"]]:
        p = Path(value)
        h.require(p.parent == input_source and p.name in INPUT_NAMES and not p.is_symlink(), "FIXTURE_REFERENCE_OUTSIDE_INPUTS")
    supply, inputs, data = root / "supply", root / "operational inputs", root / "fresh data"
    supply.mkdir(mode=0o700)
    inputs.mkdir(mode=0o700)
    for name in SUPPLY:
        h.copy_file(supply_source / name, supply / name, 0o600)
    for name in INPUT_NAMES - {"instance.json", "node.json"}:
        h.copy_file(input_source / name, inputs / name, 0o600)
    product = remap_inputs(product, input_source, inputs)
    native = remap_inputs(native, input_source, inputs)
    product["instanceId"] = args.instance_id
    product["http"]["port"], product["p2p"]["port"] = args.http_port, args.p2p_port
    product["storage"]["dataDirectory"] = str(data)
    native["dataDirectory"] = str(data)
    native["node"]["server.port"] = args.http_port
    native["node"]["nigo.protocol.consensus.qbft.node.listen-port"] = args.p2p_port
    h.write_json(inputs / "instance.json", product)
    h.write_json(inputs / "node.json", native)
    record = {
        "sources": source_hashes,
        "supply": {p.name: h.sha(p) for p in supply.iterdir()},
        "inputs": {p.name: h.sha(p) for p in inputs.iterdir()},
        "ports": [args.http_port, args.p2p_port], "newCredentialsIssued": False,
    }
    h.write_json(root / "preparation.json", record)
    return record


def inventory(package):
    manifest = h.document(package / "manifest.json")
    h.require(h.sha(package / "manifest.json") == PINS["manifest"], "RC3_MANIFEST_PIN")
    expected = {item["path"] for item in manifest["files"]} | {"manifest.json", "manifest.sig", ".bxdl-install.json"}
    h.require(expected == {str(p.relative_to(package)) for p in package.rglob("*") if p.is_file()}, "PACKAGE_MEMBERSHIP")
    for item in manifest["files"]:
        p = package / item["path"]
        s = h.regular(p)
        h.require(stat.S_IMODE(s.st_mode) == item["mode"] and s.st_size == item["size"] and
                  h.sha(p) == item["sha256"], "PACKAGE_FILE_IDENTITY")
    for path, key in [("bin/bxdl", "executor"), ("engine/nigo-node.jar", "jar"), ("runtime/bin/java", "java")]:
        h.require(h.sha(package / path) == PINS[key], "RC3_PIN_" + key.upper())
    count, runtime_count = len(manifest["files"]), sum(x["path"].startswith("runtime/") for x in manifest["files"])
    h.require(count == 270 and runtime_count == 259, "RC3_PAYLOAD_COUNT")
    return {"fileCount": count, "jreFileCount": runtime_count, "pins": PINS, "hashSizeModeMembership": "PASS"}



def diagnostic_summary(logs, diagnose, report_sha256):
    """Preserve the envelope outcome and reasonCode, including partial results."""
    return {
        "logsOutcome": logs["outcome"], "logsCode": logs.get("reasonCode"),
        "diagnoseOutcome": diagnose["outcome"], "diagnoseCode": diagnose.get("reasonCode"),
        "reportSha256": report_sha256,
    }


def data_identity(data):
    s = data.stat()
    return {"device": s.st_dev, "inode": s.st_ino, "engineInstanceSha256": h.sha(data / "engine-instance.json")}


def cleanup_observation(root, control, evidence, pids, ports):
    jobs = []
    for p in control.rglob("*.plist"):
        label = plistlib.loads(p.read_bytes())["Label"]
        q = subprocess.run(["launchctl", "print", f"gui/{os.getuid()}/{label}"], capture_output=True, timeout=10)
        h.require(q.returncode == 113 and b"Could not find service" in q.stderr, "JOB_ABSENCE_UNCONFIRMED")
        jobs.append({"label": label, "outcome": "NOT_LOADED"})
    h.require(bool(jobs), "SERVICE_JOB_EVIDENCE_MISSING")
    for p in control.rglob("report.jsonl"):
        for line in p.read_bytes().splitlines():
            try:
                record = json.loads(line)
            except (ValueError, UnicodeError):
                continue
            if type(record.get("pid")) is int:
                pids.add(record["pid"])
    pids.update(x["pid"] for x in evidence.children)
    for pid in pids:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            continue
        raise h.Failure("RECORDED_PID_PRESENT")
    rows = subprocess.check_output(["ps", "-axo", "pid=,command="], timeout=10).decode()
    h.require(not [line for line in rows.splitlines() if str(root) in line and
                   int(line.strip().split()[0]) not in (os.getpid(), os.getppid())], "RUN_ROOT_PROCESS_PRESENT")
    listeners = h.observe_listeners(ports)
    h.require(listeners["outcome"] == "PASS", "LISTENER_REMAINS_OR_UNKNOWN")
    return {"pidsChecked": len(pids), "remainingPids": 0, "jobs": jobs,
            "listeners": listeners, "remainingRunRootProcesses": 0, "outcome": "PASS",
            "automaticStopOnFailure": False, "operatorObservationRequired": False}


def execute(args):
    h.require(args.run, "EXPLICIT_RUN_REQUIRED")
    h.require(platform.system() == "Darwin" and platform.machine() == "arm64", "MAC_ARM64_REQUIRED")
    h.require(bool(args.fixture) != bool(args.supply or args.inputs), "SELECT_FIXTURE_OR_EXPLICIT_INPUTS")
    h.require(args.fixture or args.supply and args.inputs, "SUPPLY_AND_INPUTS_REQUIRED")
    h.require(re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}", args.instance_id), "INSTANCE_ID_INVALID")
    h.require(1 <= args.http_port <= 65535 and 1 <= args.p2p_port <= 65535 and args.http_port != args.p2p_port, "PORTS_INVALID")
    root = args.output.absolute()
    h.require(not root.is_symlink(), "OUTPUT_SYMLINK")
    if root.exists():
        h.require(root.is_dir() and not any(root.iterdir()), "FRESH_OUTPUT_REQUIRED")
        h.require(root.stat().st_uid == os.getuid() and not stat.S_IMODE(root.stat().st_mode) & 0o077, "OUTPUT_NOT_PRIVATE")
    else:
        root.mkdir(mode=0o700)
    root = root.resolve()
    entry = absolute_entry(args.manager_entry)
    h.require(entry.is_symlink(), "INSTALLED_STABLE_ENTRY_REQUIRED")
    cli_root = args.cli_root.resolve()
    h.require(not root.is_relative_to(cli_root) and not cli_root.is_relative_to(root), "OUTPUT_OVERLAPS_CLI_ROOT")
    home, control, package, data = root / "management home", root / "management home/instances/local", root / "management home/packages/local", root / "fresh data"
    ports = [args.http_port, args.p2p_port]
    h.require(h.observe_listeners(ports)["outcome"] == "PASS", "PORTS_OCCUPIED_OR_UNKNOWN")
    evidence = h.Evidence(root)
    own_source = Path(__file__).resolve()
    helper_source = Path(h.__file__).resolve()
    harness_hashes = {"run_management_service.py": h.sha(own_source), "support.py": h.sha(helper_source)}
    source_copy = evidence.logs / "harness-source"
    source_copy.mkdir(mode=0o700)
    h.copy_file(own_source, source_copy / "run_management_service.py", 0o600)
    h.copy_file(helper_source, source_copy / "support.py", 0o600)
    result = {
        "schemaVersion": 1, "outcome": "INCOMPLETE", "scope": "INSTALLED_MANAGEMENT_CLI_RC3_SINGLE_VALIDATOR_LOCAL_READINESS",
        "timestampUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "host": {"system": platform.system(), "machine": platform.machine(), "macVersion": platform.mac_ver()[0]},
        "managerEntry": str(entry), "entryTargetSha256": h.sha(entry), "cliRoot": str(cli_root), "harnessSha256": harness_hashes,
        "packageRebuilt": False, "engineRebuilt": False, "newCredentialsIssued": False,
        "globalConsensus": "NOT_CHECKED", "cases": [],
        "cleanup": {"outcome": "NOT_OBSERVED", "automaticStopOnFailure": False,
                    "operatorObservationRequired": True},
        "serviceMayRemain": False,
        "failurePolicy": "PRESERVE_STATE_FOR_EXPLICIT_OPERATOR_OBSERVATION_AND_NORMAL_STOP",
        "notTested": ["GLOBAL_QUORUM", "TRANSACTION_FINALITY", "LIVE_CLI_UPDATE", "LOGOUT_SLEEP", "CRASH_RECOVERY", "LINUX_SYSTEMD", "CLI05_04_HUMAN_TASKS"],
    }
    pids, terminals = set(), []

    def command(words, label, exits=(0,)):
        result["lastCommandStage"] = label
        response = h.command(entry, words, evidence, label, exits, 900)
        details = response.get("data", {})
        if isinstance(details, dict) and type(details.get("pid")) is int:
            pids.add(details["pid"])
        return response

    def note(name):
        result["cases"].append({"case": name, "outcome": "PASS"})
        evidence.note(name)

    def setup(action, named=True):
        return ["setup", action, *(["local"] if named else []), "--home", home]

    def named(action, *extra):
        return [action, "local", "--home", home, *extra]

    def terminal(words, label):
        tty = h.Terminal(entry, words, root, evidence, label, 900)
        terminals.append(tty)
        return tty

    def queries(label):
        before = snapshot(home)
        show = command(setup("show"), label + "-show")
        command(setup("list", False), label + "-list")
        h.require(snapshot(home) == before, "QUERY_MUTATED_" + label)
        return show["data"]

    def apply(label, answer):
        tty = terminal(setup("apply"), label)
        tty.expect(APPLY_PROMPT)
        tty.send(answer)
        tty.finish(APPLY_FINISH if answer == "y" else DECLINE_FINISH, 0 if answer == "y" else 5)

    try:
        prep = prepare(args, root)
        inputs, supply = root / "operational inputs", root / "supply"
        for field, value in h.document(inputs / "instance.json")["secrets"].items():
            if "Password" in field:
                evidence.redactions.append(Path(value).read_bytes().rstrip(b"\r\n"))
        result["managerVersion"] = command(["version"], "manager-version")["data"]
        cli_info = command(["cli", "info", "--root", cli_root], "cli-info")["data"]
        h.require(cli_info["status"] == "READY" and cli_info["current"] and
                  absolute_entry(cli_info["executable"]) == entry and
                  Path(cli_info["root"]).resolve() == cli_root, "INSTALLED_CLI_IDENTITY")
        result["managerInstallation"] = cli_info
        release_id = cli_info["current"]["releaseId"]
        h.require(isinstance(release_id, str) and Path(release_id).name == release_id and
                  release_id not in (".", ".."), "MANAGER_RELEASE_ID_INVALID")
        manager_payload = cli_root / "releases" / release_id / "payload/bin/bxdl"
        h.regular(manager_payload)
        h.require(entry.resolve() == manager_payload.resolve() and
                  Path(cli_info["invokedExecutable"]).resolve() == manager_payload.resolve(), "MANAGER_ENTRY_TARGET_MISMATCH")
        result["managerPayload"] = str(manager_payload)
        manager_sha = h.sha(manager_payload)
        h.require(manager_sha != PINS["executor"], "INDEPENDENT_MANAGER_REQUIRED")
        h.require(manager_sha == args.manager_sha256, "MANAGER_BINARY_PIN")
        h.require(cli_info["current"]["revision"] == result["managerVersion"]["revision"] and
                  cli_info["current"]["version"] == result["managerVersion"]["version"], "MANAGER_RELEASE_VERSION_MISMATCH")
        result["managerSha256"] = manager_sha
        note("INSTALLED_STABLE_ENTRY_AND_CURRENT_CLI_IDENTITY")
        tty = terminal(setup("new") + ["--from", inputs / "instance.json"], "prepare-local")
        prompts = [("이 준비 세션을 만들까요?", "y"), ("패키지 archive 파일", supply / "package.tar.gz"),
                   ("패키지 검증 방법", "1"), ("독립적으로 신뢰한 공개키 파일", supply / "public-key.pem"),
                   ("엔진 버전 고정 파일 (engine lock)", supply / "engine-lock.json"),
                   ("네트워크 설정 자료를 선택하세요.", "2"), ("기존 native 설정 파일", inputs / "node.json"),
                   ("이 내용으로 설정 파일을 준비할까요?", "y")]
        for prompt, answer in prompts:
            tty.answer(prompt, answer)
        tty.finish("설정 파일 준비를 마쳤습니다")
        h.require(not any((home / n).exists() for n in ["packages", "instances", "operations"]) and not data.exists(), "OPERATIONS_CREATED_EARLY")
        note("SEQUENTIAL_PREPARATION_CREATES_CONFIGURATION_ONLY")
        queries("prepared")
        note("PREPARED_LIST_SHOW_READ_ONLY")
        before = snapshot(home)
        apply("apply-decline", "n")
        h.require(snapshot(home) == before and not data.exists(), "DECLINE_MUTATED")
        note("APPLY_DECLINE_PRESERVES_PREPARATION_AND_CREATES_NO_NODE")
        apply("apply-approved", "y")
        h.require(package.is_dir() and control.is_dir() and not data.exists(), "APPLY_INITIALIZED_DATA")
        result["initialPackage"] = inventory(package)
        show = command(["instance", "show", "--instance", control], "registered-show")["data"]
        h.require(show["initialization"] == "NOT_STARTED", "INITIALIZED_DURING_APPLY")
        h.require(h.document(control / "binding.json")["schemaVersion"] == 2, "V2_BINDING_REQUIRED")
        nodes = command(["nodes", "--home", home], "nodes")["data"]
        h.require(len(nodes["entries"]) == 1 and nodes["entries"][0]["name"] == "local", "NAME_CONNECTION_MISSING")
        note("APPROVED_APPLY_INSTALLS_AND_REGISTERS_WITHOUT_INITIALIZING")
        result["appliedShow"] = queries("applied")
        note("APPLIED_LIST_SHOW_READ_ONLY")
        initialized = command(named("init", "--confirm-initialize", "--timeout-seconds", "120"), "named-init")["data"]
        h.require(initialized["initialization"] == "INITIALIZED", "INIT_NOT_CONFIRMED")
        identity = data_identity(data)
        result["dataIdentity"] = identity
        note("NAMED_EXPLICIT_INIT_CREATES_FRESH_DB")
        attempts = []
        for index in range(2):
            stage = f"attempt-{index + 1}"
            result["serviceMayRemain"] = True
            command(named("start", "--timeout-seconds", "120"), stage + "-start", (0, 5))
            ready = h.running(command(named("status", "--wait", "--timeout-seconds", "120"), stage + "-ready"), stage)
            h.require(ready["runtimeReadiness"] == "READY" and ready.get("reason") == "MANUAL_START_INITIALIZED", "LOCAL_MANUAL_READINESS_REQUIRED")
            h.require(not attempts or ready["attemptId"] != attempts[0]["attemptId"], "RESTART_ATTEMPT_NOT_NEW")
            attempts.append({"attemptId": ready["attemptId"], "pid": ready.get("pid"), "readinessReason": ready.get("reason")})
            note("NAMED_START_LOCAL_READY" if index == 0 else "SAME_DATA_RESTART_LOCAL_READY")
            if index == 0:
                logs = command(named("logs"), "named-logs", (0, 5))
                diagnose = command(named("diagnose", "--output", root / "diagnostic.json"), "named-diagnose", (0, 5))
                h.require(logs.get("outcome") in ("SUCCEEDED", "INCOMPLETE") and
                          diagnose.get("outcome") in ("SUCCEEDED", "INCOMPLETE") and (root / "diagnostic.json").is_file(), "DIAGNOSTIC_RESULT_MISSING")
                result["diagnostics"] = diagnostic_summary(logs, diagnose, h.sha(root / "diagnostic.json"))
                note("NAMED_LOGS_AND_DIAGNOSE_PRESERVE_ACTUAL_PARTIAL_STATUS")
            h.stopped(command(named("stop", "--timeout-seconds", "120"), stage + "-stop"), stage)
            result["serviceMayRemain"] = False
            h.require(data_identity(data) == identity, "DATA_IDENTITY_CHANGED")
            note("NAMED_NORMAL_STOP" if index == 0 else "RESTARTED_NODE_NORMAL_STOP")
        result["attempts"] = attempts
        stable = {str(p): snapshot(p) for p in (package, control, data)}
        apply("reapply-stopped", "y")
        h.require(stable == {str(p): snapshot(p) for p in (package, control, data)}, "REAPPLY_MUTATED_NODE")
        note("STOPPED_REAPPLY_PRESERVES_PACKAGE_CONTROL_DB_INODE_AND_CONTENT")
        result["stoppedShow"] = queries("stopped")
        note("STOPPED_LIST_SHOW_READ_ONLY")
        result["finalPackage"] = inventory(package)
        h.require(result["initialPackage"] == result["finalPackage"], "PACKAGE_CHANGED")
        for group, folder in [("supply", supply), ("inputs", inputs)]:
            for name, digest in prep[group].items():
                h.require(h.sha(folder / name) == digest, "COPIED_INPUT_CHANGED")
        for path, digest in prep["sources"].items():
            h.require(h.sha(Path(path)) == digest, "ORIGINAL_FIXTURE_CHANGED")
        workers = list(control.glob("operations/*/bxdl-worker"))
        h.require(len(workers) == 2 and all(h.sha(p) == PINS["executor"] for p in workers), "ORIGINAL_WORKER_REQUIRED")
        result["workerSnapshots"] = {"count": len(workers), "sha256": PINS["executor"], "allMatchOriginalExecutor": True}
        note("ORIGINAL_SUPPLY_PAYLOAD_JRE_EXECUTOR_AND_FIXTURE_UNCHANGED")
        final_info = command(["cli", "info", "--root", cli_root], "final-cli-info")["data"]
        h.require(final_info == cli_info and h.sha(manager_payload) == manager_sha and
                  h.sha(entry) == result["entryTargetSha256"], "MANAGER_CHANGED_DURING_RUN")
        result["cleanup"] = cleanup_observation(root, control, evidence, pids, ports)
        note("NO_RECORDED_PID_LAUNCHD_JOB_OR_TCP_LISTENER_REMAINS")
        result["harnessUnchanged"] = harness_hashes == {"run_management_service.py": h.sha(own_source), "support.py": h.sha(helper_source)}
        h.require(result["harnessUnchanged"], "HARNESS_CHANGED_DURING_RUN")
        for tty in terminals:
            h.require(not any(secret in tty.total for secret in evidence.redactions if secret), "CREDENTIAL_PRINTED")
        result["outcome"] = "PASS"
    except BaseException as error:
        result["failure"] = str(error) if isinstance(error, h.Failure) else type(error).__name__
        print("ACCEPTANCE_INCOMPLETE_" + result["failure"], flush=True)
    finally:
        for tty in terminals:
            tty.close()
        result["children"] = evidence.children
        result["observedPids"] = sorted(pids)
        h.write_json(root / "result.json", result)
        print("RESULT_" + result["outcome"], flush=True)
    return 0 if result["outcome"] == "PASS" else 1


def parser():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--run", action="store_true", help="explicitly authorize the isolated service scenario")
    p.add_argument("--manager-entry", type=Path, required=True)
    p.add_argument("--cli-root", type=Path, required=True)
    p.add_argument("--manager-sha256", required=True, help="expected installed payload binary digest, not the entry wrapper")
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--fixture", type=Path, help="prior CLI-04 root; only supply and operational inputs are read")
    p.add_argument("--supply", type=Path)
    p.add_argument("--inputs", type=Path)
    p.add_argument("--instance-id", required=True)
    p.add_argument("--http-port", type=int, required=True)
    p.add_argument("--p2p-port", type=int, required=True)
    return p


if __name__ == "__main__":
    raise SystemExit(execute(parser().parse_args()))
