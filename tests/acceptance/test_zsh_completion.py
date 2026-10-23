"""Exercise the installed zsh completion through ZLE without executing a node.

The private provider stub records argv, so these tests cover shell transport and
registration independently of the Rust provider's read-only catalog tests.
"""
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import shlex
import shutil
import signal
import subprocess
import tempfile
import time
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
ZSH = shutil.which("zsh")


class ZshSession:
    """A private interactive shell; Ctrl-X captures BUFFER without accept-line."""

    def __init__(self, root, mode="source"):
        self.root = root
        self.capture = root / "captured-buffer"
        environment = dict(os.environ, HOME=str(root), ZDOTDIR=str(root), TERM="xterm",
                           PATH=str(root / "bin with spaces") + os.pathsep + os.environ["PATH"],
                           BXDL_TEST_ROOT=str(root), BXDL_TEST_CAPTURE=str(self.capture))
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.chdir(root)
            os.execve(ZSH, [ZSH, "-f", "-i"], environment)
        self.transcript = b""
        self.closed = False
        script = REPOSITORY / "completions" / "_bxdl"
        commands = ["PS1='BXDL_PROMPT> '; PS2=''; unsetopt beep"]
        if mode == "autoload":
            completion_dir = root / "completions"
            completion_dir.mkdir()
            shutil.copyfile(script, completion_dir / "_bxdl")
            commands.append(f"fpath=({shlex.quote(str(completion_dir))} $fpath)")
        commands.append("autoload -Uz compinit; compinit -D -i")
        if mode == "source":
            commands.append(f"source {shlex.quote(str(script))}")
        elif mode == "binary":
            commands.append("source <(bxdl completion zsh)")
        commands.extend([
            "_bxdl_test_capture() { print -rn -- \"$BUFFER\" > \"$BXDL_TEST_CAPTURE\"; BUFFER=''; zle -I; print -r -- __BXDL_CAPTURED__; zle reset-prompt; }",
            "zle -N _bxdl_test_capture; bindkey '^X' _bxdl_test_capture",
            "print -r -- __BXDL_READY__",
        ])
        try:
            os.write(self.fd, ("\n".join(commands) + "\n").encode())
            self.read_until(b"\r\n__BXDL_READY__\r\n")
        except BaseException:
            self.close()
            raise

    def read_until(self, marker, timeout=10):
        data = b""
        deadline = time.monotonic() + timeout
        while marker not in data:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise AssertionError(f"zsh did not return {marker!r}: {data!r}")
            readable, _, _ = select.select([self.fd], [], [], remaining)
            if readable:
                chunk = os.read(self.fd, 65536)
                if not chunk:
                    raise AssertionError(f"zsh exited before {marker!r}: {data!r}")
                data += chunk
        self.transcript += data
        return data

    def command(self, command):
        os.write(self.fd, (command + "; print -r -- __BXDL_READY__\n").encode())
        self.read_until(b"\r\n__BXDL_READY__\r\n")

    def complete(self, line, cursor=None):
        if cursor is None:
            cursor = len(line)
        os.write(self.fd, line.encode() + b"\x1b[D" * (len(line) - cursor) + b"\t\x18")
        self.read_until(b"__BXDL_CAPTURED__\r\n")
        return self.capture.read_text()

    def close(self):
        if self.closed:
            return
        self.closed = True
        # The only child is this test's isolated zsh; never signal process groups.
        try:
            os.write(self.fd, b"exit\n")
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                if os.waitpid(self.pid, os.WNOHANG)[0]:
                    return
                time.sleep(0.01)
            os.kill(self.pid, signal.SIGKILL)
            os.waitpid(self.pid, 0)
        finally:
            os.close(self.fd)


