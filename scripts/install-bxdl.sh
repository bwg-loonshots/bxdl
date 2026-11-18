#!/bin/sh
# Save this versioned release asset and verify it before invoking it with sh.
# Python is part of the Ubuntu bootstrap prerequisites. Only its standard library is used.
set -eu
exec python3 -I - "$0" "$@" <<'PY'
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import stat
import subprocess
import sys
import tempfile

INSTALLER = "install-bxdl.sh"
HELPER = "install-linux-helper.sh"
VERIFIER = "bxdl-linux-amd64"
CLI = "bxdl-cli-linux-amd64.tar.gz"
RUNTIME = "bxdl-runtime-linux-amd64.tar.gz"
KEY = "release-public.pem"
METADATA = "release.json"
SUMS = "SHA256SUMS"
SIG = "SHA256SUMS.sig"
MIB = 1024 * 1024
LIMIT = {INSTALLER: MIB, HELPER: MIB, VERIFIER: 128 * MIB, CLI: 256 * MIB,
         RUNTIME: 1024 * MIB, KEY: 16384, METADATA: MIB, SUMS: 16384, SIG: 64}
REPOSITORY = "https://github.com/bwg-loonshots/bxdl/releases/download"


def check(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(MIB), b""):
            digest.update(chunk)
    return digest.hexdigest()


def copy_regular(source, destination, limit):
    with source.open("rb") as stream:
        info = os.fstat(stream.fileno())
        check(stat.S_ISREG(info.st_mode) and 0 < info.st_size <= limit,
              f"파일 형식 또는 크기를 확인하세요: {source.name}")
        with destination.open("xb") as output:
            remaining = info.st_size
            while remaining:
                chunk = stream.read(min(MIB, remaining))
                check(chunk, f"읽는 중 파일이 변경됐습니다: {source.name}")
                output.write(chunk)
                remaining -= len(chunk)
            check(not stream.read(1), f"읽는 중 파일이 변경됐습니다: {source.name}")


def get_asset(args, temp, name):
    path = temp / name
    if name == INSTALLER:
        copy_regular(args.script, path, LIMIT[name])
    elif args.from_dir:
        copy_regular(args.from_dir / name, path, LIMIT[name])
    else:
        subprocess.run(["curl", "--disable", "--fail", "--silent", "--show-error", "--location",
                        "--proto", "=https", "--proto-redir", "=https", "--retry", "2",
                        "--connect-timeout", "15", "--max-time", "1800",
                        "--max-filesize", str(LIMIT[name]), "--output", str(path),
                        f"{REPOSITORY}/{args.version}/{name}"], check=True)
        check(0 < path.stat().st_size <= LIMIT[name], f"다운로드 크기 오류: {name}")
    return path


def verify_release(args, temp):
    sums = get_asset(args, temp, SUMS)
    signature = get_asset(args, temp, SIG)
    trusted = temp / "trusted.pem"
    copy_regular(args.public_key, trusted, LIMIT[KEY])
    subprocess.run(["openssl", "pkeyutl", "-verify", "-pubin", "-inkey", str(trusted),
                    "-rawin", "-in", str(sums), "-sigfile", str(signature)], check=True)
    lines = sums.read_text(encoding="ascii").splitlines()
    check(lines and lines[0] == "# BXDL release " + args.version, "서명된 릴리스 태그가 다릅니다.")
    expected = set(LIMIT) - {SUMS, SIG}
    hashes = {}
    for line in lines[1:]:
        row = re.fullmatch(r"([0-9a-f]{64})  ([-.a-zA-Z0-9]+)", line)
        check(row and row[2] in expected and row[2] not in hashes, "서명한 파일 목록 형식 오류")
        hashes[row[2]] = row[1]
    check(hashes.keys() == expected, "서명한 릴리스 구성 파일이 누락되었습니다.")
    for name, wanted in hashes.items():
        check(sha256(get_asset(args, temp, name)) == wanted, f"SHA-256 불일치: {name}")
    check((temp / KEY).read_bytes() == trusted.read_bytes(), "신뢰 공개키와 릴리스 공개키가 다릅니다.")
    metadata = json.loads((temp / METADATA).read_bytes())
    check(isinstance(metadata, dict) and metadata.get("tag") == args.version
          and metadata.get("workerSha256") == hashes[VERIFIER], "릴리스 태그 또는 실행기 정보 불일치")
    return hashes


def release_store():
    home = Path.home()
    check(home.is_absolute() and home.resolve() == home, "실제 절대 홈 경로가 필요합니다.")
    path = home
    for part in (".local", "share", "BXDL-releases"):
        path = path / part
        path.mkdir(mode=0o700, exist_ok=True)
        info = path.lstat()
        check(stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid()
              and not info.st_mode & 0o022, f"사용자 소유의 보호된 디렉터리가 필요합니다: {path}")
    return path


