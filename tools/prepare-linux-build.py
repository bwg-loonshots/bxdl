#!/usr/bin/env python3
"""Prepare pinned native-Linux build inputs without building or executing Linux code.

The selected Git commit (not the dirty checkout) supplies the product source,
locks and provenance. Orchestrator scripts have separate content pins. An
unsigned development kit is not a runtime package or release attestation.
"""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile

sys.dont_write_bytecode = True
import linux_build_contract as contract

ROOT = Path(__file__).resolve().parents[1]
LOCKS = {
    "runtimeLock": "packaging/runtime-lock.temurin-21-linux-amd64.json",
    "engineLock": "packaging/engine-lock.rc4-temurin-linux-development.json",
}
SCRIPTS = ("tools/build-linux-candidate.py", "tools/linux_build_contract.py")


def git(*args):
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    env.update(GIT_NO_REPLACE_OBJECTS="1", GIT_CONFIG_NOSYSTEM="1")
    return subprocess.check_output(["git", *args], cwd=ROOT, env=env, stderr=subprocess.DEVNULL)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode()


def safe_parent(output):
    output = Path(os.path.abspath(output))
    contract.require(not output.exists() and not output.is_symlink(), "OUTPUT_EXISTS")
    for parent in output.parents:
        m = parent.lstat()
        contract.require(stat.S_ISDIR(m.st_mode) and m.st_uid in (0, os.geteuid())
                         and (not m.st_mode & 0o022 or m.st_mode & stat.S_ISVTX), "OUTPUT_PARENT_UNSAFE")
    return output


