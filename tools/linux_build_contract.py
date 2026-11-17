"""Pinned Linux build-kit contracts; no candidate execution or service mutation.

The plan hash is external trust input. Hashes in an untrusted plan alone are not
an authenticity proof. Source extraction accepts only bounded Git archive files
and directories plus its single commit-comment global PAX header.
"""
import gzip
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import selectors
import signal
import stat
import struct
import subprocess
import tarfile
import time
import zlib

MAX_FILE = 1024 * 1024 * 1024
MAX_SOURCE = 512 * 1024 * 1024
MAX_SOURCE_FILE = 64 * 1024 * 1024
MAX_JSON = 4 * 1024 * 1024
MAX_ENTRIES = 20000
CHUNK = 128 * 1024
SHA = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
PREFIX = "bxdl-source"
RUNTIME_PATH = "packaging/runtime-lock.temurin-21-linux-amd64.json"
ENGINE_PATH = "packaging/engine-lock.rc4-temurin-linux-development.json"
SOURCE_CAPTURE = {"Cargo.lock", "rust-toolchain.toml", RUNTIME_PATH, ENGINE_PATH}


class Failure(Exception):
    """Only a stable public failure code; no raw child output or input values."""


def require(condition, code):
    if not condition:
        raise Failure(code)


def _stamp(m):
    return (m.st_dev, m.st_ino, m.st_mode, m.st_uid, m.st_gid, m.st_size,
            m.st_nlink, m.st_mtime_ns, m.st_ctime_ns)


def _directory(path):
    m = path.lstat()
    require(stat.S_ISDIR(m.st_mode) and m.st_uid in (0, os.geteuid()), "PATH_OWNER_UNSAFE")
    require(not m.st_mode & 0o022 or bool(m.st_mode & stat.S_ISVTX), "PATH_MODE_UNSAFE")
    return m


def _absolute(path):
    path = Path(path).expanduser().absolute()
    require(path.is_absolute() and ".." not in path.parts
            and not any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in str(path)), "PATH_INVALID")
    for ancestor in reversed(path.parents):
        _directory(ancestor)
    return path


def _open(path):
    path = _absolute(path)
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1
            and before.st_uid in (0, os.geteuid()) and not before.st_mode & 0o7022
            and 0 <= before.st_size <= MAX_FILE, "INPUT_FILE_UNSAFE")
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    stream = os.fdopen(fd, "rb")
    try:
        require(_stamp(os.fstat(stream.fileno())) == _stamp(before), "INPUT_CHANGED")
        return path, before, stream
    except BaseException:
        stream.close()
        raise


def fingerprint(path):
    path, before, stream = _open(path)
    count, h = 0, hashlib.sha256()
    with stream:
        while raw := stream.read(CHUNK):
            count += len(raw)
            require(count <= before.st_size, "INPUT_CHANGED")
            h.update(raw)
        require(count == before.st_size and _stamp(os.fstat(stream.fileno())) == _stamp(before)
                and _stamp(path.lstat()) == _stamp(before), "INPUT_CHANGED")
    return {"sha256": h.hexdigest(), "size": count, "mode": stat.S_IMODE(before.st_mode)}


def digest(path):
    return fingerprint(path)["sha256"]


def _read(path, maximum=MAX_JSON):
    path, before, stream = _open(path)
    with stream:
        require(before.st_size <= maximum, "INPUT_SIZE_LIMIT")
        raw = stream.read(maximum + 1)
        require(len(raw) == before.st_size and len(raw) <= maximum
                and _stamp(os.fstat(stream.fileno())) == _stamp(before)
                and _stamp(path.lstat()) == _stamp(before), "INPUT_CHANGED")
        return raw


def _json(raw):
    def pairs(values):
        result = {}
        for k, v in values:
            require(k not in result, "JSON_DUPLICATE_KEY")
            result[k] = v
        return result
    try:
        return json.loads(raw, object_pairs_hook=pairs,
                          parse_constant=lambda _: (_ for _ in ()).throw(Failure("JSON_INVALID")))
    except (ValueError, UnicodeError, RecursionError) as error:
        raise Failure("JSON_INVALID") from error


def _fields(value, fields):
    require(isinstance(value, dict) and set(value) == set(fields), "PLAN_FIELDS_INVALID")


def _hash(value):
    return isinstance(value, str) and SHA.fullmatch(value) is not None


