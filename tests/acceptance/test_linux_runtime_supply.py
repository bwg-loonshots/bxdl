"""Pinned Linux JRE preparation: adversarial archives and no-execution boundary."""
import gzip
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import tarfile
import tempfile
import unittest
from unittest.mock import patch

TOOL = Path(__file__).resolve().parents[2] / "tools" / "prepare-linux-runtime.py"
SPEC = importlib.util.spec_from_file_location("linux_runtime_supply", TOOL)
SUPPLY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SUPPLY)
ROOT = "jdk-21-test-jre"


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def snapshot(root):
    result = {}
    for path in [root, *sorted(root.rglob("*"))]:
        m = path.lstat()
        result[str(path.relative_to(root))] = (m.st_mode, m.st_ino, m.st_size, m.st_mtime_ns,
                                             path.read_bytes() if path.is_file() else None)
    return result


class Fixture:
    def __init__(self, root):
        self.root = root
        self.archive = root / "jre.tar.gz"
        self.lock = root / "runtime-lock.json"
        self.output = root / "prepared"
        self.payload = {
            "bin/java": (b"#!/bin/sh\nexit 93\n", 0o755),
            "release": (b'JAVA_VERSION="21-test"\n', 0o644),
            "legal/java.base/LICENSE": (b"pinned test license\n", 0o444),
        }
        self.links = {"legal/java.other/LICENSE": "../java.base/LICENSE",
                      "legal/java.third/LICENSE": "../java.other/LICENSE"}
        self.entries = []
        names = set(self.payload) | set(self.links)
        parents = {ROOT}
        for name in names:
            parts = name.split("/")
            for end in range(1, len(parts)):
                parents.add(ROOT + "/" + "/".join(parts[:end]))
        self.entries.extend((name, tarfile.DIRTYPE, b"", 0o755, "") for name in sorted(parents))
        self.entries.extend((ROOT + "/" + name, tarfile.REGTYPE, raw, mode, "") for name, (raw, mode) in self.payload.items())
        self.entries.extend((ROOT + "/" + name, tarfile.SYMTYPE, b"", 0o777, target) for name, target in self.links.items())
        self.write_archive()
        files = []
        for name in sorted(names):
            raw, mode = self.payload[name] if name in self.payload else self.payload["legal/java.base/LICENSE"]
            files.append({"path": name, "size": len(raw), "sha256": sha(raw), "mode": 0o755 if mode & 0o111 else 0o644})
        self.document = {
            "schemaVersion": 1, "kind": "bxdl-runtime-lock", "vendor": "Test", "version": "21-test",
            "platform": dict(SUPPLY.PLATFORM), "javaSha256": sha(self.payload["bin/java"][0]),
            "archive": {"name": self.archive.name, "url": "https://example.invalid/jre.tar.gz", "sha256": sha(self.archive.read_bytes()), "sizeBytes": self.archive.stat().st_size},
            "sourceArchive": {"name": "source.tar.gz", "url": "https://example.invalid/source.tar.gz", "sha256": "a" * 64, "sizeBytes": 1},
            "files": files,
        }
        self.write_lock()

    def write_archive(self):
        with tarfile.open(self.archive, "w:gz", format=tarfile.GNU_FORMAT) as archive:
            for name, kind, raw, mode, link in self.entries:
                item = tarfile.TarInfo(name)
                item.type, item.mode, item.linkname = kind, mode, link
                item.size = len(raw)
                archive.addfile(item, io.BytesIO(raw) if raw else None)

    def write_lock(self):
        self.document["archive"].update(sha256=sha(self.archive.read_bytes()), sizeBytes=self.archive.stat().st_size)
        self.lock.write_text(json.dumps(self.document))
        self.lock_hash = sha(self.lock.read_bytes())

    def prepare(self):
        return SUPPLY.prepare(self.archive, self.lock, self.lock_hash, self.output)


