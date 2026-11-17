#!/usr/bin/env python3
"""Native Linux development candidate build; no Java, node or systemd execution.

Consumes an independently pinned build plan and preserves all failed outputs.
Only explicit unsigned development artifacts are emitted. The receipt describes
observed bytes, not source reproducibility or an accepted executor profile.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import selectors
import shutil
import signal
import stat
import subprocess
import sys
import time

sys.dont_write_bytecode = True

import linux_build_contract as contract
from linux_build_contract import Failure, require, fingerprint, load_plan, extract_source, inspect_elf, glibc_requirements

LIMIT = 1024 * 1024
TARGET = 'x86_64-unknown-linux-gnu'
SCOPE = 'LINUX_NATIVE_DEVELOPMENT_CANDIDATE_BUILD_ONLY'
CAPABILITIES = {'linux-setup.development', 'linux-executor.development.v1',
                'linux-service.development.register', 'management.install.linux'}


def native_host():
    require(platform.system() == 'Linux' and platform.machine().lower() in ('x86_64', 'amd64'), 'NATIVE_LINUX_X64_REQUIRED')
    require(os.getuid() == os.geteuid() and os.geteuid() != 0, 'NONROOT_BUILDER_REQUIRED')
    kernel = Path('/proc/sys/kernel/osrelease').read_text().strip()
    require('microsoft' not in kernel.lower(), 'WSL_DEFERRED')
    libc, version = platform.libc_ver()
    require(libc == 'glibc' and re.fullmatch(r'[0-9]+\.[0-9]+', version), 'GLIBC_REQUIRED')
    values = {}
    for line in Path('/etc/os-release').read_text().splitlines():
        if line.startswith(('ID=', 'VERSION_ID=')):
            key, value = line.split('=', 1)
            require(key not in values, 'BUILDER_OS_INVALID')
            values[key] = value.strip('"\'')
    require(values == {'ID': 'ubuntu', 'VERSION_ID': '24.04'}, 'UBUNTU_24_04_BUILDER_REQUIRED')
    require(re.fullmatch(r'[A-Za-z0-9_.+\-]{1,256}', kernel), 'BUILDER_KERNEL_INVALID')
    return {'os': 'linux', 'arch': 'amd64', 'distribution': 'ubuntu', 'version': '24.04',
            'libc': libc, 'glibcVersion': version, 'kernelRelease': kernel, 'wsl': False}


def clean_env():
    # Keep only the locations needed by the explicitly selected rustup toolchain.
    # Ambient wrappers, RUSTFLAGS, compiler/linker options and preload variables
    # are excluded. HOME/CARGO_HOME remain operator-selected dependency caches.
    env = {'LANG': 'C', 'LC_ALL': 'C', 'PATH': '/usr/bin:/bin', 'CARGO_TERM_COLOR': 'never', 'RUSTUP_AUTO_INSTALL': '0',
           'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER': '/usr/bin/cc'}
    for key in ('HOME', 'CARGO_HOME', 'RUSTUP_HOME'):
        if key in os.environ:
            require(Path(os.environ[key]).is_absolute(), 'TOOLCHAIN_LOCATION_INVALID')
            env[key] = os.environ[key]
    cargo_home=Path(env.get('CARGO_HOME', str(Path(env.get('HOME','/nonexistent'))/'.cargo')))
    require(not any((cargo_home/name).exists() or (cargo_home/name).is_symlink() for name in ('config','config.toml')), 'AMBIENT_CARGO_CONFIG_UNSUPPORTED')
    return env


def write_bytes(path, raw):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())


def document(path, value):
    write_bytes(path, (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + '\n').encode())


def run(argv, cwd, env, label, evidence=None, timeout=60):
    """Bounded private capture, with cleanup only of this owned build/query group."""
    proc = subprocess.Popen(list(map(str, argv)), cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    selector = selectors.DefaultSelector()
    buffers = {'stdout': bytearray(), 'stderr': bytearray()}
    for name, stream in (('stdout', proc.stdout), ('stderr', proc.stderr)):
        os.set_blocking(stream.fileno(), False)
        selector.register(stream, selectors.EVENT_READ, name)
    deadline = time.monotonic() + timeout
    try:
        while selector.get_map() or proc.poll() is None:
            require(time.monotonic() < deadline, 'BUILD_COMMAND_TIMEOUT')
            for key, _ in selector.select(0.05):
                chunk = os.read(key.fd, 8192)
                if not chunk:
                    selector.unregister(key.fileobj)
                else:
                    require(len(buffers[key.data]) + len(chunk) <= LIMIT, 'BUILD_COMMAND_OUTPUT_LIMIT')
                    buffers[key.data].extend(chunk)
        require(proc.wait(timeout=1) == 0, 'BUILD_COMMAND_FAILED')
        return bytes(buffers['stdout']), bytes(buffers['stderr'])
    finally:
        try:
            # A reaped child's process group identifier may have been reused.
            # Never signal after wait/poll has established that it has exited.
            if proc.returncode is None:
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except (ProcessLookupError, PermissionError):
                    # macOS may report EPERM while our child is already exiting.
                    # Do not assume termination: bounded wait must establish it.
                    pass
            try:
                proc.wait(timeout=5)
            except (subprocess.TimeoutExpired, OSError) as error:
                raise Failure('BUILD_COMMAND_CLEANUP_FAILED') from error
        except OSError as error:
            raise Failure('BUILD_COMMAND_CLEANUP_FAILED') from error
        finally:
            selector.close()
            proc.stdout.close()
            proc.stderr.close()
            if evidence is not None:
                for name, raw in buffers.items():
                    write_bytes(evidence / (label + '.' + name), bytes(raw))


def strict(raw):
    def pairs(items):
        value = {}
        for key, item in items:
            require(key not in value, 'BUILD_JSON_INVALID')
            value[key] = item
        return value
    require(len(raw) <= LIMIT, 'BUILD_JSON_LIMIT')
    return json.loads(raw, object_pairs_hook=pairs, parse_constant=lambda _: (_ for _ in ()).throw(Failure('BUILD_JSON_INVALID')))


def read_json(path):
    with path.open('rb') as stream:
        return strict(stream.read(LIMIT + 1))


def envelope(raw, command):
    value = strict(raw)
    require(isinstance(value, dict) and value.get('schemaVersion') == 1
            and value.get('command') == command and value.get('outcome') == 'SUCCEEDED'
            and isinstance(value.get('data'), dict), 'BUILT_CLI_RESPONSE_INVALID')
    return value['data']


def pin(path):
    value = fingerprint(path)
    return {'sha256': value['sha256'], 'size': value['size']}


def copied(source, target, mode):
    before = pin(source)
    target.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    with source.open('rb') as src:
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, mode)
        with os.fdopen(fd, 'wb') as out:
            shutil.copyfileobj(src, out)
            out.flush()
            os.fsync(out.fileno())
    target.chmod(mode)
    require(pin(source) == before == pin(target), 'BUILD_INPUT_CHANGED')
    return before


def tree(root):
    rows = {}
    for base, dirs, files in os.walk(root, followlinks=False):
        for name in dirs:
            m = (Path(base) / name).lstat()
            require(stat.S_ISDIR(m.st_mode) and not stat.S_ISLNK(m.st_mode), 'STAGE_DIRECTORY_INVALID')
        for name in files:
            p = Path(base) / name
            rows[p.relative_to(root).as_posix()] = dict(pin(p), mode=stat.S_IMODE(p.lstat().st_mode))
    return rows


def versions(value):
    require(isinstance(value, str) and re.fullmatch(r'[0-9]+(?:\.[0-9]+)+', value), 'GLIBC_REQUIREMENT_INVALID')
    parts=tuple(map(int,value.split('.')))
    return parts+(0,)*(max(3,len(parts))-len(parts))


def elf_check(path, declared):
    elf = inspect_elf(path)
    glibc = glibc_requirements(path)
    maximum = glibc.get('maximumRequired')
    require(maximum is None or versions(maximum) <= versions(declared), 'GLIBC_REQUIREMENT_EXCEEDS_DECLARATION')
    return {'elf': elf, 'glibc': glibc}


def toolchain(env):
    cargo, rustc = shutil.which('cargo'), shutil.which('rustc')
    require(cargo and rustc and Path('/usr/bin/cc').is_file(), 'PINNED_TOOLCHAIN_REQUIRED')
    cargo_raw, _ = run([cargo, '+1.86.0', '--version'], '/', env, 'cargo-version')
    rust_raw, _ = run([rustc, '+1.86.0', '-vV'], '/', env, 'rustc-version')
    cargo_text = cargo_raw.decode().strip()
    require(re.fullmatch(r'cargo 1\.86\.0 \([0-9a-f]+ [0-9-]+\)', cargo_text), 'CARGO_VERSION_MISMATCH')
    fields = {}
    for line in rust_raw.decode().splitlines()[1:]:
        if ': ' in line:
            key, value = line.split(': ', 1)
            require(key not in fields, 'RUSTC_VERSION_INVALID')
            fields[key] = value
    require(fields.get('release') == '1.86.0' and fields.get('host') == TARGET
            and re.fullmatch(r'[0-9a-f]{40}', fields.get('commit-hash', '')), 'RUSTC_VERSION_MISMATCH')
    return cargo, rustc, {'rust': '1.86.0', 'cargo': cargo_text, 'rustcHost': fields['host'],
                   'rustcCommit': fields['commit-hash'], 'target': TARGET, 'linker': '/usr/bin/cc'}


def supply_checks(plan, kit):
    runtime = read_json(kit / plan['supply']['runtimeLock']['path'])
    engine = read_json(kit / plan['supply']['engineLock']['path'])
    require(runtime.get('platform') == {'os': 'linux', 'arch': 'amd64', 'javaMajor': 21, 'libc': 'glibc'}, 'LINUX_RUNTIME_REQUIRED')
    require(engine.get('javaSha256') == runtime.get('javaSha256')
            and engine.get('jarSha256') == plan['supply']['jar']['sha256']
            and engine.get('jarSizeBytes') == plan['supply']['jar']['size']
            and runtime.get('archive', {}).get('sha256') == plan['supply']['runtimeArchive']['sha256']
            and runtime['archive'].get('sizeBytes') == plan['supply']['runtimeArchive']['size'], 'SUPPLY_CONTRACT_MISMATCH')
    return runtime, engine


def version(binary, source, commit, env, evidence):
    raw, err = run([binary, 'version', '--json'], source, env, 'cli-version', evidence)
    data = envelope(raw, 'version')
    cargo = (source / 'Cargo.toml').read_text().split('[dependencies]', 1)[0]
    match = re.search(r'^version\s*=\s*"([^"]+)"', cargo, re.MULTILINE)
    require(not err and match and data.get('version') == match.group(1)
            and data.get('revision') == commit and data.get('primaryTarget') == 'linux-amd64'
            and CAPABILITIES.issubset(data.get('capabilities', [])), 'BUILT_CLI_IDENTITY_MISMATCH')
    return data['version']


def artifact_report(raw, command, archive, stage):
    data = envelope(raw, command)
    actual = pin(archive)
    require(data.get('authenticity') == 'unsigned-development'
            and isinstance(data.get('manifest'), dict)
            and re.fullmatch(r'[0-9a-f]{64}', data.get('manifestSha256', '')),
            'ARTIFACT_VERIFICATION_INVALID')
    if 'archiveSha256' in data:
        require(data['archiveSha256'] == actual['sha256'], 'ARTIFACT_ARCHIVE_MISMATCH')
    rows = data['manifest'].get('files')
    require(isinstance(rows, list) and len(rows) == len(stage), 'ARTIFACT_INVENTORY_MISMATCH')
    expected = {p: dict(v, path=p) for p, v in stage.items()}
    require(len({r.get('path') for r in rows}) == len(rows)
            and all(expected.get(row.get('path')) == row for row in rows), 'ARTIFACT_INVENTORY_MISMATCH')
    return dict(actual, manifestSha256=data['manifestSha256'], authenticity=data['authenticity'],
                files=len(rows)), data['manifest']


def build(args):
    host = native_host()  # Must precede every input read, subprocess and mutation.
    require(args.allow_unsigned_development, 'UNSIGNED_DEVELOPMENT_APPROVAL_REQUIRED')
    kit = args.kit.expanduser().absolute()
    plan = load_plan(kit, args.plan_sha256)
    for name, source in [('tools/build-linux-candidate.py', Path(__file__)),
                         ('tools/linux_build_contract.py', Path(contract.__file__))]:
        require(pin(source) == pin(kit / name), 'BUILD_TOOL_PIN_MISMATCH')
    runtime, engine = supply_checks(plan, kit)
    env = clean_env()
    cargo, rustc, compiler = toolchain(env)
    output = args.output.expanduser().absolute()
    require(output.parent.is_dir() and not output.exists() and not output.is_symlink(), 'NEW_OUTPUT_REQUIRED')
    output = output.parent.resolve(strict=True) / output.name
    require(not output.is_relative_to(kit.resolve()) and not kit.resolve().is_relative_to(output), 'OUTPUT_OVERLAPS_KIT')
    # Existing parents are never created/changed. Private output preserves failed
    # work for inspection; no root or node installation path participates.
    output.mkdir(mode=0o700)
    work = output / 'work'
    work.mkdir(mode=0o700)
    evidence = work / 'evidence'
    evidence.mkdir(mode=0o700)
    receipt = {'schemaVersion': 1, 'kind': 'bxdl-linux-native-build-receipt', 'scope': SCOPE,
               'outcome': 'FAILED', 'status':'FAILED', 'reasonCode': 'BUILD_NOT_COMPLETED',
               'planSha256': args.plan_sha256, 'sourceCommit': plan['source']['commit'],
               'sourceArchiveSha256': plan['source']['archive']['sha256'], 'host': host, 'toolchain': compiler,
               'startedAt': datetime.now(timezone.utc).isoformat(), 'developmentOnly': True,
               'trust': 'CALLER_PINNED_PLAN_UNSIGNED_DEVELOPMENT', 'artifacts': {},
               'execution': {'nativeBuild': 'NOT_RUN', 'java': 'NOT_RUN', 'cold': 'NOT_RUN',
                             'nativeRocksdb': 'NOT_RUN', 'initialization': 'NOT_RUN', 'systemd': 'NOT_RUN',
                             'runtimePackageAcceptance': 'NOT_ACCEPTED', 'executorProfile': 'NOT_ADMITTED'},
               'sourceReproducibility': 'NOT_TESTED', 'minimumOsAcceptance': 'NOT_TESTED', 'serviceMutations': 0}
    try:
        source = extract_source(kit / plan['source']['archive']['path'], work / 'source',
                                expected_commit=plan['source']['commit'])
        source = Path(source) if source is not None else work / 'source'
        require(pin(source / 'Cargo.lock')['sha256'] == plan['source']['cargoLockSha256']
                and pin(source / 'rust-toolchain.toml')['sha256'] == plan['source']['rustToolchainSha256'], 'SOURCE_BUILD_INPUT_MISMATCH')
        initial_source = tree(source)
        for ancestor in (source,*source.parents):
            require(not any((ancestor/'.cargo'/n).exists() or (ancestor/'.cargo'/n).is_symlink() for n in ('config','config.toml')), 'SOURCE_CARGO_CONFIG_UNSUPPORTED')
        build_env = dict(env, BXDL_REVISION=plan['source']['commit'], CARGO_TARGET_DIR=str(work / 'target'), RUSTC=rustc, RUSTUP_TOOLCHAIN='1.86.0')
        run([cargo, '+1.86.0', 'build', '--locked', '--release', '--target', TARGET], source,
            build_env, 'cargo-build', evidence, timeout=1800)
        binary = work / 'target' / TARGET / 'release/bxdl'
        fingerprint(binary)  # refuse links/special files before permission normalization
        binary.chmod(0o755)
        worker = pin(binary)
        worker_elf = elf_check(binary, plan['platform']['declaredMinGlibc'])
        product_version = version(binary, source, plan['source']['commit'], env, evidence)
        require(pin(binary) == worker, 'BUILD_INPUT_CHANGED')
        receipt['execution']['nativeBuild'] = 'PASS'
        prepared = work / 'prepared'
        run([sys.executable, '-B', source / 'tools/prepare-linux-runtime.py',
             '--archive', kit / plan['supply']['runtimeArchive']['path'],
             '--runtime-lock', kit / plan['supply']['runtimeLock']['path'],
             '--runtime-lock-sha256', plan['supply']['runtimeLock']['sha256'], '--output', prepared],
            source, env, 'runtime-prepare', evidence, timeout=300)
        stage = work / 'runtime-stage'
        stage.mkdir(mode=0o700)
        copied(binary, stage / 'bin/bxdl', 0o755)
        copied(kit / plan['supply']['jar']['path'], stage / 'engine/nigo-node.jar', 0o644)
        native_elf = {'bin/bxdl': worker_elf}
        for row in runtime['files']:
            src = prepared / 'runtime' / row['path']
            value = pin(src)
            require(value == {'sha256': row['sha256'], 'size': row['size']}, 'RUNTIME_PREPARE_MISMATCH')
            dst = stage / 'runtime' / row['path']
            copied(src, dst, row['mode'])
            with dst.open('rb') as stream:
                if stream.read(4) == b'\x7fELF':
                    native_elf[dst.relative_to(stage).as_posix()] = elf_check(dst, plan['platform']['declaredMinGlibc'])
        # Product-wide license completeness remains a separate release gate.
        copied(source / 'packaging/runtime-notices.temurin-21-linux-amd64.txt', stage / 'licenses/THIRD_PARTY_NOTICES', 0o644)
        copied(source / 'packaging/runtime-bom.temurin-21-linux-amd64.cdx.json', stage / 'licenses/SBOM.json', 0o644)
        before_stage = tree(stage)
        spec = {'schemaVersion': 1, 'kind': 'bxdl-package', 'channel': 'development',
                'product': {'name': 'BXDL', 'version': product_version, 'revision': plan['source']['commit']},
                'platform': {'os': 'linux', 'arch': 'amd64', 'libc': 'glibc', 'minGlibc': plan['platform']['declaredMinGlibc'], 'javaMajor': 21, 'backend': 'rocksdb'},
                'engine': {'revision': engine['expected']['source']['commit'], 'jarSha256': engine['jarSha256'], 'contractStatus': 'proposed', 'contractRevision': engine['expected']['contract']['fingerprint']},
                'runtime': {k: runtime[k] for k in ('vendor', 'version', 'javaSha256')}, 'files': []}
        document(work / 'package-spec.json', spec)
        runtime_archive = output / 'runtime.tar.gz'
        run([binary, 'package', 'build', '--root', stage, '--spec', work / 'package-spec.json', '--output', runtime_archive,
             '--allow-unsigned-development', '--json'], source, env, 'runtime-package-build', evidence, timeout=300)
        raw, err = run([binary, 'package', 'verify', runtime_archive, '--allow-unsigned-development', '--json'], source, env,
                       'runtime-package-verify', evidence, timeout=300)
        require(not err and tree(stage) == before_stage, 'STAGE_CHANGED')
        runtime_artifact, runtime_manifest = artifact_report(raw, 'package verify', runtime_archive, before_stage)
        require(runtime_manifest['product'] == spec['product'] and runtime_manifest['platform'] == spec['platform']
                and runtime_manifest['engine'] == spec['engine'] and runtime_manifest['runtime'] == spec['runtime'], 'RUNTIME_MANIFEST_MISMATCH')
        cli_stage = work / 'cli-stage'
        cli_stage.mkdir(mode=0o700)
        for relative, src in [('bin/bxdl', binary), ('share/zsh/site-functions/_bxdl', source / 'completions/_bxdl'),
                              ('docs/management-cli.md', source / 'docs/management-cli.md'), ('docs/completion.md', source / 'docs/completion.md')]:
            copied(src, cli_stage / relative, 0o755 if relative == 'bin/bxdl' else 0o644)
        before_cli = tree(cli_stage)
        cli_archive = output / 'management-cli.tar.gz'
        run([binary, 'cli', 'bundle', 'build', '--root', cli_stage, '--output', cli_archive, '--version', product_version,
             '--revision', plan['source']['commit'], '--allow-unsigned-development', '--json'], source, env, 'cli-bundle-build', evidence, timeout=120)
        raw, err = run([binary, 'cli', 'bundle', 'verify', cli_archive, '--allow-unsigned-development', '--json'], source, env, 'cli-bundle-verify', evidence, timeout=120)
        require(not err and tree(cli_stage) == before_cli, 'STAGE_CHANGED')
        cli_artifact, cli_manifest = artifact_report(raw, 'cli bundle verify', cli_archive, before_cli)
        require(cli_manifest.get('revision') == plan['source']['commit'] and cli_manifest.get('target') == 'linux-amd64'
                and cli_manifest.get('version') == product_version, 'CLI_MANIFEST_MISMATCH')
        require(tree(source) == initial_source and pin(binary) == worker and load_plan(kit, args.plan_sha256) == plan, 'BUILD_INPUT_CHANGED')
        receipt.update(outcome='PASS', status='NATIVE_BUILT_PACKAGE_PINNED_DEVELOPMENT', reasonCode='LINUX_DEVELOPMENT_CANDIDATE_ASSEMBLED',
                       artifacts={'runtime': dict(runtime_artifact, path=runtime_archive.name), 'management': dict(cli_artifact, path=cli_archive.name)},
                       binaries={'executor': worker, 'management': worker,
                              'jar': {'sha256': engine['jarSha256'], 'size': engine['jarSizeBytes']},
                              'java': next({'sha256': f['sha256'], 'size': f['size']} for f in runtime['files'] if f['path'] == 'bin/java')},
                       initialExecutorAndManagementSameBytes=True, elfObservations=native_elf, runtimeInventoryFiles=len(runtime['files']), inputsUnchanged=True,
                       supply={'runtimeLockSha256': plan['supply']['runtimeLock']['sha256'], 'engineLockSha256': plan['supply']['engineLock']['sha256']},
                       legalCoverage='SELECTED_JRE_ONLY_PRODUCT_LICENSE_SBOM_GATE_OPEN')
    except Exception as error:
        receipt['reasonCode'] = str(error) if isinstance(error, Failure) else 'NATIVE_BUILD_FAILED'
    receipt['completedAt'] = datetime.now(timezone.utc).isoformat()
    document(output / 'receipt.json', receipt)
    return receipt


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kit', required=True, type=Path)
    parser.add_argument('--plan-sha256', required=True)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--allow-unsigned-development', action='store_true')
    try:
        result = build(parser.parse_args(argv))
    except Exception as error:
        result = {'schemaVersion': 1, 'scope': SCOPE, 'outcome': 'FAILED',
                  'reasonCode': str(error) if isinstance(error, Failure) else 'NATIVE_BUILD_INPUT_FAILED',
                  'serviceMutations': 0}
    # Full receipt is a file; stdout never dumps host-private tool output or paths.
    print(json.dumps({k: result[k] for k in ('schemaVersion', 'scope', 'outcome', 'reasonCode', 'serviceMutations')}, sort_keys=True))
    return 0 if result['outcome'] == 'PASS' else 3


if __name__ == '__main__':
    sys.exit(main())
