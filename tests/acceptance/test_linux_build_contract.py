"""Linux kit contract tests use tiny fixtures; no compiler, JVM or service runs."""
import copy
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import struct
import tarfile
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "linux_build_contract", Path(__file__).resolve().parents[2] / "tools/linux_build_contract.py")
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)
COMMIT = "a" * 40


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()


def record(name, data=b"", kind=tarfile.REGTYPE, mode=None, link="", size=None):
    item = tarfile.TarInfo(name)
    item.type = kind
    item.mode = mode if mode is not None else (0o755 if kind == tarfile.DIRTYPE else 0o644)
    item.linkname = link
    item.size = len(data) if size is None else size
    return item.tobuf(format=tarfile.USTAR_FORMAT) + data + b"\0" * ((-len(data)) % 512)


def archive(files=None, commit=COMMIT, records=None, tail=b""):
    parts = []
    if commit is not None:
        payload = ("52 comment=" + commit + "\n").encode()
        parts.append(record("pax_global_header", payload, tarfile.XGLTYPE))
    parts.append(record("bxdl-source/", kind=tarfile.DIRTYPE))
    if records is not None:
        parts.extend(records)
    else:
        directories = {""}
        for path, data in sorted((files or {"Cargo.toml": b"fixture"}).items()):
            for parent in reversed(Path(path).parents):
                name = parent.as_posix()
                if name != "." and name not in directories:
                    parts.append(record("bxdl-source/" + name + "/", kind=tarfile.DIRTYPE))
                    directories.add(name)
            parts.append(record("bxdl-source/" + path, data))
    return gzip.compress(b"".join(parts) + b"\0" * 1024 + tail, mtime=0)


def elf(machine=62, kind=3, phoff=0, phnum=0, shoff=0, shnum=0):
    return b"\x7fELF\x02\x01\x01\0" + b"\0" * 8 + struct.pack(
        "<HHIQQQIHHHHHH", kind, machine, 1, 0, phoff, shoff, 0, 64, 56, phnum, 64, shnum, 0)


class ContractTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bxdl-build-contract-")
        self.root = Path(self.temporary.name).resolve()

    def tearDown(self):
        self.temporary.cleanup()

    def write(self, path, data, mode=0o644):
        path = self.root / path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        path.chmod(mode)
        return path

    def rejects(self, code, call):
        with self.assertRaises(C.Failure) as result:
            call()
        self.assertEqual(code, str(result.exception))

    def kit(self, source_commit=COMMIT, runtime_override=None, engine_override=None):
        runtime_bytes, jar_bytes = b"fixture-runtime", b"fixture-jar"
        runtime = {"schemaVersion": 1, "kind": "bxdl-runtime-lock",
                   "platform": {"os": "linux", "arch": "amd64", "libc": "glibc", "javaMajor": 21},
                   "archive": {"sha256": sha(runtime_bytes), "sizeBytes": len(runtime_bytes)},
                   "javaSha256": "b" * 64}
        engine = {"schemaVersion": 1, "javaSha256": "b" * 64,
                  "jarSha256": sha(jar_bytes), "jarSizeBytes": len(jar_bytes),
                  "expected": {"source": {"commit": "c" * 40, "dirty": False},
                               "java": {"requiredMajor": 21}, "contract": {"fingerprint": "d" * 64},
                               "engine": "NIGO", "buildInfoStatus": "AVAILABLE",
                               "distribution": {"officialRelease": False, "channel": "development"}}}
        if runtime_override is not None:
            runtime = runtime_override(runtime)
        if engine_override is not None:
            engine = engine_override(engine)
        source = {"Cargo.toml": b"fixture package", "Cargo.lock": b"fixture lock",
                  "rust-toolchain.toml": b'[toolchain]\nchannel = "1.86.0"\n',
                  C.RUNTIME_PATH: encoded(runtime), C.ENGINE_PATH: encoded(engine)}
        kit = self.root / "kit"
        kit.mkdir()
        files = {"source/bxdl-source.tar.gz": archive(source, commit=source_commit),
                 "supply/jre.tar.gz": runtime_bytes, "supply/nigo-node.jar": jar_bytes,
                 C.RUNTIME_PATH: encoded(runtime), C.ENGINE_PATH: encoded(engine),
                 "tools/linux_build_contract.py": b"# fixture contract\n",
                 "tools/build-linux-candidate.py": b"# fixture builder\n"}
        inventory = []
        for name, data in files.items():
            path = self.write("kit/" + name, data)
            inventory.append({"path": name, **C.fingerprint(path)})
        refs = {entry["path"]: {key: entry[key] for key in ("path", "sha256", "size")} for entry in inventory}
        plan = {"schemaVersion": 1, "kind": "bxdl-linux-native-build-plan",
                "source": {"commit": COMMIT, "archive": refs["source/bxdl-source.tar.gz"],
                           "cargoLockSha256": sha(source["Cargo.lock"]),
                           "rustToolchainSha256": sha(source["rust-toolchain.toml"])},
                "toolchain": {"rust": "1.86.0", "target": "x86_64-unknown-linux-gnu"},
                "platform": {"os": "linux", "arch": "amd64", "libc": "glibc",
                             "builderDistribution": "ubuntu", "builderVersion": "24.04", "declaredMinGlibc": "2.34"},
                "supply": {"runtimeArchive": refs["supply/jre.tar.gz"], "jar": refs["supply/nigo-node.jar"],
                           "runtimeLock": refs[C.RUNTIME_PATH], "engineLock": refs[C.ENGINE_PATH]},
                "files": inventory}
        return kit, plan, self.save_plan(plan)

    def save_plan(self, plan):
        return C.digest(self.write("kit/build-plan.json", encoded(plan)))

    def test_fingerprint_hash_mode_and_link_rejection(self):
        path = self.write("input", b"content", 0o755)
        self.assertEqual({"sha256": sha(b"content"), "size": 7, "mode": 0o755}, C.fingerprint(path))
        link = self.root / "symbolic"
        link.symlink_to(path)
        self.rejects("INPUT_FILE_UNSAFE", lambda: C.digest(link))
        hard = self.root / "hard"
        os.link(path, hard)
        self.rejects("INPUT_FILE_UNSAFE", lambda: C.digest(path))
        hard.unlink()
        path.chmod(0o666)
        self.rejects("INPUT_FILE_UNSAFE", lambda: C.digest(path))

    def test_symlink_parent_and_group_writable_parent_rejected(self):
        path = self.write("directory/file", b"safe")
        link = self.root / "directory-link"
        link.symlink_to(path.parent, target_is_directory=True)
        self.rejects("PATH_OWNER_UNSAFE", lambda: C.digest(link / "file"))
        path.parent.chmod(0o777)
        self.rejects("PATH_MODE_UNSAFE", lambda: C.digest(path))
        path.parent.chmod(0o755)

    def test_valid_source_extracts_without_prefix_and_preserves_modes(self):
        source = self.write("source.tar.gz", archive({"Cargo.toml": b"source", "src/main.rs": b"fn main() {}"}))
        out = self.root / "output"
        self.assertEqual(out, C.extract_source(source, out, expected_commit=COMMIT))
        self.assertEqual(b"source", (out / "Cargo.toml").read_bytes())
        self.assertFalse((out / "bxdl-source").exists())
        self.assertEqual(0o644, (out / "src/main.rs").stat().st_mode & 0o777)
        self.rejects("SOURCE_OUTPUT_EXISTS", lambda: C.extract_source(source, out, expected_commit=COMMIT))

    def test_wrong_or_missing_git_commit_rejected_before_output(self):
        for commit in ("e" * 40, None):
            with self.subTest(commit=commit):
                source = self.write("source.tar.gz", archive(commit=commit))
                output = self.root / "output"
                self.rejects("SOURCE_COMMIT_MISMATCH", lambda: C.extract_source(source, output, COMMIT))
                self.assertFalse(output.exists())

    def test_unsafe_archive_entries_rejected_before_output(self):
        cases = {
            "duplicate": [record("bxdl-source/file", b"one"), record("bxdl-source/file", b"two")],
            "parent_escape": [record("bxdl-source/../outside", b"bad")],
            "absolute": [record("/outside", b"bad")],
            "symlink": [record("bxdl-source/link", kind=tarfile.SYMTYPE, link="/outside")],
            "hardlink": [record("bxdl-source/link", kind=tarfile.LNKTYPE, link="outside")],
            "sparse": [record("bxdl-source/sparse", kind=tarfile.GNUTYPE_SPARSE)],
            "pax_path": [record("pax", b"20 path=../outside\n", tarfile.XHDTYPE)],
            "global_pax_again": [record("global", b"52 comment=" + COMMIT.encode() + b"\n", tarfile.XGLTYPE)],
            "missing_parent": [record("bxdl-source/missing/file", b"bad")],
            "directory_missing_parent": [record("bxdl-source/missing/child", kind=tarfile.DIRTYPE)],
            "file_parent": [record("bxdl-source/file", b"one"), record("bxdl-source/file/child", b"bad")],
            "unsafe_mode": [record("bxdl-source/file", b"bad", mode=0o666)],
            "oversized_file": [record("bxdl-source/file", size=C.MAX_SOURCE_FILE + 1)],
        }
        for name, records in cases.items():
            with self.subTest(name=name):
                source = self.write("source.tar.gz", archive(records=records))
                output = self.root / "output"
                with self.assertRaises(C.Failure):
                    C.extract_source(source, output, COMMIT)
                self.assertFalse(output.exists())
                self.assertFalse((self.root / "outside").exists())

    def test_nonzero_trailing_truncated_and_bad_gzip_rejected(self):
        for raw in (archive(tail=b"unexpected"), archive()[:-8], b"not gzip"):
            with self.subTest(size=len(raw)):
                source = self.write("source.tar.gz", raw)
                with self.assertRaises(C.Failure):
                    C.extract_source(source, self.root / "output", COMMIT)
                self.assertFalse((self.root / "output").exists())

    def test_source_cannot_change_between_validation_and_extraction(self):
        source = self.write("source.tar.gz", archive({"Cargo.toml": b"first"}))
        original = C._source_scan
        def scan_then_replace(*args, **kwargs):
            result = original(*args, **kwargs)
            if kwargs.get("output") is None:
                source.write_bytes(archive({"Cargo.toml": b"second"}))
            return result
        with mock.patch.object(C, "_source_scan", side_effect=scan_then_replace):
            self.rejects("INPUT_CHANGED", lambda: C.extract_source(source, self.root / "output", COMMIT))
        self.assertFalse((self.root / "output/Cargo.toml").exists())

    def test_valid_plan_checks_inventory_source_and_supply_before_execution(self):
        kit, plan, external = self.kit()
        with mock.patch.object(C.subprocess, "Popen", side_effect=AssertionError("must not execute")):
            self.assertEqual(plan, C.load_plan(kit, external))
        self.rejects("PLAN_HASH_MISMATCH", lambda: C.load_plan(kit, "0" * 64))
        (kit / "supply/nigo-node.jar").write_bytes(b"modified")
        self.rejects("KIT_FILE_MISMATCH", lambda: C.load_plan(kit, external))

    def test_extra_file_or_empty_directory_rejected(self):
        kit, _, external = self.kit()
        extra = self.write("kit/unlisted", b"extra")
        self.rejects("KIT_INVENTORY_MISMATCH", lambda: C.load_plan(kit, external))
        extra.unlink()
        (kit / "unlisted-dir").mkdir()
        self.rejects("KIT_INVENTORY_MISMATCH", lambda: C.load_plan(kit, external))

    def test_duplicate_inventory_and_source_pin_rejected(self):
        kit, plan, _ = self.kit()
        duplicate = copy.deepcopy(plan)
        duplicate["files"].append(duplicate["files"][0])
        external = self.save_plan(duplicate)
        self.rejects("PLAN_INVENTORY_INVALID", lambda: C.load_plan(kit, external))
        plan["source"]["cargoLockSha256"] = "0" * 64
        external = self.save_plan(plan)
        self.rejects("SOURCE_PIN_MISMATCH", lambda: C.load_plan(kit, external))

    def test_plan_source_commit_is_git_archive_commit(self):
        kit, _, external = self.kit(source_commit="e" * 40)
        self.rejects("SOURCE_COMMIT_MISMATCH", lambda: C.load_plan(kit, external))

    def test_plan_does_not_allow_schema_bool_or_different_toolchain(self):
        kit, plan, _ = self.kit()
        for key, value, code in (("schemaVersion", True, "PLAN_KIND_INVALID"),
                                 ("toolchain", {"rust": "nightly", "target": "x86_64-unknown-linux-gnu"}, "PLAN_TOOLCHAIN_INVALID"),
                                 ("platform", {"os": "linux"}, "PLAN_PLATFORM_INVALID")):
            with self.subTest(key=key):
                changed = copy.deepcopy(plan)
                changed[key] = value
                external = self.save_plan(changed)
                self.rejects(code, lambda: C.load_plan(kit, external))

    def test_supply_pin_mismatch_rejected(self):
        def changed(runtime):
            runtime["archive"]["sha256"] = "0" * 64
            return runtime
        kit, _, external = self.kit(runtime_override=changed)
        self.rejects("SUPPLY_PIN_MISMATCH", lambda: C.load_plan(kit, external))

    def test_malformed_supply_lock_fails_with_public_code(self):
        kit, _, external = self.kit(engine_override=lambda _: [])
        self.rejects("SUPPLY_LOCK_INVALID", lambda: C.load_plan(kit, external))

    def test_dirty_engine_identity_rejected(self):
        def dirty(engine):
            engine["expected"]["source"]["dirty"] = True
            return engine
        kit, _, external = self.kit(engine_override=dirty)
        self.rejects("ENGINE_IDENTITY_INVALID", lambda: C.load_plan(kit, external))

    def test_strict_json_rejects_duplicates_and_nonfinite_numbers(self):
        for raw, code in ((b'{"a":1,"a":2}', "JSON_DUPLICATE_KEY"),
                          (b'{"value":NaN}', "JSON_INVALID"), (b'\xff', "JSON_INVALID")):
            self.rejects(code, lambda: C._json(raw))

    def test_elf_requires_supported_architecture_and_complete_tables(self):
        path = self.write("bxdl", elf(), 0o755)
        self.assertEqual({"class": "ELF64", "endianness": "little", "machine": "x86_64", "type": "DYN", "size": 64}, C.inspect_elf(path))
        for invalid in (elf(machine=183), elf()[:48], b"\x7fELF\x01" + elf()[5:],
                        b"\x7fELF\x02\x02" + elf()[6:], elf(phoff=64, phnum=1), elf(shoff=64, shnum=1)):
            path.write_bytes(invalid)
            with self.assertRaises(C.Failure):
                C.inspect_elf(path)

    def test_glibc_uses_requirements_not_definitions_and_numeric_order(self):
        raw = """Version definition section '.gnu.version_d' contains 2 entries:
  0x001c: Rev: 1 Flags: none Index: 2 Cnt: 1 Name: GLIBC_9.99
Version needs section '.gnu.version_r' contains 1 entry:
  0x0010: Name: GLIBC_2.34 Flags: none Version: 2
  0x0020: Name: GLIBC_2.9 Flags: none Version: 3
  0x0030: Name: GLIBC_2.2.5 Flags: none Version: 4
  0x0040: Name: GLIBC_2.34 Flags: none Version: 2
Version symbols section '.gnu.version' contains 1 entry:
  Name: GLIBC_8.88
"""
        result = C._parse_version_needs(raw)
        self.assertEqual(["2.2.5", "2.9", "2.34"], result["requiredVersions"])
        self.assertEqual("2.34", result["maximumRequired"])
        self.assertEqual("VERSION_NEEDS_ONLY", result["scope"])
        self.assertIsNone(C._parse_version_needs("No version information found in this file.")["maximumRequired"])

    def test_glibc_unknown_requirement_fails_closed(self):
        for version in ("GLIBC_PRIVATE", "GLIBC_ABI_DT_RELR", "GLIBC_2.34broken"):
            raw = "Version needs section '.gnu.version_r' contains 1 entry:\n Name: " + version
            self.rejects("GLIBC_REQUIREMENT_UNSUPPORTED", lambda: C._parse_version_needs(raw))
        self.rejects("READELF_OUTPUT_INVALID", lambda: C._parse_version_needs("unrecognized output"))

    def test_glibc_reader_does_not_execute_candidate_and_detects_changes(self):
        path = self.write("bxdl", elf(), 0o755)
        with mock.patch.object(C, "_run_readelf", return_value="No version information found in this file.") as reader:
            self.assertEqual([], C.glibc_requirements(path)["requiredVersions"])
            reader.assert_called_once_with(path)
        def changed(_):
            path.write_bytes(elf(kind=2))
            return "No version information found in this file."
        with mock.patch.object(C, "_run_readelf", side_effect=changed):
            self.rejects("INPUT_CHANGED", lambda: C.glibc_requirements(path))


if __name__ == "__main__":
    unittest.main()
