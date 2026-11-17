#!/usr/bin/env python3
"""Build a native Mac arm64 or Linux x86_64/glibc management CLI archive.

A normal build requires a clean checkout and embeds its exact Git commit.
--development permits a dirty checkout only with explicit unsigned output;
its revision is honestly "development", so it is not a release identity.
No network download, key generation, shell configuration or publication occurs.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
TARGETS = {"darwin-arm64": "aarch64-apple-darwin", "linux-amd64": "x86_64-unknown-linux-gnu"}


def native_target():
    system, machine = platform.system(), platform.machine().lower()
    if system == "Darwin" and machine in ("arm64", "aarch64"):
        return "darwin-arm64"
    if system == "Linux" and machine in ("x86_64", "amd64") and platform.libc_ver()[0] == "glibc":
        return "linux-amd64"
    fail("A native Mac arm64 or Linux x86_64/glibc builder is required.")


def select_target(requested):
    native = native_target()
    if requested is not None and requested != native:
        fail("The selected target must match the native builder; cross-compiled candidates are not executed or published.")
    return native



def git(*arguments):
    return subprocess.check_output(["git", *arguments], cwd=ROOT, text=True).strip()


def fail(message):
    raise SystemExit(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path, help="new tar.gz output path (existing parent)")
    trust = parser.add_mutually_exclusive_group(required=True)
    trust.add_argument("--signing-key", type=Path, help="external Ed25519 private PEM key")
    trust.add_argument("--allow-unsigned-development", action="store_true")
    parser.add_argument("--development", action="store_true", help="allow dirty sources; unsigned only; revision=development")
    parser.add_argument("--target", choices=tuple(TARGETS), help="native distribution target; defaults to the current supported host")
    args = parser.parse_args()
    target = select_target(args.target)
    rust_target = TARGETS[target]
    if args.development and not args.allow_unsigned_development:
        fail("--development requires --allow-unsigned-development; dirty sources are never signed.")
    output = args.output.expanduser().absolute()
    if output.exists() or output.is_symlink() or not output.parent.is_dir():
        fail("Choose a new output file with an existing parent directory.")
    head = git("rev-parse", "HEAD")
    if not re.fullmatch(r"[0-9a-f]{40}", head):
        fail("Cannot identify an exact source commit.")
    if not args.development and git("status", "--porcelain", "--untracked-files=all"):
        fail("The checkout is dirty. Commit the intended source or explicitly choose an unsigned --development build.")
    revision = "development" if args.development else head
    package = (ROOT / "Cargo.toml").read_text(encoding="utf-8").split("[dependencies]", 1)[0]
    match = re.search(r'^version\s*=\s*"([^"]+)"\s*$', package, re.MULTILINE)
    if not match:
        fail("Cannot read the management CLI package version.")
    version = match.group(1)
    environment = dict(os.environ, BXDL_REVISION=revision,
                       CARGO_TARGET_DIR=str(ROOT / "target" / "management-distribution"))
    subprocess.run(["cargo", "build", "--locked", "--release", "--target", rust_target],
                   cwd=ROOT, env=environment, check=True)
    binary = Path(environment["CARGO_TARGET_DIR"]) / rust_target / "release" / "bxdl"
    observed = json.loads(subprocess.check_output([str(binary), "version", "--json"], text=True))
    if (observed.get("data", {}).get("version") != version
            or observed["data"].get("revision") != revision
            or observed["data"].get("primaryTarget") != target):
        fail("The built CLI does not report the selected version, revision and native target.")
    if not args.development and (git("rev-parse", "HEAD") != head or git("status", "--porcelain", "--untracked-files=all")):
        fail("Sources changed during the build; no archive was published.")
    # Stage only the explicit distribution inventory; customer material, keys,
    # engine/runtime files and arbitrary repository files cannot be collected.
    with tempfile.TemporaryDirectory(prefix="bxdl-cli-stage-") as temporary:
        stage = Path(temporary).resolve()
        payloads = {
            "bin/bxdl": binary,
            "share/zsh/site-functions/_bxdl": ROOT / "completions" / "_bxdl",
            "docs/management-cli.md": ROOT / "docs" / "management-cli.md",
            "docs/completion.md": ROOT / "docs" / "completion.md",
        }
        for relative, source in payloads.items():
            destination = stage / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
            destination.chmod(0o755 if relative == "bin/bxdl" else 0o644)
        if not args.development and (git("rev-parse", "HEAD") != head or git("status", "--porcelain", "--untracked-files=all")):
            fail("Sources changed while staging; no archive was published.")
        command = [str(binary), "cli", "bundle", "build", "--root", str(stage),
                   "--output", str(output), "--version", version, "--revision", revision, "--json"]
        if args.signing_key:
            command.extend(["--signing-key", str(args.signing_key.expanduser().absolute())])
        else:
            command.append("--allow-unsigned-development")
        subprocess.run(command, cwd=ROOT, check=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, subprocess.CalledProcessError, json.JSONDecodeError):
        fail("Management CLI build failed. No existing distribution file was replaced.")