def _relative(value):
    require(isinstance(value, str) and 0 < len(value.encode()) <= 255
            and not any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in value)
            and not any(c in value for c in "\\:")
            and not value.startswith("/") and not value.endswith("/"), "RELATIVE_PATH_INVALID")
    parts = value.split("/")
    require(all(p and p not in (".", "..") for p in parts), "RELATIVE_PATH_INVALID")
    return PurePosixPath(value)


def _ref(value, expected_path, entries):
    _fields(value, ("path", "sha256", "size"))
    require(value["path"] == expected_path and _hash(value["sha256"])
            and type(value["size"]) is int and 0 < value["size"] <= MAX_FILE, "PLAN_REFERENCE_INVALID")
    require(expected_path in entries and all(entries[expected_path][k] == value[k]
                                           for k in ("sha256", "size")), "PLAN_REFERENCE_MISMATCH")


class _Decoded:
    def __init__(self, stream):
        self.stream, self.count = stream, 0

    def read(self, amount):
        require(0 <= amount <= CHUNK, "SOURCE_READ_LIMIT")
        raw = self.stream.read(amount)
        self.count += len(raw)
        require(self.count <= MAX_SOURCE + MAX_ENTRIES * 1024 + 10240, "SOURCE_SIZE_LIMIT")
        return raw

    def exact(self, amount):
        result = self.read(amount)
        require(len(result) == amount, "SOURCE_TRUNCATED")
        return result


def _source_scan_unchecked(archive, output=None, expected_commit=None, expected_archive=None):
    """Manual bounded headers avoid tarfile's implicit extension processing."""
    before = fingerprint(archive)
    require(expected_archive is None or before == expected_archive, "INPUT_CHANGED")
    names, files, directories, captures = set(), set(), set(), {}
    commit, total, entries, ended = None, 0, 0, False
    with _open(archive)[2] as source:
        with gzip.GzipFile(fileobj=source, mode="rb") as gz:
            stream = _Decoded(gz)
            while True:
                header = stream.exact(512)
                if header == b"\0" * 512:
                    require(stream.exact(512) == b"\0" * 512, "SOURCE_END_MARKER")
                    while raw := stream.read(CHUNK):
                        require(not any(raw), "SOURCE_TRAILING_DATA")
                    ended = True
                    break
                entries += 1
                require(entries <= MAX_ENTRIES, "SOURCE_ENTRY_LIMIT")
                try:
                    item = tarfile.TarInfo.frombuf(header, "utf-8", "strict")
                except (tarfile.HeaderError, UnicodeError, ValueError) as error:
                    raise Failure("SOURCE_HEADER_INVALID") from error
                require(header[257:263] == b"ustar\0", "SOURCE_FORMAT_INVALID")
                if item.type == tarfile.XGLTYPE:
                    require(entries == 1 and commit is None and 0 < item.size <= 1024,
                            "SOURCE_PAX_INVALID")
                    raw = stream.exact(item.size)
                    padding = stream.exact((-item.size) % 512)
                    match = re.fullmatch(rb"([0-9]+) comment=([0-9a-f]{40})\n", raw)
                    require(match is not None and int(match[1]) == len(raw) and not any(padding),
                            "SOURCE_PAX_INVALID")
                    commit = match[2].decode("ascii")
                    continue
                require(item.type in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE)
                        and not item.linkname and 0 <= item.size <= MAX_SOURCE_FILE,
                        "SOURCE_ENTRY_INVALID")
                name = item.name.rstrip("/") if item.isdir() else item.name
                path = _relative(name)
                require(path.parts[0] == PREFIX and name not in names, "SOURCE_PATH_INVALID")
                names.add(name)
                require(item.mode in ((0o755,) if item.isdir() else (0o644, 0o755)), "SOURCE_MODE_INVALID")
                require(not any(str(parent) in files for parent in path.parents), "SOURCE_PATH_CONFLICT")
                if name == PREFIX:
                    require(item.isdir() and not directories and not files, "SOURCE_ROOT_INVALID")
                else:
                    require(PREFIX in directories and str(path.parent) in directories, "SOURCE_ROOT_INVALID")
                relative = PurePosixPath(*path.parts[1:])
                destination = output.joinpath(*relative.parts) if output is not None else None
                if item.isdir():
                    require(item.size == 0, "SOURCE_DIRECTORY_INVALID")
                    directories.add(name)
                    if destination is not None and name != PREFIX:
                        destination.mkdir(mode=0o755)
                    continue
                require(len(path.parts) > 1 and name not in directories
                        and str(path.parent) in directories, "SOURCE_PATH_CONFLICT")
                files.add(name)
                total += item.size
                require(total <= MAX_SOURCE, "SOURCE_SIZE_LIMIT")
                capture = str(relative) in SOURCE_CAPTURE
                if capture:
                    require(item.size <= MAX_JSON, "SOURCE_CAPTURE_LIMIT")
                chunks, left = [], item.size
                handle = None
                try:
                    if destination is not None:
                        fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
                        handle = os.fdopen(fd, "wb")
                    while left:
                        raw = stream.exact(min(left, CHUNK))
                        left -= len(raw)
                        if capture:
                            chunks.append(raw)
                        if handle is not None:
                            handle.write(raw)
                    if handle is not None:
                        handle.flush()
                        os.fchmod(handle.fileno(), item.mode)
                        os.fsync(handle.fileno())
                finally:
                    if handle is not None:
                        handle.close()
                require(not any(stream.exact((-item.size) % 512)), "SOURCE_PADDING_INVALID")
                if capture:
                    captures[str(relative)] = b"".join(chunks)
    require(ended and files and (expected_commit is None or commit == expected_commit), "SOURCE_COMMIT_MISMATCH")
    require(fingerprint(archive) == before, "INPUT_CHANGED")
    return {"commit": commit, "files": len(files), "bytes": total, "captures": captures, "archive": before}


