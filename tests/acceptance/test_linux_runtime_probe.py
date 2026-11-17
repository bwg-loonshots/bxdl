"""Native-probe boundary regressions, not actual Linux/JRE acceptance."""
import argparse
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import linux_runtime_probe as probe


CANARY = "PRIVATE-PROBE-PATH-CANARY"
COMMIT = "a" * 40
VERSION = "21.0.13+9-LTS"
HOST = {"os": "linux", "architecture": "amd64", "distribution": "ubuntu", "versionId": "24.04",
        "kernelRelease": "6.8.0-test", "libc": "glibc", "glibcVersion": "2.39", "wsl": False, "pid1Systemd": True}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def elf():
    data = bytearray(64)
    data[:7] = b"\x7fELF\x02\x01\x01"
    data[16:18] = b"\x03\x00"
    data[18:20] = b"\x3e\x00"
    return bytes(data)


def save(path, content, mode=0o644):
    path.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    path.write_bytes(content if isinstance(content, bytes) else content.encode())
    path.chmod(mode)


def document(path, value):
    save(path, json.dumps(value))


def envelope(command, data):
    return json.dumps({"schemaVersion": 1, "command": command, "outcome": "SUCCEEDED", "data": data}).encode(), b""


class Fixture:
    def __init__(self, root):
        self.root = root
        self.home = root / CANARY / "runtime"
        save(self.home / "bin/java", elf(), 0o755)
        save(self.home / "release", f'JAVA_RUNTIME_VERSION="{VERSION}"\nMODULES="java.base java.management jdk.management java.naming jdk.crypto.ec jdk.compiler jdk.javadoc"\n')
        save(self.home / "legal/java.base/LICENSE", "test fixture only\n")
        self.runtime = {"schemaVersion": 1, "kind": "bxdl-runtime-lock", "version": VERSION,
                        "platform": {"os": "linux", "arch": "amd64", "javaMajor": 21, "libc": "glibc"},
                        "javaSha256": digest(self.home / "bin/java"), "files": []}
        for path in sorted(self.home.rglob("*")):
            if path.is_file():
                self.runtime["files"].append({"path": path.relative_to(self.home).as_posix(), "sha256": digest(path),
                                              "size": path.stat().st_size, "mode": stat.S_IMODE(path.stat().st_mode)})
        self.cli = root / "bxdl"
        self.jar = root / "candidate.jar"
        save(self.cli, elf(), 0o755)
        save(self.jar, b"synthetic RC4 only")
        self.engine = {"schemaVersion": 1, "jarSha256": digest(self.jar), "jarSizeBytes": self.jar.stat().st_size,
                       "javaSha256": self.runtime["javaSha256"], "expected": {"source": {"commit": "b" * 40}, "engine": "NIGO"}}
        self.runtime_path = root / "runtime-lock.json"
        self.engine_path = root / "engine-lock.json"
        self.refresh_locks()
        self.calls = []

    def refresh_locks(self):
        document(self.runtime_path, self.runtime)
        document(self.engine_path, self.engine)

    def args(self, name="result"):
        return argparse.Namespace(cli=self.cli, cli_sha256=digest(self.cli), cli_source_commit=COMMIT,
                                  runtime_home=self.home, runtime_lock=self.runtime_path,
                                  runtime_lock_sha256=digest(self.runtime_path), engine_lock=self.engine_path,
                                  engine_lock_sha256=digest(self.engine_path), jar=self.jar, output=self.root / name)

    def command(self, argv, root, label, timeout=40):
        self.calls.append((label, list(map(str, argv))))
        if label == "cli-version":
            return envelope("version", {"primaryTarget": "linux-amd64", "revision": COMMIT,
                                        "capabilities": ["host.check", "engine.inspect", "runtime.stage"]})
        if label == "host-check":
            return envelope("host check", {"reasonCode": "HOST_BASELINE_OBSERVED", "serviceMutations": 0,
                                           "engineAcceptance": "NOT_RUN", "manager": {"version": "255.4", "major": 255,
                                                                                       "systemState": "running"}})
        if label == "java-properties":
            return b"", (f"    java.runtime.version = {VERSION}\n    java.specification.version = 21\n"
                         "    java.vendor = Eclipse Adoptium\n    os.arch = amd64\n    os.name = Linux\n").encode()
        if label == "java-modules":
            return b"java.base@21.0.13\njava.management@21.0.13\njdk.management@21.0.13\njava.naming@21.0.13\njdk.crypto.ec@21.0.13\n", b""
        if label == "engine-inspect":
            return envelope("engine inspect", {"outcome": "INSPECTED_DEVELOPMENT", "engineExitCode": 0,
                                               "developmentOnly": True, "identity": self.engine["expected"],
                                               "jarSha256": self.engine["jarSha256"], "javaSha256": self.runtime["javaSha256"]})
        raise AssertionError("unplanned child")