def install(args, temp, store, hashes):
    cli = temp / VERIFIER
    cli.chmod(0o700)
    trust = ["--public-key", str(temp / "trusted.pem")]
    subprocess.run([cli, "cli", "bundle", "verify", temp / CLI, *trust], check=True)

    release = store / args.version
    release.mkdir(mode=0o700, exist_ok=True)
    info = release.lstat()
    check(stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid()
          and stat.S_IMODE(info.st_mode) == 0o700, "릴리스 디렉터리 소유자·권한을 확인하세요.")
    runtime = release / "runtime-package"
    command = [cli, "install", temp / RUNTIME, "--destination", runtime, *trust]
    if os.path.lexists(runtime):
        command.append("--verify-existing")
    try:
        subprocess.run(command, check=True)
    except subprocess.CalledProcessError:
        if os.path.lexists(runtime):
            print(f"설치 폴더를 보존했습니다. 기존 설치 내용과 권한을 확인하세요: {runtime}",
                  file=sys.stderr)
        raise
    worker = runtime / "bin/bxdl"
    check(sha256(worker) == hashes[VERIFIER], "설치된 실행기와 릴리스 SHA-256 불일치")

    local_helper = release / HELPER
    try:
        os.link(temp / HELPER, local_helper)
    except FileExistsError:
        info = local_helper.lstat()
        check(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
              and not info.st_mode & 0o022 and sha256(local_helper) == hashes[HELPER],
              "보관된 helper 설치 파일이 변경되었습니다.")
    subprocess.run([cli, "cli", "install", "--from", temp / CLI, *trust], check=True)
    helper_command = ["/bin/sh", str(local_helper), str(worker), hashes[VERIFIER]]
    if args.install_helper:
        subprocess.run(helper_command, check=True)
    else:
        print("노드 설치 전에 관리자 실행기를 준비하세요. 이미 같은 실행기가 있으면 재사용합니다.")
        print(shlex.join(helper_command))
    print("네트워크·노드 설정을 준비한 뒤 다음 순서로 진행하세요.")
    entry = str(Path.home() / ".local/bin/bxdl")
    print(shlex.join([entry, "host", "check"]))
    print(shlex.join([entry, "setup", "new", "local", "--runtime", str(runtime / "runtime"),
                      "--jar", str(runtime / "engine/nigo-node.jar")]))
    print(shlex.join([entry, "setup", "apply", "local"]))


def main():
    parser = argparse.ArgumentParser(prog=INSTALLER, description="BXDL Linux x86_64용 서명 릴리스 설치")
    parser.add_argument("script", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--version", required=True, help="GitHub Releases의 정확한 태그")
    parser.add_argument("--public-key", required=True, type=Path, help="별도로 신뢰한 Ed25519 공개키 PEM")
    parser.add_argument("--from-dir", type=Path, help="이미 내려받은 릴리스 파일 디렉터리")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--verify-only", action="store_true", help="다운로드·외부 서명·해시만 검사")
    mode.add_argument("--install-helper", action="store_true", help="검증한 시스템 helper도 sudo로 설치")
    args = parser.parse_args()
    check(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,95}", args.version) and ".." not in args.version
          and not args.version.endswith((".", ".lock")), "허용되지 않는 릴리스 태그")
    os.umask(0o077)
    store = None
    if not args.verify_only:
        check(platform.system() == "Linux" and platform.machine() == "x86_64" and os.getuid() != 0,
              "Ubuntu 24.04 x86_64의 일반 사용자로 실행하세요.")
        # Service readiness is checked when the operator prepares a node. Download
        # and installation of user-owned files also work on a fresh Ubuntu builder.
        with Path("/etc/os-release").open() as source:
            os_info = dict(line.rstrip().split("=", 1) for line in source if "=" in line)
        check(os_info.get("ID", "").strip('"') == "ubuntu"
              and os_info.get("VERSION_ID", "").strip('"') == "24.04"
              and platform.libc_ver()[0] == "glibc"
              and "microsoft" not in platform.release().lower(),
              "지원하는 배포판은 네이티브 Ubuntu 24.04 x86_64/glibc입니다.")
        store = release_store()
    with tempfile.TemporaryDirectory(prefix=".bxdl-download-", dir=store) as work:
        temp = Path(work)
        hashes = verify_release(args, temp)
        print(f"BXDL {args.version} 서명 및 파일 SHA-256 확인 완료", flush=True)
        if not args.verify_only:
            install(args, temp, store, hashes)


try:
    main()
except (OSError, ValueError, subprocess.CalledProcessError) as error:
    sys.exit(f"BXDL 설치를 중단했습니다. {error}")
PY
