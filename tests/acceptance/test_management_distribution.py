"""Opt-in Mac CLI distribution acceptance; never starts an engine or service.

Provide two independently identified CLI bundles and a trusted test bootstrap
binary. This checks the actual installed executable and fresh interactive zsh,
using only a temporary HOME. Build/signing is deliberately outside this test.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import stat
import subprocess
import tarfile
import tempfile
import unittest

from test_zsh_completion import ZSH, ZshSession


_REQUIRED = ("BXDL_MANAGEMENT_BINARY", "BXDL_MANAGEMENT_BUNDLE_A", "BXDL_MANAGEMENT_BUNDLE_B")
_PAYLOADS = {"bin/bxdl", "share/zsh/site-functions/_bxdl", "docs/management-cli.md", "docs/completion.md"}
_METADATA = {"manifest.json", "manifest.sig"}
_CANARY = "PRIVATE-CLI-DISTRIBUTION-CANARY"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot(root):
    """Read-only fingerprint; atime is intentionally not a mutation oracle."""
    result = {}
    for path in [root, *sorted(root.rglob("*"))]:
        metadata = path.lstat()
        result[str(path.relative_to(root))] = {
            "mode": metadata.st_mode, "device": metadata.st_dev,
            "inode": metadata.st_ino, "mtime": metadata.st_mtime_ns,
            "ctime": metadata.st_ctime_ns,
            "data": digest(path) if stat.S_ISREG(metadata.st_mode)
            else os.readlink(path) if stat.S_ISLNK(metadata.st_mode) else None,
        }
    return result


def unpack_fixture(source, destination):
    """Copy only bounded fixed regular entries, never tarfile.extractall."""
    destination.mkdir(mode=0o700)
    seen = set()
    if source.is_dir():
        entries = []
        for name in sorted(_PAYLOADS | _METADATA):
            path = source / name
            if not path.exists() and not path.is_symlink():
                continue
            metadata = path.lstat()
            if not stat.S_ISREG(metadata.st_mode) or not 0 < metadata.st_size <= 64 * 1024 * 1024:
                raise AssertionError("acceptance bundle has an unsafe fixture file")
            entries.append((name, path.read_bytes(), metadata.st_mode & 0o777))
    else:
        entries = []
        with tarfile.open(source, "r:gz") as archive:
            for member in archive:
                if (member.name not in _PAYLOADS | _METADATA or not member.isfile()
                        or member.name in seen or member.size > 64 * 1024 * 1024):
                    raise AssertionError("acceptance bundle has an unexpected tar entry")
                seen.add(member.name)
                content = archive.extractfile(member).read(member.size + 1)
                if len(content) != member.size:
                    raise AssertionError("acceptance bundle entry is truncated")
                entries.append((member.name, content, member.mode))
    names = {name for name, _, _ in entries}
    if not _PAYLOADS | {"manifest.json"} <= names:
        raise AssertionError("acceptance bundle is missing required files")
    for name, content, mode in entries:
        path = destination / name
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        path.write_bytes(content)
        path.chmod(mode)
    return json.loads((destination / "manifest.json").read_text())


def seed_management_home(home, root):
    home.mkdir(mode=0o700, parents=True)
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
        "schemaVersion": 1, "kind": "BXDL_CONSOLE_HOME", "root": identity(home),
        "catalog": identity(catalog), "lock": identity(lock), "record": identity(marker),
    }))
    record = catalog / "revision00000001.json"
    record.write_text(json.dumps({
        "schemaVersion": 2, "kind": "BXDL_CONSOLE_CATALOG", "entries": [{
            "id": "1" * 32, "alias": "local", "bindingSha256": "a" * 64,
            "label": "existing local node", "recordedAt": "2026-10-23T00:00:00Z",
            "target": {"kind": "INSTANCE", "path": str(root / "protected" / "control"),
                       "identity": {"device": 1, "inode": 1}, "schemaVersion": 1,
                       "name": "validator", "state": "REGISTERED", "cli": "MISMATCH"},
        }],
    }))
    record.chmod(0o600)
    for namespace in ("sessions", "operations"):
        namespace_path = home / namespace
        namespace_path.mkdir(mode=0o700)
        directory = namespace_path / "local"
        directory.mkdir(mode=0o700)
        (directory / "private-record").write_text(_CANARY)
        (directory / "private-record").chmod(0o600)


@unittest.skipUnless(
    platform.system() == "Darwin" and platform.machine() == "arm64" and ZSH
    and all(os.environ.get(name) for name in _REQUIRED),
    "set BXDL_MANAGEMENT_BINARY/BUNDLE_A/BUNDLE_B on Mac arm64 for actual distribution acceptance",
)
class ManagementDistribution(unittest.TestCase):
    def test_install_update_rollback_and_preservation(self):
        bootstrap = Path(os.environ["BXDL_MANAGEMENT_BINARY"]).resolve(strict=True)
        sources = [Path(os.environ[name]).resolve(strict=True)
                   for name in ("BXDL_MANAGEMENT_BUNDLE_A", "BXDL_MANAGEMENT_BUNDLE_B")]
        public_key = os.environ.get("BXDL_MANAGEMENT_PUBLIC_KEY")
        trust_initial = ["--public-key", str(Path(public_key).resolve(strict=True))] if public_key else ["--allow-unsigned-development"]
        trust_update = [] if public_key else ["--allow-unsigned-development"]
        evidence = {"status": "RUNNING", "scope": "CLI distribution; no engine or service acceptance",
                    "buildProvenance": os.environ.get("BXDL_MANAGEMENT_BUILD_PROVENANCE", "caller-supplied acceptance builds"),
                    "bootstrapSha256": digest(bootstrap),
                    "host": {"system": platform.system(), "architecture": platform.machine()},
                    "scenarios": []}
        with tempfile.TemporaryDirectory(prefix="bxdl-cli-distribution-") as temporary:
            root = Path(temporary).resolve()
            root.chmod(0o700)
            environment = dict(os.environ, HOME=str(root), ZDOTDIR=str(root))
            bundle_a, bundle_b = root / "bundle A", root / "bundle B"
            manifests = [unpack_fixture(sources[0], bundle_a), unpack_fixture(sources[1], bundle_b)]
            identities = [{key: manifest[key] for key in ("version", "revision")} for manifest in manifests]
            self.assertNotEqual(identities[0], identities[1], "two different CLI identities are required")
            evidence["bundles"] = [{"identity": identity, "manifestSha256": digest(directory / "manifest.json"),
                                     "binarySha256": digest(directory / "bin/bxdl"),
                                     "archiveSha256": digest(source) if source.is_file() else None}
                                    for identity, directory, source in zip(identities, (bundle_a, bundle_b), sources)]
            home = root / "Library/Application Support/BXDL"
            seed_management_home(home, root)
            protected = root / "protected"
            for name in ("control", "data", "runtime-package"):
                path = protected / name
                path.mkdir(mode=0o700, parents=True)
                (path / "private-preserved").write_text(_CANARY)
            preserved = (snapshot(home), snapshot(protected))
            cli_root = root / "Library/Application Support/BXDL-CLI"
            bin_dir = root / ".local/bin"
            entry = bin_dir / "bxdl"

            def run(binary, args, success=True):
                completed = subprocess.run([str(binary), *args, "--json"], input=b"", stdout=subprocess.PIPE,
                                           stderr=subprocess.PIPE, env=environment, timeout=30, check=False,
                                           preexec_fn=lambda: os.umask(0o077))
                output = completed.stdout.decode()
                self.assertNotIn(_CANARY, output + completed.stderr.decode())
                self.assertEqual(completed.stderr, b"", completed.stderr.decode())
                response = json.loads(output)
                self.assertEqual(response["schemaVersion"], 1)
                if success:
                    self.assertEqual(completed.returncode, 0, output)
                    self.assertEqual(response["outcome"], "SUCCEEDED", output)
                else:
                    self.assertNotEqual(completed.returncode, 0, output)
                return response

            def step(name):
                self.assertEqual((snapshot(home), snapshot(protected)), preserved)
                evidence["scenarios"].append({"name": name, "status": "PASS"})

            def selected(identity):
                version = run(entry, ["version"])["data"]
                for key, value in identity.items():
                    self.assertEqual(version[key], value)
                current_binary = entry.resolve(strict=True)
                self.assertTrue(current_binary.is_relative_to(cli_root / "releases"))
                return str((cli_root / "current").readlink())

            def fresh_shell(identity, mode="source"):
                # Initialize without source, then choose the actual installed entry.
                shell = ZshSession(root, "none")
                try:
                    query = root / "shell-version.json"
                    selected_path = root / "shell-command-path"
                    shell.command(f"export PATH={shlex.quote(str(bin_dir))}:$PATH; rehash")
                    if mode == "autoload":
                        completion_dir = cli_root / "current/share/zsh/site-functions"
                        shell.command(f"fpath=({shlex.quote(str(completion_dir))} $fpath); compinit -D -i")
                    else:
                        shell.command("source <(bxdl completion zsh)")
                    shell.command(f"command -v bxdl > {shlex.quote(str(selected_path))}; bxdl version --json > {shlex.quote(str(query))}")
                    self.assertEqual(selected_path.read_text().strip(), str(entry))
                    response = json.loads(query.read_text())["data"]
                    for key, value in identity.items():
                        self.assertEqual(response[key], value)
                    self.assertEqual(shell.complete("bxdl cli inf"), "bxdl cli info ")
                    self.assertEqual(shell.complete("bxdl status loc"), "bxdl status local ")
                    self.assertNotIn(_CANARY.encode(), shell.transcript)
                finally:
                    shell.close()

            self.assertEqual(run(bootstrap, ["cli", "info"])["data"]["status"], "NOT_INSTALLED")
            self.assertFalse(cli_root.exists())
            self.assertFalse(bin_dir.exists())
            step("missing default installation info is read-only")

            run(bundle_a / "bin/bxdl", ["cli", "install", *trust_initial])
            selected(identities[0])
            run(entry, ["cli", "info"])
            fresh_shell(identities[0])
            self.assertEqual(entry.resolve().stat().st_mode & 0o777, 0o755)
            self.assertEqual((cli_root / "current/share/zsh/site-functions/_bxdl").stat().st_mode & 0o777, 0o644)
            step("self installation under umask 077, exact payload modes, default paths, new zsh, existing names")

            before = snapshot(cli_root)
            run(entry, ["cli", "install", "--from", str(sources[0]), *trust_initial])
            self.assertEqual(snapshot(cli_root), before)
            step("same identity installation is unchanged")

            run(entry, ["cli", "update", "--from", str(sources[1]), *trust_update])
            selected(identities[1])
            fresh_shell(identities[1], "autoload")
            completion = subprocess.check_output([str(entry), "completion", "zsh"], env=environment, timeout=10)
            self.assertEqual(completion, (cli_root / "current/share/zsh/site-functions/_bxdl").read_bytes())
            step("local bundle update and new zsh autoload use second identity")

            before = snapshot(cli_root)
            run(entry, ["cli", "info"])
            self.assertEqual(snapshot(cli_root), before)
            step("installed info is read-only")

            bad = root / "damaged-bundle.tar.gz"
            bad.write_bytes(b"not a CLI archive\n")
            before = snapshot(cli_root)
            run(entry, ["cli", "update", "--from", str(bad), *trust_update], success=False)
            self.assertEqual(snapshot(cli_root), before)
            selected(identities[1])
            step("invalid supply preserves installed version")

            before = snapshot(cli_root)
            rejected_trust = ["--allow-unsigned-development"] if public_key else []
            run(entry, ["cli", "update", "--from", str(sources[0]), *rejected_trust], success=False)
            self.assertEqual(snapshot(cli_root), before)
            selected(identities[1])
            step("installed trust policy cannot be omitted or weakened")

            entry_target = entry.readlink()
            entry.unlink()
            entry.write_text("USER-REPLACED-ENTRY")
            before = snapshot(cli_root)
            try:
                run(bootstrap, ["cli", "update", "--from", str(sources[0]), *trust_update], success=False)
                self.assertEqual(entry.read_text(), "USER-REPLACED-ENTRY")
                self.assertEqual(snapshot(cli_root), before)
            finally:
                entry.unlink()
                entry.symlink_to(entry_target)
            selected(identities[1])
            step("replaced installed entry is preserved and update is refused")

            prior_link = (cli_root / "current").readlink()
            mode = cli_root.stat().st_mode & 0o777
            cli_root.chmod(0o500)
            try:
                run(entry, ["cli", "update", "--from", str(sources[0]), *trust_update], success=False)
                self.assertEqual((cli_root / "current").readlink(), prior_link)
            finally:
                cli_root.chmod(mode)
            selected(identities[1])
            step("unsafe installation permissions preserve selected release")

            collision_bin = root / "collision bin"
            collision_bin.mkdir(mode=0o700)
            collision_entry = collision_bin / "bxdl"
            collision_entry.write_text("USER-OWNED-ENTRY")
            before = snapshot(collision_bin)
            run(bootstrap, ["cli", "install", "--from", str(sources[0]), "--root", str(root / "collision installation"),
                            "--bin-dir", str(collision_bin), *trust_initial], success=False)
            self.assertEqual(snapshot(collision_bin), before)
            step("foreign executable entry is not replaced")

            run(bootstrap, ["cli", "install", "--from", str(sources[0]), "--root", str(home),
                            "--bin-dir", str(root / "invalid bin"), *trust_initial], success=False)
            step("node management home cannot be adopted as CLI root")

            run(entry, ["cli", "rollback"])
            selected(identities[0])
            fresh_shell(identities[0])
            step("rollback restores first identity and its completion")

            run(entry, ["cli", "update", "--from", str(sources[1]), "--root", str(cli_root), *trust_update])
            selected(identities[1])
            step("explicit root reuses its recorded entry directory")

            self.assertFalse((root / ".zshrc").exists())
            self.assertFalse((root / ".zprofile").exists())
            evidence.update({"status": "PASS", "existingManagementAndNodeFixturesUnchanged": True,
                             "shellStartupFilesNotCreated": True, "executedNodeCommands": 0,
                             "mutationProcessUmask": "0077",
                             "trustMode": "external-ed25519" if public_key else "unsigned-development-explicit",
                             "zshVersion": subprocess.check_output([ZSH, "--version"], text=True).strip()})
            result = os.environ.get("BXDL_MANAGEMENT_RESULT")
            if result:
                Path(result).write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    unittest.main()
