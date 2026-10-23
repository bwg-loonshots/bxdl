#!/usr/bin/env python3
"""Explicit Mac acceptance of an installed manager and the unchanged RC.3 runtime.

Only fresh test-owned output/catalog/data are accepted. The existing network
oracles are reused, but the runtime archive is never rebuilt. Import is inert.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import stat
import subprocess

import fixture_guard as fg
import run_acceptance as network
import support as h
from candidates import CANDIDATES

ARCHIVE_SHA = "8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017"
MANIFEST_SHA = "2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123"
EXECUTOR_SHA = "b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef"
EXECUTOR_SIZE = 3024064
PUBLIC_SHA = "0f03b13417dbb9c362a8762afc7e45bedbf20a983024105b2dcbf55b4dc40f17"
LOCK_SHA = "2d6ebff73f8f9c741922b3324fc1d5fc162be64e53ce830cb546ee6fb5d8c344"
PROFILE = "BXDL_RC3_EXECUTOR_V1"
NAMED = {"init", "resume-init", "start", "status", "stop", "logs", "diagnose", "preflight"}


def stable_entry(path):
    """Preserve the public symlink, not its current release's resolved target."""
    path = Path(path).expanduser()
    h.require(path.is_absolute() and path == Path(os.path.normpath(str(path)))
              and path.name == "bxdl" and path.is_symlink(), "STABLE_MANAGER_ENTRY_REQUIRED")
    return path


def named_arguments(node, arguments, home):
    values = list(arguments)
    if node is None or not values or values[0] not in NAMED:
        return values
    h.require(values.count("--instance") == 1 and "--home" not in values,
              "NAMED_MAPPING_INPUT")
    index = values.index("--instance")
    h.require(index + 1 < len(values) and Path(values[index + 1]) == node["control"],
              "NAMED_MAPPING_TARGET")
    del values[index:index + 2]
    return [values[0], node["label"], "--home", home, *values[1:]]


def verify_payload(package):
    h.require(h.sha(package / "manifest.json") == MANIFEST_SHA, "RUNTIME_MANIFEST_PIN")
    manifest = h.document(package / "manifest.json")
    files = manifest["files"]
    h.require(len(files) == 270 and len({f["path"] for f in files}) == 270,
              "RUNTIME_MEMBER_COUNT")
    paths = set()
    for path in package.rglob("*"):
        h.require(not path.is_symlink(), "RUNTIME_SYMLINK")
        if not path.is_dir():
            paths.add(path.relative_to(package).as_posix())
    h.require(paths == {f["path"] for f in files}
              | {"manifest.json", "manifest.sig", ".bxdl-install.json"}, "RUNTIME_MEMBERSHIP")
    for item in files:
        relative = Path(item["path"])
        h.require(not relative.is_absolute() and ".." not in relative.parts, "RUNTIME_PATH")
        path = package / relative
        metadata = h.regular(path)
        h.require(metadata.st_size == item["size"]
                  and stat.S_IMODE(metadata.st_mode) == item["mode"]
                  and h.sha(path) == item["sha256"], "RUNTIME_FILE_CHANGED")
    h.require(h.sha(package / "bin/bxdl") == EXECUTOR_SHA
              and (package / "bin/bxdl").stat().st_size == EXECUTOR_SIZE, "EXECUTOR_PIN")
    h.require(h.sha(package / "engine/nigo-node.jar") == CANDIDATES["rc3"]["jarSha256"]
              and (package / "engine/nigo-node.jar").stat().st_size == CANDIDATES["rc3"]["jarSizeBytes"],
              "ENGINE_PIN")
    h.require(h.sha(package / "runtime/bin/java") == network.JAVA_SHA, "JAVA_PIN")
    runtime = [f for f in files if f["path"].startswith("runtime/")]
    h.require(len(runtime) == 259 and sum(f["size"] for f in files) == 325299323,
              "RUNTIME_TOTALS")
    return {"files": len(files), "bytes": sum(f["size"] for f in files),
            "runtimeFiles": len(runtime), "manifestSha256": MANIFEST_SHA,
            "executorSha256": EXECUTOR_SHA, "engineJarSha256": CANDIDATES["rc3"]["jarSha256"],
            "javaSha256": network.JAVA_SHA}