class LinuxRuntimeSupply(unittest.TestCase):
    def fixture(self):
        temporary = tempfile.TemporaryDirectory(prefix="bxdl-linux-supply-test-")
        self.addCleanup(temporary.cleanup)
        return Fixture(Path(temporary.name).resolve())

    def reject(self, fixture):
        before = snapshot(fixture.root)
        with self.assertRaises((SUPPLY.SupplyError, OSError, tarfile.TarError, EOFError, ValueError)):
            fixture.prepare()
        self.assertFalse(fixture.output.exists())
        self.assertEqual(snapshot(fixture.root), before)

    def test_materializes_relative_chain_preserves_license_and_never_executes(self):
        fixture = self.fixture()
        inputs = {path.name: path.read_bytes() for path in (fixture.archive, fixture.lock)}
        with patch.object(os, "system", side_effect=AssertionError("no execution")):
            result = fixture.prepare()
        self.assertEqual(result["fileCount"], 5)
        self.assertEqual(result["symlinksMaterialized"], 2)
        self.assertEqual(result["runtimeExecution"], "NOT_RUN")
        self.assertEqual(result["serviceAcceptance"], "NOT_RUN")
        self.assertEqual(stat.S_IMODE(fixture.output.stat().st_mode), 0o700)
        for item in fixture.document["files"]:
            path = fixture.output / "runtime" / item["path"]
            self.assertFalse(path.is_symlink())
            self.assertEqual(sha(path.read_bytes()), item["sha256"])
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), item["mode"])
        self.assertEqual({p.name: p.read_bytes() for p in (fixture.archive, fixture.lock)}, inputs)

    def test_existing_output_or_symlink_is_never_replaced(self):
        for symlink in (False, True):
            with self.subTest(symlink=symlink):
                fixture = self.fixture()
                if symlink:
                    fixture.output.symlink_to(fixture.root / "absent")
                else:
                    fixture.output.mkdir()
                    (fixture.output / "keep").write_bytes(b"original")
                before = snapshot(fixture.root)
                with self.assertRaises(OSError):
                    fixture.prepare()
                self.assertEqual(snapshot(fixture.root), before)

    def test_hash_and_size_failures_precede_output(self):
        for field in ("lock", "archive-hash", "archive-size"):
            fixture = self.fixture()
            if field == "lock":
                fixture.lock_hash = "b" * 64
            else:
                fixture.document["archive"]["sha256" if field == "archive-hash" else "sizeBytes"] = "b" * 64 if field == "archive-hash" else 1
                fixture.lock.write_text(json.dumps(fixture.document))
                fixture.lock_hash = sha(fixture.lock.read_bytes())
            self.reject(fixture)

    def test_strict_lock_json_and_target_combinations(self):
        mutations = [
            lambda d: d.update(extra=True), lambda d: d.update(schemaVersion=True),
            lambda d: d["platform"].update(os="darwin"), lambda d: d["platform"].update(libc="musl"),
            lambda d: d["platform"].pop("libc"), lambda d: d["files"][0].update(size=True),
            lambda d: d["files"].append(dict(d["files"][0])),
            lambda d: d["files"][0].update(path="../escape"),
            lambda d: d["files"][0].update(path="legal/private.pem"),
            lambda d: d["files"][0].update(mode=0o777),
            lambda d: d.update(javaSha256="c" * 64),
        ]
        for mutate in mutations:
            with self.subTest(mutation=mutations.index(mutate)):
                fixture = self.fixture()
                mutate(fixture.document)
                fixture.write_lock()
                self.reject(fixture)
        fixture = self.fixture()
        fixture.lock.write_text('{"schemaVersion":1,"schemaVersion":1}')
        fixture.lock_hash = sha(fixture.lock.read_bytes())
        self.reject(fixture)

    def test_malicious_archive_graphs_fail_before_any_output(self):
        additions = [
            (ROOT + "/release", tarfile.REGTYPE, b"duplicate", 0o644, ""),
            ("/outside", tarfile.REGTYPE, b"escape", 0o644, ""),
            (ROOT + "/../outside", tarfile.REGTYPE, b"escape", 0o644, ""),
            (ROOT + "/extra", tarfile.REGTYPE, b"unexpected", 0o644, ""),
            (ROOT + "/extra", tarfile.LNKTYPE, b"", 0o644, ROOT + "/release"),
            (ROOT + "/extra", tarfile.FIFOTYPE, b"", 0o644, ""),
            (ROOT + "/extra", tarfile.CHRTYPE, b"", 0o644, ""),
            (ROOT + "/extra", tarfile.XHDTYPE, b"20 path=outside\n", 0o644, ""),
            (ROOT + "/extra", tarfile.GNUTYPE_LONGNAME, b"outside\0", 0o644, ""),
            ("another-root", tarfile.DIRTYPE, b"", 0o755, ""),
            (ROOT + "/bin/java/child", tarfile.REGTYPE, b"conflict", 0o644, ""),
        ]
        for addition in additions:
            with self.subTest(addition=addition[:2]):
                fixture = self.fixture()
                fixture.entries.append(addition)
                fixture.write_archive()
                fixture.write_lock()
                self.reject(fixture)

    def test_symlink_escape_missing_directory_and_cycles_are_rejected(self):
        for target in ("/outside", "../../../outside", "../java.missing/LICENSE", "../java.other/LICENSE", "..", ""):
            with self.subTest(target=target):
                fixture = self.fixture()
                fixture.entries = [(n, k, raw, mode, target if n == ROOT + "/legal/java.other/LICENSE" else link)
                                   for n, k, raw, mode, link in fixture.entries]
                fixture.write_archive()
                fixture.write_lock()
                self.reject(fixture)

    def test_unsupported_modes_inventory_content_and_limits(self):
        for mode in (0o666, 0o4755, 0o000, 0o700):
            fixture = self.fixture()
            fixture.entries = [(n, k, raw, mode if n == ROOT + "/release" else old_mode, link)
                               for n, k, raw, old_mode, link in fixture.entries]
            fixture.write_archive()
            fixture.write_lock()
            self.reject(fixture)
        fixture = self.fixture()
        fixture.document["files"][0]["sha256"] = "d" * 64
        fixture.write_lock()
        self.reject(fixture)
        for limit in ("MAX_ENTRIES", "MAX_TOTAL_BYTES", "MAX_TAR_BYTES", "MAX_FILES", "MAX_FILE_BYTES"):
            fixture = self.fixture()
            with self.subTest(limit=limit), patch.object(SUPPLY, limit, 1):
                self.reject(fixture)

    def test_gzip_crc_truncation_and_trailing_content_are_rejected(self):
        for mutate in (lambda b: b[:-4], lambda b: b[:-8] + bytes([b[-8] ^ 1]) + b[-7:],
                       lambda b: gzip.compress(gzip.decompress(b) + b"hidden-content")):
            fixture = self.fixture()
            fixture.archive.write_bytes(mutate(fixture.archive.read_bytes()))
            fixture.write_lock()
            self.reject(fixture)

    def test_inputs_changed_after_inspection_fail_before_output(self):
        for field in ("lock", "archive"):
            fixture = self.fixture()
            original = SUPPLY.scan_archive
            def inspect(*args, **kwargs):
                result = original(*args, **kwargs)
                path = getattr(fixture, field)
                path.write_bytes(path.read_bytes() + b" ")
                return result
            with patch.object(SUPPLY, "scan_archive", side_effect=inspect):
                with self.assertRaises(SUPPLY.SupplyError):
                    fixture.prepare()
            self.assertFalse(fixture.output.exists())

    def test_output_subdirectory_symlink_cannot_redirect_writes(self):
        fixture = self.fixture()
        outside = fixture.root / "outside"
        outside.mkdir()
        original = SUPPLY.Output.create_file
        replaced = False
        def create(output, relative, mode):
            nonlocal replaced
            if not replaced:
                replaced = True
                target = fixture.output / "runtime" / "bin"
                target.rmdir()
                target.symlink_to(outside, target_is_directory=True)
            return original(output, relative, mode)
        with patch.object(SUPPLY.Output, "create_file", new=create):
            with self.assertRaises(OSError):
                fixture.prepare()
        self.assertEqual(list(outside.iterdir()), [])
        self.assertTrue(fixture.output.exists(), "failed new output is deliberately retained")

    def test_input_links_and_case_aliases_are_not_adopted(self):
        for kind in ("symlink", "hardlink", "case-alias"):
            fixture = self.fixture()
            if kind == "case-alias":
                alias = dict(fixture.document["files"][0])
                alias["path"] = alias["path"].upper()
                fixture.document["files"].append(alias)
                fixture.write_lock()
            elif kind == "symlink":
                actual = fixture.root / "original.tar.gz"
                fixture.archive.rename(actual)
                fixture.archive.symlink_to(actual)
            else:
                os.link(fixture.archive, fixture.root / "hardlink.tar.gz")
            self.reject(fixture)

    def test_unexpected_output_members_prevent_success(self):
        fixture = self.fixture()
        original = SUPPLY.Output.verify
        def verify(output, inventory):
            (fixture.output / "runtime" / "unexpected").write_bytes(b"extra")
            return original(output, inventory)
        with patch.object(SUPPLY.Output, "verify", new=verify):
            with self.assertRaises(SUPPLY.SupplyError):
                fixture.prepare()
        self.assertTrue(fixture.output.exists())

    def test_cli_rejects_invalid_arguments_without_echoing_values(self):
        output = io.StringIO()
        with patch("sys.stdout", output):
            self.assertEqual(SUPPLY.main(["--secret", "PRIVATE-CANARY"]), 3)
        self.assertNotIn("PRIVATE-CANARY", output.getvalue())
        self.assertEqual(json.loads(output.getvalue())["reasonCode"], "ARGUMENTS_INVALID")


if __name__ == "__main__":
    unittest.main()