def _source_scan(archive, output=None, expected_commit=None, expected_archive=None):
    try:
        return _source_scan_unchecked(archive, output, expected_commit, expected_archive)
    except (gzip.BadGzipFile, EOFError, zlib.error) as error:
        raise Failure("SOURCE_COMPRESSION_INVALID") from error


def extract_source(archive, output, expected_commit=None):
    output = _absolute(output)
    require(not output.exists() and not output.is_symlink(), "SOURCE_OUTPUT_EXISTS")
    if expected_commit is not None:
        require(isinstance(expected_commit, str) and COMMIT.fullmatch(expected_commit), "SOURCE_COMMIT_INVALID")
    snapshot = _source_scan(archive, expected_commit=expected_commit)
    output.mkdir(mode=0o700)
    _source_scan(archive, output=output, expected_commit=snapshot["commit"], expected_archive=snapshot["archive"])
    return output


def load_plan(kit, expected_sha):
    require(_hash(expected_sha), "PLAN_HASH_INVALID")
    kit = _absolute(kit)
    _directory(kit)
    raw = _read(kit / "build-plan.json")
    require(hashlib.sha256(raw).hexdigest() == expected_sha, "PLAN_HASH_MISMATCH")
    plan = _json(raw)
    _fields(plan, ("schemaVersion", "kind", "source", "toolchain", "platform", "supply", "files"))
    require(type(plan["schemaVersion"]) is int and plan["schemaVersion"] == 1
            and plan["kind"] == "bxdl-linux-native-build-plan", "PLAN_KIND_INVALID")
    require(plan["toolchain"] == {"rust": "1.86.0", "target": "x86_64-unknown-linux-gnu"}, "PLAN_TOOLCHAIN_INVALID")
    require(plan["platform"] == {"os": "linux", "arch": "amd64", "libc": "glibc",
            "builderDistribution": "ubuntu", "builderVersion": "24.04", "declaredMinGlibc": "2.34"}, "PLAN_PLATFORM_INVALID")
    _fields(plan["source"], ("commit", "archive", "cargoLockSha256", "rustToolchainSha256"))
    require(isinstance(plan["source"]["commit"], str) and COMMIT.fullmatch(plan["source"]["commit"])
            and _hash(plan["source"]["cargoLockSha256"]) and _hash(plan["source"]["rustToolchainSha256"]), "PLAN_SOURCE_INVALID")
    require(isinstance(plan["files"], list) and 0 < len(plan["files"]) <= MAX_ENTRIES, "PLAN_INVENTORY_INVALID")
    entries, expected_dirs = {}, set()
    for entry in plan["files"]:
        _fields(entry, ("path", "sha256", "size", "mode"))
        relative = _relative(entry["path"])
        require(entry["path"] != "build-plan.json" and entry["path"] not in entries
                and _hash(entry["sha256"]) and type(entry["size"]) is int and 0 <= entry["size"] <= MAX_FILE
                and type(entry["mode"]) is int and entry["mode"] in (0o644, 0o755), "PLAN_INVENTORY_INVALID")
        require(fingerprint(kit / entry["path"]) == {k: entry[k] for k in ("sha256", "size", "mode")}, "KIT_FILE_MISMATCH")
        entries[entry["path"]] = entry
        expected_dirs.update(str(parent) for parent in relative.parents if str(parent) != ".")
    actual, dirs = set(), set()
    for base, child_dirs, child_files in os.walk(kit, followlinks=False):
        for child in child_dirs:
            path = Path(base) / child
            _directory(path)
            dirs.add(path.relative_to(kit).as_posix())
        for child in child_files:
            path = Path(base) / child
            require(stat.S_ISREG(path.lstat().st_mode), "KIT_FILE_UNSAFE")
            actual.add(path.relative_to(kit).as_posix())
    require(actual == set(entries) | {"build-plan.json"} and dirs == expected_dirs, "KIT_INVENTORY_MISMATCH")
    _ref(plan["source"]["archive"], "source/bxdl-source.tar.gz", entries)
    _fields(plan["supply"], ("runtimeArchive", "jar", "runtimeLock", "engineLock"))
    for field, path in (("runtimeArchive", "supply/jre.tar.gz"), ("jar", "supply/nigo-node.jar"),
                        ("runtimeLock", RUNTIME_PATH), ("engineLock", ENGINE_PATH)):
        _ref(plan["supply"][field], path, entries)
    require({"tools/linux_build_contract.py", "tools/build-linux-candidate.py"}.issubset(entries), "KIT_TOOLS_REQUIRED")
    source = _source_scan(kit / plan["source"]["archive"]["path"], expected_commit=plan["source"]["commit"])
    for name, expected in (("Cargo.lock", plan["source"]["cargoLockSha256"]),
                           ("rust-toolchain.toml", plan["source"]["rustToolchainSha256"]),
                           (RUNTIME_PATH, plan["supply"]["runtimeLock"]["sha256"]),
                           (ENGINE_PATH, plan["supply"]["engineLock"]["sha256"])):
        require(name in source["captures"] and hashlib.sha256(source["captures"][name]).hexdigest() == expected,
                "SOURCE_PIN_MISMATCH")
    runtime = _json(_read(kit / RUNTIME_PATH))
    engine = _json(_read(kit / ENGINE_PATH))
    require(isinstance(runtime, dict) and isinstance(engine, dict)
            and type(runtime.get("schemaVersion")) is int and runtime["schemaVersion"] == 1
            and runtime.get("kind") == "bxdl-runtime-lock"
            and runtime.get("platform") == {"os": "linux", "arch": "amd64", "libc": "glibc", "javaMajor": 21}
            and type(engine.get("schemaVersion")) is int and engine["schemaVersion"] == 1
            and isinstance(runtime.get("archive"), dict) and isinstance(engine.get("expected"), dict),
            "SUPPLY_LOCK_INVALID")
    identity = engine["expected"]
    require(all(isinstance(identity.get(key), dict) for key in ("source", "java", "contract", "distribution")),
            "ENGINE_IDENTITY_INVALID")
    require(runtime["archive"].get("sha256") == plan["supply"]["runtimeArchive"]["sha256"]
            and type(runtime["archive"].get("sizeBytes")) is int
            and runtime["archive"]["sizeBytes"] == plan["supply"]["runtimeArchive"]["size"]
            and engine.get("jarSha256") == plan["supply"]["jar"]["sha256"]
            and type(engine.get("jarSizeBytes")) is int
            and engine["jarSizeBytes"] == plan["supply"]["jar"]["size"], "SUPPLY_PIN_MISMATCH")
    require(_hash(runtime.get("javaSha256")) and runtime["javaSha256"] == engine.get("javaSha256")
            and identity["source"].get("dirty") is False
            and isinstance(identity["source"].get("commit"), str)
            and COMMIT.fullmatch(identity["source"]["commit"])
            and type(identity["java"].get("requiredMajor")) is int and identity["java"]["requiredMajor"] == 21
            and _hash(identity["contract"].get("fingerprint"))
            and identity.get("engine") == "NIGO" and identity.get("buildInfoStatus") == "AVAILABLE"
            and identity["distribution"].get("officialRelease") is False
            and identity["distribution"].get("channel") == "development", "ENGINE_IDENTITY_INVALID")
    require(hashlib.sha256(_read(kit / "build-plan.json")).hexdigest() == expected_sha, "PLAN_CHANGED")
    return plan