@unittest.skipUnless(ZSH, "zsh is unavailable; real ZLE completion tests require zsh")
class ZshCompletion(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bxdl-zsh-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.bin = self.root / "bin with spaces"
        self.bin.mkdir()
        self.binary = self.bin / "bxdl"
        self.binary.write_text('''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys
root = Path(os.environ["BXDL_TEST_ROOT"])
with (root / "calls.jsonl").open("a") as stream:
    stream.write(json.dumps(sys.argv[1:]) + "\\n")
if sys.argv[1:3] != ["completion", "__complete"]:
    sys.exit(99)
config = json.loads((root / "response.json").read_text())
sys.stderr.write(config.get("stderr", ""))
sys.stdout.write(config["output"])
sys.exit(config.get("status", 0))
''')
        self.binary.chmod(0o700)
        self.respond("values\nstatus\n")

    def respond(self, output, status=0, stderr=""):
        (self.root / "response.json").write_text(json.dumps(
            {"output": output, "status": status, "stderr": stderr}))

    def session(self, mode="source"):
        shell = ZshSession(self.root, mode)
        self.addCleanup(shell.close)
        return shell

    def calls(self):
        return [json.loads(line) for line in (self.root / "calls.jsonl").read_text().splitlines()]

    def assert_request(self, cursor, words):
        self.assertEqual(self.calls()[-1], ["completion", "__complete", "--cursor", str(cursor), "--", *words])
        self.assertTrue(all(call[:2] == ["completion", "__complete"] for call in self.calls()))

    def test_source_and_autoload_register_first_and_repeated_completion(self):
        for mode in ("source", "autoload"):
            with self.subTest(mode=mode):
                shell = self.session(mode)
                path_line = shlex.quote(str(self.binary)) + " sta"
                self.assertEqual(shell.complete(path_line), shlex.quote(str(self.binary)) + " status ")
                self.assert_request(0, ["sta"])
                self.assertEqual(shell.complete("bxdl sta"), "bxdl status ")
                self.assertEqual(shell.complete("bxdl sta"), "bxdl status ")
                self.assertFalse((self.root / ".zcompdump").exists())
                shell.close()

    def test_blank_prefix_and_flags_are_transported_literally(self):
        shell = self.session()
        self.assertEqual(shell.complete("bxdl "), "bxdl status ")
        self.assert_request(0, [""])
        line = "bxdl --json --home '/tmp/home with spaces' status "
        shell.complete(line)
        self.assert_request(4, ["--json", "--home", "/tmp/home with spaces", "status", ""])
        shell.complete("bxdl status -- ")
        self.assert_request(2, ["status", "--", ""])
        literal = "$(touch must-not-run)"
        shell.complete("bxdl --home " + shlex.quote(literal) + " sta")
        self.assert_request(2, ["--home", literal, "sta"])
        self.assertFalse((self.root / "must-not-run").exists())

    def test_quoted_binary_and_alias_use_the_selected_executable(self):
        shell = self.session()
        line = shlex.quote(str(self.binary)) + " sta"
        self.assertEqual(shell.complete(line), shlex.quote(str(self.binary)) + " status ")
        self.assert_request(0, ["sta"])
        shell.command("alias bd=bxdl")
        self.assertEqual(shell.complete("bd sta"), "bd status ")
        self.assert_request(0, ["sta"])
        shell.command("alias jsonbx='bxdl --json'")
        self.assertEqual(shell.complete("jsonbx sta"), "jsonbx status ")
        self.assert_request(1, ["--json", "sta"])

    def test_only_unquoted_home_tilde_is_expanded_without_evaluation(self):
        shell = self.session()
        for spelling, expected in [("~/managed", str(self.root / "managed")),
                                    ("'~/literal'", "~/literal"),
                                    ('"~/literal"', "~/literal"),
                                    (r"\~/literal", "~/literal"),
                                    ("'$HOME/literal'", "$HOME/literal")]:
            shell.complete("bxdl --home " + spelling + " sta")
            self.assert_request(2, ["--home", expected, "sta"])
        self.assertFalse((self.root / "managed").exists())

    def test_equals_values_preserve_the_option_and_complete_the_value(self):
        shell = self.session()
        self.respond("values\nhttpPort\n")
        self.assertEqual(shell.complete("bxdl setup edit local --field=http"),
                         "bxdl setup edit local --field=httpPort ")
        self.assert_request(3, ["setup", "edit", "local", "--field=http"])
        directory = self.root / "equals candidates"
        directory.mkdir()
        (directory / "home directory").mkdir()
        shell.command(f"cd {shlex.quote(str(directory))}")
        self.respond("directories\n")
        self.assertEqual(shell.complete("bxdl --home=hom"), "bxdl --home=home\\ directory/")
        self.assert_request(0, ["--home=hom"])
        (directory / "data=1").mkdir()
        (directory / "data=1" / "nested home").mkdir()
        self.assertEqual(shell.complete("bxdl status --home=data=1/ne"),
                         "bxdl status --home=data=1/nested\\ home/")

    def test_completion_does_not_invoke_shell_functions_or_change_shell_options(self):
        shell = self.session()
        shell.command("bxdl() { print should-not-execute > \"$BXDL_TEST_ROOT/function-ran\"; }; setopt shwordsplit; unsetopt extendedglob")
        self.assertEqual(shell.complete("bxdl sta"), "bxdl status ")
        self.assertFalse((self.root / "function-ran").exists())
        shell.command("print -r -- $options[shwordsplit],$options[extendedglob] > \"$BXDL_TEST_ROOT/shell-options\"")
        self.assertEqual((self.root / "shell-options").read_text(), "on,off\n")

    def test_cursor_uses_prefix_and_preserves_following_home(self):
        shell = self.session()
        shell.command("setopt completeinword")
        line = "bxdl status --home '/tmp/later home'"
        shell.complete(line, cursor=len("bxdl sta"))
        self.assert_request(0, ["sta", "--home", "/tmp/later home"])

    def test_provider_failure_invalid_output_and_none_do_not_fall_back(self):
        shell = self.session()
        for output, status in [("values\nstatus\n", 7), ("none\n", 0), ("bad-kind\nstatus\n", 0),
                               ("values\n$(touch_pwned)\n", 0), ("values\n" + "x" * 65537, 0)]:
            with self.subTest(kind=output[:20], status=status):
                self.respond(output, status, "BXDL_PRIVATE_ERROR")
                self.assertEqual(shell.complete("bxdl sta"), "bxdl sta")
        self.assertNotIn(b"BXDL_PRIVATE_ERROR", shell.transcript)
        self.assert_request(0, ["sta"])

    def test_file_helpers_preserve_literal_command_substitution(self):
        shell = self.session()
        (self.root / "$(touch file-canary)-package").write_text("fixture")
        (self.root / "$(touch directory-canary)-home").mkdir()
        self.respond("files\n")
        self.assertIn("-package", shell.complete("bxdl install '$(touch file-canary)-pac"))
        self.respond("directories\n")
        self.assertIn("-home", shell.complete("bxdl --home '$(touch directory-canary)-ho"))
        self.assertFalse((self.root / "file-canary").exists())
        self.assertFalse((self.root / "directory-canary").exists())

    def test_only_explicit_file_kinds_offer_files_and_directories(self):
        shell = self.session()
        directory = self.root / "file candidates"
        directory.mkdir()
        (directory / "package archive.tar.gz").write_text("fixture")
        (directory / "home directory").mkdir()
        shell.command(f"cd {shlex.quote(str(directory))}")
        self.respond("files\n")
        self.assertEqual(shell.complete("bxdl install --package pac"), "bxdl install --package package\\ archive.tar.gz ")
        self.respond("directories\n")
        self.assertEqual(shell.complete("bxdl --home hom"), "bxdl --home home\\ directory/")
        self.respond("none\n")
        self.assertEqual(shell.complete("bxdl unknown pac"), "bxdl unknown pac")


@unittest.skipUnless(ZSH and os.environ.get("BXDL_COMPLETION_BINARY"),
                     "set BXDL_COMPLETION_BINARY to run actual CLI/ZLE acceptance")
class BinaryZshCompletion(unittest.TestCase):
    def test_actual_binary_completion_is_read_only(self):
        binary = Path(os.environ["BXDL_COMPLETION_BINARY"]).resolve(strict=True)
        with tempfile.TemporaryDirectory(prefix="bxdl-zsh-binary-") as temporary:
            root = Path(temporary).resolve()
            bin_directory = root / "bin with spaces"
            bin_directory.mkdir()
            selected_binary = bin_directory / "bxdl"
            selected_binary.symlink_to(binary)
            home = root / "management home"
            home.mkdir(mode=0o700)
            catalog = home / "catalog"
            catalog.mkdir(mode=0o700)
            lock = home / ".catalog.lock"
            marker = home / ".bxdl-home.json"
            for path in (lock, marker):
                path.touch(mode=0o600)

            def identity(path):
                metadata = path.stat()
                return {"device": metadata.st_dev, "inode": metadata.st_ino}

            marker.write_text(json.dumps({
                "schemaVersion": 1, "kind": "BXDL_CONSOLE_HOME",
                "root": identity(home), "catalog": identity(catalog),
                "lock": identity(lock), "record": identity(marker),
            }))
            canary = "PRIVATE-ZSH-COMPLETION-CANARY"
            entries = []
            for index, name in enumerate(("local", "ledger"), 1):
                entries.append({
                    "id": str(index) * 32, "alias": name, "bindingSha256": "a" * 64,
                    "label": canary, "recordedAt": "2026-10-23T00:00:00Z",
                    "target": {"kind": "INSTANCE", "path": str(root / f"absent-{canary}-{index}"),
                               "identity": {"device": 1, "inode": index}, "schemaVersion": 1,
                               "name": name, "state": "REGISTERED", "cli": "MISMATCH"},
                })
            record = catalog / "revision00000001.json"
            record.write_text(json.dumps({"schemaVersion": 2, "kind": "BXDL_CONSOLE_CATALOG", "entries": entries}))
            record.chmod(0o600)
            for namespace, name in (("sessions", "alpha"), ("operations", "completed")):
                directory = home / namespace
                directory.mkdir(mode=0o700)
                (directory / name).mkdir(mode=0o700)
                secret = directory / name / "private-record"
                secret.write_text(canary)
                secret.chmod(0o600)

            def snapshot():
                state = {}
                for path in [home, *sorted(home.rglob("*"))]:
                    metadata = path.lstat()
                    state[str(path.relative_to(home))] = (
                        metadata.st_mode, metadata.st_ino, metadata.st_mtime_ns, metadata.st_ctime_ns,
                        hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None)
                return state

            before = snapshot()
            quote_home = shlex.quote(str(home))
            package = root / "package archive.tar.gz"
            package.write_text("FAKE_PACKAGE_NEVER_OPENED_BY_PROVIDER")
            (root / "data=1").mkdir()
            (root / "data=1" / "nested home").mkdir()
            scenarios = [
                ("bxdl diagn", "bxdl diagnose "),
                (f"bxdl status --home {quote_home} loc", f"bxdl status --home {quote_home} local "),
                (f"bxdl status led --home {quote_home}", f"bxdl status ledger  --home {quote_home}"),
                (f"bxdl --json status --home {quote_home} loc", f"bxdl --json status --home {quote_home} local "),
                (f"bxdl setup resume --home {quote_home} al", f"bxdl setup resume --home {quote_home} alpha "),
                (f"bxdl setup show --home {quote_home} comp", f"bxdl setup show --home {quote_home} completed "),
                ("bxdl setup edit alpha --field=httpP", "bxdl setup edit alpha --field=httpPort "),
                ("bxdl install pac", "bxdl install package\\ archive.tar.gz "),
                ("bxdl status --home=man", "bxdl status --home=management\\ home/"),
                ("bxdl status --home=data=1/ne", "bxdl status --home=data=1/nested\\ home/"),
                ("bxdl status --home ~/management\\ home loc", "bxdl status --home ~/management\\ home local "),
                ("bxdl status -- unknown", "bxdl status -- unknown"),
                (f"bxdl status --home {shlex.quote(str(root / 'missing'))} loc", f"bxdl status --home {shlex.quote(str(root / 'missing'))} loc"),
            ]
            shell = ZshSession(root, "binary")
            results = []
            try:
                for line, expected in scenarios:
                    # For a word before --home, move the cursor to that name.
                    cursor = line.index(" --home") if line.startswith("bxdl status led ") else None
                    with self.subTest(line=line):
                        actual = shell.complete(line, cursor)
                        self.assertEqual(actual, expected)
                        results.append({"input": line.replace(str(home), "<HOME>").replace(str(root), "<TEST_ROOT>"), "status": "PASS"})
                self.assertEqual(len(results), len(scenarios))
                quoted = shlex.quote(str(selected_binary))
                self.assertEqual(shell.complete(quoted + " diagn"), quoted + " diagnose ")
                self.assertNotIn(canary.encode(), shell.transcript)
                self.assertEqual(snapshot(), before)
                self.assertFalse((root / "missing").exists())
            finally:
                shell.close()
            result_path = os.environ.get("BXDL_COMPLETION_RESULT")
            if result_path:
                Path(result_path).write_text(json.dumps({
                    "status": "PASS", "binarySha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                    "zshVersion": subprocess.check_output([ZSH, "--version"], text=True).strip(),
                    "registration": "source <(bxdl completion zsh)", "scenarios": results,
                    "quotedBinaryPathWithSpaces": "PASS", "managementSnapshotUnchanged": True,
                    "privateCanaryAbsentFromTerminal": True, "missingHomeNotCreated": True,
                    "executedNodeCommands": 0,
                }, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    unittest.main()
