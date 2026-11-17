"""Offline receiver fixtures; trusted CLI reports are mocked, never admission evidence."""
import copy
import gzip
import importlib.util
import io
import json
from pathlib import Path
import sys
import tarfile
import unittest
from unittest import mock

import test_linux_build_contract as fixtures

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
SPEC = importlib.util.spec_from_file_location("receive_linux_candidate", ROOT / "tools/receive-linux-candidate.py")
R = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(R)


def package(files):
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name, raw in sorted(files.items()):
            info = tarfile.TarInfo(name)
            info.size = len(raw)
            info.mode = 0o755 if name.endswith("/bxdl") or name.endswith("/java") else 0o644
            archive.addfile(info, io.BytesIO(raw))
    return gzip.compress(stream.getvalue(), mtime=0)


def pin(raw):
    return {"sha256": fixtures.sha(raw), "size": len(raw)}


class ReceiveLinuxCandidateTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.ContractTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.tearDown)
        self.root = self.fixture.root
        self.worker, self.java = fixtures.elf(), fixtures.elf(kind=2)
        self.runtime_files = {"bin/java": self.java, "legal/java.base/LICENSE": b"fixture license"}
        def runtime(lock):
            lock["javaSha256"] = fixtures.sha(self.java)
            lock["files"] = [{"path": name, **pin(raw), "mode": 0o755 if name == "bin/java" else 0o644}
                             for name, raw in self.runtime_files.items()]
            return lock
        def engine(lock):
            lock["javaSha256"] = fixtures.sha(self.java)
            return lock
        self.kit, self.plan, self.plan_sha = self.fixture.kit(runtime_override=runtime, engine_override=engine)
        self.received = self.root / "received"
        self.received.mkdir()
        self.cli = self.fixture.write("trusted-local-cli", b"fixture never executed", 0o755)
        self.cli_sha = R.contract.digest(self.cli)
        self.runtime_payload = {"bin/bxdl": self.worker, "engine/nigo-node.jar": b"fixture-jar",
                                **{"runtime/" + key: value for key, value in self.runtime_files.items()}}
        self.management_payload = {"bin/bxdl": self.worker}
        self.manifests = {
            "runtime": {"schemaVersion": 1, "kind": "bxdl-package", "channel": "development",
                        "product": {"name": "BXDL", "version": "fixture", "revision": fixtures.COMMIT},
                        "platform": {"os": "linux", "arch": "amd64", "libc": "glibc", "minGlibc": "2.34", "javaMajor": 21, "backend": "rocksdb"},
                        "engine": {"jarSha256": fixtures.sha(b"fixture-jar"), "revision": "c" * 40, "contractRevision": "d" * 64},
                        "runtime": {"javaSha256": fixtures.sha(self.java)}, "files": self.rows(self.runtime_payload)},
            "management": {"schemaVersion": 1, "kind": "bxdl-management-cli", "version": "fixture", "revision": fixtures.COMMIT,
                           "target": "linux-amd64", "files": self.rows(self.management_payload)}}
        self.receipt = {"schemaVersion": 1, "kind": "bxdl-linux-native-build-receipt",
                        "scope": "LINUX_NATIVE_DEVELOPMENT_CANDIDATE_BUILD_ONLY", "outcome": "PASS",
                        "status": "NATIVE_BUILT_PACKAGE_PINNED_DEVELOPMENT", "sourceCommit": fixtures.COMMIT,
                        "planSha256": self.plan_sha, "inputsUnchanged": True, "developmentOnly": True,
                        "initialExecutorAndManagementSameBytes": True,
                        "artifacts": {}, "binaries": {"executor": pin(self.worker), "management": pin(self.worker),
                                                      "java": pin(self.java), "jar": pin(b"fixture-jar")}}
        self.replace_archive("runtime", self.runtime_payload)
        self.replace_archive("management", self.management_payload)
        self.receipt_sha = self.save_receipt()
        self.calls = []

    def rows(self, files):
        return [{"path": name, **pin(raw), "mode": 0o755 if name.endswith("/bxdl") or name.endswith("/java") else 0o644}
                for name, raw in files.items()]

    def replace_archive(self, role, files):
        name = "runtime.tar.gz" if role == "runtime" else "management-cli.tar.gz"
        raw = package(files)
        self.fixture.write("received/" + name, raw)
        self.receipt["artifacts"][role] = {"path": name, **pin(raw), "authenticity": "unsigned-development",
                                            "manifestSha256": fixtures.sha(fixtures.encoded(self.manifests[role]))}

    def save_receipt(self):
        return R.contract.digest(self.fixture.write("received/receipt.json", fixtures.encoded(self.receipt)))

    def verifier(self, cli, arguments):
        self.calls.append((cli, arguments))
        self.assertEqual(self.cli, cli)
        role = "runtime" if arguments[:2] == ["package", "verify"] else "management"
        return {"authenticity": "unsigned-development",
                "manifestSha256": self.receipt["artifacts"][role]["manifestSha256"],
                "manifest": copy.deepcopy(self.manifests[role])}

    def receive(self, verifier=None):
        with mock.patch.object(R, "verify_cli", side_effect=verifier or self.verifier), \
                mock.patch.object(R.subprocess, "Popen", side_effect=AssertionError("no real process in receipt fixtures")):
            return R.receive(self.kit, self.plan_sha, self.received, self.receipt_sha, self.cli, self.cli_sha)

    def test_valid_offline_fixture_preserves_nonacceptance_scope(self):
        result = self.receive()
        self.assertEqual("VERIFIED", result["outcome"])
        self.assertEqual("REPORTED_BY_BUILDER_NOT_REEXECUTED", result["nativeBuild"])
        self.assertEqual("NOT_RUN", result["engineNativeAcceptance"])
        self.assertEqual("NOT_RUN", result["systemdAcceptance"])
        self.assertEqual("NOT_ACCEPTED", result["runtimeProfile"])
        self.assertEqual(0, result["receivedCodeExecutions"])
        self.assertEqual(2, len(self.calls))
        self.assertEqual(self.receipt["binaries"]["executor"], result["binaries"]["executor"])

    def test_receipt_and_local_verifier_pins_are_independent(self):
        for field, code in (("receipt_sha", "RECEIPT_PIN_MISMATCH"), ("cli_sha", "LOCAL_VERIFIER_PIN_MISMATCH")):
            with self.subTest(field=field):
                original = getattr(self, field)
                setattr(self, field, "0" * 64)
                with self.assertRaisesRegex(R.contract.Failure, code):
                    self.receive()
                setattr(self, field, original)
        self.assertEqual([], self.calls)

    def test_received_archive_change_is_not_accepted_from_receipt_alone(self):
        (self.received / "runtime.tar.gz").write_bytes(b"changed archive")
        with self.assertRaisesRegex(R.contract.Failure, "RECEIPT_PIN_MISMATCH"):
            self.receive()
        self.assertEqual([], self.calls)

    def test_receipt_requires_successful_development_build_identity(self):
        original = copy.deepcopy(self.receipt)
        for key, value in (("schemaVersion", True), ("outcome", "FAILED"), ("scope", "ACCEPTED"),
                           ("inputsUnchanged", False), ("developmentOnly", False), ("sourceCommit", "e" * 40)):
            with self.subTest(field=key):
                self.receipt = copy.deepcopy(original)
                self.receipt[key] = value
                self.receipt_sha = self.save_receipt()
                with self.assertRaisesRegex(R.contract.Failure, "RECEIPT_IDENTITY_MISMATCH"):
                    self.receive()
        self.assertEqual([], self.calls)

    def test_malformed_receipt_has_stable_failure(self):
        self.receipt = []
        self.receipt_sha = self.save_receipt()
        with self.assertRaisesRegex(R.contract.Failure, "RECEIPT_IDENTITY_MISMATCH"):
            self.receive()

    def test_receipt_cannot_redirect_archive_path(self):
        self.receipt["artifacts"]["runtime"]["path"] = "../outside.tar.gz"
        self.receipt_sha = self.save_receipt()
        with self.assertRaisesRegex(R.contract.Failure, "RECEIPT_PATH_INVALID"):
            self.receive()
        self.assertEqual([], self.calls)

    def test_verifier_manifest_hash_must_match_independent_receipt(self):
        def wrong_hash(cli, arguments):
            result = self.verifier(cli, arguments)
            result["manifestSha256"] = "0" * 64
            return result
        with self.assertRaisesRegex(R.contract.Failure, "MANIFEST_PIN_MISMATCH"):
            self.receive(wrong_hash)

    def test_archive_change_during_trusted_verification_is_detected(self):
        def changed(cli, arguments):
            result = self.verifier(cli, arguments)
            with (self.received / "runtime.tar.gz").open("ab") as stream:
                stream.write(b"changed")
            return result
        with self.assertRaisesRegex(R.contract.Failure, "RECEIVED_INPUT_CHANGED"):
            self.receive(changed)

    def test_package_platform_or_revision_cannot_drift_from_plan(self):
        self.manifests["management"]["target"] = "darwin-arm64"
        with self.assertRaisesRegex(R.contract.Failure, "PACKAGE_IDENTITY_MISMATCH"):
            self.receive()

    def test_engine_and_runtime_inventory_must_match_source_locks(self):
        self.manifests["runtime"]["engine"]["revision"] = "e" * 40
        with self.assertRaisesRegex(R.contract.Failure, "SUPPLY_IDENTITY_MISMATCH"):
            self.receive()
        self.manifests["runtime"]["engine"]["revision"] = "c" * 40
        self.manifests["runtime"]["files"] = [row for row in self.manifests["runtime"]["files"]
                                                if row["path"] != "runtime/legal/java.base/LICENSE"]
        with self.assertRaisesRegex(R.contract.Failure, "RUNTIME_INVENTORY_MISMATCH"):
            self.receive()

    def test_initial_executor_and_management_must_be_same_bytes(self):
        alternative = fixtures.elf(kind=2)
        self.manifests["management"]["files"] = self.rows({"bin/bxdl": alternative})
        self.receipt["binaries"]["management"] = pin(alternative)
        self.receipt_sha = self.save_receipt()
        with self.assertRaisesRegex(R.contract.Failure, "RECEIPT_PIN_MISMATCH"):
            self.receive()

    def test_elf_member_is_read_but_wrong_architecture_is_rejected(self):
        wrong = fixtures.elf(machine=183)
        self.runtime_payload["bin/bxdl"] = wrong
        self.management_payload["bin/bxdl"] = wrong
        for role, payload in (("runtime", self.runtime_payload), ("management", self.management_payload)):
            self.manifests[role]["files"] = self.rows(payload)
            self.replace_archive(role, payload)
        self.receipt["binaries"]["executor"] = self.receipt["binaries"]["management"] = pin(wrong)
        self.receipt_sha = self.save_receipt()
        with self.assertRaisesRegex(R.contract.Failure, "LINUX_ELF_REQUIRED"):
            self.receive()

    def test_trusted_cli_execution_has_clean_environment_and_no_stdin(self):
        report = {"outcome": "SUCCEEDED", "data": {"fixture": "trusted report"}}
        def child(arguments, **kwargs):
            self.assertEqual(str(self.cli), arguments[0])
            self.assertEqual(["--allow-unsigned-development", "--json"], arguments[-2:])
            self.assertEqual(R.subprocess.DEVNULL, kwargs["stdin"])
            self.assertEqual("/", kwargs["cwd"])
            self.assertEqual({"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"}, kwargs["env"])
            kwargs["stdout"].write(fixtures.encoded(report))
            kwargs["stdout"].flush()
            return mock.Mock(returncode=0, poll=mock.Mock(return_value=0))
        with mock.patch.object(R.subprocess, "Popen", side_effect=child) as command:
            self.assertEqual(report["data"], R.verify_cli(self.cli, ["package", "verify", "fixture.tar.gz"]))
        self.assertEqual(1, command.call_count)


if __name__ == "__main__":
    unittest.main()
