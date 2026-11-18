"""Privileged bootstrap integration in an isolated mount namespace on Ubuntu CI."""
import hashlib
import os
from pathlib import Path
import platform
import pwd
import subprocess
import tempfile
import unittest


HELPER = Path(__file__).resolve().parents[2] / "scripts/install-linux-helper.sh"


class TestReleaseHelper(unittest.TestCase):
    def test_refuses_missing_release_digest(self):
        result = subprocess.run(["/bin/sh", str(HELPER), "/missing", "f" * 63], capture_output=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn(b"SHA-256", result.stderr)

    @unittest.skipUnless(os.environ.get("BXDL_TEST_LINUX_RELEASE_HELPER") == "1",
                         "set BXDL_TEST_LINUX_RELEASE_HELPER=1 on a dedicated Linux sudo runner")
    def test_immutable_worker_verified_before_root_publication(self):
        self.assertEqual(platform.system(), "Linux")
        self.assertNotEqual(os.getuid(), 0)
        operator = pwd.getpwuid(os.getuid()).pw_name
        with tempfile.TemporaryDirectory() as temp:
            worker = Path(temp) / "worker"
            # Deliberately not runnable. Installation must only transfer authenticated bytes.
            worker.write_bytes(b"Original verified worker\n")
            second = Path(temp) / "different-worker"
            second.write_bytes(b"New release\n")
            digest = hashlib.sha256(worker.read_bytes()).hexdigest()
            different = hashlib.sha256(second.read_bytes()).hexdigest()
            # Only this namespace sees the tmpfs mounted on /usr/local. The test invokes
            # the same installer and sudo sequence used on a fresh Ubuntu machine.
            scenario = '''
                mount -t tmpfs -o mode=0755 tmpfs /usr/local
                bootstrap() { runuser -u "$operator" -- /bin/sh "$helper" "$@"; }
                operator=$1 helper=$2 worker=$3 expected=$4 second=$5 different=$6
                path=/usr/local/libexec/bxdl/bxdl
                if bootstrap "$worker" "$different"; then exit 1; fi
                test ! -e "$path"
                bootstrap "$worker" "$expected"
                test "$(stat -c %u:%g:%a:%h "$path")" = 0:0:755:1
                cmp "$path" "$worker"
                inode=$(stat -c %i "$path")
                bootstrap "$worker" "$expected"
                test "$(stat -c %i "$path")" = "$inode"
                if bootstrap "$second" "$different"; then exit 1; fi
                cmp "$path" "$worker"
                mv "$path" "$path.saved"
                ln -s "$path.saved" "$path"
                if bootstrap "$worker" "$expected"; then exit 1; fi
                test -L "$path"
                rm "$path"
                chmod 0777 /usr/local/libexec/bxdl
                if bootstrap "$worker" "$expected"; then exit 1; fi
                test ! -e "$path"
            '''
            subprocess.run(["sudo", "-n", "unshare", "--mount", "--propagation", "private", "--",
                            "/bin/sh", "-eu", "-c", scenario, "bxdl-test", operator, str(HELPER),
                            str(worker), digest, str(second), different],
                           check=True, timeout=90, stdin=subprocess.DEVNULL)