def initialization_journal(control):
    journal = control / "journal"
    h.require(stat.S_ISDIR(journal.lstat().st_mode), "INITIALIZATION_JOURNAL_DIRECTORY")
    files = sorted(journal.iterdir())
    h.require(0 < len(files) <= 4096, "INITIALIZATION_JOURNAL_MEMBERSHIP")
    inventory = {}
    for path in files:
        h.require(re.fullmatch(r"revision[0-9]{8}\.json", path.name) is not None,
                  "INITIALIZATION_JOURNAL_MEMBER")
        h.regular(path)
        inventory[path.name] = h.sha(path)
    return inventory


def preserved_node_identity(node, attempt):
    h.require(isinstance(attempt, str) and re.fullmatch(r"[A-Za-z0-9-]+", attempt) is not None,
              "LIVE_ATTEMPT_PATH")
    data = node["base"] / "data"
    metadata = data.lstat()
    h.require(stat.S_ISDIR(metadata.st_mode), "LIVE_DATA_DIRECTORY")
    paths = {"engineIdentitySha256": data / "engine-instance.json",
             "productSha256": node["product"], "nativeSha256": node["native"],
             "bindingSha256": node["control"] / "binding.json",
             "workerSha256": node["control"] / "operations" / attempt / "bxdl-worker"}
    hashes = {}
    for key, path in paths.items():
        h.regular(path)
        hashes[key] = h.sha(path)
    h.require(hashes["workerSha256"] == EXECUTOR_SHA
              and paths["workerSha256"].stat().st_size == EXECUTOR_SIZE, "LIVE_WORKER_EXECUTOR_PIN")
    return {"dataDirectory": {"device": metadata.st_dev, "inode": metadata.st_ino},
            "initializationJournal": initialization_journal(node["control"]), **hashes}


def report_processes(path):
    """Malformed, missing or partial engine evidence is UNKNOWN, never absence."""
    try:
        metadata = h.regular(path)
        h.require(0 < metadata.st_size <= h.MAX_CAPTURE, "REPORT_PARSE_UNKNOWN")
        with path.open("rb") as stream:
            raw = stream.read(h.MAX_CAPTURE + 1)
        h.require(len(raw) == metadata.st_size and raw.endswith(b"\n"), "REPORT_PARSE_UNKNOWN")
        records = [json.loads(line) for line in raw.splitlines()]
        h.require(0 < len(records) <= 4096, "REPORT_PARSE_UNKNOWN")
        for record in records:
            h.require(isinstance(record, dict) and record.get("attemptId") == path.parent.name
                      and type(record.get("pid")) is int and record["pid"] > 0
                      and record.get("command") in ("init", "run", "resume-init"), "REPORT_PARSE_UNKNOWN")
        commands = {record["command"] for record in records}
        h.require(len(commands) == 1, "REPORT_PARSE_UNKNOWN")
        return {record["pid"] for record in records}, commands.pop()
    except Exception:
        raise h.Failure("REPORT_PARSE_UNKNOWN") from None


def require_attempt_membership(workers, jobs, started):
    h.require(bool(started) and set(workers) == started and len(workers) == len(started),
              "WORKER_ATTEMPT_MEMBERSHIP")
    h.require(set(jobs) == started and len(jobs) == len(started), "JOB_ATTEMPT_MEMBERSHIP")


def root_processes(stdout, root, excluded):
    """Parse explicit ps observations; do not turn parse failure into no process."""
    try:
        found = []
        for line in stdout.decode("utf-8", errors="strict").splitlines():
            pid_text, command = line.strip().split(None, 1)
            pid = int(pid_text)
            h.require(pid > 0, "PROCESS_OBSERVATION_UNKNOWN")
            if str(root) in command and pid not in excluded:
                found.append(pid)
        return sorted(set(found))
    except Exception:
        raise h.Failure("PROCESS_OBSERVATION_UNKNOWN") from None