def inspect_elf(path):
    path, before, stream = _open(path)
    with stream:
        header = stream.read(64)
        require(len(header) == 64 and header[:7] == b"\x7fELF\x02\x01\x01"
                and header[7] in (0, 3), "ELF_X86_64_REQUIRED")
        values = struct.unpack("<HHIQQQIHHHHHH", header[16:64])
        kind, machine, version, _, phoff, shoff, _, ehsize, phsize, phnum, shsize, shnum, shstr = values
        require(kind in (2, 3) and machine == 62 and version == 1 and ehsize == 64
                and phnum <= 1024 and shnum < 65535
                and (phnum == 0 or phsize == 56 and 64 <= phoff and phoff + phnum * phsize <= before.st_size)
                and (shnum == 0 or shsize == 64 and 64 <= shoff and shoff + shnum * shsize <= before.st_size and shstr < shnum),
                "ELF_HEADER_INVALID")
        require(_stamp(os.fstat(stream.fileno())) == _stamp(before) and _stamp(path.lstat()) == _stamp(before), "INPUT_CHANGED")
    return {"class": "ELF64", "endianness": "little", "machine": "x86_64",
            "type": "EXEC" if kind == 2 else "DYN", "size": before.st_size}


def _run_readelf(path):
    tool = Path("/usr/bin/readelf").resolve(strict=True)
    _absolute(tool)
    require(tool.stat().st_uid == 0 and fingerprint(tool)["mode"] & 0o111, "READELF_UNTRUSTED")
    command = [str(tool), "--version-info", "--wide", "--", str(path)]
    process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               env={"LANG": "C", "LC_ALL": "C", "PATH": "/usr/bin:/bin"},
                               close_fds=True, start_new_session=True)
    chunks = {"stdout": bytearray(), "stderr": bytearray()}
    selector = selectors.DefaultSelector()
    try:
        for name, pipe in (("stdout", process.stdout), ("stderr", process.stderr)):
            os.set_blocking(pipe.fileno(), False)
            selector.register(pipe, selectors.EVENT_READ, name)
        deadline = time.monotonic() + 30
        while selector.get_map():
            require(time.monotonic() < deadline, "READELF_TIMEOUT")
            for key, _ in selector.select(0.1):
                raw = os.read(key.fd, CHUNK)
                if not raw:
                    selector.unregister(key.fileobj)
                    continue
                chunks[key.data].extend(raw)
                require(sum(map(len, chunks.values())) <= MAX_JSON, "READELF_OUTPUT_LIMIT")
        require(process.wait(timeout=max(0.1, deadline - time.monotonic())) == 0
                and not chunks["stderr"], "READELF_FAILED")
        return bytes(chunks["stdout"]).decode("ascii")
    except (UnicodeError, subprocess.TimeoutExpired) as error:
        raise Failure("READELF_FAILED") from error
    finally:
        selector.close()
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        process.stdout.close()
        process.stderr.close()


