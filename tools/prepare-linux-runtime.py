#!/usr/bin/env python3
"""Offline materialization of a pinned Linux full JRE; never executes a binary.

The supplied lock digest is external trust input. This is intentionally not a
universal tar extractor: only ordinary tar regular files, directories and
relative file symlinks in one root are accepted. Extended/sparse/hardlink entries
are rejected before their metadata is processed. Output is a new private root;
failed output is retained for inspection and never automatically removed.
"""
import argparse
from contextlib import ExitStack
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import tarfile

MAX_FILES = 10_000
MAX_ENTRIES = 20_000
MAX_FILE_BYTES = 512 * 1024 * 1024
MAX_TOTAL_BYTES = 4 * 1024 * 1024 * 1024
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
MAX_LOCK_BYTES = 4 * 1024 * 1024
MAX_TAR_BYTES = MAX_TOTAL_BYTES + MAX_ENTRIES * 1024 + 10240
CHUNK = 128 * 1024
PLATFORM = {"os": "linux", "arch": "amd64", "libc": "glibc", "javaMajor": 21}
FORBIDDEN = {".git", "node_modules", "secret", "secrets", "key", "keys", "db", "data", "wal", "credentials", "passwords"}


class SupplyError(Exception):
    pass


def require(condition, code):
    if not condition:
        raise SupplyError(code)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def hex_hash(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def label(value, limit=256):
    return (isinstance(value, str) and 0 < len(value.encode()) <= limit
            and value.strip() == value and not any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in value))


def exact_fields(value, names, code):
    require(isinstance(value, dict) and set(value) == set(names), code)


def relative_name(value, limit=247):
    require(label(value, limit) and value.isascii() and not any(c in value for c in "\\:")
            and not value.startswith("/") and not value.endswith("/"), "PATH_INVALID")
    parts = value.split("/")
    require(all(p and p not in (".", "..") for p in parts)
            and len(parts[-1].encode()) <= 100, "PATH_INVALID")
    require(not any(p.lower() in FORBIDDEN for p in parts)
            and not value.lower().endswith((".key", ".pem", ".p12", ".pfx", ".jks", ".db", ".wal", ".env")), "PATH_FORBIDDEN")
    return parts


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "JSON_DUPLICATE_KEY")
        result[key] = value
    return result


def decode_lock(raw):
    def invalid_constant(_):
        raise SupplyError("JSON_INVALID")
    try:
        value = json.loads(raw, object_pairs_hook=unique_object, parse_constant=invalid_constant)
    except (ValueError, UnicodeError, RecursionError) as error:
        raise SupplyError("JSON_INVALID") from error
    exact_fields(value, ("schemaVersion", "kind", "vendor", "version", "platform", "archive", "sourceArchive", "javaSha256", "files"), "LOCK_INVALID")
    require(type(value["schemaVersion"]) is int and value["schemaVersion"] == 1
            and value["kind"] == "bxdl-runtime-lock" and label(value["vendor"])
            and label(value["version"]) and value["platform"] == PLATFORM
            and type(value["platform"].get("javaMajor")) is int and hex_hash(value["javaSha256"]), "LOCK_INVALID")
    for key in ("archive", "sourceArchive"):
        item = value[key]
        exact_fields(item, ("name", "url", "sha256", "sizeBytes"), "LOCK_INVALID")
        require(label(item["name"]) and item["name"] not in (".", "..")
                and not any(c in item["name"] for c in "/\\") and label(item["url"], 2048)
                and item["url"].startswith("https://") and not any(c.isspace() for c in item["url"])
                and hex_hash(item["sha256"]) and type(item["sizeBytes"]) is int
                and 0 < item["sizeBytes"] <= 2**64 - 1, "LOCK_INVALID")
    require(value["archive"]["sizeBytes"] <= MAX_ARCHIVE_BYTES, "ARCHIVE_LIMIT")
    require(isinstance(value["files"], list) and 0 < len(value["files"]) <= MAX_FILES, "LOCK_INVALID")
    inventory, parents, folded = {}, set(), {}
    total = 0
    for item in value["files"]:
        exact_fields(item, ("path", "sha256", "size", "mode"), "LOCK_INVALID")
        parts = relative_name(item["path"])
        require(item["path"] not in inventory and hex_hash(item["sha256"])
                and type(item["size"]) is int and 0 <= item["size"] <= MAX_FILE_BYTES
                and type(item["mode"]) is int and item["mode"] in (0o644, 0o755), "LOCK_INVALID")
        total += item["size"]
        require(total <= MAX_TOTAL_BYTES, "INVENTORY_LIMIT")
        inventory[item["path"]] = item
        for end in range(1, len(parts)):
            parents.add("/".join(parts[:end]))
    require(not parents.intersection(inventory), "PATH_CONFLICT")
    # A Linux archive may be prepared on a case-insensitive Mac filesystem.
    for name in sorted(parents | set(inventory)):
        require(name.casefold() not in folded, "PATH_ALIAS")
        folded[name.casefold()] = name
    for name in ("bin/java", "release", "legal/java.base/LICENSE"):
        require(name in inventory and inventory[name]["size"] > 0, "LOCK_REQUIRED_FILE")
    require(inventory["bin/java"]["mode"] == 0o755
            and inventory["bin/java"]["sha256"] == value["javaSha256"], "LOCK_JAVA_IDENTITY")
    return value, inventory, parents