class LinuxRuntimeProbe(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="bxdl-probe-unit-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.fixture = Fixture(self.root)

    def run_probe(self, args=None, command=None):
        with patch.object(probe, "native_host", return_value=HOST), \
             patch.object(probe, "run_command", side_effect=command or self.fixture.command):
            return probe.probe(args or self.fixture.args())

    def test_mac_is_rejected_before_input_reads_output_or_executable(self):
        args = self.fixture.args()
        with patch.object(probe.platform, "system", return_value="Darwin"), \
             patch.object(probe, "fingerprint") as read, patch.object(probe, "run_command") as command:
            with self.assertRaisesRegex(probe.Failure, "NATIVE_LINUX_X64_REQUIRED"):
                probe.probe(args)
            read.assert_not_called()
            command.assert_not_called()
        self.assertFalse(args.output.exists())

    def test_wsl_is_deferred_before_any_runtime_execution(self):
        with patch.object(probe.platform, "system", return_value="Linux"), \
             patch.object(probe.platform, "machine", return_value="x86_64"), \
             patch.object(Path, "read_text", return_value="6.6-microsoft-standard-WSL2\n"), \
             patch.object(probe, "run_command") as command:
            with self.assertRaisesRegex(probe.Failure, "WSL_DEFERRED"):
                probe.probe(self.fixture.args())
            command.assert_not_called()

    def test_nonroot_and_selected_os_requirements_are_independent(self):
        with patch.object(probe.platform, "system", return_value="Linux"), \
             patch.object(probe.platform, "machine", return_value="x86_64"), \
             patch.object(Path, "read_text", return_value="6.8.0-test\n"), \
             patch.object(probe.os, "geteuid", return_value=0):
            with self.assertRaisesRegex(probe.Failure, "NONROOT_PROBE_REQUIRED"):
                probe.native_host()
        with patch.object(probe.platform, "system", return_value="Linux"), \
             patch.object(probe.platform, "machine", return_value="x86_64"), \
             patch.object(Path, "read_text", side_effect=["6.8.0-test\n", 'ID=debian\nVERSION_ID="12"\n']), \
             patch.object(probe.os, "geteuid", return_value=1000), \
             patch.object(probe.platform, "libc_ver", return_value=("glibc", "2.39")):
            with self.assertRaisesRegex(probe.Failure, "UBUNTU_24_04_REQUIRED"):
                probe.native_host()

    def test_synthetic_cold_success_never_claims_native_or_service_acceptance(self):
        result = self.run_probe()
        self.assertEqual(result["outcome"], "PASS")
        self.assertEqual(result["scope"], "LINUX_RUNTIME_COLD_IDENTITY_ONLY")
        self.assertTrue(result["inputsUnchanged"])
        self.assertTrue(result["engineIdentityMatched"])
        self.assertEqual(result["runtimeFileCount"], 3)
        self.assertEqual(result["moduleCount"], 5)
        for name in ("engineNative", "rocksdb", "initialization", "serviceAcceptance", "packageAcceptance"):
            self.assertEqual(result[name], "NOT_RUN")
        self.assertEqual([label for label, _ in self.fixture.calls],
                         ["cli-version", "host-check", "java-properties", "java-modules", "engine-inspect"])
        self.assertEqual(result["serviceMutations"], 0)
        saved = (self.root / "result/result.json").read_text()
        self.assertNotIn(CANARY, saved)
        self.assertNotIn(str(self.root), saved)
        self.assertEqual(stat.S_IMODE((self.root / "result").stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE((self.root / "result/result.json").stat().st_mode), 0o600)

    def test_full_jre_can_omit_build_jdk_modules_without_claiming_complete_engine_module_coverage(self):
        result = self.run_probe()
        self.assertEqual(result["outcome"], "PASS")
        self.assertEqual(result["releaseBuildModuleCount"], 7)
        self.assertEqual(result["moduleCount"], 5)
        self.assertNotIn("jdk.compiler@21.0.13", result["observedModules"])
        self.assertEqual(set(result["requiredModuleChecks"]), probe.REQUIRED_MODULES)
        self.assertEqual(result["moduleCoverage"], "KNOWN_REQUIREMENTS_ONLY_NOT_COMPLETE_NIGO_MINIMUM")

    def test_missing_required_wrong_version_unknown_or_duplicate_modules_stop_before_engine(self):
        good = self.fixture.command([], self.root, "java-modules")[0]
        variants = {
            "required": good.replace(b"java.naming@21.0.13\n", b""),
            "version": good.replace(b"java.base@21.0.13", b"java.base@21.0.12"),
            "unknown": good + b"not.declared@21.0.13\n",
            "duplicate": good + b"java.base@21.0.13\n",
        }
        for label, raw in variants.items():
            with self.subTest(label=label):
                def command(argv, root, current, timeout=40):
                    if current == "java-modules":
                        return raw, b""
                    return self.fixture.command(argv, root, current)
                result = self.run_probe(self.fixture.args(label), command)
                self.assertEqual(result["outcome"], "FAILED")
                self.assertNotIn("engine-inspect", result["commandsAttempted"])
                self.assertTrue(result["inputsUnchanged"])

    def test_runtime_drift_extra_symlink_and_hardlink_reject_before_output(self):
        for kind in ("drift", "extra", "symlink", "hardlink"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                fixture = Fixture(Path(directory))
                if kind == "drift":
                    (fixture.home / "bin/java").write_bytes(elf() + b"changed")
                elif kind == "extra":
                    save(fixture.home / "unexpected", b"extra")
                elif kind == "symlink":
                    (fixture.home / "bin/java").unlink()
                    (fixture.home / "bin/java").symlink_to(fixture.cli)
                else:
                    os.link(fixture.home / "bin/java", fixture.root / "other-link")
                with patch.object(probe, "native_host", return_value=HOST), patch.object(probe, "run_command") as run:
                    with self.assertRaises(probe.Failure):
                        probe.probe(fixture.args())
                    run.assert_not_called()
                self.assertFalse((fixture.root / "result").exists())

    def test_world_writable_runtime_root_is_rejected_before_queries_or_output(self):
        self.fixture.home.chmod(0o777)
        with patch.object(probe, "native_host", return_value=HOST), patch.object(probe, "run_command") as command:
            with self.assertRaisesRegex(probe.Failure, "RUNTIME_HOME_INVALID"):
                probe.probe(self.fixture.args())
            command.assert_not_called()
        self.assertFalse(self.fixture.args().output.exists())

    def test_foreign_lock_missing_libc_and_untrusted_lock_hash_are_rejected(self):
        for selected in ({"os": "darwin", "arch": "arm64", "javaMajor": 21},
                         {"os": "linux", "arch": "amd64", "javaMajor": 21}):
            self.fixture.runtime["platform"] = selected
            self.fixture.refresh_locks()
            with self.assertRaisesRegex(probe.Failure, "LINUX_RUNTIME_LOCK_REQUIRED"):
                self.run_probe()
        args = self.fixture.args()
        args.runtime_lock_sha256 = "0" * 64
        with self.assertRaisesRegex(probe.Failure, "INPUT_PIN_MISMATCH"):
            self.run_probe(args)
        self.assertFalse(args.output.exists())

    def test_dirty_cli_revision_hash_and_nonlinux_binary_are_rejected(self):
        args = self.fixture.args()
        args.cli_source_commit = "development"
        with self.assertRaisesRegex(probe.Failure, "CLEAN_CLI_SOURCE_REQUIRED"):
            self.run_probe(args)
        args = self.fixture.args()
        args.cli_sha256 = "0" * 64
        with self.assertRaisesRegex(probe.Failure, "INPUT_PIN_MISMATCH"):
            self.run_probe(args)
        save(self.fixture.cli, b"not ELF", 0o755)
        with self.assertRaisesRegex(probe.Failure, "LINUX_X64_EXECUTABLE_REQUIRED"):
            self.run_probe()
        self.assertFalse(args.output.exists())

    def test_version_mismatch_prevents_java_and_engine_calls(self):
        def command(argv, root, label, timeout=40):
            return envelope("version", {"primaryTarget": "darwin-arm64", "revision": COMMIT})
        result = self.run_probe(command=command)
        self.assertEqual(result["reasonCode"], "CLI_IDENTITY_MISMATCH")
        self.assertEqual(result["commandsCompleted"], ["cli-version"])
        self.assertTrue(result["inputsUnchanged"])

    def test_module_mismatch_and_full_engine_identity_mismatch_are_failures(self):
        for label in ("java-modules", "engine-inspect"):
            with self.subTest(label=label):
                def command(argv, root, current, timeout=40):
                    if current == label:
                        if label == "java-modules":
                            return b"java.base@21.0.13\n", b""
                        data = json.loads(self.fixture.command(argv, root, current)[0])
                        data["data"]["identity"] = {"engine": "NIGO"}
                        return json.dumps(data).encode(), b""
                    return self.fixture.command(argv, root, current)
                result = self.run_probe(self.fixture.args(label), command)
                self.assertEqual(result["outcome"], "FAILED")
                self.assertTrue(result["inputsUnchanged"])
                self.assertNotIn("engineIdentityMatched", result)

    def test_mutated_runtime_after_queries_prevents_pass(self):
        def command(argv, root, label, timeout=40):
            result = self.fixture.command(argv, root, label)
            if label == "engine-inspect":
                (self.fixture.home / "bin/java").write_bytes(elf() + b"mutation")
            return result
        result = self.run_probe(command=command)
        self.assertEqual(result["outcome"], "FAILED")
        self.assertEqual(result["reasonCode"], "INPUTS_CHANGED")
        self.assertFalse(result["inputsUnchanged"])

    def test_existing_or_overlapping_output_is_never_reused(self):
        args = self.fixture.args()
        args.output.mkdir()
        save(args.output / "preserved", b"old")
        with self.assertRaisesRegex(probe.Failure, "OUTPUT_EXISTS"):
            self.run_probe(args)
        self.assertEqual((args.output / "preserved").read_bytes(), b"old")
        args.output = self.fixture.home / "new-output"
        with self.assertRaisesRegex(probe.Failure, "OUTPUT_OVERLAPS_RUNTIME"):
            self.run_probe(args)
        self.assertFalse(args.output.exists())

    def test_duplicate_json_and_parser_errors_never_echo_input(self):
        with self.assertRaisesRegex(probe.Failure, "JSON_DUPLICATE_KEY"):
            probe.strict_json(b'{"version":1,"version":2}')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            code = probe.main(["--unknown", CANARY])
        self.assertEqual(code, 3)
        self.assertNotIn(CANARY, output.getvalue())
        self.assertEqual(json.loads(output.getvalue())["reasonCode"], "ARGUMENTS_INVALID")


class TrustedOwnership(unittest.TestCase):
    @staticmethod
    def directory(uid, mode=0o755):
        return SimpleNamespace(st_uid=uid, st_mode=stat.S_IFDIR | mode)

    def test_only_invoking_uid_or_root_can_own_files_and_directories(self):
        with patch.object(probe.os, "geteuid", return_value=1000):
            for uid in (0, 1000):
                probe.trusted_owner(SimpleNamespace(st_uid=uid, st_mode=stat.S_IFREG | 0o644))
                probe.directory_metadata(self.directory(uid))
            with self.assertRaisesRegex(probe.Failure, "INPUT_OWNER_UNTRUSTED"):
                probe.trusted_owner(SimpleNamespace(st_uid=1001, st_mode=stat.S_IFREG | 0o644))
            for mode in (0o755, 0o1777):
                with self.assertRaisesRegex(probe.Failure, "INPUT_OWNER_UNTRUSTED"):
                    probe.directory_metadata(self.directory(1001, mode), allow_sticky=True)

    def test_file_runtime_root_and_descendant_ownership_are_checked(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = Fixture(Path(directory).resolve())
            real_lstat = Path.lstat
            foreign = os.geteuid() + 10001
            for target in (fixture.cli, fixture.home, fixture.home / "bin"):
                def metadata(path, *args, **kwargs):
                    actual = real_lstat(path, *args, **kwargs)
                    if path == target:
                        return SimpleNamespace(st_uid=foreign, st_mode=actual.st_mode, st_nlink=actual.st_nlink)
                    return actual
                with self.subTest(target=target.name), patch.object(Path, "lstat", metadata):
                    with self.assertRaisesRegex(probe.Failure, "INPUT_OWNER_UNTRUSTED"):
                        if target == fixture.cli:
                            probe.fingerprint(target)
                        else:
                            probe.inventory(fixture.home, fixture.runtime)

    def test_ancestors_reject_writable_paths_and_untrusted_owners_but_allow_trusted_sticky(self):
        path = Path("/shared/operator/input")
        records = {Path("/"): self.directory(0), Path("/shared"): self.directory(0, 0o1777),
                   Path("/shared/operator"): self.directory(1000, 0o700)}
        def metadata(current):
            return records[current]
        with patch.object(probe.os, "geteuid", return_value=1000), patch.object(Path, "lstat", metadata):
            probe.trusted_ancestors(path)
            records[Path("/shared")] = self.directory(0, 0o777)
            with self.assertRaisesRegex(probe.Failure, "PATH_ANCESTOR_UNSAFE"):
                probe.trusted_ancestors(path)
            records[Path("/shared")] = self.directory(0, 0o1777)
            records[Path("/shared/operator")] = self.directory(1001, 0o700)
            with self.assertRaisesRegex(probe.Failure, "INPUT_OWNER_UNTRUSTED"):
                probe.trusted_ancestors(path)

    def test_writable_output_parent_is_rejected_before_any_query(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = Fixture(Path(directory).resolve())
            parent = fixture.root / "unsafe-parent"
            parent.mkdir()
            parent.chmod(0o777)
            args = fixture.args()
            args.output = parent / "new-result"
            with patch.object(probe, "native_host", return_value=HOST), patch.object(probe, "run_command") as command:
                with self.assertRaisesRegex(probe.Failure, "PATH_ANCESTOR_UNSAFE"):
                    probe.probe(args)
                command.assert_not_called()
            self.assertFalse(args.output.exists())


class BoundedQueries(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="bxdl-query-unit-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        for name in ("home", "tmp", "cwd", "evidence"):
            (self.root / name).mkdir(mode=0o700)

    def test_environment_injection_is_not_inherited_and_evidence_is_private(self):
        script = "import os; print(os.environ.get('JAVA_TOOL_OPTIONS', 'CLEAN')); print(os.environ['HOME'])"
        with patch.dict(os.environ, {"JAVA_TOOL_OPTIONS": CANARY}):
            out, err = probe.run_command([sys.executable, "-c", script], self.root, "env")
        self.assertEqual(out.splitlines()[0], b"CLEAN")
        self.assertEqual(err, b"")
        for path in (self.root / "evidence").iterdir():
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)

    def test_cleanup_group_exit_race_preserves_output_and_successful_query(self):
        with patch.object(probe, "group_exists", side_effect=[False, True]), \
             patch.object(probe.os, "killpg", side_effect=ProcessLookupError):
            out, err = probe.run_command([sys.executable, "-c", "print('preserved')"], self.root, "race")
        self.assertEqual(out, b"preserved\n")
        self.assertEqual(err, b"")
        self.assertEqual((self.root / "evidence/race.stdout").read_bytes(), out)
        self.assertEqual((self.root / "evidence/race.stderr").read_bytes(), err)

    def test_timeout_and_output_limit_end_only_owned_query(self):
        for label, script, timeout, expected in [
            ("timeout", "import time; time.sleep(60)", 0.1, "QUERY_TIMEOUT"),
            ("limit", "import os; os.write(1,b'x'*300000)", 5, "QUERY_OUTPUT_LIMIT"),
        ]:
            with self.subTest(label=label), self.assertRaisesRegex(probe.Failure, expected):
                probe.run_command([sys.executable, "-c", script], self.root, label, timeout)
            self.assertLessEqual((self.root / "evidence" / (label + ".stdout")).stat().st_size, probe.LIMIT)


if __name__ == "__main__":
    unittest.main()
