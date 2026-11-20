"""Synthetic native-build orchestration. Never compiles or runs Linux candidates."""
import argparse
import copy
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import importlib.util
import io
import json
import os
import shlex
from pathlib import Path
import shutil
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
SPEC = importlib.util.spec_from_file_location('bxdl_linux_builder', ROOT / 'tools/build-linux-candidate.py')
builder = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(builder)
COMMIT = 'a' * 40
HOST = {'os': 'linux', 'arch': 'amd64', 'distribution': 'ubuntu', 'version': '24.04', 'libc': 'glibc', 'glibcVersion': '2.39'}
COMPILER = {'rust': '1.86.0', 'cargo': 'cargo 1.86.0 (adf9b6ad1 2025-02-28)', 'target': builder.TARGET}


def save(path, value, mode=0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(value if isinstance(value, bytes) else value.encode())
    path.chmod(mode)


def doc(path, value):
    save(path, json.dumps(value))


def elf():
    data=bytearray(64)
    data[:7]=b'\x7fELF\x02\x01\x01'
    data[16:18]=b'\x03\x00'
    data[18:20]=b'\x3e\x00'
    return bytes(data)


def envelope(command, data):
    return json.dumps({'schemaVersion':1,'command':command,'outcome':'SUCCEEDED','data':data}).encode(), b''


class Fixture:
    def __init__(self, root):
        self.root=root
        self.kit=root/'kit'
        self.output=root/'result'
        self.kit.mkdir()
        for name in ('build-linux-candidate.py','linux_build_contract.py'):
            save(self.kit/'tools'/name,(ROOT/'tools'/name).read_bytes())
        save(self.kit/'supply/jre.tar.gz',b'fixture archive not Java')
        save(self.kit/'supply/nigo-node.jar',b'fixture JAR never executed')
        save(self.kit/'source/source.tar.gz',b'fixture source archive')
        self.runtime_files={'bin/java':elf(),'release':b'fixture release','legal/java.base/LICENSE':b'fixture license'}
        self.runtime={'vendor':'Eclipse Adoptium Temurin','version':'21.0.13+9-LTS','platform':{'os':'linux','arch':'amd64','javaMajor':21,'libc':'glibc'},
                      'archive':{'sha256':builder.pin(self.kit/'supply/jre.tar.gz')['sha256'],'sizeBytes':(self.kit/'supply/jre.tar.gz').stat().st_size},
                      'javaSha256':hashlib.sha256(elf()).hexdigest(),
                      'files':[{'path':name,'sha256':hashlib.sha256(raw).hexdigest(),'size':len(raw),'mode':0o755 if name=='bin/java' else 0o644} for name,raw in self.runtime_files.items()]}
        self.engine={'schemaVersion':1,'javaSha256':self.runtime['javaSha256'],'jarSha256':builder.pin(self.kit/'supply/nigo-node.jar')['sha256'],
                     'jarSizeBytes':(self.kit/'supply/nigo-node.jar').stat().st_size,
                     'expected':{'source':{'commit':'b'*40},'contract':{'fingerprint':'c'*64}}}
        doc(self.kit/'packaging/runtime-lock.temurin-21-linux-amd64.json',self.runtime)
        doc(self.kit/'packaging/engine-lock.rc4-temurin-linux-development.json',self.engine)
        self.source_files={'Cargo.toml':b'[package]\nname="bxdl"\nversion="0.1.0-dev"\n[dependencies]\n',
                           'Cargo.lock':b'fixture cargo lock','rust-toolchain.toml':b'fixture Rust1.86 toolchain',
                           'tools/prepare-linux-runtime.py':b'# fake preparer never executed',
                           'packaging/runtime-notices.temurin-21-linux-amd64.txt':b'fixture notices',
                           'packaging/runtime-bom.temurin-21-linux-amd64.cdx.json':b'{"fixture":true}',
                           'completions/_bxdl':b'# fixture completion','docs/management-cli.md':b'fixture management',
                           'docs/completion.md':b'fixture completion'}
        def ref(name): return dict(builder.pin(self.kit/name),path=name)
        self.plan={'source':{'commit':COMMIT,'archive':ref('source/source.tar.gz'),
                             'cargoLockSha256':hashlib.sha256(self.source_files['Cargo.lock']).hexdigest(),
                             'rustToolchainSha256':hashlib.sha256(self.source_files['rust-toolchain.toml']).hexdigest()},
                   'toolchain':{'rust':'1.86.0','target':builder.TARGET},
                   'platform':{'declaredMinGlibc':'2.34'},
                   'supply':{'runtimeArchive':ref('supply/jre.tar.gz'),'jar':ref('supply/nigo-node.jar'),
                             'runtimeLock':ref('packaging/runtime-lock.temurin-21-linux-amd64.json'),
                             'engineLock':ref('packaging/engine-lock.rc4-temurin-linux-development.json')}}
        self.calls=[]
        self.stages={}
        self.unexpected_cargo_link=False
        self.bad_revision=False
        self.change_source=False
        self.change_payload=False

    def args(self):
        return argparse.Namespace(kit=self.kit,plan_sha256='d'*64,output=self.output,allow_unsigned_development=True)

    def extract(self, archive, output, expected_commit=None):
        assert expected_commit == COMMIT
        output.mkdir()
        for name,raw in self.source_files.items():save(output/name,raw)
        return output

    def command(self, argv, cwd, env, label, evidence=None, timeout=60):
        self.calls.append((label,list(map(str,argv)),dict(env)))
        if label=='cargo-build':
            binary = Path(env['CARGO_TARGET_DIR']) / builder.TARGET / 'release/bxdl'
            alias = binary.parent / 'deps/bxdl-794e3e638fd5246d'
            save(alias, elf(), 0o700)
            os.link(alias, binary)
            if self.unexpected_cargo_link:
                os.link(binary, self.root / 'unexpected-extra-link')
            if self.change_source:save(cwd/'Cargo.lock',b'changed source')
            return b'',b''
        if label=='cli-version':
            return envelope('version',{'version':'0.1.0-dev','revision':'e'*40 if self.bad_revision else COMMIT,'primaryTarget':'linux-amd64','capabilities':list(builder.CAPABILITIES)})
        if label=='runtime-prepare':
            output=Path(argv[argv.index('--output')+1])
            for name,raw in self.runtime_files.items():save(output/'runtime'/name,raw,0o755 if name=='bin/java' else 0o644)
            return b'{"outcome":"SUCCEEDED"}',b''
        if label in ('runtime-package-build','cli-bundle-build'):
            stage=Path(argv[argv.index('--root')+1]);output=Path(argv[argv.index('--output')+1])
            rows=[dict(value,path=name) for name,value in builder.tree(stage).items()]
            if label=='runtime-package-build':
                manifest=json.loads(Path(argv[argv.index('--spec')+1]).read_text());manifest['files']=rows
            else:manifest={'schemaVersion':1,'kind':'bxdl-management-cli','version':'0.1.0-dev','revision':COMMIT,'target':'linux-amd64','files':rows}
            save(output,b'fixture archive '+label.encode())
            self.stages[label]=(stage,output,manifest)
            return b'{}',b''
        if label in ('runtime-package-verify','cli-bundle-verify'):
            stage,archive,manifest=self.stages[label.replace('-verify','-build')]
            if self.change_payload:save(stage/'bin/bxdl',elf()+b'changed',0o755)
            command='package verify' if label=='runtime-package-verify' else 'cli bundle verify'
            return envelope(command,{'manifest':manifest,'manifestSha256':'f'*64,'authenticity':'unsigned-development','archiveSha256':builder.pin(archive)['sha256']})
        raise AssertionError('unexpected child '+label)


class NativeBuilderTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='bxdl-native-builder-test-')
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name).resolve()
        self.fixture=Fixture(self.root)

    def build(self, **patches):
        with patch.object(builder,'native_host',return_value=HOST), \
             patch.object(builder,'load_plan',return_value=copy.deepcopy(self.fixture.plan)), \
             patch.object(builder,'toolchain',return_value=('/usr/bin/cargo','/usr/bin/rustc',COMPILER)), \
             patch.object(builder,'clean_env',return_value={'LANG':'C','PATH':'/usr/bin:/bin'}), \
             patch.object(builder,'extract_source',side_effect=self.fixture.extract), \
             patch.object(builder,'elf_check',side_effect=patches.get('elf_check',lambda *a:{'elf':{'machine':'x86_64'},'glibc':{'maximumRequired':'2.34'}})), \
             patch.object(builder,'run',side_effect=self.fixture.command):
            return builder.build(self.fixture.args())

    def test_foreign_host_stops_before_plan_reads_commands_or_output(self):
        with patch.object(builder.platform,'system',return_value='Darwin'), \
             patch.object(builder,'load_plan') as plan,patch.object(builder,'run') as run:
            with self.assertRaisesRegex(builder.Failure,'NATIVE_LINUX_X64_REQUIRED'):builder.build(self.fixture.args())
            plan.assert_not_called();run.assert_not_called()
        self.assertFalse(self.fixture.output.exists())

    def test_wsl_and_root_are_separate_explicit_rejections(self):
        with patch.object(builder.platform,'system',return_value='Linux'),patch.object(builder.platform,'machine',return_value='x86_64'), \
             patch.object(builder.os,'getuid',return_value=0),patch.object(builder.os,'geteuid',return_value=0):
            with self.assertRaisesRegex(builder.Failure,'NONROOT_BUILDER_REQUIRED'):builder.native_host()
        with patch.object(builder.platform,'system',return_value='Linux'),patch.object(builder.platform,'machine',return_value='x86_64'), \
             patch.object(builder.os,'getuid',return_value=1000),patch.object(builder.os,'geteuid',return_value=1000), \
             patch.object(Path,'read_text',return_value='6.6-microsoft-standard-WSL2'):
            with self.assertRaisesRegex(builder.Failure,'WSL_DEFERRED'):builder.native_host()

    def test_group_writable_ancestor_is_identified_only_on_local_stderr(self):
        shared = self.root / "user's shared folder"
        shared.mkdir()
        shared.chmod(0o775)
        args = ['--kit', str(shared / 'kit'), '--plan-sha256', 'a' * 64,
                '--output', str(self.fixture.output), '--allow-unsigned-development']
        stdout, stderr = io.StringIO(), io.StringIO()
        with patch.object(builder, 'native_host', return_value=HOST), redirect_stdout(stdout), redirect_stderr(stderr):
            self.assertEqual(builder.main(args), 3)
        self.assertEqual(json.loads(stdout.getvalue())['reasonCode'], 'PATH_MODE_UNSAFE')
        self.assertNotIn(str(self.root), stdout.getvalue())
        self.assertIn('0775', stderr.getvalue())
        self.assertIn('chmod -- go-w ' + shlex.quote(str(shared)), stderr.getvalue())
        self.assertFalse(self.fixture.output.exists())

    def test_external_plan_and_toolchain_failure_preserve_absent_output(self):
        with patch.object(builder,'native_host',return_value=HOST),patch.object(builder,'load_plan',side_effect=builder.Failure('PLAN_PIN_MISMATCH')),patch.object(builder,'run') as run:
            with self.assertRaisesRegex(builder.Failure,'PLAN_PIN_MISMATCH'):builder.build(self.fixture.args())
            run.assert_not_called()
        self.assertFalse(self.fixture.output.exists())
        with patch.object(builder,'native_host',return_value=HOST),patch.object(builder,'load_plan',return_value=self.fixture.plan), \
             patch.object(builder,'clean_env',return_value={}),patch.object(builder,'toolchain',side_effect=builder.Failure('RUSTC_VERSION_MISMATCH')):
            with self.assertRaisesRegex(builder.Failure,'RUSTC_VERSION_MISMATCH'):builder.build(self.fixture.args())
        self.assertFalse(self.fixture.output.exists())

    def test_unapproved_development_or_changed_orchestrator_never_runs_toolchain(self):
        args=self.fixture.args();args.allow_unsigned_development=False
        with patch.object(builder,'native_host',return_value=HOST),patch.object(builder,'load_plan') as plan:
            with self.assertRaisesRegex(builder.Failure,'UNSIGNED_DEVELOPMENT_APPROVAL_REQUIRED'):builder.build(args)
            plan.assert_not_called()
        save(self.fixture.kit/'tools/build-linux-candidate.py',b'changed orchestration')
        with patch.object(builder,'native_host',return_value=HOST),patch.object(builder,'load_plan',return_value=self.fixture.plan),patch.object(builder,'toolchain') as compiler:
            with self.assertRaisesRegex(builder.Failure,'BUILD_TOOL_PIN_MISMATCH'):builder.build(self.fixture.args())
            compiler.assert_not_called()
        self.assertFalse(self.fixture.output.exists())

    def test_synthetic_build_separates_artifact_roles_from_runtime_acceptance(self):
        value=self.build()
        self.assertEqual(value['outcome'],'PASS',value)
        self.assertEqual(value['status'],'NATIVE_BUILT_PACKAGE_PINNED_DEVELOPMENT')
        self.assertEqual(value['binaries']['executor'],value['binaries']['management'])
        self.assertEqual(value['artifacts']['runtime']['path'],'runtime.tar.gz')
        self.assertEqual(value['artifacts']['management']['path'],'management-cli.tar.gz')
        self.assertEqual(value['execution']['nativeBuild'],'PASS')
        for key in ('java','cold','nativeRocksdb','initialization','systemd'):self.assertEqual(value['execution'][key],'NOT_RUN')
        self.assertEqual(value['execution']['executorProfile'],'NOT_ADMITTED')
        self.assertEqual(value['serviceMutations'],0)
        self.assertEqual(len(self.fixture.calls),7)
        raw=(self.fixture.output/'receipt.json').read_text()
        self.assertNotIn(str(self.root),raw)
        argv=[str(a) for _,args,_ in self.fixture.calls for a in args]
        self.assertNotIn('sudo',argv);self.assertNotIn('systemctl',argv)
        self.assertFalse(any(a.endswith('/bin/java') for a in argv))

    def test_cargo_hardlink_preserves_alias_and_exports_single_link(self):
        value = self.build()
        self.assertEqual(value['outcome'], 'PASS', value)
        work = self.fixture.output / 'work'
        cargo = work / 'cargo-target' / builder.CARGO_ARTIFACT
        staged = work / 'target' / builder.CARGO_ARTIFACT
        self.assertEqual(cargo.stat().st_nlink, 2)
        self.assertEqual(cargo.stat().st_mode & 0o777, 0o700)
        self.assertEqual(staged.stat().st_nlink, 1)
        self.assertEqual(staged.stat().st_mode & 0o777, 0o755)
        self.assertFalse(cargo.samefile(staged))
        self.assertEqual(cargo.read_bytes(), staged.read_bytes())
        self.assertEqual(builder.pin(staged), value['binaries']['executor'])
        with self.assertRaisesRegex(builder.Failure, 'INPUT_FILE_UNSAFE'):
            builder.pin(cargo)

    def test_unexpected_alias_reports_safe_artifact_name_and_condition(self):
        self.fixture.unexpected_cargo_link = True
        receipt = self.build()
        self.assertEqual(receipt['outcome'], 'FAILED')
        self.assertEqual(receipt['reasonCode'], 'CARGO_OUTPUT_LINKS_UNEXPECTED')
        self.assertIn(str(builder.CARGO_ARTIFACT), receipt['message'])
        self.assertIn('release/deps', receipt['message'])
        self.assertNotIn(str(self.root), receipt['message'])
        self.assertEqual([c[0] for c in self.fixture.calls], ['cargo-build'])
        stdout, stderr = io.StringIO(), io.StringIO()
        with patch.object(builder, 'build', return_value=receipt), redirect_stdout(stdout), redirect_stderr(stderr):
            self.assertEqual(builder.main(['--kit', 'kit', '--output', 'output', '--plan-sha256', 'a'*64]), 3)
        self.assertEqual(json.loads(stdout.getvalue())['message'], receipt['message'])
        self.assertIn(receipt['message'], stderr.getvalue())

    def test_cargo_snapshot_checks_type_permissions_and_never_overwrites(self):
        cargo = self.root / 'cargo-target' / builder.CARGO_ARTIFACT
        cargo.parent.mkdir(parents=True)
        cargo.symlink_to(self.root / 'private')
        with self.assertRaisesRegex(builder.Failure, 'CARGO_OUTPUT_NOT_REGULAR'):
            builder.stage_cargo_binary(self.root)
        cargo.unlink()
        save(cargo, b'fixture', 0o777)
        with self.assertRaisesRegex(builder.Failure, 'CARGO_OUTPUT_MODE_UNSAFE'):
            builder.stage_cargo_binary(self.root)
        cargo.chmod(0o700)
        staged, _ = builder.stage_cargo_binary(self.root)
        self.assertEqual(staged.read_bytes(), b'fixture')
        with self.assertRaises(FileExistsError):
            builder.stage_cargo_binary(self.root)

    def test_wrong_revision_and_elf_stop_before_materializing_runtime(self):
        self.fixture.bad_revision=True
        value=self.build()
        self.assertEqual(value['reasonCode'],'BUILT_CLI_IDENTITY_MISMATCH')
        self.assertEqual([c[0] for c in self.fixture.calls],['cargo-build','cli-version'])
        self.fixture.output=self.root/'elf-rejected'
        self.fixture.calls=[]
        def reject(*_):raise builder.Failure('ELF_MACHINE_INVALID')
        value=self.build(elf_check=reject)
        self.assertEqual(value['reasonCode'],'ELF_MACHINE_INVALID')
        self.assertEqual([c[0] for c in self.fixture.calls],['cargo-build'])

    def test_source_and_payload_drift_never_publish_success_receipt(self):
        self.fixture.change_source=True
        value=self.build();self.assertEqual(value['outcome'],'FAILED');self.assertEqual(value['reasonCode'],'BUILD_INPUT_CHANGED')
        self.fixture.change_source=False;self.fixture.change_payload=True;self.fixture.output=self.root/'payload-drift'
        value=self.build();self.assertEqual(value['outcome'],'FAILED');self.assertEqual(value['reasonCode'],'STAGE_CHANGED')

    def test_glibc_requirement_above_declared_profile_is_not_silently_published(self):
        with patch.object(builder,'inspect_elf',return_value={}),patch.object(builder,'glibc_requirements',return_value={'maximumRequired':'2.35'}):
            with self.assertRaisesRegex(builder.Failure,'GLIBC_REQUIREMENT_EXCEEDS_DECLARATION'):builder.elf_check(Path('unused'),'2.34')
        with patch.object(builder,'inspect_elf',return_value={}),patch.object(builder,'glibc_requirements',return_value={'maximumRequired':'2.34.0'}):
            builder.elf_check(Path('unused'),'2.34')

    def test_ambient_compiler_injection_is_not_forwarded(self):
        with patch.dict(os.environ,{'HOME':str(self.root),'RUSTFLAGS':'--cfg evil','RUSTC_WRAPPER':'/private/tool','LD_PRELOAD':'/private/lib','CARGO_BUILD_RUSTC_WRAPPER':'bad'},clear=True):
            env=builder.clean_env()
        for key in ('RUSTFLAGS','RUSTC_WRAPPER','LD_PRELOAD','CARGO_BUILD_RUSTC_WRAPPER'):self.assertNotIn(key,env)
        self.assertEqual(env['CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER'],'/usr/bin/cc')
        save(self.root/'.cargo/config.toml','[build]\nrustc-wrapper="bad"')
        with patch.dict(os.environ,{'HOME':str(self.root)},clear=True):
            with self.assertRaisesRegex(builder.Failure,'AMBIENT_CARGO_CONFIG_UNSUPPORTED'):builder.clean_env()

    def test_completed_child_is_never_signaled_after_reaping(self):
        with patch.object(builder.os, 'killpg') as kill:
            stdout, stderr = builder.run([sys.executable, '-c', 'print(123)'], self.root, {}, 'completed')
            self.assertEqual(stdout, b'123\n')
            self.assertEqual(stderr, b'')
            kill.assert_not_called()
            with self.assertRaisesRegex(builder.Failure, 'BUILD_COMMAND_FAILED'):
                builder.run([sys.executable, '-c', 'raise SystemExit(1)'], self.root, {}, 'failed')
            kill.assert_not_called()

    def test_command_output_limit_is_bounded_and_child_is_reaped(self):
        with patch.object(builder,'LIMIT',128):
            with self.assertRaisesRegex(builder.Failure,'BUILD_COMMAND_OUTPUT_LIMIT'):
                builder.run([sys.executable,'-c','import os; os.write(1,b"x"*4096)'],self.root,{},'bounded',timeout=5)

    def assert_eperm_cleanup(self, terminated):
        process=MagicMock()
        process.returncode=None
        process.pid=4321
        process.stdout.fileno.return_value=100
        process.stderr.fileno.return_value=101
        def wait(timeout):
            if not terminated:
                raise builder.subprocess.TimeoutExpired('private command', timeout)
            process.returncode=0
            return 0
        process.wait.side_effect=wait
        selector=MagicMock()
        selector.get_map.return_value={'pending':True}
        selector.select.return_value=[(SimpleNamespace(fd=100,data='stdout'),None)]
        evidence=self.root/'cleanup-evidence'
        evidence.mkdir(mode=0o700)
        with patch.object(builder.subprocess,'Popen',return_value=process), \
             patch.object(builder.selectors,'DefaultSelector',return_value=selector), \
             patch.object(builder.os,'set_blocking'), \
             patch.object(builder.os,'read',side_effect=[b'x',b'xx']), \
             patch.object(builder.os,'killpg',side_effect=PermissionError('private OS detail')) as kill, \
             patch.object(builder,'LIMIT',1):
            expected='BUILD_COMMAND_OUTPUT_LIMIT' if terminated else 'BUILD_COMMAND_CLEANUP_FAILED'
            with self.assertRaisesRegex(builder.Failure,expected):
                builder.run(['private-child'],self.root,{},'eperm',evidence)
            kill.assert_called_once_with(process.pid,builder.signal.SIGKILL)
        process.wait.assert_called_once_with(timeout=5)
        process.poll.assert_not_called()
        process.stdout.close.assert_called_once()
        process.stderr.close.assert_called_once()
        selector.close.assert_called_once()
        self.assertEqual((evidence/'eperm.stdout').read_bytes(),b'x')
        self.assertEqual((evidence/'eperm.stderr').read_bytes(),b'')
        self.assertEqual((evidence/'eperm.stdout').stat().st_mode & 0o777,0o600)

    def test_exited_child_eperm_preserves_original_failure_and_evidence(self):
        self.assert_eperm_cleanup(terminated=True)

    def test_live_child_eperm_cleanup_timeout_is_reported_and_pipes_close(self):
        self.assert_eperm_cleanup(terminated=False)

    def test_bad_cargo_and_rustc_versions_are_rejected(self):
        with patch.object(builder.shutil,'which',side_effect=lambda n:'/usr/bin/'+n),patch.object(builder,'run',return_value=(b'cargo 1.87.0 (abc 2025-01-01)',b'')):
            with self.assertRaisesRegex(builder.Failure,'CARGO_VERSION_MISMATCH'):builder.toolchain({})
        with patch.object(builder.shutil,'which',side_effect=lambda n:'/usr/bin/'+n),patch.object(builder,'run',side_effect=[
            (b'cargo 1.86.0 (abc 2025-01-01)',b''),
            (b'rustc 1.86.0\nrelease: 1.86.0\nhost: aarch64-unknown-linux-gnu\ncommit-hash: '+b'a'*40+b'\n',b'')]):
            with self.assertRaisesRegex(builder.Failure,'RUSTC_VERSION_MISMATCH'):builder.toolchain({})


if __name__=='__main__':unittest.main()