def create_file(path, raw, mode=0o644):
    path.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, mode)
    with os.fdopen(fd, "wb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    path.chmod(mode)


def copy_pinned(source, destination, expected):
    before = contract.fingerprint(source)
    contract.require(before["sha256"] == expected["sha256"] and before["size"] == expected["size"], "SUPPLY_PIN_MISMATCH")
    destination.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    fd = os.open(source, os.O_RDONLY | os.O_NOFOLLOW)
    out = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    copied = hashlib.sha256()
    size = 0
    with os.fdopen(fd, "rb") as incoming, os.fdopen(out, "wb") as outgoing:
        while chunk := incoming.read(1024 * 1024):
            copied.update(chunk)
            size += len(chunk)
            contract.require(size <= expected["size"], "SUPPLY_CHANGED")
            outgoing.write(chunk)
        outgoing.flush()
        os.fsync(outgoing.fileno())
    destination.chmod(0o644)
    contract.require(size == expected["size"] and copied.hexdigest() == expected["sha256"]
                     and contract.fingerprint(source) == before, "SUPPLY_CHANGED")


def prepare(source_commit, runtime_archive, jar, output):
    contract.require(re.fullmatch(r"[0-9a-f]{40}", source_commit) is not None, "SOURCE_COMMIT_INVALID")
    actual = git("rev-parse", "--verify", source_commit + "^{commit}").decode().strip()
    contract.require(actual == source_commit, "SOURCE_COMMIT_INVALID")
    entries = git("ls-tree", "-rlz", source_commit).split(b"\0")
    total, count = 0, 0
    for item in entries:
        if not item:
            continue
        header, _ = item.split(b"\t", 1)
        mode, kind, _, size = header.split()
        contract.require(mode in (b"100644", b"100755") and kind == b"blob", "SOURCE_ENTRY_UNSUPPORTED")
        total += int(size)
        count += 1
        contract.require(total <= 512 * 1024 * 1024 and count <= 20000, "SOURCE_ARCHIVE_LIMIT")
    # Export only committed files and stable Git tar metadata. The archive's
    # bounded global comment records the commit, and is independently hashed.
    source_tar = git("-c", "tar.umask=022", "archive", "--format=tar", "--prefix=bxdl-source/", source_commit)
    contract.require(len(source_tar) <= 512 * 1024 * 1024, "SOURCE_ARCHIVE_LIMIT")
    source_gzip = gzip.compress(source_tar, mtime=0)
    committed = lambda path: git("show", source_commit + ":" + path)
    raw_locks = {key: committed(path) for key, path in LOCKS.items()}
    runtime_lock = json.loads(raw_locks["runtimeLock"])
    engine_lock = json.loads(raw_locks["engineLock"])
    contract.require(runtime_lock["platform"] == {"os": "linux", "arch": "amd64", "libc": "glibc", "javaMajor": 21}, "RUNTIME_PLATFORM_INVALID")
    supply = {
        "runtimeArchive": {"path": "supply/jre.tar.gz", "sha256": runtime_lock["archive"]["sha256"], "size": runtime_lock["archive"]["sizeBytes"]},
        "jar": {"path": "supply/nigo-node.jar", "sha256": engine_lock["jarSha256"], "size": engine_lock["jarSizeBytes"]},
    }
    for key in LOCKS:
        supply[key] = {"path": LOCKS[key], "sha256": sha(raw_locks[key]), "size": len(raw_locks[key])}
    # Reject invalid sources before reserving any output. Never bless observed
    # JAR/JRE bytes by replacing the expected values from committed locks.
    for name, path in [("runtimeArchive", runtime_archive), ("jar", jar)]:
        observed = contract.fingerprint(path)
        contract.require(all(observed[key] == supply[name][key] for key in ("sha256", "size")), "SUPPLY_PIN_MISMATCH")
    scripts = {}
    for name in SCRIPTS:
        before = contract.fingerprint(ROOT / name)
        raw = (ROOT / name).read_bytes()
        contract.require(sha(raw) == before["sha256"] and len(raw) == before["size"], "TOOL_CHANGED")
        scripts[name] = (raw, before)
    output = safe_parent(output)
    output.mkdir(mode=0o700)
    create_file(output / "source/bxdl-source.tar.gz", source_gzip)
    for key, name in LOCKS.items():
        create_file(output / name, raw_locks[key])
    for name, (raw, _) in scripts.items():
        create_file(output / name, raw, 0o755 if name.endswith("candidate.py") else 0o644)
    copy_pinned(runtime_archive, output / supply["runtimeArchive"]["path"], supply["runtimeArchive"])
    copy_pinned(jar, output / supply["jar"]["path"], supply["jar"])
    files = []
    for path in sorted(output.rglob("*")):
        if path.is_file():
            files.append({"path": path.relative_to(output).as_posix(), **contract.fingerprint(path)})
    plan = {
        "schemaVersion": 1, "kind": "bxdl-linux-native-build-plan",
        "source": {"commit": source_commit, "archive": {"path": "source/bxdl-source.tar.gz", "sha256": sha(source_gzip), "size": len(source_gzip)},
                   "cargoLockSha256": sha(committed("Cargo.lock")), "rustToolchainSha256": sha(committed("rust-toolchain.toml"))},
        "toolchain": {"rust": "1.86.0", "target": "x86_64-unknown-linux-gnu"},
        "platform": {"os": "linux", "arch": "amd64", "libc": "glibc", "builderDistribution": "ubuntu", "builderVersion": "24.04", "declaredMinGlibc": "2.34"},
        "supply": supply, "files": files,
    }
    create_file(output / "build-plan.json", encoded(plan))
    for name, (_, before) in scripts.items():
        contract.require(contract.fingerprint(ROOT / name) == before, "TOOL_CHANGED")
    plan_sha = contract.digest(output / "build-plan.json")
    contract.load_plan(output, plan_sha)
    return {"schemaVersion": 1, "outcome": "PREPARED", "reasonCode": "LINUX_NATIVE_BUILD_INPUTS_PINNED",
            "sourceCommit": source_commit, "planSha256": plan_sha, "kitFiles": len(files),
            "nativeBuild": "NOT_RUN", "runtimePackage": "NOT_ASSEMBLED", "systemd": "NOT_RUN"}


def archive_kit(root, destination):
    destination = safe_parent(destination)
    contract.require(not destination.is_relative_to(root), "OUTPUT_OVERLAP")
    # No source archive symlink or extended tar headers are introduced in the
    # outer delivery. This archive is checksummed, not signed/admitted.
    fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as stream, gzip.GzipFile(fileobj=stream, mode="wb", filename="", mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w|", format=tarfile.USTAR_FORMAT) as tar:
            for path in sorted(root.rglob("*")):
                if path.is_file():
                    before = contract.fingerprint(path)
                    info = tarfile.TarInfo("bxdl-linux-build/" + path.relative_to(root).as_posix())
                    info.size, info.mode, info.mtime = before["size"], before["mode"], 0
                    with path.open("rb") as source:
                        tar.addfile(info, source)
                    contract.require(contract.fingerprint(path) == before, "KIT_CHANGED")
    return {"sha256": contract.digest(destination), "size": destination.stat().st_size}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--runtime-archive", required=True, type=Path)
    parser.add_argument("--jar", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--archive-output", type=Path)
    parser.add_argument("--allow-unsigned-development", action="store_true", required=True)
    args = parser.parse_args()
    os.umask(0o077)
    if args.archive_output:
        archive_path = safe_parent(args.archive_output)
        output_path = Path(os.path.abspath(args.output))
        contract.require(not archive_path.is_relative_to(output_path), "OUTPUT_OVERLAP")
    value = prepare(args.source_commit, Path(os.path.abspath(args.runtime_archive)), Path(os.path.abspath(args.jar)), args.output)
    if args.archive_output:
        value["kitArchive"] = archive_kit(Path(os.path.abspath(args.output)), args.archive_output)
    print(json.dumps(value, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (contract.Failure, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        code = getattr(error, "code", None) or (str(error) if isinstance(error, contract.Failure) else "LINUX_BUILD_PREPARATION_FAILED")
        print(json.dumps({"schemaVersion": 1, "outcome": "FAILED", "reasonCode": code, "nativeBuild": "NOT_RUN"}))
        raise SystemExit(3)