def stamp(metadata):
    return (metadata.st_dev, metadata.st_ino, metadata.st_mode, metadata.st_nlink,
            metadata.st_size, metadata.st_mtime_ns, metadata.st_ctime_ns)


class PinnedFile:
    def __init__(self, path, expected_hash, limit, expected_size=None):
        require(hex_hash(expected_hash), "INPUT_HASH_INVALID")
        supplied = Path(path).expanduser().absolute()
        self.path = supplied.parent.resolve(strict=True) / supplied.name
        before = self.path.lstat()
        require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and 0 < before.st_size <= limit, "INPUT_FILE_INVALID")
        self.stream = os.fdopen(os.open(self.path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb")
        try:
            self.initial = stamp(before)
            require(stamp(os.fstat(self.stream.fileno())) == self.initial, "INPUT_CHANGED")
            self.expected_hash = expected_hash
            self.size = before.st_size
            require(expected_size is None or self.size == expected_size, "INPUT_SIZE_MISMATCH")
            self.recheck()
        except BaseException:
            self.stream.close()
            raise

    def recheck(self):
        require(stamp(self.path.lstat()) == self.initial
                and stamp(os.fstat(self.stream.fileno())) == self.initial, "INPUT_CHANGED")
        self.stream.seek(0)
        hasher = hashlib.sha256()
        while chunk := self.stream.read(CHUNK):
            hasher.update(chunk)
        require(hasher.hexdigest() == self.expected_hash, "INPUT_HASH_MISMATCH")
        require(stamp(self.path.lstat()) == self.initial
                and stamp(os.fstat(self.stream.fileno())) == self.initial, "INPUT_CHANGED")
        self.stream.seek(0)

    def close(self):
        self.stream.close()


class PlainMember(tarfile.TarInfo):
    def _proc_member(self, archive):
        # Reject extension headers before tarfile allocates/decodes their payload.
        require(self.type in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE, tarfile.SYMTYPE), "ARCHIVE_ENTRY_TYPE")
        require(0 <= self.size <= MAX_FILE_BYTES, "ARCHIVE_FILE_LIMIT")
        return super()._proc_member(archive)


class LimitedDecoded:
    def __init__(self, stream):
        self.stream, self.count = stream, 0

    def read(self, count):
        require(0 <= count <= CHUNK, "ARCHIVE_READ_LIMIT")
        raw = self.stream.read(count)
        self.count += len(raw)
        require(self.count <= MAX_TAR_BYTES, "ARCHIVE_DECODED_LIMIT")
        return raw


def scan_archive(pinned, sink=None):
    """First pass validates/hash-observes; a second pass may copy regular bytes."""
    pinned.stream.seek(0)
    entries, roots, total = {}, set(), 0
    with gzip.GzipFile(fileobj=pinned.stream, mode="rb") as decoded:
        with tarfile.open(fileobj=LimitedDecoded(decoded), mode="r|", tarinfo=PlainMember) as archive:
            for member in archive:
                require(len(entries) < MAX_ENTRIES and not member.pax_headers, "ARCHIVE_ENTRY_LIMIT")
                name = member.name
                parts = relative_name(name, 255)
                require(name not in entries, "ARCHIVE_DUPLICATE")
                roots.add(parts[0])
                require(len(roots) == 1, "ARCHIVE_ROOT")
                if member.isdir():
                    require(member.size == 0 and member.mode == 0o755, "ARCHIVE_MODE")
                    item = {"kind": "directory"}
                elif member.issym():
                    require(member.size == 0 and member.mode == 0o777 and label(member.linkname)
                            and not any(c in member.linkname for c in "\\:"), "ARCHIVE_SYMLINK")
                    item = {"kind": "symlink", "link": member.linkname}
                else:
                    require(member.mode in (0o444, 0o644, 0o555, 0o755), "ARCHIVE_MODE")
                    total += member.size
                    require(total <= MAX_TOTAL_BYTES, "ARCHIVE_TOTAL_LIMIT")
                    item = {"kind": "file", "size": member.size,
                            "mode": 0o755 if member.mode & 0o111 else 0o644}
                    stream = archive.extractfile(member)
                    require(stream is not None, "ARCHIVE_FILE")
                    hasher, count = hashlib.sha256(), 0
                    with ExitStack() as stack:
                        target = stack.enter_context(sink(name, item)) if sink else None
                        while chunk := stream.read(CHUNK):
                            count += len(chunk)
                            require(count <= member.size, "ARCHIVE_FILE_LIMIT")
                            hasher.update(chunk)
                            if target is not None:
                                target.write(chunk)
                    require(count == member.size, "ARCHIVE_FILE_SIZE")
                    item["sha256"] = hasher.hexdigest()
                entries[name] = item
            # Consume tar padding and gzip trailers too; hidden extra archives,
            # nonzero trailing bytes and corrupt gzip CRC must not be accepted.
            trailing = 0
            while chunk := archive.fileobj.read(CHUNK):
                trailing += len(chunk)
                require(not any(chunk), "ARCHIVE_TRAILING_DATA")
            require(trailing >= 512, "ARCHIVE_END_MARKER")
    require(len(roots) == 1, "ARCHIVE_ROOT")
    root = next(iter(roots))
    require(entries.get(root) == {"kind": "directory"}, "ARCHIVE_ROOT")
    return root, entries


def resolve_graph(root, entries, inventory, parents):
    prefix = root + "/"
    names = {name[len(prefix):]: value for name, value in entries.items() if name != root}
    require(all(name.startswith(prefix) for name in entries if name != root), "ARCHIVE_ROOT")
    require({name for name, value in names.items() if value["kind"] != "directory"} == set(inventory), "INVENTORY_MEMBERSHIP")
    require({name for name, value in names.items() if value["kind"] == "directory"} == parents, "DIRECTORY_MEMBERSHIP")
    resolved = {}
    for original, expected in inventory.items():
        current, visited = original, set()
        while True:
            require(current not in visited and len(visited) <= MAX_FILES, "SYMLINK_LOOP")
            visited.add(current)
            item = names.get(current)
            require(item is not None and item["kind"] != "directory", "SYMLINK_TARGET")
            if item["kind"] == "file":
                require(all(item[key] == expected[key] for key in ("size", "mode", "sha256")), "INVENTORY_MISMATCH")
                resolved[original] = current
                break
            link = item["link"]
            require(not link.startswith("/"), "SYMLINK_ESCAPE")
            parts = current.split("/")[:-1]
            for part in link.split("/"):
                require(bool(part), "SYMLINK_TARGET")
                if part == "..":
                    require(bool(parts), "SYMLINK_ESCAPE")
                    parts.pop()
                elif part != ".":
                    parts.append(part)
            current = "/".join(parts)
            relative_name(current)
    return resolved


class Output:
    def __init__(self, path, parents):
        supplied = Path(path).expanduser().absolute()
        require(supplied.name not in ("", ".", ".."), "OUTPUT_INVALID")
        self.parent = supplied.parent.resolve(strict=True)
        self.name = supplied.name
        self.parent_fd = os.open(self.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        self.root_fd = None
        try:
            os.mkdir(self.name, 0o700, dir_fd=self.parent_fd)
            self.root_fd = os.open(self.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=self.parent_fd)
            os.fchmod(self.root_fd, 0o700)
            self.identity = stamp(os.fstat(self.root_fd))[:2]
            self.check()
            os.mkdir("runtime", 0o700, dir_fd=self.root_fd)
            for name in sorted(parents, key=lambda name: (name.count("/"), name)):
                fd, leaf = self.parent_for("runtime/" + name)
                try:
                    os.mkdir(leaf, 0o700, dir_fd=fd)
                finally:
                    os.close(fd)
        except BaseException:
            self.close()
            raise

    def check(self):
        require(stamp(self.parent.lstat())[:2] == stamp(os.fstat(self.parent_fd))[:2], "OUTPUT_CHANGED")
        metadata = os.stat(self.name, dir_fd=self.parent_fd, follow_symlinks=False)
        require(stat.S_ISDIR(metadata.st_mode) and stat.S_IMODE(metadata.st_mode) == 0o700
                and stamp(metadata)[:2] == self.identity == stamp(os.fstat(self.root_fd))[:2], "OUTPUT_CHANGED")

    def parent_for(self, relative):
        self.check()
        parts = relative.split("/")
        fd = os.dup(self.root_fd)
        try:
            for part in parts[:-1]:
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                os.close(fd)
                fd = child
            return fd, parts[-1]
        except BaseException:
            os.close(fd)
            raise

    def create_file(self, relative, mode):
        parent, leaf = self.parent_for("runtime/" + relative)
        try:
            fd = os.open(leaf, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=parent)
        finally:
            os.close(parent)
        os.fchmod(fd, mode)
        return os.fdopen(fd, "wb")

    def alias_copy(self, source, destination, expected):
        parent, leaf = self.parent_for("runtime/" + source)
        try:
            fd = os.open(leaf, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
        finally:
            os.close(parent)
        with os.fdopen(fd, "rb") as stream, self.create_file(destination, expected["mode"]) as target:
            before = os.fstat(stream.fileno())
            require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1
                    and before.st_size == expected["size"], "OUTPUT_CHANGED")
            hasher, count = hashlib.sha256(), 0
            while chunk := stream.read(CHUNK):
                count += len(chunk)
                require(count <= expected["size"], "OUTPUT_CHANGED")
                hasher.update(chunk)
                target.write(chunk)
            require(count == expected["size"] and hasher.hexdigest() == expected["sha256"]
                    and stamp(os.fstat(stream.fileno())) == stamp(before), "OUTPUT_CHANGED")

    def verify(self, inventory):
        self.check()
        files, directories = set(), set()
        def visit(fd, prefix):
            for name in os.listdir(fd):
                relative = prefix + name
                metadata = os.stat(name, dir_fd=fd, follow_symlinks=False)
                require(len(files) + len(directories) < MAX_ENTRIES, "OUTPUT_CHANGED")
                if stat.S_ISDIR(metadata.st_mode):
                    require(stat.S_IMODE(metadata.st_mode) == 0o700, "OUTPUT_CHANGED")
                    directories.add(relative)
                    child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                    try:
                        require(stamp(os.fstat(child))[:2] == stamp(metadata)[:2], "OUTPUT_CHANGED")
                        visit(child, relative + "/")
                    finally:
                        os.close(child)
                else:
                    require(stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1, "OUTPUT_CHANGED")
                    files.add(relative)
        visit(self.root_fd, "")
        expected_directories = {"runtime"}
        for name in inventory:
            parts = name.split("/")
            expected_directories.update("runtime/" + "/".join(parts[:end]) for end in range(1, len(parts)))
        require(files == {"runtime/" + name for name in inventory}
                and directories == expected_directories, "OUTPUT_CHANGED")
        for relative, expected in inventory.items():
            parent, leaf = self.parent_for("runtime/" + relative)
            try:
                fd = os.open(leaf, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
            finally:
                os.close(parent)
            with os.fdopen(fd, "rb") as stream:
                before = os.fstat(stream.fileno())
                require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1
                        and before.st_size == expected["size"] and stat.S_IMODE(before.st_mode) == expected["mode"], "OUTPUT_CHANGED")
                hasher, count = hashlib.sha256(), 0
                while chunk := stream.read(CHUNK):
                    count += len(chunk)
                    require(count <= expected["size"], "OUTPUT_CHANGED")
                    hasher.update(chunk)
                require(count == expected["size"] and hasher.hexdigest() == expected["sha256"]
                        and stamp(os.fstat(stream.fileno())) == stamp(before), "OUTPUT_CHANGED")
        self.check()

    def close(self):
        if self.root_fd is not None:
            os.close(self.root_fd)
            self.root_fd = None
        os.close(self.parent_fd)


def prepare(archive_path, runtime_lock_path, runtime_lock_sha256, output_path):
    with ExitStack() as stack:
        pinned_lock = PinnedFile(runtime_lock_path, runtime_lock_sha256, MAX_LOCK_BYTES)
        stack.callback(pinned_lock.close)
        lock, inventory, parents = decode_lock(pinned_lock.stream.read(MAX_LOCK_BYTES + 1))
        supply = lock["archive"]
        pinned_archive = PinnedFile(archive_path, supply["sha256"], MAX_ARCHIVE_BYTES, supply["sizeBytes"])
        stack.callback(pinned_archive.close)
        root, entries = scan_archive(pinned_archive)
        resolved = resolve_graph(root, entries, inventory, parents)
        pinned_lock.recheck()
        pinned_archive.recheck()
        output = Output(output_path, parents)
        stack.callback(output.close)
        def sink(name, item):
            relative = name[len(root) + 1:]
            require(relative in inventory and resolved[relative] == relative, "ARCHIVE_CHANGED")
            return output.create_file(relative, item["mode"])
        require(scan_archive(pinned_archive, sink) == (root, entries), "ARCHIVE_CHANGED")
        for destination, source in sorted(resolved.items()):
            if source != destination:
                output.alias_copy(source, destination, inventory[destination])
        output.verify(inventory)
        pinned_lock.recheck()
        pinned_archive.recheck()
        return {"schemaVersion": 1, "scope": "LINUX_RUNTIME_SUPPLY_OFFLINE", "outcome": "SUCCEEDED",
                "platform": PLATFORM, "runtimeLockSha256": runtime_lock_sha256,
                "archiveSha256": supply["sha256"], "archiveSizeBytes": supply["sizeBytes"],
                "archiveRoot": root, "fileCount": len(inventory),
                "fileBytes": sum(item["size"] for item in inventory.values()),
                "symlinksMaterialized": sum(name != source for name, source in resolved.items()),
                "runtimeExecution": "NOT_RUN", "engineAcceptance": "NOT_RUN", "serviceAcceptance": "NOT_RUN"}


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise SupplyError("ARGUMENTS_INVALID")


def main(argv=None):
    try:
        parser = Parser(description=__doc__, allow_abbrev=False)
        for name in ("archive", "runtime-lock", "runtime-lock-sha256", "output"):
            parser.add_argument("--" + name, required=True)
        words = sys.argv[1:] if argv is None else argv
        require(all(sum(word == "--" + name or word.startswith("--" + name + "=") for word in words) <= 1
                    for name in ("archive", "runtime-lock", "runtime-lock-sha256", "output")), "ARGUMENTS_INVALID")
        args = parser.parse_args(words)
        result = prepare(args.archive, args.runtime_lock, args.runtime_lock_sha256, args.output)
        print(json.dumps(result, ensure_ascii=False, sort_keys=True))
        return 0
    except (SupplyError, OSError, tarfile.TarError, EOFError, ValueError) as error:
        code = str(error) if isinstance(error, SupplyError) else "SUPPLY_IO_OR_ARCHIVE_INVALID"
        print(json.dumps({"schemaVersion": 1, "outcome": "FAILED", "reasonCode": code,
                          "message": "Linux runtime preparation failed; existing files were not replaced. A partial new output may remain.",
                          "runtimeExecution": "NOT_RUN", "engineAcceptance": "NOT_RUN"}, sort_keys=True))
        return 3


if __name__ == "__main__":
    sys.exit(main())
