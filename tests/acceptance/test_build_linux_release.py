"""Check the supply and authenticity boundaries of Linux release assembly."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
spec = importlib.util.spec_from_file_location("linux_release", ROOT / "tools/build-linux-release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class LinuxReleaseTests(unittest.TestCase):
    def test_rejects_jar_outside_selected_rc4_lock(self):
        with tempfile.TemporaryDirectory() as tmp:
            jar = Path(tmp).resolve() / "repacked.jar"
            jar.write_bytes(b"different bytes from selected NIGO JAR")
            with self.assertRaisesRegex(release.contract.Failure, "SUPPLY_PIN_MISMATCH"):
                release.selected_supply(jar)

    def test_rejects_partial_jre_legal_material_as_product_evidence(self):
        pkg = ROOT / "packaging"
        with self.assertRaisesRegex(release.contract.Failure, "PRODUCT_LEGAL_INPUT_REQUIRED"):
            release.check_legal_inputs(pkg / "runtime-notices.temurin-21-linux-amd64.txt",
                                       pkg / "runtime-bom.temurin-21-linux-amd64.cdx.json")

    def test_unlisted_files_and_injected_checksum_lines_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            assets = Path(tmp).resolve()
            for name in release.SIGNED_FILES:
                (assets / name).write_bytes(b"fixture")
            (assets / "build-secrets.env").write_bytes(b"never publish me")
            with self.assertRaisesRegex(release.contract.Failure, "UNEXPECTED_RELEASE_CONTENT"):
                release.sign_checksums(assets, "v0.1.0-preview.1", assets / "unused-key")
            with self.assertRaisesRegex(release.contract.Failure, "RELEASE_TAG_INVALID"):
                release.sign_checksums(assets, "v1\nother-tag", assets / "unused-key")
            self.assertFalse((assets / "SHA256SUMS").exists())

    def test_real_detached_signature_covers_exact_tag_and_all_payloads(self):
        executable = "/usr/bin/openssl"
        for binary in (Path(executable), Path("/opt/homebrew/opt/openssl@3/bin/openssl")):
            if binary.is_file() and subprocess.check_output([binary, "version"]).startswith(b"OpenSSL 3"):
                executable = str(binary)
                break
        else:
            self.skipTest("release signing verification requires OpenSSL 3")
        with tempfile.TemporaryDirectory() as tmp, patch.object(release, "OPENSSL", executable):
            root = Path(tmp).resolve()
            assets = root / "assets"
            assets.mkdir()
            key = root / "unused-outside-this-test.pem"
            subprocess.run([executable, "genpkey", "-algorithm", "ED25519", "-out", key], check=True)
            for name in release.SIGNED_FILES:
                (assets / name).write_bytes(name.encode())
            pub = assets / release.PUBLIC_KEY
            subprocess.run([executable, "pkey", "-in", key, "-pubout", "-out", pub], check=True)
            release.sign_checksums(assets, "v0.1.0-preview.1", key)
            sums = assets / "SHA256SUMS"
            lines = sums.read_text().splitlines()
            self.assertEqual(lines[0], "# BXDL release v0.1.0-preview.1")
            self.assertEqual(lines[1:], [f"{release.contract.digest(assets / name)}  {name}"
                                         for name in sorted(release.SIGNED_FILES)])
            sums.write_text(sums.read_text().replace("preview.1", "preview.2"))
            changed = subprocess.run([executable, "pkeyutl", "-verify", "-pubin", "-inkey", pub,
                                      "-sigfile", assets / "SHA256SUMS.sig", "-rawin", "-in", sums],
                                     capture_output=True)
            self.assertNotEqual(changed.returncode, 0)

    def test_signed_manifest_must_describe_approved_stage_and_platform(self):
        with tempfile.TemporaryDirectory() as tmp:
            stage = Path(tmp).resolve()
            (stage / "worker").write_bytes(b"approved bytes")
            entry = dict(release.contract.fingerprint(stage / "worker"), path="worker")
            manifest = {"target": "linux-amd64", "files": [entry]}
            def verify(data):
                with patch.object(release.native, "run", return_value=(b"", b"")), \
                     patch.object(release.native, "envelope", return_value=data):
                    return release.verified_assets("bxdl", ["cli", "bundle"], "archive", "key",
                                                   {"target": "linux-amd64"}, stage)
            report = {"authenticity": "verified-external-ed25519", "manifest": manifest, "manifestSha256": "f" * 64}
            self.assertEqual(verify(report), "f" * 64)
            with self.assertRaisesRegex(release.contract.Failure, "RELEASE_SIGNATURE_INVALID"):
                verify(dict(report, authenticity="unsigned-development"))
            with self.assertRaisesRegex(release.contract.Failure, "RELEASE_MANIFEST_MISMATCH"):
                verify(dict(report, manifest=dict(manifest, target="darwin-arm64")))
            (stage / "worker").write_bytes(b"other bytes")
            with self.assertRaisesRegex(release.contract.Failure, "RELEASE_INVENTORY_MISMATCH"):
                verify(report)
