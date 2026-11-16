#!/usr/bin/env python3
"""Opt-in installed-management-CLI / explicitly pinned Mac runtime acceptance.

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

import runtime_profiles as rp
import prepare_management_fixture as mf

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


def draft_instance_id(args):
    if not getattr(args, "exercise_setup_recovery", False):
        return args.instance_id
    h.require(re.fullmatch(r"[a-z][a-z0-9-]{0,31}", args.instance_id) is not None,
              "RECOVERY_INSTANCE_ID_INVALID")
    value = "draft-" + hashlib.sha256(args.instance_id.encode()).hexdigest()[:16]
    h.require(value != args.instance_id, "RECOVERY_DRAFT_ID_COLLISION")
    return value


def preparation_state(detail, revision, instance_id, prepared, next_input=None):
    inspection = detail.get("inspection")
    h.require(isinstance(inspection, dict) and detail.get("application") is None,
              "PREPARATION_INSPECTION_REQUIRED")
    summary = inspection.get("summary", {})
    fields = inspection.get("fields", [])
    h.require(isinstance(fields, list) and all(isinstance(field, dict) for field in fields),
              "PREPARATION_FIELDS_REQUIRED")
    names = [field.get("name") for field in fields]
    h.require(all(isinstance(name, str) for name in names) and
              len(names) == len(set(names)) and "instanceId" in names, "PREPARATION_FIELDS_REQUIRED")
    values = {field["name"]: field.get("value") for field in fields}
    h.require(summary.get("planRevision") == revision and
              summary.get("preparation") == ("PREPARED_RECORDED" if prepared else "DRAFT") and
              summary.get("installation") == "NOT_PERFORMED" and
              inspection.get("verification") == ("VERIFIED" if prepared else "NOT_PREPARED") and
              inspection.get("nextInput") == next_input and values["instanceId"] == instance_id,
              "PREPARATION_STATE_MISMATCH")
    if next_input == "trustPolicy":
        h.require("archive" not in inspection.get("missingInputs", []) and
                  "trustPolicy" in inspection.get("missingInputs", []), "ARCHIVE_NOT_SAVED_BEFORE_CANCEL")
    return {"revision": revision, "preparation": summary["preparation"],
            "verification": inspection["verification"], "nextInput": inspection.get("nextInput")}


def revision_files(home, revision):
    generated = home / "sessions/local/generated"
    result = {}
    for kind in ("instance", "node"):
        path = generated / f"{kind}-{revision:04d}.json"
        metadata = h.regular(path)
        h.require(stat.S_IMODE(metadata.st_mode) == 0o600, "GENERATED_FILE_NOT_PRIVATE")
        result[kind] = {"path": path, "snapshot": snapshot(path), "document": h.document(path)}
    return result


def require_old_revision_unchanged(before):
    h.require(all(snapshot(item["path"]) == item["snapshot"] for item in before.values()),
              "PREVIOUS_GENERATED_REVISION_CHANGED")


def require_managed_application(detail, revision):
    inspection = detail.get("inspection", {})
    application = detail.get("application", {})
    summary = application.get("summary", {}) if isinstance(application, dict) else {}
    h.require(isinstance(inspection, dict) and isinstance(application, dict) and
              inspection.get("verification") == "NOT_CHECKED" and
              inspection.get("summary", {}).get("preparation") == "PREPARED_RECORDED" and
              application.get("record") == "READABLE" and application.get("nameBinding") == "CONNECTED" and
              application.get("runtime") == "NOT_OBSERVED" and summary.get("sourceMatch") == "MATCH" and
              summary.get("verification") == "PASSED" and summary.get("registrationConfirmed") is True and
              summary.get("installation") == "CONFIRMED" and summary.get("registration") == "CONFIRMED" and
              summary.get("preparationRevision") == revision, "MANAGED_APPLICATION_NOT_VERIFIED")


def prepare_session(args, terminal, setup, queries, home, inputs, supply, result, note):
    """Drive only preparation; optional recovery never starts or initializes a node."""
    recovery = getattr(args, "exercise_setup_recovery", False)
    tty = terminal(setup("new") + ["--from", inputs / "instance.json"], "prepare-local")
    tty.answer("이 준비 세션을 만들까요?", "y")
    tty.answer("패키지 archive 파일", supply / "package.tar.gz")
    if recovery:
        tty.answer("패키지 검증 방법", ":cancel")
        tty.finish("준비를 중단했습니다. 승인하여 저장한 자료는 보존됩니다.", 5)
        state = preparation_state(queries("cancelled-incomplete"), 0, draft_instance_id(args),
                                  False, "trustPolicy")
        h.require((home / "sessions/local/generated").is_dir() and
                  not any((home / "sessions/local/generated").iterdir()), "CANCEL_CREATED_GENERATED_FILES")
        result["setupRecovery"] = {"cancelled": state}
        note("ARCHIVE_SAVED_TRUST_CANCEL_INCOMPLETE_QUERIES_READ_ONLY")
        tty = terminal(setup("resume"), "resume-trust")
    for prompt, answer in [("패키지 검증 방법", "1"),
                           ("독립적으로 신뢰한 공개키 파일", supply / "public-key.pem"),
                           ("엔진 버전 고정 파일 (engine lock)", supply / "engine-lock.json"),
                           ("네트워크 설정 자료를 선택하세요.", "2"),
                           ("기존 native 설정 파일", inputs / "node.json"),
                           ("이 내용으로 설정 파일을 준비할까요?", "y")]:
        tty.answer(prompt, answer)
    tty.finish("설정 파일 준비를 마쳤습니다")
    if not recovery:
        return
    result["setupRecovery"]["firstPrepared"] = preparation_state(
        queries("resumed-prepared"), 1, draft_instance_id(args), True)
    old = revision_files(home, 1)
    result["setupRecovery"]["firstRevisionSnapshots"] = {kind: item["snapshot"] for kind, item in old.items()}
    tty = terminal(setup("edit") + ["--field", "instanceId"], "edit-instance-id")
    tty.answer("local · 인스턴스 이름 수정", args.instance_id)
    tty.finish("한 항목을 저장했습니다. 이전 생성 파일은 보존하며 새 내용은 다시 검토해야 합니다.")
    result["setupRecovery"]["edited"] = preparation_state(
        queries("edited-unreviewed"), 1, args.instance_id, False)
    require_old_revision_unchanged(old)
    result["setupRecovery"]["afterEditOldRevisionSnapshots"] = {kind: snapshot(item["path"]) for kind, item in old.items()}
    h.require(not (home / "sessions/local/generated/instance-0002.json").exists() and
              not (home / "sessions/local/generated/node-0002.json").exists(), "EDIT_PUBLISHED_WITHOUT_REVIEW")
    note("ONE_FIELD_EDIT_INVALIDATES_REVIEW_AND_PRESERVES_PRIOR_FILES")
    tty = terminal(setup("resume"), "resume-edited-review")
    tty.answer("이 내용으로 설정 파일을 준비할까요?", "y")
    tty.finish("설정 파일 준비를 마쳤습니다")
    result["setupRecovery"]["finalPrepared"] = preparation_state(
        queries("edited-reprepared"), 2, args.instance_id, True)
    require_old_revision_unchanged(old)
    result["setupRecovery"]["afterResumeOldRevisionSnapshots"] = {kind: snapshot(item["path"]) for kind, item in old.items()}
    new = revision_files(home, 2)
    expected = {**old["instance"]["document"], "instanceId": args.instance_id}
    h.require(new["instance"]["document"] == expected and
              new["node"]["document"] == old["node"]["document"] and
              h.sha(new["node"]["path"]) == h.sha(old["node"]["path"]), "EDIT_CHANGED_MORE_THAN_INSTANCE_ID")
    result["setupRecovery"].update({"oldFilesPreserved": True, "changedField": "instanceId",
                                    "finalRevision": 2, "onlyInstanceIdChanged": True})
    note("RESUME_APPROVAL_PUBLISHES_NEW_REVISION_WITH_FINAL_INSTANCE_ID")


def retained_supply(control, supply, expected):
    binding = h.document(control / "binding.json")
    h.require(binding.get("schemaVersion") == 2, "V2_BINDING_REQUIRED")
    provenance = binding.get("supply", {})
    fields = {"archive": "package.tar.gz", "publicKey": "public-key.pem", "lock": "engine-lock.json"}
    h.require(set(provenance) == set(fields), "MANAGED_SUPPLY_PROVENANCE_REQUIRED")
    for field, name in fields.items():
        retained = control / "trust" / name
        metadata = h.regular(retained)
        h.require(binding.get(field) == str(retained) and
                  provenance[field] == {"path": str(supply / name), "sha256": expected[name]} and
                  stat.S_IMODE(metadata.st_mode) == 0o600 and h.sha(retained) == expected[name],
                  "MANAGED_SUPPLY_IDENTITY")
    return {"schemaVersion": 2, "filesVerified": 3, "ownedTrustCopies": "MATCHED"}


class OwnedSupplyAbsence:
    """Temporarily move only the three copies owned by this fresh run root."""
    def __init__(self, root, expected):
        self.root, self.supply, self.held = root, root / "supply", root / "withheld supply"
        h.require(set(expected) == set(SUPPLY), "SUPPLY_ABSENCE_MEMBERSHIP")
        self.expected = dict(expected)
        self.directories, self.files = {}, {}
        self.active = False
        self.restore_attempted = False

    def directory_identity(self, path):
        metadata = path.lstat()
        h.require(stat.S_ISDIR(metadata.st_mode) and path.resolve(strict=True) == path and
                  metadata.st_uid == os.getuid(), "SUPPLY_ABSENCE_DIRECTORY")
        return metadata.st_dev, metadata.st_ino, stat.S_IMODE(metadata.st_mode)

    def file_identity(self, path, name):
        metadata = h.regular(path)
        h.require(h.sha(path) == self.expected[name], "SUPPLY_ABSENCE_INPUT_CHANGED")
        return metadata.st_dev, metadata.st_ino, metadata.st_size, stat.S_IMODE(metadata.st_mode)

    def check_directories(self):
        h.require(all(self.directory_identity(path) == identity for path, identity in self.directories.items()),
                  "SUPPLY_ABSENCE_DIRECTORY_CHANGED")

    def hide(self):
        h.require(not self.active and not os.path.lexists(self.held), "SUPPLY_ABSENCE_ALREADY_USED")
        self.directories = {path: self.directory_identity(path) for path in (self.root, self.supply)}
        h.require({p.name for p in self.supply.iterdir()} == set(SUPPLY), "SUPPLY_ABSENCE_MEMBERSHIP")
        self.files = {name: self.file_identity(self.supply / name, name) for name in SUPPLY}
        self.held.mkdir(mode=0o700)
        self.active = True
        self.directories[self.held] = self.directory_identity(self.held)
        for name in SUPPLY:
            self.check_directories()
            h.require(self.file_identity(self.supply / name, name) == self.files[name],
                      "SUPPLY_ABSENCE_INPUT_CHANGED")
            os.rename(self.supply / name, self.held / name)
        self.require_absent()

    def require_absent(self):
        self.check_directories()
        h.require(self.active and all(not os.path.lexists(self.supply / name) and
                  self.file_identity(self.held / name, name) == self.files[name] for name in SUPPLY),
                  "ORIGINAL_RUN_SUPPLY_NOT_ABSENT")

    def restore(self):
        if not self.active:
            return {"outcome": "NOT_MOVED", "filesRestored": 0}
        h.require(not self.restore_attempted, "SUPPLY_RESTORATION_ALREADY_ATTEMPTED")
        self.restore_attempted = True
        self.check_directories()
        errors = []
        for name in SUPPLY:
            try:
                self.check_directories()
                source, held = self.supply / name, self.held / name
                source_exists, held_exists = os.path.lexists(source), os.path.lexists(held)
                h.require(source_exists != held_exists, "SUPPLY_RESTORATION_COLLISION_OR_MISSING")
                current = source if source_exists else held
                h.require(self.file_identity(current, name) == self.files[name], "SUPPLY_RESTORATION_INPUT_CHANGED")
                if held_exists:
                    os.rename(held, source)
            except Exception as error:
                errors.append(str(error) if isinstance(error, h.Failure) else type(error).__name__)
        h.require(not errors, "SUPPLY_RESTORATION_FAILED_" + (errors[0] if errors else "UNKNOWN"))
        h.require(not any(self.held.iterdir()), "SUPPLY_RESTORATION_UNEXPECTED_MEMBER")
        self.held.rmdir()
        self.active = False
        return {"outcome": "PASS", "filesRestored": 3, "hashesAndFileIdentitiesPreserved": True}


def restore_owned_supply(relocation, result):
    if relocation is None or relocation.restore_attempted:
        return
    try:
        result["supplyRestoration"] = relocation.restore()
    except BaseException as error:
        reason = str(error) if isinstance(error, h.Failure) else type(error).__name__
        result["supplyRestoration"] = {"outcome": "INCOMPLETE", "reason": reason}
        result.setdefault("failure", "SUPPLY_RESTORATION_FAILED")
        result["outcome"] = "INCOMPLETE"


def prepare(args, root):
    profile = rp.runtime_profile(getattr(args, "candidate", "rc3"))
    pins = profile["pins"]
    supply_source = (args.fixture / "supply" if args.fixture else args.supply).resolve()
    input_source = (args.fixture / "operational inputs" if args.fixture else args.inputs).resolve()
    h.require(supply_source.is_dir() and input_source.is_dir(), "FIXTURE_DIRECTORIES_REQUIRED")
    for source in (supply_source, input_source):
        h.require(not root.is_relative_to(source) and not source.is_relative_to(root), "OUTPUT_OVERLAPS_FIXTURE")
    provenance = None
    if profile["candidate"] == "rc4":
        h.require(supply_source.parent == input_source.parent and
                  supply_source.name == "supply" and input_source.name == "operational inputs",
                  "RC4_PREPARED_SERVICE_FIXTURE_REQUIRED")
        provenance = mf.validate_service_fixture(input_source.parent)
    h.require({p.name for p in input_source.iterdir()} == INPUT_NAMES, "FIXTURE_INPUT_MEMBERSHIP")
    source_hashes = {}
    for name, pin in SUPPLY.items():
        p = supply_source / name
        h.regular(p)
        source_hashes[str(p)] = h.sha(p)
        h.require(source_hashes[str(p)] == pins[pin], profile["candidate"].upper() + "_SUPPLY_PIN_" + pin.upper())
    rp.verify_runtime_supply(supply_source / "package.tar.gz", supply_source / "public-key.pem",
                             supply_source / "engine-lock.json", profile["candidate"])
    if provenance is not None:
        source_hashes[str(input_source.parent / mf.RECORD)] = provenance["recordSha256"]
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
              node["nigo.protocol.consensus.qbft.sync.enabled"] is False, "MANUAL_SINGLE_NODE_FIXTURE_REQUIRED")
    h.require(not any(".peers[" in k for k in node), "PEERLESS_FIXTURE_REQUIRED")
    h.require(args.instance_id != product["instanceId"] and draft_instance_id(args) != product["instanceId"],
              "FRESH_INSTANCE_ID_REQUIRED")
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
    product["instanceId"] = draft_instance_id(args)
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
        "candidate": profile["candidate"], "runtimeProfile": profile["profile"], "freshFixtureProvenance": provenance,
    }
    h.write_json(root / "preparation.json", record)
    return record


def inventory(package, candidate="rc3"):
    verified = rp.verify_installed_payload(package, candidate)
    return {"fileCount": verified["files"], "jreFileCount": verified["runtimeFiles"],
            "bytes": verified["bytes"], "pins": rp.runtime_profile(candidate)["pins"],
            "hashSizeModeMembership": "PASS"}


def diagnostic_summary(logs, diagnose, report_sha256):
    """Preserve the envelope outcome and reasonCode, including partial results."""
    return {
        "logsOutcome": logs["outcome"], "logsCode": logs.get("reasonCode"),
        "diagnoseOutcome": diagnose["outcome"], "diagnoseCode": diagnose.get("reasonCode"),
        "reportSha256": report_sha256,
    }


def diagnostic_collection(response, require_complete=False):
    """Inspect actual collection results; policy-withheld raw streams are allowed."""
    data = response.get("data", {})
    h.require(isinstance(data, dict), "DIAGNOSTIC_DATA_INVALID")
    sources, events = data.get("sources", []), data.get("events", [])
    h.require(isinstance(sources, list) and isinstance(events, list) and
              all(isinstance(item, dict) for item in [*sources, *events]), "DIAGNOSTIC_COLLECTION_INVALID")
    initialization_sources = [item for item in sources if item.get("name") == "initialization.report"]
    initialization_events = [item for item in events if item.get("source") == "initialization.report"]
    if require_complete:
        h.require(response.get("outcome") == "SUCCEEDED" and data.get("partial") is False,
                  "COMPLETE_DIAGNOSTICS_REQUIRED")
        h.require(len(initialization_sources) == 1 and
                  initialization_sources[0].get("result") == "COLLECTED" and
                  initialization_sources[0].get("reason") == "PRIVATE_SNAPSHOT",
                  "INITIALIZATION_REPORT_NOT_COLLECTED")
        h.require(any(item.get("command") == "init" and item.get("status") == "INITIALIZED"
                      for item in initialization_events), "INITIALIZATION_EVENT_NOT_COLLECTED")
    return {"partial": data.get("partial"), "initializationSources": initialization_sources,
            "initializationEvents": initialization_events}


def report_observation(path):
    """Read the fresh init report without chmod, replacement, or permission repair."""
    def identity(metadata):
        return {"device": metadata.st_dev, "inode": metadata.st_ino,
                "mode": f"{stat.S_IMODE(metadata.st_mode):04o}", "bytes": metadata.st_size,
                "mtimeNs": metadata.st_mtime_ns, "ctimeNs": metadata.st_ctime_ns}
    before = identity(h.regular(path))
    digest = h.sha(path)
    h.require(identity(h.regular(path)) == before, "INITIALIZATION_REPORT_CHANGED_DURING_READ")
    return {**before, "sha256": digest}


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
    profile = rp.runtime_profile(getattr(args, "candidate", "rc3"))
    pins = profile["pins"]
    h.require(platform.system() == "Darwin" and platform.machine() == "arm64", "MAC_ARM64_REQUIRED")
    h.require(bool(args.fixture) != bool(args.supply or args.inputs), "SELECT_FIXTURE_OR_EXPLICIT_INPUTS")
    h.require(args.fixture or args.supply and args.inputs, "SUPPLY_AND_INPUTS_REQUIRED")
    h.require(re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}", args.instance_id), "INSTANCE_ID_INVALID")
    h.require(1 <= args.http_port <= 65535 and 1 <= args.p2p_port <= 65535 and args.http_port != args.p2p_port, "PORTS_INVALID")
    draft_instance_id(args)  # Validate the final edit answer before any output or CLI invocation.
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
    harness_sources = {"run_management_service.py": own_source, "support.py": helper_source,
                       "runtime_profiles.py": Path(rp.__file__).resolve(),
                       "prepare_management_fixture.py": Path(mf.__file__).resolve()}
    for name in ("fixture_guard.py", "candidates.py"):
        harness_sources[name] = own_source.parent / name
    harness_hashes = {name: h.sha(path) for name, path in harness_sources.items()}
    source_copy = evidence.logs / "harness-source"
    source_copy.mkdir(mode=0o700)
    for name, path in harness_sources.items():
        h.copy_file(path, source_copy / name, 0o600)
    result = {
        "schemaVersion": 1, "outcome": "INCOMPLETE", "scope": "INSTALLED_MANAGEMENT_CLI_" + profile["candidate"].upper() + "_MANUAL_SINGLE_NODE_LOCAL_READINESS",
        "candidate": profile["candidate"], "runtimeProfile": profile["profile"],
        "timestampUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "host": {"system": platform.system(), "machine": platform.machine(), "macVersion": platform.mac_ver()[0]},
        "managerEntry": str(entry), "entryTargetSha256": h.sha(entry), "cliRoot": str(cli_root), "harnessSha256": harness_hashes,
        "packageRebuilt": False, "engineRebuilt": False, "newCredentialsIssued": False,
        "globalConsensus": "NOT_CHECKED", "cases": [],
        "callerUmask": getattr(args, "caller_umask_observation", {"status": "NOT_RECORDED"}),
        "expectCompleteDiagnostics": getattr(args, "expect_complete_diagnostics", False),
        "exerciseSetupRecovery": getattr(args, "exercise_setup_recovery", False),
        "querySnapshots": {},
        "cleanup": {"outcome": "NOT_OBSERVED", "automaticStopOnFailure": False,
                    "operatorObservationRequired": True},
        "serviceMayRemain": False,
        "failurePolicy": "PRESERVE_STATE_FOR_EXPLICIT_OPERATOR_OBSERVATION_AND_NORMAL_STOP",
        "notTested": ["GLOBAL_QUORUM", "TRANSACTION_FINALITY", "LIVE_CLI_UPDATE", "LOGOUT_SLEEP", "CRASH_RECOVERY", "LINUX_SYSTEMD", "CLI05_04_HUMAN_TASKS"],
    }
    pids, terminals = set(), []
    relocation = None

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
        listed = command(setup("list", False), label + "-list")
        after = snapshot(home)
        proof = evidence.logs / (label + "-query-snapshot.json")
        h.write_json(proof, {"before": before, "after": after, "equal": before == after})
        result["querySnapshots"][label] = {"evidence": proof.name, "sha256": h.sha(proof),
                                            "showOutcome": show["outcome"], "listOutcome": listed["outcome"]}
        h.require(after == before, "QUERY_MUTATED_" + label)
        h.require(listed.get("data", {}).get("partial") is False and
                  [entry.get("name") for entry in listed.get("data", {}).get("entries", [])] == ["local"],
                  "PREPARATION_LIST_MEMBERSHIP")
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
        rp.require_manager_capability(result["managerVersion"], profile)
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
        h.require(manager_sha != pins["executor"], "INDEPENDENT_MANAGER_REQUIRED")
        h.require(manager_sha == args.manager_sha256, "MANAGER_BINARY_PIN")
        h.require(cli_info["current"]["revision"] == result["managerVersion"]["revision"] and
                  cli_info["current"]["version"] == result["managerVersion"]["version"], "MANAGER_RELEASE_VERSION_MISMATCH")
        result["managerSha256"] = manager_sha
        note("INSTALLED_STABLE_ENTRY_AND_CURRENT_CLI_IDENTITY")
        prepare_session(args, terminal, setup, queries, home, inputs, supply, result, note)
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
        result["initialPackage"] = inventory(package, profile["candidate"])
        show = command(["instance", "show", "--instance", control], "registered-show")["data"]
        h.require(show["initialization"] == "NOT_STARTED", "INITIALIZED_DURING_APPLY")
        binding = h.document(control / "binding.json")
        h.require(binding["schemaVersion"] == 2, "V2_BINDING_REQUIRED")
        h.require(binding["instanceId"] == args.instance_id, "REGISTERED_INSTANCE_ID_MISMATCH")
        nodes = command(["nodes", "--home", home], "nodes")["data"]
        h.require(len(nodes["entries"]) == 1 and nodes["entries"][0]["name"] == "local", "NAME_CONNECTION_MISSING")
        note("APPROVED_APPLY_INSTALLS_AND_REGISTERS_WITHOUT_INITIALIZING")
        if result["exerciseSetupRecovery"]:
            result["retainedSupply"] = retained_supply(control, supply, prep["supply"])
            relocation = OwnedSupplyAbsence(root, prep["supply"])
            relocation.hide()
            result["supplyAbsence"] = {"scope": "ONLY_CURRENT_RUN_SUPPLY", "filesMoved": 3,
                                      "originalFixtureSupplyMoved": False, "outcome": "ABSENT_VERIFIED"}
            note("OWN_RUN_SUPPLY_ABSENT_RETAINED_V2_COPIES_VERIFIED")
        result["appliedShow"] = queries("applied")
        if relocation is not None:
            require_managed_application(result["appliedShow"], 2)
            relocation.require_absent()
        note("APPLIED_LIST_SHOW_READ_ONLY")
        initialized = command(named("init", "--confirm-initialize", "--timeout-seconds", "120"), "named-init")["data"]
        h.require(initialized["initialization"] == "INITIALIZED", "INIT_NOT_CONFIRMED")
        init_attempt = initialized.get("attemptId")
        h.require(isinstance(init_attempt, str) and
                  re.fullmatch(r"[A-Za-z0-9_-]{1,80}", init_attempt), "INIT_ATTEMPT_INVALID")
        init_report = control / "operations" / init_attempt / "report.jsonl"
        result["initializationReport"] = {"created": report_observation(init_report)}
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
                before_diagnostics = report_observation(init_report)
                h.require(before_diagnostics == result["initializationReport"]["created"],
                          "INITIALIZATION_REPORT_CHANGED_BEFORE_DIAGNOSTICS")
                logs = command(named("logs"), "named-logs", (0, 5))
                diagnose = command(named("diagnose", "--output", root / "diagnostic.json"), "named-diagnose", (0, 5))
                h.require(logs.get("outcome") in ("SUCCEEDED", "INCOMPLETE") and
                          diagnose.get("outcome") in ("SUCCEEDED", "INCOMPLETE") and (root / "diagnostic.json").is_file(), "DIAGNOSTIC_RESULT_MISSING")
                result["diagnostics"] = diagnostic_summary(logs, diagnose, h.sha(root / "diagnostic.json"))
                after_diagnostics = report_observation(init_report)
                result["initializationReport"].update({
                    "beforeDiagnostics": before_diagnostics, "afterDiagnostics": after_diagnostics,
                    "unchangedByDiagnostics": before_diagnostics == after_diagnostics,
                })
                h.require(before_diagnostics == after_diagnostics, "DIAGNOSTICS_MUTATED_INITIALIZATION_REPORT")
                complete = result["expectCompleteDiagnostics"]
                result["diagnostics"]["collection"] = {
                    "logs": diagnostic_collection(logs, complete),
                    "diagnose": diagnostic_collection(diagnose, complete),
                }
                if complete:
                    h.require(before_diagnostics["mode"] == "0600", "PRIVATE_INITIALIZATION_REPORT_REQUIRED")
                note("NAMED_LOGS_AND_DIAGNOSE_COLLECT_INITIALIZATION_REPORT" if complete else
                     "NAMED_LOGS_AND_DIAGNOSE_PRESERVE_ACTUAL_PARTIAL_STATUS")
            h.stopped(command(named("stop", "--timeout-seconds", "120"), stage + "-stop"), stage)
            result["serviceMayRemain"] = False
            h.require(data_identity(data) == identity, "DATA_IDENTITY_CHANGED")
            note("NAMED_NORMAL_STOP" if index == 0 else "RESTARTED_NODE_NORMAL_STOP")
        result["attempts"] = attempts
        stable = {str(p): snapshot(p) for p in (package, control, data)}
        apply("reapply-stopped", "y")
        after_reapply = {str(p): snapshot(p) for p in (package, control, data)}
        reapply_proof = evidence.logs / "stopped-reapply-snapshot.json"
        h.write_json(reapply_proof, {"before": stable, "after": after_reapply, "equal": stable == after_reapply})
        result["stoppedReapplySnapshot"] = {"evidence": reapply_proof.name, "sha256": h.sha(reapply_proof)}
        h.require(stable == after_reapply, "REAPPLY_MUTATED_NODE")
        note("STOPPED_REAPPLY_PRESERVES_PACKAGE_CONTROL_DB_INODE_AND_CONTENT")
        result["stoppedShow"] = queries("stopped")
        if relocation is not None:
            relocation.require_absent()
            require_managed_application(result["stoppedShow"], 2)
            result["supplyAbsence"]["lifecycleAndReapplyCompletedWhileAbsent"] = True
            restore_owned_supply(relocation, result)
            h.require(result["supplyRestoration"]["outcome"] == "PASS", "RUN_SUPPLY_RESTORATION_FAILED")
            note("OWN_RUN_SUPPLY_RESTORED_AFTER_LIFECYCLE_AND_REAPPLY")
        note("STOPPED_LIST_SHOW_READ_ONLY")
        result["initializationReport"]["final"] = report_observation(init_report)
        h.require(result["initializationReport"]["final"] == result["initializationReport"]["created"],
                  "INITIALIZATION_REPORT_CHANGED_AFTER_LIFECYCLE")
        result["finalPackage"] = inventory(package, profile["candidate"])
        h.require(result["initialPackage"] == result["finalPackage"], "PACKAGE_CHANGED")
        for group, folder in [("supply", supply), ("inputs", inputs)]:
            for name, digest in prep[group].items():
                h.require(h.sha(folder / name) == digest, "COPIED_INPUT_CHANGED")
        for path, digest in prep["sources"].items():
            h.require(h.sha(Path(path)) == digest, "ORIGINAL_FIXTURE_CHANGED")
        workers = list(control.glob("operations/*/bxdl-worker"))
        h.require(len(workers) == 2 and all(h.sha(p) == pins["executor"] for p in workers), "ORIGINAL_WORKER_REQUIRED")
        result["workerSnapshots"] = {"count": len(workers), "sha256": pins["executor"], "allMatchOriginalExecutor": True}
        note("ORIGINAL_SUPPLY_PAYLOAD_JRE_EXECUTOR_AND_FIXTURE_UNCHANGED")
        final_info = command(["cli", "info", "--root", cli_root], "final-cli-info")["data"]
        h.require(final_info == cli_info and h.sha(manager_payload) == manager_sha and
                  h.sha(entry) == result["entryTargetSha256"], "MANAGER_CHANGED_DURING_RUN")
        result["cleanup"] = cleanup_observation(root, control, evidence, pids, ports)
        note("NO_RECORDED_PID_LAUNCHD_JOB_OR_TCP_LISTENER_REMAINS")
        result["harnessUnchanged"] = harness_hashes == {name: h.sha(path) for name, path in harness_sources.items()}
        h.require(result["harnessUnchanged"], "HARNESS_CHANGED_DURING_RUN")
        for tty in terminals:
            h.require(not any(secret in tty.total for secret in evidence.redactions if secret), "CREDENTIAL_PRINTED")
        result["outcome"] = "PASS"
    except BaseException as error:
        result["failure"] = str(error) if isinstance(error, h.Failure) else type(error).__name__
        print("ACCEPTANCE_INCOMPLETE_" + result["failure"], flush=True)
    finally:
        restore_owned_supply(relocation, result)
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
    p.add_argument("--candidate", choices=sorted(rp.RUNTIME_PROFILES), default="rc3",
                   help="explicit fixed runtime; legacy RC.3 remains the default")
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
    p.add_argument("--caller-umask", choices=("002", "022", "077"),
                   help="record and apply this harness umask before any CLI/PTY child; default records the inherited mask")
    p.add_argument("--exercise-setup-recovery", action="store_true",
                   help="also exercise cancel/resume, one-field edit and absent original run supply")
    p.add_argument("--expect-complete-diagnostics", action="store_true",
                   help="require complete logs/diagnose collection of the private initialization report")
    return p


def main(argv=None):
    args = parser().parse_args(argv)
    if not args.run:
        return execute(args)
    # This standalone, single-threaded entry point owns its process mask. Never
    # change a caller's process mask from execute() or parallel offline tests.
    inherited = os.umask(0o077)
    applied = inherited if args.caller_umask is None else int(args.caller_umask, 8)
    os.umask(applied)
    args.caller_umask_observation = {
        "requested": args.caller_umask, "inherited": f"{inherited:03o}", "applied": f"{applied:03o}",
        "scope": "HARNESS_PROCESS_BEFORE_CLI_AND_PTY_SPAWN", "cliMayOverrideInheritedMask": True,
    }
    try:
        return execute(args)
    finally:
        os.umask(inherited)


if __name__ == "__main__":
    raise SystemExit(main())