class ManagementAcceptance(network.Acceptance):
    def __init__(self, args):
        entry = stable_entry(args.manager_entry)
        for name in ("archive", "public_key", "lock", "fixture", "cli_root", "bundle_next"):
            setattr(args, name, getattr(args, name).expanduser().resolve(strict=True))
        args.output = args.output.expanduser()
        h.require(args.output.is_absolute() and not args.output.exists()
                  and args.output.parent.resolve(strict=True) == args.output.parent,
                  "NEW_ABSOLUTE_OUTPUT_REQUIRED")
        args.manager_home = args.manager_home.expanduser()
        h.require(args.manager_home.is_absolute()
                  and args.manager_home == args.output / "management-home"
                  and not args.manager_home.exists(), "NEW_TEST_MANAGEMENT_HOME_REQUIRED")
        h.require(entry.readlink() == args.cli_root / "current/bin/bxdl", "MANAGER_ENTRY_ROOT")
        h.require(re.fullmatch(r"[0-9a-f]{40}", args.revision) is not None, "FULL_REVISION_REQUIRED")
        args.candidate = "rc3"
        self.a = args
        self.candidate = CANDIDATES["rc3"]
        self.candidate_name, self.scenario = "rc3", "full"
        self.input_pins = fg.validate_inputs(args)
        for path, digest in ((args.archive, ARCHIVE_SHA), (args.public_key, PUBLIC_SHA), (args.lock, LOCK_SHA)):
            h.regular(path)
            h.require(h.sha(path) == digest, "FIXED_RUNTIME_SUPPLY_PIN")
        h.regular(args.bundle_next)
        self.bundle_next_sha = h.sha(args.bundle_next)
        self.root = args.output
        self.root.mkdir(mode=0o700)
        self.e = h.Evidence(self.root)
        self.cli = entry
        self.archive, self.public, self.lock = [self.root / n for n in
                                               ("candidate.tar.gz", "runtime-public.pem", "engine-lock.json")]
        h.copy_file(args.lock, self.lock, 0o600)
        self.nodes, self.running, self.unresolved_mutations = [], set(), {}
        self.round, self.submissions, self.recovery_resubmit_nonce = 0, {}, None
        self.observed_pids, self.started_attempts = set(), set()
        self.results = {"fixtureInputPins": self.input_pins, "candidate": "rc3", "scenario": "full",
                        "submissionAttempts": [], "managerTransitions": [],
                        "managerCommands": [], "nextBundleSha256": self.bundle_next_sha,
                        "runtimeProfile": PROFILE}
        self.freeze_harness()
        self.fixture = h.document(args.fixture / "fixture-summary.json")
        self.manifest = h.document(args.fixture / "generated/devnet.json")
        self.transactions = [h.document(args.fixture / f"transactions/tx-{i}.json")
                             for i in range(self.candidate["transactionCount"])]
        for tx in self.transactions:
            self.e.redactions.append(tx["rawTransaction"].encode())
        self.initial_manager = None
        self.next_manager = None

    def freeze_harness(self):
        source = Path(__file__).resolve().parent
        names = ("run_acceptance.py", "support.py", "fixture_guard.py", "prepare_fixture.py", "candidates.py",
                 "run_management_acceptance.py", "test_management_acceptance.py")
        self.harness_pins = {name: h.sha(source / name) for name in names}
        snapshot = self.e.logs / "harness-source"
        snapshot.mkdir(mode=0o700)
        for name, digest in self.harness_pins.items():
            h.copy_file(source / name, snapshot / name, 0o600)
            h.require(h.sha(snapshot / name) == digest, "HARNESS_SNAPSHOT_CHANGED")
        self.results["harnessSources"] = self.harness_pins

    def command(self, node, args, label, expected=(0,), timeout=700):
        values = named_arguments(node, args, self.a.manager_home)
        h.require(stable_entry(self.cli) == self.cli
                  and self.cli.readlink() == self.a.cli_root / "current/bin/bxdl", "MANAGER_ENTRY_CHANGED")
        binary_hash = h.sha(self.cli)
        self.results["managerCommands"].append({"stage": label, "command": str(values[0]),
                                                "namedTarget": node["label"] if node and args[0] in NAMED else None,
                                                "binarySha256": binary_hash})
        result = h.command(self.cli, values, self.e, label, expected, timeout)
        data = result.get("data", {})
        pid = data.get("pid")
        if type(pid) is int and pid > 0:
            self.observed_pids.add(pid)
        if node and args[0] == "start" and data.get("attemptId"):
            self.started_attempts.add((node["label"], data["attemptId"]))
        return result

    def manager_identity(self, label):
        info = self.command(None, ["cli", "info", "--root", self.a.cli_root], label + "-info")["data"]
        version = self.command(None, ["version"], label + "-version")["data"]
        current = info.get("current") or {}
        h.require(info.get("status") == "READY" and Path(info["executable"]) == self.cli
                  and Path(info["root"]).resolve(strict=True) == self.a.cli_root, "MANAGER_INFO_ENTRY")
        h.require(all(current.get(k) == version.get(k) for k in ("version", "revision")),
                  "MANAGER_VERSION_MISMATCH")
        release_id = current.get("releaseId")
        h.require(isinstance(release_id, str) and Path(release_id).name == release_id
                  and release_id not in (".", ".."), "MANAGER_RELEASE_ID_INVALID")
        payload = self.a.cli_root / "releases" / release_id / "payload/bin/bxdl"
        h.regular(payload)
        selected = self.cli.resolve(strict=True)
        h.require(selected == payload.resolve(strict=True)
                  and Path(info["invokedExecutable"]).resolve(strict=True) == selected,
                  "MANAGER_ENTRY_TARGET_MISMATCH")
        return {"version": version["version"], "revision": version["revision"],
                "manifestSha256": current["manifestSha256"], "binarySha256": h.sha(self.cli),
                "releaseId": current["releaseId"]}

    def package(self):
        self.initial_manager = self.manager_identity("initial-manager")
        h.require(self.initial_manager["revision"] != self.a.revision, "TWO_MANAGER_IDENTITIES_REQUIRED")
        self.results["initialManager"] = self.initial_manager
        h.copy_file(self.a.archive, self.archive, 0o600)
        h.copy_file(self.a.public_key, self.public, 0o600)
        verified = self.command(None, ["package", "verify", self.archive, "--public-key", self.public],
                                "fixed-runtime-verify")
        h.require(verified["data"]["authenticity"] == "verified-external-ed25519"
                  and verified["data"]["manifestSha256"] == MANIFEST_SHA
                  and h.sha(self.archive) == ARCHIVE_SHA, "FIXED_RUNTIME_AUTHENTICATION")
        self.results["package"] = {"archiveSha256": ARCHIVE_SHA, "manifestSha256": MANIFEST_SHA,
                                   "cliSha256": EXECUTOR_SHA, "engineSource": self.candidate["source"],
                                   "engineJarSha256": self.candidate["jarSha256"], "javaSha256": network.JAVA_SHA,
                                   "runtimeRebuilt": False, "publicKeySha256": PUBLIC_SHA, "lockSha256": LOCK_SHA}
        self.e.note("EXISTING_RC3_PACKAGE_AUTHENTICATED")

    def prepare_node(self, index, bad=False):
        fg.assert_inputs_unchanged(self.a, self.input_pins)
        label = "wrong-pin" if bad else f"n{index}"
        base = self.root / label
        base.mkdir(mode=0o700)
        product = copy.deepcopy(h.document(self.a.fixture / f"products/n{index}.json"))
        product["instanceId"] = "cli05-" + label + "-" + hashlib.sha256(str(self.root).encode()).hexdigest()[:8]
        product["storage"]["dataDirectory"] = str(base / "data")
        productpath = base / "instance.json"
        h.write_json(productpath, product)
        network_file = self.a.fixture / "network.json"
        if bad:
            values = copy.deepcopy(h.document(network_file))
            for i, peer in enumerate(values["nodes"]):
                if peer["nodeId"] != product["nodeId"]:
                    peer["credentialFingerprints"] = ["0x" + hashlib.sha256(f"wrong-expected-peer-{i}".encode()).hexdigest()]
            network_file = base / "wrong-network.json"
            h.write_json(network_file, values)
        native = base / "node.json"
        terminal = h.Terminal(self.cli, ["setup", "--network", "--workspace", base / "network-session",
                                       "--from", productpath, "--network-file", network_file, "--output", native],
                              self.root, self.e, label + "-network", 180)
        try:
            terminal.answer("TLS keystore 형식", "PKCS12")
            terminal.answer("TLS truststore 형식", "PKCS12")
            terminal.answer("네트워크 시작 방식", "new-network" if bad else "join-network")
            terminal.answer("엔진 실행 후 합의 참여 시작", "yes")
            terminal.answer("블록 아카이브 사용", "yes")
            terminal.expect(network.MENU)
            terminal.send("export")
            terminal.expect("[y/N] > ")
            terminal.send("yes")
            terminal.finish("SETUP_NETWORK_WRITTEN")
        finally:
            terminal.close()
        package = base / "package"
        installed = self.command(None, ["install", self.archive, "--destination", package,
                                       "--public-key", self.public], label + "-install")
        h.require(installed["outcome"] == "SUCCEEDED", "INSTALL_" + label)
        initial_payload = verify_payload(package)
        node = {"label": label, "index": index, "base": base, "cli": self.cli, "package": package,
                "control": base / "control", "port": product["http"]["port"],
                "p2pPort": product["p2p"]["port"], "product": productpath, "native": native,
                "initialPayload": initial_payload}
        self.nodes.append(node)
        registered = self.command(node, ["instance", "register", "--instance", node["control"],
                                        "--package", package, "--archive", self.archive,
                                        "--public-key", self.public, "--config", productpath,
                                        "--engine-config", native, "--lock", self.lock,
                                        "--allow-development", "--timeout-seconds", "120"], label + "-register")
        h.require(registered["data"]["initialization"] == "NOT_STARTED" and not (base / "data").exists(),
                  "REGISTER_CREATED_DATA_OR_STATE")
        self.command(None, ["nodes", "add", label, "--instance", node["control"],
                            "--home", self.a.manager_home], label + "-name")
        self.unresolved_mutations[label] = {"node": label, "phase": "INIT", "evidenceStage": label + "-init"}
        initialized = self.command(node, ["init", "--instance", node["control"], "--confirm-initialize",
                                         "--timeout-seconds", "180"], label + "-init")
        h.require(initialized["outcome"] == "SUCCEEDED", "INIT_" + label)
        node["genesisHash"] = h.document(base / "data/engine-instance.json")["genesisHash"]
        node["initialJournal"] = initialization_journal(node["control"])
        del self.unresolved_mutations[label]
        self.e.note("PREPARED_NAMED_" + label)
        return node

    def live_identity(self, nodes, label):
        values = {}
        for node in nodes:
            observed = self.command(node, ["status", "--instance", node["control"]], label + "-" + node["label"])
            data = h.running(observed, label)
            h.require(type(data.get("pid")) is int and data["pid"] > 0, "LIVE_PID_REQUIRED")
            identity = {k: data[k] for k in ("attemptId", "nodeInstanceId", "pid")}
            h.require(identity["attemptId"] == node["lastAttempt"]
                      and identity["nodeInstanceId"] == node["nodeInstanceId"], "LIVE_ATTEMPT_CHANGED")
            identity.update(preserved_node_identity(node, identity["attemptId"]))
            values[node["label"]] = identity
        return values

    def transition(self, action, nodes, label):
        before = self.live_identity(nodes, label + "-before")
        if action == "update":
            h.require(h.sha(self.a.bundle_next) == self.bundle_next_sha, "NEXT_BUNDLE_CHANGED")
            args = ["cli", "update", "--root", self.a.cli_root, "--from", self.a.bundle_next]
            if self.a.allow_unsigned_development:
                args.append("--allow-unsigned-development")
            expected = "UPDATED"
        else:
            args = ["cli", "rollback", "--root", self.a.cli_root]
            expected = "ROLLED_BACK"
        result = self.command(None, args, label + "-switch")
        h.require(result["data"]["status"] == expected and result["data"]["changed"] is True,
                  "MANAGER_NOT_SWITCHED")
        selected = self.manager_identity(label + "-selected")
        if action == "update":
            h.require(selected["revision"] == self.a.revision
                      and selected["binarySha256"] != self.initial_manager["binarySha256"], "NEXT_MANAGER_IDENTITY")
            if self.next_manager is None:
                self.next_manager = selected
            else:
                h.require(selected == self.next_manager, "NEXT_MANAGER_CHANGED")
        else:
            h.require(selected == self.initial_manager, "ROLLBACK_MANAGER_CHANGED")
        after = self.live_identity(nodes, label + "-after")
        h.require(after == before, "CLI_CHANGE_RESTARTED_NODE")
        self.results["managerTransitions"].append({"action": action, "selected": selected,
                                                   "runningNodes": sorted(before), "sameAttemptsAndPids": True,
                                                   "sameDataIdentityAndBoundInputs": True, "sameFixedWorker": True,
                                                   "before": before, "after": after})
        self.e.note("MANAGER_" + label.upper().replace("-", "_"))

    def start_all(self, nodes, label):
        super().start_all(nodes, label)
        if label == "first-start":
            self.transition("update", nodes, "update-live-four")

    def finality(self, nodes, txhash, label, seconds=90):
        result = super().finality(nodes, txhash, label, seconds)
        if label == "four-validator":
            self.transition("rollback", nodes, "rollback-live-four")
        elif label == "three-validator-while-n4-stopped":
            self.transition("update", nodes, "update-live-three")
        return result

    def audit_final(self):
        payloads, workers, jobs = {}, [], []
        report_pids, run_reports, init_reports = set(), set(), set()
        for node in self.nodes:
            payloads[node["label"]] = verify_payload(node["package"])
            h.require(payloads[node["label"]] == node["initialPayload"], "PAYLOAD_CHANGED_AFTER_RUN")
            h.require(initialization_journal(node["control"]) == node["initialJournal"],
                      "INITIALIZATION_JOURNAL_CHANGED_AFTER_RUN")
            for path in node["control"].glob("operations/*/bxdl-worker"):
                h.require(h.regular(path).st_size == EXECUTOR_SIZE and h.sha(path) == EXECUTOR_SHA,
                          "WORKER_EXECUTOR_CHANGED")
                workers.append((node["label"], path.parent.name))
            for path in node["control"].glob("operations/*/*.plist"):
                h.regular(path)
                label = plistlib.loads(path.read_bytes())["Label"]
                observed = subprocess.run(["launchctl", "print", f"gui/{os.getuid()}/{label}"],
                                          capture_output=True, timeout=10)
                h.require(observed.returncode == 113 and b"Could not find service" in observed.stderr,
                          "LAUNCHAGENT_NOT_ABSENT")
                jobs.append((node["label"], path.parent.name))
            for path in node["control"].glob("operations/*/report.jsonl"):
                pids, command = report_processes(path)
                report_pids.update(pids)
                if command == "run":
                    run_reports.add((node["label"], path.parent.name))
                else:
                    init_reports.add(node["label"])
        require_attempt_membership(workers, jobs, self.started_attempts)
        h.require(run_reports == self.started_attempts
                  and init_reports == {node["label"] for node in self.nodes}, "REPORT_ATTEMPT_MEMBERSHIP")
        h.require(len(self.results["managerTransitions"]) == 3 and self.next_manager is not None,
                  "MANAGER_TRANSITIONS_INCOMPLETE")
        h.require(self.manager_identity("final-manager") == self.next_manager, "FINAL_MANAGER_NOT_NEXT")
        pids = self.observed_pids | report_pids | {p["pid"] for p in self.e.children}
        for pid in pids:
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                continue
            raise h.Failure("RECORDED_PID_PRESENT")
        processes = subprocess.run(["ps", "-axo", "pid=,command="], capture_output=True, timeout=10)
        h.require(processes.returncode == 0 and not processes.stderr, "PROCESS_OBSERVATION_UNKNOWN")
        remaining = root_processes(processes.stdout, self.root, {os.getpid(), os.getppid()})
        h.require(not remaining, "RUN_ROOT_PROCESS_PRESENT")
        for path, digest in ((self.archive, ARCHIVE_SHA), (self.a.archive, ARCHIVE_SHA),
                             (self.public, PUBLIC_SHA), (self.a.public_key, PUBLIC_SHA),
                             (self.lock, LOCK_SHA), (self.a.lock, LOCK_SHA),
                             (self.a.bundle_next, self.bundle_next_sha)):
            h.require(h.sha(path) == digest, "RUNTIME_SUPPLY_CHANGED")
        return {"outcome": "PASS", "payloads": payloads, "workersChecked": len(workers),
                "allWorkersUseFixedExecutor": True, "jobsNotLoaded": len(jobs),
                "recordedPidsAbsent": len(pids), "reportPidsIncluded": len(report_pids),
                "runReportsChecked": len(run_reports), "rootProcessesAbsent": True,
                "initializationJournalsUnchanged": True,
                "supplyUnchanged": True}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--run", action="store_true")
    for name in ("manager-entry", "manager-home", "cli-root", "bundle-next", "archive", "public-key", "lock", "fixture", "output"):
        p.add_argument("--" + name, type=Path, required=True)
    p.add_argument("--revision", required=True, help="full next management CLI source revision")
    p.add_argument("--allow-unsigned-development", action="store_true")
    a = p.parse_args()
    if not a.run:
        print("No execution. Add --run for fresh test-owned manager/runtime acceptance.")
        return 0
    os.umask(0o077)
    h.require(platform.system() == "Darwin" and platform.machine() == "arm64", "MAC_ARM64_REQUIRED")
    acceptance = ManagementAcceptance(a)
    error = None
    try:
        acceptance.run()
    except (Exception, KeyboardInterrupt) as exc:
        error = str(exc) if isinstance(exc, h.Failure) else "HARNESS_" + type(exc).__name__
    finally:
        cleaned = acceptance.cleanup()
    try:
        fg.assert_inputs_unchanged(acceptance.a, acceptance.input_pins)
        if error is None and cleaned:
            acceptance.results["finalAudit"] = acceptance.audit_final()
    except Exception as exc:
        error = error or (str(exc) if isinstance(exc, h.Failure) else "FINAL_AUDIT_" + type(exc).__name__)
    preserved = acceptance.harness_unchanged()
    acceptance.results["harnessSourcesUnchanged"] = preserved
    if not preserved:
        error = error or "HARNESS_SOURCE_CHANGED_DURING_RUN"
    result = {"outcome": "PASS" if error is None and cleaned else "INCOMPLETE", "reason": error,
              "scope": "CLI05-03_LOCAL_MAC_INSTALLED_MANAGER_FIXED_RC3_NETWORK",
              "results": acceptance.results, "steps": acceptance.e.steps,
              "childProcesses": acceptance.e.children, "observedPids": sorted(acceptance.observed_pids)}
    h.write_json(a.output / "result.json", result)
    print(result["outcome"], error or "", str(a.output), flush=True)
    return 0 if result["outcome"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
