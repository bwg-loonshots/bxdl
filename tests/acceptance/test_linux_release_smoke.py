"""Exercise the signed Linux download layout with a native CLI and inert test engine.

Set BXDL_TEST_RELEASE_CLI to the release executable built with BXDL_REVISION set
to the checkout commit. Only a temporary user installation is created.
"""
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
from types import SimpleNamespace
import unittest

from test_build_linux_release import release
from test_linux_runtime_supply import Fixture as RuntimeFixture


TEST_CLI = os.environ.get("BXDL_TEST_RELEASE_CLI")


@unittest.skipUnless(TEST_CLI, "set BXDL_TEST_RELEASE_CLI on the native Ubuntu release builder")
class LinuxReleaseSmoke(unittest.TestCase):
    def test_install_retry_and_reject_tampered_runtime(self):
        binary = Path(TEST_CLI).resolve(strict=True)
        release.native.native_host()
        with tempfile.TemporaryDirectory(prefix="bxdl-linux-download-test-") as temporary:
            root = Path(temporary).resolve()
            home = root / "operator"
            home.mkdir(mode=0o700)
            env = {"HOME": str(home), "PATH": "/usr/bin:/bin", "LC_ALL": "C"}

            def run(*argv, ok=True):
                result = subprocess.run(argv, stdin=subprocess.DEVNULL, capture_output=True,
                                        env=env, timeout=120, text=True)
                self.assertEqual(result.returncode == 0, ok, result.stdout + result.stderr)
                return result.stdout

            identity = json.loads(run(binary, "version", "--json"))["data"]
            self.assertRegex(identity["revision"], r"^[0-9a-f]{40}$")
            self.assertEqual(identity["primaryTarget"], "linux-amd64")
            tag = "v0.0.0-smoke.1"
            assets = root / "assets"
            assets.mkdir(mode=0o700)
            release.native.copied(binary, assets / release.WORKER, 0o755)
            release.native.elf_check(assets / release.WORKER, release.MIN_GLIBC)
            for script in release.SCRIPTS:
                release.native.copied(release.ROOT / "scripts" / script, assets / script, 0o755)
            signing_key = root / "ephemeral-test-key.pem"
            trusted_pub = root / "trusted.pem"
            run("/usr/bin/openssl", "genpkey", "-algorithm", "ED25519", "-out", signing_key)
            run("/usr/bin/openssl", "pkey", "-in", signing_key, "-pubout", "-out", trusted_pub)
            shutil.copyfile(trusted_pub, assets / release.PUBLIC_KEY)

            supply = RuntimeFixture(root)
            args = SimpleNamespace(jar=root / "fixture.jar", jre_archive=supply.archive, signing_key=signing_key,
                                   notices=root / "NOTICE", sbom=root / "SBOM.json")
            args.jar.write_bytes(b"inert engine for packaging fixture")
            args.notices.write_text("Fixture unsigned inputs for local test only.")
            args.sbom.write_text('{"fixture":true}')
            engine = {"jarSha256": release.contract.digest(args.jar),
                      "expected": {"source": {"commit": "b"*40}, "contract": {"fingerprint": "c"*64}}}
            candidate = {"runtimeLock": {"path": str(supply.lock), "sha256": supply.lock_hash}}
            manifests = release.build_archives(args, root, assets, identity["revision"], identity["version"],
                                               candidate, supply.document, engine)
            worker_sha = release.contract.digest(binary)
            release.native.document(assets / "release.json", {"tag": tag, "sourceCommit": identity["revision"],
                                    "workerSha256": worker_sha, "manifestSha256": manifests})
            release.sign_checksums(assets, tag, signing_key)
            command = ["/bin/sh", assets / "install-bxdl.sh", "--version", tag,
                       "--public-key", trusted_pub, "--from-dir", assets]
            run(*command)
            runtime = home / ".local/share/BXDL-releases" / tag / "runtime-package"
            entry = home / ".local/bin/bxdl"
            jar = runtime / "engine/nigo-node.jar"
            java = runtime / "runtime/bin/java"
            receipt = json.loads((runtime / ".bxdl-install.json").read_bytes())
            self.assertEqual(receipt["authenticity"], "verified-external-ed25519")
            self.assertEqual(receipt["manifestSha256"], manifests["runtime"])
            self.assertEqual(release.contract.digest(jar), engine["jarSha256"])
            self.assertEqual(release.contract.digest(java), supply.document["javaSha256"])
            self.assertEqual(release.contract.digest(runtime / "bin/bxdl"), worker_sha)
            info = json.loads(run(entry, "cli", "info", "--json"))["data"]
            self.assertEqual(info["status"], "READY")
            self.assertEqual(info["current"]["revision"], identity["revision"])
            for path, mode in ((runtime.parent, 0o700), (runtime, 0o700), (java, 0o755), (jar, 0o644)):
                self.assertEqual(path.stat().st_uid, os.getuid())
                self.assertEqual(stat.S_IMODE(path.stat().st_mode), mode)
            original = jar.stat()
            run(*command)
            self.assertEqual((jar.stat().st_ino, jar.stat().st_mtime_ns), (original.st_ino, original.st_mtime_ns))

            jar.write_bytes(b"modified after verified install")
            run(*command, ok=False)
            self.assertEqual(jar.read_bytes(), b"modified after verified install")
            self.assertEqual(json.loads(run(entry, "cli", "info", "--json"))["data"]["current"], info["current"])