def _parse_version_needs(raw):
    require(isinstance(raw, str) and len(raw) <= MAX_JSON, "READELF_OUTPUT_INVALID")
    required, in_needs = set(), False
    recognized = "No version information found in this file." in raw
    for line in raw.splitlines():
        if re.match(r"^Version \w+ section ", line):
            in_needs = line.startswith("Version needs section ")
            recognized = True
        if not in_needs:
            continue
        match = re.search(r"\bName:\s+(GLIBC_\S+)", line)
        if match:
            numeric = match[1][6:]
            require(re.fullmatch(r"[0-9]+(?:\.[0-9]+){1,3}", numeric) is not None
                    and len(numeric) <= 24, "GLIBC_REQUIREMENT_UNSUPPORTED")
            required.add(numeric)
    require(recognized, "READELF_OUTPUT_INVALID")
    ordered = sorted(required, key=lambda s: tuple(map(int, s.split("."))))
    return {"requiredVersions": ordered, "maximumRequired": ordered[-1] if ordered else None,
            "tool": "readelf --version-info", "scope": "VERSION_NEEDS_ONLY"}


def glibc_requirements(path):
    path = _absolute(path)
    before = fingerprint(path)
    inspect_elf(path)
    result = _parse_version_needs(_run_readelf(path))
    require(fingerprint(path) == before, "INPUT_CHANGED")
    return result
