"""Preparation fixtures pin committed synthetic inputs; never run a compiler/JVM."""
import gzip
import importlib.util
import json
from pathlib import Path
import sys
import tarfile
import unittest
from unittest import mock

import test_linux_build_contract as fixtures

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
SPEC = importlib.util.spec_from_file_location("prepare_linux_build", ROOT / "tools/prepare-linux-build.py")
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)


class PrepareLinuxBuildTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.ContractTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.tearDown)
        self.root = self.fixture.root
        kit, plan, _ = self.fixture.kit()
        self.plan = plan
        self.runtime = kit / "supply/jre.tar.gz"
        self.jar = kit / "supply/nigo-node.jar"
        self.source_archive = gzip.decompress((kit / "source/bxdl-source.tar.gz").read_bytes())
        self.committed = {name: value for name, value in fixtures.C._source_scan(
            kit / "source/bxdl-source.tar.gz", expected_commit=fixtures.COMMIT)["captures"].items()}
        self.repo = self.root / "repository"
        self.repo.mkdir()
        for name in P.SCRIPTS:
            self.fixture.write("repository/" + name, ("# synthetic pinned " + name + "\n").encode())
        self.fixture.write("repository/Cargo.lock", b"dirty checkout must never be exported")
        self.output = self.root / "prepared"
        self.git_calls = []
        self.tree_mode = b"100644"

    def git(self, *args):
        self.git_calls.append(args)
        if args[:2] == ("rev-parse", "--verify"):
            self.assertEqual(fixtures.COMMIT + "^{commit}", args[2])
            return (fixtures.COMMIT + "\n").encode()
        if args[:2] == ("ls-tree", "-rlz"):
            return self.tree_mode + b" blob " + b"b" * 40 + b" 10\tCargo.lock\0"
        if args[:3] == ("-c", "tar.umask=022", "archive"):
            self.assertEqual(("--format=tar", "--prefix=bxdl-source/", fixtures.COMMIT), args[3:])
            return self.source_archive
        if args[0] == "show":
            commit, name = args[1].split(":", 1)
            self.assertEqual(fixtures.COMMIT, commit)
            return self.committed[name]
        raise AssertionError("unexpected Git read " + repr(args))

    def prepare(self):
        with mock.patch.object(P, "ROOT", self.repo), mock.patch.object(P, "git", side_effect=self.git):
            return P.prepare(fixtures.COMMIT, self.runtime, self.jar, self.output)

    def test_preparation_pins_committed_source_not_dirty_checkout(self):
        with mock.patch.object(P.subprocess, "Popen", side_effect=AssertionError("no child execution")):
            result = self.prepare()
        self.assertEqual("PREPARED", result["outcome"])
        self.assertEqual("NOT_RUN", result["nativeBuild"])
        self.assertEqual("NOT_ASSEMBLED", result["runtimePackage"])
        self.assertEqual("NOT_RUN", result["systemd"])
        plan = P.contract.load_plan(self.output, result["planSha256"])
        self.assertEqual(fixtures.sha(self.committed["Cargo.lock"]), plan["source"]["cargoLockSha256"])
        self.assertNotEqual(fixtures.sha((self.repo / "Cargo.lock").read_bytes()), plan["source"]["cargoLockSha256"])
        self.assertEqual(7, result["kitFiles"])
        self.assertEqual(0o755, (self.output / "tools/build-linux-candidate.py").stat().st_mode & 0o777)
        self.assertEqual(0o644, (self.output / "tools/linux_build_contract.py").stat().st_mode & 0o777)
        self.assertTrue(all("HEAD" not in args for args in self.git_calls))

    def test_wrong_supplied_jar_stops_before_output_is_created(self):
        self.jar.write_bytes(b"replacement jar")
        with self.assertRaisesRegex(P.contract.Failure, "SUPPLY_PIN_MISMATCH"):
            self.prepare()
        self.assertFalse(self.output.exists())

    def test_git_symlink_source_is_not_exported(self):
        self.tree_mode = b"120000"
        with self.assertRaisesRegex(P.contract.Failure, "SOURCE_ENTRY_UNSUPPORTED"):
            self.prepare()
        self.assertFalse(self.output.exists())
        self.assertFalse(any("archive" in args for args in self.git_calls))

    def test_existing_output_is_never_overwritten(self):
        self.output.mkdir()
        sentinel = self.output / "keep"
        sentinel.write_bytes(b"existing")
        with self.assertRaisesRegex(P.contract.Failure, "OUTPUT_EXISTS"):
            self.prepare()
        self.assertEqual(b"existing", sentinel.read_bytes())
        self.assertEqual(["keep"], sorted(path.name for path in self.output.iterdir()))

    def test_git_export_mutating_a_source_pin_is_rejected(self):
        committed = dict(self.committed)
        committed["Cargo.lock"] = b"export-subst changed lock"
        self.source_archive = gzip.decompress(fixtures.archive(committed))
        with self.assertRaisesRegex(P.contract.Failure, "SOURCE_PIN_MISMATCH"):
            self.prepare()
        self.assertTrue(self.output.exists())  # failed evidence is preserved

    def test_delivery_archive_is_repeatable_and_cannot_overlap_kit(self):
        self.prepare()
        first, second = self.root / "one.tar.gz", self.root / "two.tar.gz"
        self.assertEqual(P.archive_kit(self.output, first), P.archive_kit(self.output, second))
        self.assertEqual(first.read_bytes(), second.read_bytes())
        with tarfile.open(first, "r:gz") as archive:
            entries = archive.getmembers()
        self.assertEqual(8, len(entries))
        self.assertTrue(all(item.isreg() and item.mtime == 0 and item.name.startswith("bxdl-linux-build/") for item in entries))
        with self.assertRaisesRegex(P.contract.Failure, "OUTPUT_OVERLAP"):
            P.archive_kit(self.output, self.output / "nested.tar.gz")
        with self.assertRaisesRegex(P.contract.Failure, "OUTPUT_EXISTS"):
            P.archive_kit(self.output, first)

    def test_git_read_discards_ambient_git_redirects(self):
        with mock.patch.dict(P.os.environ, {"GIT_DIR": "/untrusted", "GIT_WORK_TREE": "/untrusted", "GIT_CONFIG_GLOBAL": "/untrusted"}), \
                mock.patch.object(P.subprocess, "check_output", return_value=b"fixture") as command:
            self.assertEqual(b"fixture", P.git("show", "a" * 40 + ":Cargo.lock"))
        env = command.call_args.kwargs["env"]
        self.assertNotIn("GIT_DIR", env)
        self.assertNotIn("GIT_WORK_TREE", env)
        self.assertNotIn("GIT_CONFIG_GLOBAL", env)
        self.assertEqual("1", env["GIT_NO_REPLACE_OBJECTS"])
        self.assertEqual("1", env["GIT_CONFIG_NOSYSTEM"])


if __name__ == "__main__":
    unittest.main()
