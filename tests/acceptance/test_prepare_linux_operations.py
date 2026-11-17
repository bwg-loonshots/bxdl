"""Synthetic archive tests; no Linux CLI, JVM, sudo or systemd is executed."""
import gzip
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import shlex
import shutil
import subprocess
import sys
import tarfile
from types import SimpleNamespace
import unittest
from unittest import mock

import test_receive_linux_candidate as fixtures

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
SPEC = importlib.util.spec_from_file_location("prepare_linux_operations", ROOT / "tools/prepare-linux-operations.py")
O = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(O)


def archive_bytes(files, rows=None, extra=None):
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name, raw in files.items():
            member = tarfile.TarInfo(name)
            member.size = len(raw)
            member.mode = (rows or {}).get(name, {}).get("mode", 0o644)
            archive.addfile(member, io.BytesIO(raw))
        if extra:
            member, raw = extra
            archive.addfile(member, io.BytesIO(raw))
    return gzip.compress(stream.getvalue(), mtime=0)


class LinuxOperationsPreparationTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.ReceiveLinuxCandidateTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root = self.fixture.root
        f = self.fixture
        f.cli = f.fixture.write("received/work/target/x86_64-unknown-linux-gnu/release/bxdl", f.worker, 0o755)
        f.cli_sha = O.contract.digest(f.cli)
        for role in ("runtime", "management"):
            self.replace_archive(role)
        self.args = SimpleNamespace(kit=f.kit, plan_sha256=f.plan_sha, built=f.received,
                                    receipt_sha256=f.save_receipt(), cli=f.cli, cli_sha256=f.cli_sha,
                                    output=self.root / "operations", allow_unsigned_development=True)

    def replace_archive(self, role):
        f = self.fixture
        filename = "runtime.tar.gz" if role == "runtime" else "management-cli.tar.gz"
        payload = f.runtime_payload if role == "runtime" else f.management_payload
        manifest = f.manifests[role]
        files = {"manifest.json": fixtures.fixtures.encoded(manifest), **payload}
        raw = archive_bytes(files, {row["path"]: row for row in manifest["files"]})
        f.fixture.write("received/" + filename, raw)
        f.receipt["artifacts"][role] = {"path": filename, **fixtures.pin(raw),
                                        "manifestSha256": fixtures.fixtures.sha(files["manifest.json"]),
                                        "authenticity": "unsigned-development"}

    def prepare(self, verifier=None):
        with mock.patch.object(O.builder, "native_host", return_value={"os": "linux", "fixture": True}), \
                mock.patch.object(O.receiver, "verify_cli", side_effect=verifier or self.fixture.verifier), \
                mock.patch.object(O.receiver.subprocess, "Popen", side_effect=AssertionError("no process allowed")):
            return O.prepare(self.args)

    def test_verified_payloads_are_private_inputs_not_installation_acceptance(self):
        result = self.prepare()
        self.assertEqual("PREPARED", result["outcome"])
        self.assertEqual("NOT_RUN", result["nodeInstallation"])
        self.assertEqual("NOT_RUN", result["initialization"])
        self.assertEqual("NOT_RUN", result["systemdAcceptance"])
        self.assertEqual("NOT_ACCEPTED", result["runtimeProfile"])
        self.assertEqual(0, result["serviceMutations"])
        self.assertEqual(0, result["jvmInvocations"])
        root = self.args.output
        self.assertEqual(0o700, stat.S_IMODE(root.stat().st_mode))
        self.assertEqual(0o600, stat.S_IMODE((root / "operations-plan.json").stat().st_mode))
        self.assertEqual(self.fixture.worker, (root / "runtime-package/bin/bxdl").read_bytes())
        self.assertEqual(self.fixture.java, (root / "runtime-package/runtime/bin/java").read_bytes())
        self.assertEqual(self.fixture.worker, (root / "management-cli/bin/bxdl").read_bytes())
        self.assertEqual(0o755, stat.S_IMODE((root / "management-cli/bin/bxdl").stat().st_mode))
        self.assertEqual(result, json.loads((root / "operations-plan.json").read_text()))
        self.assertEqual(4, len(self.fixture.calls))
        self.assertNotIn("sudo", result["commands"]["managementInstall"])
        self.assertTrue(result["bootstrapRequirements"]["refuseExistingHelper"])
        self.assertIn("test ! -e /usr/local/libexec/bxdl/bxdl", result["commands"]["bootstrapManualSteps"][3])
        self.assertIn("test ! -L /usr/local/libexec/bxdl/bxdl", result["commands"]["bootstrapManualSteps"][3])

    def bootstrap_directory_script(self):
        # Run the emitted shell guard against isolated directories without sudo.
        # Only stat's owner field is synthetic; actual modes/link types/mkdir and
        # existing bytes are observed. No real system path is touched.
        root = self.root / "fake-root"
        local = root / "usr/local"
        local.mkdir(parents=True, mode=0o755)
        stat_tool = self.root / "fake-stat"
        stat_tool.write_text("#!" + sys.executable + "\nimport os, sys\n"
                             "m = os.lstat(sys.argv[-1])\n"
                             "print('0' if sys.argv[2] == '%u' else format(m.st_mode & 0o7777, 'o'))\n")
        stat_tool.chmod(0o755)
        command = O.bootstrap_steps(Path("/private/unused-worker"), "a" * 64)[2]
        script = shlex.split(command)[-1]
        script = script.replace("/usr/local", str(local))
        script = script.replace("for bxdl_dir in / /usr ", "for bxdl_dir in " + str(root) + " " + str(root / "usr") + " ")
        script = script.replace("/usr/bin/stat", shlex.quote(str(stat_tool)))
        script = script.replace("/usr/bin/mkdir", shlex.quote(shutil.which("mkdir")))
        subprocess.run(["/bin/sh", "-n", "-c", script], check=True, capture_output=True)
        return local, script

    def test_bootstrap_creates_only_missing_directories_and_preserves_existing_modes(self):
        local, script = self.bootstrap_directory_script()
        completed = subprocess.run(["/bin/sh", "-eu", "-c", script], capture_output=True)
        self.assertEqual(0, completed.returncode)
        directory = local / "libexec/bxdl"
        self.assertEqual(0o755, stat.S_IMODE(directory.stat().st_mode))
        directory.chmod(0o711)
        before = directory.stat()
        completed = subprocess.run(["/bin/sh", "-eu", "-c", script], capture_output=True)
        self.assertEqual(0, completed.returncode)
        after = directory.stat()
        self.assertEqual((before.st_ino, before.st_mode, before.st_uid, before.st_gid),
                         (after.st_ino, after.st_mode, after.st_uid, after.st_gid))
        worker = directory / "bxdl"
        worker.write_bytes(b"retain existing helper")
        completed = subprocess.run(["/bin/sh", "-eu", "-c", script], capture_output=True)
        self.assertNotEqual(0, completed.returncode)
        self.assertEqual(b"retain existing helper", worker.read_bytes())
        self.assertEqual(before.st_mode, directory.stat().st_mode)

    def test_bootstrap_refuses_unsafe_or_symlink_directory_without_repairing_it(self):
        local, script = self.bootstrap_directory_script()
        libexec = local / "libexec"
        libexec.mkdir(mode=0o775)
        for mode in (0o775, 0o700):
            with self.subTest(mode=oct(mode)):
                libexec.chmod(mode)
                completed = subprocess.run(["/bin/sh", "-eu", "-c", script], capture_output=True)
                self.assertNotEqual(0, completed.returncode)
                self.assertEqual(mode, stat.S_IMODE(libexec.stat().st_mode))
                self.assertFalse((libexec / "bxdl").exists())
        libexec.rmdir()
        outside = self.root / "outside-root"
        outside.mkdir()
        libexec.symlink_to(outside, target_is_directory=True)
        completed = subprocess.run(["/bin/sh", "-eu", "-c", script], capture_output=True)
        self.assertNotEqual(0, completed.returncode)
        self.assertEqual([], list(outside.iterdir()))

    def test_host_guard_precedes_inputs_children_or_mutation(self):
        with mock.patch.object(O.builder, "native_host", side_effect=O.contract.Failure("NATIVE_LINUX_X64_REQUIRED")), \
                mock.patch.object(O.receiver, "receive", side_effect=AssertionError("not before host guard")):
            with self.assertRaisesRegex(O.contract.Failure, "NATIVE_LINUX_X64_REQUIRED"):
                O.prepare(self.args)
        self.assertFalse(self.args.output.exists())

    def test_explicit_development_and_new_output_required(self):
        self.args.allow_unsigned_development = False
        with self.assertRaisesRegex(O.contract.Failure, "EXPLICIT_DEVELOPMENT_REQUIRED"):
            self.prepare()
        self.args.allow_unsigned_development = True
        self.args.output.mkdir()
        marker = self.args.output / "keep"
        marker.write_bytes(b"keep")
        with self.assertRaisesRegex(O.contract.Failure, "NEW_OUTPUT_REQUIRED"):
            self.prepare()
        self.assertEqual(b"keep", marker.read_bytes())
        self.assertEqual([], self.fixture.calls)

    def test_downloaded_archive_cli_cannot_replace_retained_local_builder(self):
        self.args.cli = self.fixture.fixture.write("downloaded/bxdl", self.fixture.worker, 0o755)
        with self.assertRaisesRegex(O.contract.Failure, "LOCAL_BUILD_CLI_REQUIRED"):
            self.prepare()
        self.assertFalse(self.args.output.exists())
        self.assertEqual([], self.fixture.calls)

    def test_external_pins_checked_before_executing_verifier(self):
        for field, reason in (("receipt_sha256", "RECEIPT_PIN_MISMATCH"),
                              ("cli_sha256", "LOCAL_VERIFIER_PIN_MISMATCH"),
                              ("plan_sha256", "PLAN_HASH_MISMATCH")):
            with self.subTest(field=field):
                old = getattr(self.args, field)
                setattr(self.args, field, "0" * 64)
                with self.assertRaises(O.contract.Failure) as error:
                    self.prepare()
                self.assertIn(reason, str(error.exception))
                setattr(self.args, field, old)
        self.assertEqual([], self.fixture.calls)
        self.assertFalse(self.args.output.exists())

    def test_output_cannot_overlap_inputs_or_follow_symlink_ancestors(self):
        self.args.output = self.args.built / "new-output"
        with self.assertRaisesRegex(O.contract.Failure, "OUTPUT_OVERLAP"):
            self.prepare()
        (self.root / "linked").symlink_to(self.args.built, target_is_directory=True)
        self.args.output = self.root / "linked" / "new-output"
        with self.assertRaisesRegex(O.contract.Failure, "PATH_OWNER_UNSAFE"):
            self.prepare()
        self.assertEqual([], self.fixture.calls)

    def test_archive_mutation_after_verification_leaves_no_ready_plan(self):
        def changed(cli, arguments):
            result = self.fixture.verifier(cli, arguments)
            if len(self.fixture.calls) == 4:
                with (self.args.built / "runtime.tar.gz").open("ab") as stream:
                    stream.write(b"mutation")
            return result
        with self.assertRaisesRegex(O.contract.Failure, "PACKAGE_CHANGED"):
            self.prepare(changed)
        self.assertFalse((self.args.output / "operations-plan.json").exists())

    def extraction(self, raw, manifest=None, manifest_sha=None):
        f = self.fixture
        path = f.fixture.write("extract.tar.gz", raw)
        manifest = f.manifests["management"] if manifest is None else manifest
        expected_sha = fixtures.fixtures.sha(fixtures.fixtures.encoded(manifest)) if manifest_sha is None else manifest_sha
        return O.extract_verified(path, self.root / "extracted", manifest, expected_sha, O.pin(path))

    def test_extraction_rechecks_manifest_exact_bytes(self):
        manifest = self.fixture.manifests["management"]
        raw = archive_bytes({"manifest.json": b"{}", "bin/bxdl": self.fixture.worker},
                            {row["path"]: row for row in manifest["files"]})
        with self.assertRaisesRegex(O.contract.Failure, "MANIFEST_PIN_MISMATCH"):
            self.extraction(raw)
        self.assertFalse((self.root / "extracted/bin/bxdl").exists())

    def test_payload_digest_cannot_drift_during_extraction(self):
        manifest = self.fixture.manifests["management"]
        raw = archive_bytes({"manifest.json": fixtures.fixtures.encoded(manifest), "bin/bxdl": b"X" * len(self.fixture.worker)},
                            {row["path"]: row for row in manifest["files"]})
        with self.assertRaisesRegex(O.contract.Failure, "PAYLOAD_CHANGED"):
            self.extraction(raw)

    def test_payload_mode_must_match_verified_manifest(self):
        manifest = self.fixture.manifests["management"]
        raw = archive_bytes({"manifest.json": fixtures.fixtures.encoded(manifest), "bin/bxdl": self.fixture.worker})
        with self.assertRaisesRegex(O.contract.Failure, "ARCHIVE_MEMBER_INVALID"):
            self.extraction(raw)
        self.assertFalse((self.root / "extracted/bin/bxdl").exists())

    def test_manifest_payload_cannot_name_a_parent_or_exceed_bounded_inventory(self):
        manifest = dict(self.fixture.manifests["management"])
        manifest["files"] = [{"path": "../outside", "size": 1, "sha256": "0" * 64, "mode": 0o644}]
        with self.assertRaisesRegex(O.contract.Failure, "RELATIVE_PATH_INVALID"):
            self.extraction(b"not opened", manifest=manifest)
        self.assertFalse((self.root / "extracted").exists())
        manifest["files"] = [{"path": "large", "size": O.MAX_PAYLOAD + 1, "sha256": "0" * 64, "mode": 0o644}]
        with self.assertRaisesRegex(O.contract.Failure, "PAYLOAD_INVALID"):
            self.extraction(b"not opened", manifest=manifest)

    def test_symlink_member_is_rejected_without_following_it(self):
        manifest = self.fixture.manifests["management"]
        link = tarfile.TarInfo("bin/bxdl")
        link.type, link.linkname, link.mode = tarfile.SYMTYPE, "/tmp/bxdl-forbidden", 0o755
        raw = archive_bytes({"manifest.json": fixtures.fixtures.encoded(manifest)}, extra=(link, b""))
        with self.assertRaisesRegex(O.contract.Failure, "ARCHIVE_MEMBER_INVALID"):
            self.extraction(raw)
        self.assertFalse(os.path.lexists(self.root / "extracted/bin/bxdl"))

    def test_traversal_duplicate_and_unexpected_members_rejected(self):
        manifest = self.fixture.manifests["management"]
        for name in ("../outside", "manifest.json", "extra"):
            with self.subTest(name=name):
                extra = tarfile.TarInfo(name)
                raw = archive_bytes({"manifest.json": fixtures.fixtures.encoded(manifest)}, extra=(extra, b""))
                root = self.root / "extracted"
                if root.exists():
                    (root / "manifest.json").unlink()
                    root.rmdir()
                with self.assertRaises(O.contract.Failure):
                    self.extraction(raw)
        self.assertFalse((self.root / "outside").exists())

    def test_missing_payload_refused_after_member_scan(self):
        manifest = self.fixture.manifests["management"]
        raw = archive_bytes({"manifest.json": fixtures.fixtures.encoded(manifest)})
        with self.assertRaisesRegex(O.contract.Failure, "PACKAGE_CHANGED"):
            self.extraction(raw)


if __name__ == "__main__":
    unittest.main()
