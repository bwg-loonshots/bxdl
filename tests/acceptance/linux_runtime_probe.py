#!/usr/bin/env python3
"""Explicit native Linux cold identity probe, never a node/service installer.

Run with independently trusted hashes, a clean-source Linux CLI, and a
materialized runtime stage. No downloads, sudo, configuration, init, RocksDB or
service actions are performed. PASS applies only to this cold identity scope.
Root and the invoking UID are trusted; third-UID owners and writable path
ancestors are rejected, except trusted-owner sticky shared ancestors such as /tmp.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import selectors
import signal
import stat
import subprocess
import sys
import time


LIMIT = 256 * 1024
JSON_LIMIT = 4 * 1024 * 1024
SHA = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
SCOPE = "LINUX_RUNTIME_COLD_IDENTITY_ONLY"
# Known management/TLS prerequisites, not a complete NIGO minimum-module claim.
REQUIRED_MODULES = {"java.base", "java.management", "jdk.management", "java.naming", "jdk.crypto.ec"}


class Failure(Exception):
    """A fixed public code, never an input path or child output."""


def require(condition, code):
    if not condition:
        raise Failure(code)


def strict_json(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, "JSON_DUPLICATE_KEY")
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=pairs,
                      parse_constant=lambda _: (_ for _ in ()).throw(Failure("JSON_NONFINITE")))


def trusted_owner(metadata):
    require(metadata.st_uid in (0, os.geteuid()), "INPUT_OWNER_UNTRUSTED")


def directory_metadata(metadata, allow_sticky=False, code="INPUT_DIRECTORY_INVALID"):
    require(stat.S_ISDIR(metadata.st_mode), code)
    trusted_owner(metadata)
    require(not metadata.st_mode & 0o022 or allow_sticky and metadata.st_mode & stat.S_ISVTX, code)


def trusted_ancestors(path):
    require(path.is_absolute(), "ABSOLUTE_INPUT_REQUIRED")
    # Each child on this chain must also have a trusted owner. A sticky shared
    # parent therefore cannot let a third UID replace the selected child.
    for ancestor in path.parents:
        directory_metadata(ancestor.lstat(), allow_sticky=True, code="PATH_ANCESTOR_UNSAFE")


def fingerprint(path):
    trusted_ancestors(path)
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1, "INPUT_NOT_REGULAR")
    trusted_owner(before)
    require(not before.st_mode & 0o022, "INPUT_WRITABLE_BY_OTHERS")
    digest = hashlib.sha256()
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        opened = os.fstat(stream.fileno())
        require((opened.st_dev, opened.st_ino) == (before.st_dev, before.st_ino), "INPUT_CHANGED")
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    after = path.lstat()
    stamp = lambda m: (m.st_dev, m.st_ino, m.st_size, m.st_mode, m.st_uid, m.st_mtime_ns, m.st_ctime_ns, m.st_nlink)
    require(stamp(before) == stamp(after), "INPUT_CHANGED")
    return {"sha256": digest.hexdigest(), "size": after.st_size,
            "mode": stat.S_IMODE(after.st_mode), "identity": stamp(after)}


def pinned(path, digest):
    require(bool(SHA.fullmatch(digest)), "HASH_INVALID")
    value = fingerprint(path)
    require(value["sha256"] == digest, "INPUT_PIN_MISMATCH")
    return value


def read_document(path, digest):
    before = pinned(path, digest)
    require(before["size"] <= JSON_LIMIT, "JSON_INPUT_LIMIT")
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        raw = stream.read(JSON_LIMIT + 1)
    require(len(raw) <= JSON_LIMIT, "JSON_INPUT_LIMIT")
    require(hashlib.sha256(raw).hexdigest() == digest, "INPUT_CHANGED")
    value = strict_json(raw)
    require(isinstance(value, dict), "JSON_OBJECT_REQUIRED")
    return value


def release_values(raw):
    values = {}
    for line in raw.splitlines():
        require(bool(re.fullmatch(r'[A-Z][A-Z0-9_]*="[^"\r\n]*"', line)), "RELEASE_FORMAT_INVALID")
        key, value = line.split("=", 1)
        require(key not in values, "RELEASE_FORMAT_INVALID")
        values[key] = value[1:-1]
    return values


def inventory(home, lock):
    require(lock.get("schemaVersion") == 1 and lock.get("kind") == "bxdl-runtime-lock"
            and lock.get("platform") == {"os": "linux", "arch": "amd64", "javaMajor": 21, "libc": "glibc"},
            "LINUX_RUNTIME_LOCK_REQUIRED")
    trusted_ancestors(home)
    directory_metadata(home.lstat(), code="RUNTIME_HOME_INVALID")
    rows = lock.get("files")
    require(isinstance(rows, list) and 1 <= len(rows) <= 10000, "RUNTIME_INVENTORY_INVALID")
    expected, dirs = {}, set()
    total = 0
    for row in rows:
        require(isinstance(row, dict) and set(row) == {"path", "sha256", "size", "mode"}, "RUNTIME_INVENTORY_INVALID")
        name = row["path"]
        require(isinstance(name, str) and 1 <= len(name.encode()) <= 247, "RUNTIME_PATH_INVALID")
        path = PurePosixPath(name)
        require(not path.is_absolute() and str(path) == name and not any(p in (".", "..") for p in path.parts)
                and not any(ord(c) < 32 or c in "\\:\x7f" for c in name), "RUNTIME_PATH_INVALID")
        require(name not in expected and isinstance(row["sha256"], str) and SHA.fullmatch(row["sha256"])
                and type(row["size"]) is int and 0 <= row["size"] <= 512 * 1024 * 1024
                and type(row["mode"]) is int and row["mode"] in (0o644, 0o755), "RUNTIME_INVENTORY_INVALID")
        total += row["size"]
        require(total <= 4 * 1024 ** 3, "RUNTIME_INVENTORY_LIMIT")
        expected[name] = row
        dirs.update(str(parent) for parent in path.parents if str(parent) != ".")
    require(not dirs.intersection(expected), "RUNTIME_PATH_CONFLICT")
    require({"bin/java", "release", "legal/java.base/LICENSE"}.issubset(expected), "RUNTIME_REQUIRED_FILE_MISSING")
    observed, observed_dirs = {}, set()
    for base, directory_names, file_names in os.walk(home, followlinks=False):
        for name in directory_names:
            path = Path(base) / name
            m = path.lstat()
            directory_metadata(m, code="RUNTIME_DIRECTORY_INVALID")
            observed_dirs.add(path.relative_to(home).as_posix())
        for name in file_names:
            path = Path(base) / name
            relative = path.relative_to(home).as_posix()
            require(relative in expected, "RUNTIME_EXTRA_FILE")
            value = fingerprint(path)
            require(all(value[key] == expected[relative][key] for key in ("sha256", "size", "mode")), "RUNTIME_FILE_MISMATCH")
            observed[relative] = value
    require(set(observed) == set(expected) and observed_dirs == dirs, "RUNTIME_INVENTORY_MISMATCH")
    require(observed["bin/java"]["sha256"] == lock.get("javaSha256")
            and observed["bin/java"]["mode"] == 0o755, "RUNTIME_JAVA_MISMATCH")
    return observed


def elf64(path):
    with path.open("rb") as stream:
        head = stream.read(20)
    require(len(head) == 20 and head[:7] == b"\x7fELF\x02\x01\x01"
            and int.from_bytes(head[16:18], "little") in (2, 3)
            and head[18:20] == b"\x3e\x00", "LINUX_X64_EXECUTABLE_REQUIRED")


def native_host():
    require(platform.system() == "Linux" and platform.machine().lower() in ("x86_64", "amd64"), "NATIVE_LINUX_X64_REQUIRED")
    kernel = Path("/proc/sys/kernel/osrelease").read_text().strip()
    require("microsoft" not in kernel.lower(), "WSL_DEFERRED")
    require(os.geteuid() != 0, "NONROOT_PROBE_REQUIRED")
    libc, version = platform.libc_ver()
    require(libc == "glibc" and bool(re.fullmatch(r"[0-9]+\.[0-9]+", version)), "GLIBC_REQUIRED")
    data = {}
    for line in Path("/etc/os-release").read_text().splitlines():
        if line.startswith(("ID=", "VERSION_ID=")):
            key, value = line.split("=", 1)
            require(key not in data, "HOST_DISTRIBUTION_INVALID")
            if len(value) >= 2 and value[0] in "\"'" and value[-1] == value[0]:
                value = value[1:-1]
            require(bool(re.fullmatch(r"[a-z0-9_.-]+", value)), "HOST_DISTRIBUTION_INVALID")
            data[key] = value
    require(data == {"ID": "ubuntu", "VERSION_ID": "24.04"}, "UBUNTU_24_04_REQUIRED")
    require(Path("/proc/1/comm").read_text().strip() == "systemd", "SYSTEMD_PID1_REQUIRED")
    require(bool(re.fullmatch(r"[a-zA-Z0-9_.+\-]{1,256}", kernel)), "HOST_KERNEL_INVALID")
    return {"os": "linux", "architecture": "amd64", "distribution": "ubuntu", "versionId": "24.04",
            "kernelRelease": kernel, "libc": libc, "glibcVersion": version, "wsl": False, "pid1Systemd": True}


def write_json(path, value):
    write_bytes(path, (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode())


def write_bytes(path, raw):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(raw)


def group_exists(pid):
    try:
        os.killpg(pid, 0)
        return True
    except ProcessLookupError:
        return False


def run_command(argv, root, label, timeout=40):
    """Bounded capture of an owned cold-query group; never a service group."""
    env = {"LANG": "C", "LC_ALL": "C", "HOME": str(root / "home"), "TMPDIR": str(root / "tmp"), "PATH": "/usr/bin:/bin"}
    proc = subprocess.Popen([str(value) for value in argv], cwd=root / "cwd", env=env,
                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            start_new_session=True)
    output = {"stdout": bytearray(), "stderr": bytearray()}
    selector = selectors.DefaultSelector()
    for name, stream in (("stdout", proc.stdout), ("stderr", proc.stderr)):
        os.set_blocking(stream.fileno(), False)
        selector.register(stream, selectors.EVENT_READ, name)
    failure = None
    deadline = time.monotonic() + timeout
    try:
        while selector.get_map() or proc.poll() is None:
            require(time.monotonic() < deadline, "QUERY_TIMEOUT")
            for key, _ in selector.select(min(0.05, max(0, deadline - time.monotonic()))):
                raw = os.read(key.fd, 8192)
                if not raw:
                    selector.unregister(key.fileobj)
                else:
                    require(len(output[key.data]) + len(raw) <= LIMIT, "QUERY_OUTPUT_LIMIT")
                    output[key.data].extend(raw)
        proc.wait(timeout=1)
        require(not group_exists(proc.pid), "QUERY_DESCENDANTS_REMAIN")
        require(proc.returncode == 0, "QUERY_EXIT_FAILED")
    except (Failure, subprocess.TimeoutExpired) as error:
        failure = error if isinstance(error, Failure) else Failure("QUERY_TIMEOUT")
    finally:
        try:
            if group_exists(proc.pid):
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass  # The owned group can exit between observation and signal.
                except OSError:
                    failure = Failure("QUERY_CLEANUP_INCOMPLETE")
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                failure = Failure("QUERY_CLEANUP_INCOMPLETE")
        finally:
            # Preserve bounded diagnostics even when cleanup cannot be confirmed.
            selector.close()
            proc.stdout.close()
            proc.stderr.close()
            for name, raw in output.items():
                write_bytes(root / "evidence" / (label + "." + name), raw)
    if failure:
        raise failure
    return bytes(output["stdout"]), bytes(output["stderr"])


def envelope(raw, command):
    result = strict_json(raw)
    require(isinstance(result, dict) and result.get("schemaVersion") == 1
            and result.get("command") == command and result.get("outcome") == "SUCCEEDED"
            and isinstance(result.get("data"), dict), "CLI_RESPONSE_INVALID")
    return result["data"]


def properties(raw, lock):
    text = raw.decode("utf-8")
    names = {"java.runtime.version", "java.specification.version", "java.vendor", "os.arch", "os.name"}
    found = {}
    for line in text.splitlines():
        if " = " in line:
            key, value = line.strip().split(" = ", 1)
            if key in names:
                require(key not in found, "JAVA_PROPERTIES_INVALID")
                found[key] = value
    require(found == {"java.runtime.version": lock["version"], "java.specification.version": "21",
                      "java.vendor": "Eclipse Adoptium", "os.arch": "amd64", "os.name": "Linux"}, "JAVA_PROPERTIES_MISMATCH")
    return found


def probe(args):
    host = native_host()  # A foreign executable is never invoked on Mac/WSL.
    require(bool(COMMIT.fullmatch(args.cli_source_commit)), "CLEAN_CLI_SOURCE_REQUIRED")
    for name in ("cli", "runtime_home", "runtime_lock", "engine_lock", "jar"):
        original = getattr(args, name).expanduser().absolute()
        require(not original.is_symlink(), "INPUT_SYMLINK")
        setattr(args, name, original.resolve(strict=True))
    runtime = read_document(args.runtime_lock, args.runtime_lock_sha256)
    engine = read_document(args.engine_lock, args.engine_lock_sha256)
    require(engine.get("schemaVersion") == 1 and engine.get("javaSha256") == runtime.get("javaSha256")
            and isinstance(engine.get("expected"), dict), "ENGINE_RUNTIME_LOCK_MISMATCH")
    before = inventory(args.runtime_home, runtime)
    cli_pin = pinned(args.cli, args.cli_sha256)
    require(cli_pin["mode"] == 0o755, "CLI_EXECUTABLE_MODE_REQUIRED")
    jar_pin = pinned(args.jar, engine["jarSha256"])
    require(jar_pin["size"] == engine["jarSizeBytes"], "JAR_SIZE_MISMATCH")
    elf64(args.cli)
    java = args.runtime_home / "bin/java"
    elf64(java)
    release = release_values((args.runtime_home / "release").read_text())
    # Temurin's release MODULES can describe the build JDK, including compiler
    # tools absent from the supplied full JRE. Do not require set equality.
    build_modules = set(release.get("MODULES", "").split())
    require(REQUIRED_MODULES.issubset(build_modules), "RUNTIME_MODULES_REQUIRED")
    module_version = runtime["version"].split("+", 1)[0]
    require(bool(re.fullmatch(r"21(?:\.[0-9]+){1,3}", module_version)), "RUNTIME_VERSION_INVALID")
    root = args.output.expanduser().absolute()
    require(root.name not in ("", ".", "..") and root.parent.is_dir(), "OUTPUT_PARENT_REQUIRED")
    root = root.parent.resolve(strict=True) / root.name
    trusted_ancestors(root)
    require(not root.exists() and not root.is_symlink(), "OUTPUT_EXISTS")
    require(not root.is_relative_to(args.runtime_home), "OUTPUT_OVERLAPS_RUNTIME")
    root.mkdir(mode=0o700)
    for name in ("home", "tmp", "cwd", "evidence"):
        (root / name).mkdir(mode=0o700)
    result = {"schemaVersion": 1, "scope": SCOPE, "outcome": "FAILED", "host": host,
              "reasonCode": "PROBE_NOT_COMPLETED", "engineNative": "NOT_RUN", "rocksdb": "NOT_RUN",
              "initialization": "NOT_RUN", "serviceAcceptance": "NOT_RUN", "packageAcceptance": "NOT_RUN",
              "serviceMutations": 0, "automaticRetries": 0, "commandsAttempted": [], "commandsCompleted": [],
              "startedAt": datetime.now(timezone.utc).isoformat(),
              "probeSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "cliSourceVerification": "CALLER_PIN_AND_BINARY_REVISION",
              "cliSourceCommit": args.cli_source_commit, "cliSha256": args.cli_sha256,
              "runtimeLockSha256": args.runtime_lock_sha256, "engineLockSha256": args.engine_lock_sha256,
              "jarSha256": engine["jarSha256"], "javaSha256": runtime["javaSha256"],
              "runtimeFileCount": len(before), "inputsUnchanged": False}
    def run(label, argv):
        result["commandsAttempted"].append(label)
        out, err = run_command(argv, root, label)
        result["commandsCompleted"].append(label)
        return out, err
    try:
        out, err = run("cli-version", [args.cli, "version", "--json"])
        require(not err, "CLI_STDERR_NOT_EMPTY")
        version = envelope(out, "version")
        require(version.get("primaryTarget") == "linux-amd64" and version.get("revision") == args.cli_source_commit
                and {"host.check", "engine.inspect", "runtime.stage"}.issubset(version.get("capabilities", [])), "CLI_IDENTITY_MISMATCH")
        out, err = run("host-check", [args.cli, "host", "check", "--json"])
        require(not err, "CLI_STDERR_NOT_EMPTY")
        observed = envelope(out, "host check")
        require(observed.get("reasonCode") == "HOST_BASELINE_OBSERVED" and observed.get("serviceMutations") == 0
                and observed.get("engineAcceptance") == "NOT_RUN", "CLI_HOST_BASELINE_REQUIRED")
        manager = observed.get("manager", {})
        require(isinstance(manager, dict) and type(manager.get("major")) is int and manager["major"] >= 255
                and manager.get("systemState") == "running" and isinstance(manager.get("version"), str)
                and re.fullmatch(r"[0-9][A-Za-z0-9.+~()_ -]{0,127}", manager["version"]), "SYSTEMD_MANAGER_INVALID")
        result["systemd"] = {key: manager[key] for key in ("version", "major", "systemState")}
        java_options = ["-XX:-UsePerfData", "-Djava.io.tmpdir=" + str(root / "tmp"), "-Duser.home=" + str(root / "home")]
        out, err = run("java-properties", [java, *java_options, "-XshowSettings:properties", "-version"])
        require(not out, "JAVA_PROPERTIES_STDOUT_NOT_EMPTY")
        result["runtimeProperties"] = properties(err, runtime)
        out, err = run("java-modules", [java, *java_options, "--list-modules"])
        require(not err, "JAVA_MODULES_STDERR_NOT_EMPTY")
        lines = out.decode("utf-8").splitlines()
        require(all(re.fullmatch(r"[a-zA-Z0-9_.]+@[a-zA-Z0-9_.+\-]+", line) for line in lines), "JAVA_MODULES_INVALID")
        module_entries = [line.split("@", 1) for line in lines]
        modules = [entry[0] for entry in module_entries]
        require(len(modules) == len(set(modules)) and REQUIRED_MODULES.issubset(modules)
                and set(modules).issubset(build_modules), "JAVA_MODULES_MISMATCH")
        require(all(entry[1] == module_version for entry in module_entries), "JAVA_MODULE_VERSION_MISMATCH")
        result["moduleCount"] = len(modules)
        result["observedModules"] = sorted(lines)
        result["releaseBuildModuleCount"] = len(build_modules)
        result["requiredModuleChecks"] = sorted(REQUIRED_MODULES)
        result["moduleCoverage"] = "KNOWN_REQUIREMENTS_ONLY_NOT_COMPLETE_NIGO_MINIMUM"
        out, err = run("engine-inspect", [args.cli, "engine", "inspect", "--java", java, "--jar", args.jar,
                                          "--lock", args.engine_lock, "--allow-development", "--timeout-seconds", "20", "--json"])
        require(not err, "CLI_STDERR_NOT_EMPTY")
        inspected = envelope(out, "engine inspect")
        require(inspected.get("outcome") == "INSPECTED_DEVELOPMENT" and inspected.get("engineExitCode") == 0
                and inspected.get("developmentOnly") is True and inspected.get("identity") == engine["expected"]
                and inspected.get("jarSha256") == engine["jarSha256"]
                and inspected.get("javaSha256") == runtime["javaSha256"], "ENGINE_IDENTITY_MISMATCH")
        result["engineIdentityMatched"] = True
        result["outcome"] = "PASS"
        result["reasonCode"] = "LINUX_COLD_IDENTITY_VERIFIED"
    except Exception as error:
        result["reasonCode"] = str(error) if isinstance(error, Failure) else "PROBE_EXECUTION_FAILED"
    finally:
        try:
            require(inventory(args.runtime_home, runtime) == before and pinned(args.cli, args.cli_sha256) == cli_pin
                    and pinned(args.jar, engine["jarSha256"]) == jar_pin, "INPUTS_CHANGED")
            pinned(args.runtime_lock, args.runtime_lock_sha256)
            pinned(args.engine_lock, args.engine_lock_sha256)
            result["inputsUnchanged"] = True
        except Exception:
            result["outcome"] = "FAILED"
            result["reasonCode"] = "INPUTS_CHANGED"
        result["completedAt"] = datetime.now(timezone.utc).isoformat()
        write_json(root / "result.json", result)
    return result


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise Failure("ARGUMENTS_INVALID")


def main(argv=None):
    parser = Parser(description=__doc__)
    for name in ("cli", "runtime-home", "runtime-lock", "engine-lock", "jar", "output"):
        parser.add_argument("--" + name, required=True, type=Path)
    for name in ("cli-sha256", "cli-source-commit", "runtime-lock-sha256", "engine-lock-sha256"):
        parser.add_argument("--" + name, required=True)
    try:
        result = probe(parser.parse_args(argv))
    except Exception as error:
        result = {"schemaVersion": 1, "scope": SCOPE, "outcome": "FAILED",
                  "reasonCode": str(error) if isinstance(error, Failure) else "PROBE_INPUT_FAILED",
                  "engineNative": "NOT_RUN", "rocksdb": "NOT_RUN", "initialization": "NOT_RUN",
                  "serviceAcceptance": "NOT_RUN", "packageAcceptance": "NOT_RUN", "serviceMutations": 0}
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    return 0 if result["outcome"] == "PASS" else 3


if __name__ == "__main__":
    sys.exit(main())
