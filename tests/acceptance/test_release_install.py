"""Exercise the saved curl bootstrap with real Ed25519 signatures and offline release assets."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/install-bxdl.sh"
TAG = "v0.2.0-preview.1"
ASSETS = (
    "install-bxdl.sh", "install-linux-helper.sh", "bxdl-linux-amd64",
    "bxdl-cli-linux-amd64.tar.gz", "bxdl-runtime-linux-amd64.tar.gz",
    "release.json", "release-public.pem",
)


class ReleaseInstallTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / "home"
        self.home.mkdir()
        self.release = self.root / "assets"
        self.release.mkdir()
        self.bin = self.root / "commands"
        self.bin.mkdir()
        # macOS system LibreSSL does not implement OpenSSL 3's -rawin.
        openssl = shutil.which("openssl")
        candidates = [openssl, "/opt/homebrew/opt/openssl@3/bin/openssl",
                      "/usr/local/opt/openssl@3/bin/openssl"]
        for candidate in candidates:
            if candidate and Path(candidate).is_file():
                version = subprocess.check_output([candidate, "version"], text=True)
                if version.startswith("OpenSSL 3."):
                    (self.bin / "openssl").symlink_to(candidate)
                    break
        else:
            self.skipTest("OpenSSL 3 is required to exercise signed release bootstrap")
        self.env = {**os.environ, "HOME": str(self.home), "PATH": str(self.bin) + ":" + os.environ["PATH"]}
        self.key = self.root / "trusted.pem"
        self.private = self.root / "signing.pem"
        self.openssl("genpkey", "-algorithm", "ED25519", "-out", self.private)
        self.openssl("pkey", "-in", self.private, "-pubout", "-out", self.key)
        shutil.copyfile(self.key, self.release / "release-public.pem")
        shutil.copyfile(SCRIPT, self.release / "install-bxdl.sh")
        self.log = self.root / "cli.log"
        mock = """#!/bin/sh
set -eu
echo "$1 ${2:-}" >> "$BXDL_TEST_LOG"
case "$1" in
install)
  test "${BXDL_TEST_INSTALL_FAIL:-0}" = 0
  test "$3" = --destination
  if [ -d "$4" ]; then test "$7" = --verify-existing; else mkdir -p "$4/bin"; cp "$0" "$4/bin/bxdl"; fi
  ;;
esac
"""
        self.env["BXDL_TEST_LOG"] = str(self.log)
        (self.release / "bxdl-linux-amd64").write_text(mock)
        for filename in ["bxdl-cli-linux-amd64.tar.gz", "bxdl-runtime-linux-amd64.tar.gz"]:
            (self.release / filename).write_bytes(b"signed test fixture")
        (self.release / "install-linux-helper.sh").write_text('echo "helper invoked" >> "$BXDL_TEST_LOG"\n')
        self.update_metadata()
        self.sign()

    def openssl(self, *args):
        subprocess.run(["openssl", *args], check=True, env=self.env, capture_output=True)

    def update_metadata(self):
        (self.release / "release.json").write_text(json.dumps({
            "tag": TAG,
            "workerSha256": hashlib.sha256((self.release / "bxdl-linux-amd64").read_bytes()).hexdigest(),
        }))

    def sign(self, assets=ASSETS):
        lines = ["# BXDL release " + TAG]
        for filename in sorted(assets):
            digest = hashlib.sha256((self.release / filename).read_bytes()).hexdigest()
            lines.append(f"{digest}  {filename}")
        sums = self.release / "SHA256SUMS"
        sums.write_text("\n".join(lines) + "\n")
        self.openssl("pkeyutl", "-sign", "-rawin", "-inkey", self.private, "-in", sums,
                     "-out", self.release / "SHA256SUMS.sig")

    def script(self, *opts, version=TAG, offline=True):
        args = ["/bin/sh", str(SCRIPT), "--public-key", str(self.key), "--version", version]
        if offline:
            args.extend(["--from-dir", str(self.release)])
        return subprocess.run(args + list(opts), env=self.env, text=True, capture_output=True)

    def test_verified_mirror_then_wrong_tag_tamper_and_incomplete_signed_list(self):
        ok = self.script("--verify-only")
        self.assertEqual(ok.returncode, 0, ok.stderr)
        self.assertNotEqual(self.script("--verify-only", version="v0.2.1").returncode, 0)
        with (self.release / "bxdl-linux-amd64").open("ab") as file:
            file.write(b"tampered")
        self.assertNotEqual(self.script("--verify-only").returncode, 0)
        self.sign(ASSETS[:-1])
        self.assertNotEqual(self.script("--verify-only").returncode, 0)
        self.assertEqual(list(self.home.iterdir()), [])
        self.assertFalse(self.log.exists())

    def test_wrong_key_does_not_trust_packaged_public_key(self):
        other = self.root / "other-private.pem"
        self.openssl("genpkey", "-algorithm", "ED25519", "-out", other)
        self.openssl("pkey", "-in", other, "-pubout", "-out", self.key)
        self.assertNotEqual(self.script("--verify-only").returncode, 0)
        self.assertFalse(self.log.exists())

    def test_curl_fetches_only_fixed_tagged_assets(self):
        log = self.root / "curl.log"
        mock = self.bin / "curl"
        mock.write_text(f"""#!/bin/sh
set -eu
while [ "$1" != '--output' ]; do shift; done
echo "$3" >> {shlex.quote(str(log))}
cp {shlex.quote(str(self.release))}/"${{3##*/}}" "$2"
""")
        mock.chmod(0o700)
        ok = self.script("--verify-only", offline=False)
        self.assertEqual(ok.returncode, 0, ok.stderr)
        files = set(ASSETS) - {"install-bxdl.sh"} | {"SHA256SUMS", "SHA256SUMS.sig"}
        self.assertEqual(set(log.read_text().splitlines()), {
            f"https://github.com/bwg-loonshots/bxdl/releases/download/{TAG}/{name}" for name in files
        })
        self.assertFalse(self.log.exists())

    @unittest.skipUnless(platform.system() == "Linux" and platform.machine() == "x86_64"
                         and os.getuid() != 0, "installation requires a regular Linux x86_64 user")
    def test_runtime_verification_precedes_activation_and_helper_is_explicit(self):
        self.env["BXDL_TEST_INSTALL_FAIL"] = "1"
        self.assertNotEqual(self.script().returncode, 0)
        self.assertNotIn("cli install", self.log.read_text())
        self.env.pop("BXDL_TEST_INSTALL_FAIL")
        installed = self.script()
        self.assertEqual(installed.returncode, 0, installed.stderr)
        self.assertIn("cli install", self.log.read_text())
        self.assertIn("setup new local --runtime", installed.stdout)
        self.assertNotIn("helper invoked", self.log.read_text())
        retry = self.script("--install-helper")
        self.assertEqual(retry.returncode, 0, retry.stderr)
        self.assertIn("helper invoked", self.log.read_text())


if __name__ == "__main__":
    unittest.main()
