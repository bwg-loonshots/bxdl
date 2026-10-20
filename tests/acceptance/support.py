#!/usr/bin/env python3
"""Portable stdlib helpers for private BXDL acceptance harnesses.

Importing this module does not run commands, inspect fixtures, or create files.
Callers own input identities, expected outcomes, lifecycle decisions, and cleanup.
Captured stdout/stderr stays in private evidence; only fixed stage labels are
printed. Register test credentials in Evidence.redactions before invoking tools.
A command timeout preserves partial output and never kills a process. Pass the
expected exit codes explicitly when UNKNOWN/INCOMPLETE is an expected observation;
finish_command returns the original JSON envelope without upgrading its outcome.
"""
import errno
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import shutil
import stat
import subprocess
import termios
import time

MAX_CAPTURE = 8 * 1024 * 1024

class Failure(Exception):
    """Fixed stage code only. Never include untrusted command output."""


def require(condition, code):
    if not condition:
        raise Failure(code)


def document(path):
    return json.loads(path.read_bytes())


def regular(path):
    m = path.lstat()
    require(stat.S_ISREG(m.st_mode) and m.st_nlink == 1, 'INPUT_NOT_REGULAR')
    return m


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def write(path, data, mode=0o600):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, mode)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())


def write_json(path, value):
    write(path, (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode())


def copy_file(source, destination, mode):
    before = regular(source)
    fd = os.open(source, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, 'rb') as incoming:
        opened = os.fstat(incoming.fileno())
        require((opened.st_dev, opened.st_ino) == (before.st_dev, before.st_ino), 'INPUT_CHANGED')
        out_fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, mode)
        os.fchmod(out_fd, mode)  # Payload modes must remain exact despite private umask.
        with os.fdopen(out_fd, 'wb') as outgoing:
            shutil.copyfileobj(incoming, outgoing, 1024 * 1024)
            outgoing.flush()
            os.fsync(outgoing.fileno())
        after = regular(source)
        require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) ==
                (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns), 'INPUT_CHANGED')


def copy_tree(source, destination):
    require(source.is_dir() and not source.is_symlink(), 'INPUT_TREE_INVALID')
    destination.mkdir(mode=0o700)
    for entry in sorted(source.iterdir()):
        require(not entry.is_symlink(), 'INPUT_TREE_SYMLINK')
        target = destination / entry.name
        if entry.is_dir():
            copy_tree(entry, target)
        else:
            mode = 0o755 if regular(entry).st_mode & 0o111 else 0o644
            copy_file(entry, target, mode)


class Evidence:
    def __init__(self, root):
        self.root = root
        self.logs = root / 'evidence'
        self.logs.mkdir(mode=0o700)
        self.redactions = []
        self.children = []
        self.steps = []

    def safe(self, raw):
        for secret in self.redactions:
            if secret:
                raw = raw.replace(secret, b'[REDACTED_TEST_CREDENTIAL]')
        return raw

    def note(self, stage, **values):
        self.steps.append({'stage': stage, **values})
        print(stage, flush=True)

    def capture(self, name, raw):
        write(self.logs / name, self.safe(raw))


class Terminal:
    def __init__(self, cli, arguments, cwd, evidence, label, timeout):
        self.evidence = evidence
        self.label = label
        self.timeout = timeout
        self.pending = bytearray()
        self.total = bytearray()
        self.master, slave = pty.openpty()
        attributes = termios.tcgetattr(slave)
        attributes[3] &= ~termios.ECHO
        termios.tcsetattr(slave, termios.TCSANOW, attributes)
        self.process = subprocess.Popen([str(cli), *map(str, arguments)], cwd=cwd,
                                        stdin=slave, stdout=slave, stderr=slave,
                                        start_new_session=True)
        evidence.children.append({'stage': label, 'pid': self.process.pid})
        os.close(slave)
        self.closed = False

    def pump(self, deadline):
        left = deadline - time.monotonic()
        require(left > 0, 'PTY_PROMPT_TIMEOUT_' + self.label)
        readable, _, _ = select.select([self.master], [], [], min(left, 0.25))
        if not readable:
            require(self.process.poll() is None, 'PTY_EXIT_BEFORE_PROMPT_' + self.label)
            return
        try:
            raw = os.read(self.master, 65536)
        except OSError as error:
            if error.errno == errno.EIO:
                raise Failure('PTY_EOF_BEFORE_PROMPT_' + self.label) from None
            raise
        require(bool(raw), 'PTY_EOF_BEFORE_PROMPT_' + self.label)
        require(len(self.total) + len(raw) <= MAX_CAPTURE, 'PTY_CAPTURE_LIMIT_' + self.label)
        self.pending.extend(raw)
        self.total.extend(raw)

    def expect(self, token):
        needle = token.encode()
        deadline = time.monotonic() + self.timeout
        while needle not in self.pending:
            self.pump(deadline)
        end = self.pending.index(needle) + len(needle)
        observed = bytes(self.pending[:end])
        del self.pending[:end]
        return observed.decode('utf-8', 'replace')

    def send(self, answer):
        raw = (str(answer) + '\n').encode()
        while raw:
            raw = raw[os.write(self.master, raw):]

    def answer(self, label, value):
        self.expect(label)
        self.expect('> ')
        self.send(value)

    def finish(self, expected_reason, expected_exit=0):
        self.expect(expected_reason)
        deadline = time.monotonic() + self.timeout
        while self.process.poll() is None:
            require(time.monotonic() < deadline, 'PTY_EXIT_TIMEOUT_' + self.label)
            readable, _, _ = select.select([self.master], [], [], 0.1)
            if readable:
                try:
                    raw = os.read(self.master, 65536)
                except OSError as error:
                    if error.errno == errno.EIO:
                        break
                    raise
                if raw:
                    require(len(self.total) + len(raw) <= MAX_CAPTURE, 'PTY_CAPTURE_LIMIT_' + self.label)
                    self.total.extend(raw)
        remaining = max(0.01, deadline - time.monotonic())
        try:
            code = self.process.wait(timeout=remaining)
        except subprocess.TimeoutExpired:
            raise Failure('PTY_EXIT_TIMEOUT_' + self.label) from None
        self.close()
        require(code == expected_exit, 'PTY_UNEXPECTED_EXIT_' + self.label)

    def close(self):
        if not self.closed:
            self.closed = True
            self.evidence.capture(self.label + '.pty.txt', bytes(self.total))
            os.close(self.master)


def start_command(cli, arguments, evidence, label):
    process = subprocess.Popen([str(cli), *map(str, arguments), '--json'], cwd=evidence.root,
                               stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    evidence.children.append({'stage': label, 'pid': process.pid})
    return process


def command(cli, arguments, evidence, label, expected_exits=(0,), timeout=600):
    return finish_command(start_command(cli, arguments, evidence, label), evidence, label, expected_exits, timeout)


def finish_command(process, evidence, label, expected_exits=(0,), timeout=600):
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired as error:
        # No kill/terminate, including for the owning CLI: a timed-out mutation
        # may still own its Java child and lifetime lock. Operator inspection next.
        evidence.capture(label + '.partial.stdout', error.output or b'')
        evidence.capture(label + '.partial.stderr', error.stderr or b'')
        raise Failure('COMMAND_TIMEOUT_' + label) from None
    require(len(stdout) <= MAX_CAPTURE and len(stderr) <= MAX_CAPTURE, 'COMMAND_CAPTURE_LIMIT_' + label)
    evidence.capture(label + '.stdout.json', stdout)
    evidence.capture(label + '.stderr.txt', stderr)
    require(process.returncode in expected_exits, 'COMMAND_EXIT_' + label)
    try:
        result = json.loads(stdout)
    except (ValueError, UnicodeError):
        raise Failure('COMMAND_JSON_' + label) from None
    require(result.get('schemaVersion') == 1, 'COMMAND_SCHEMA_' + label)
    return result


def running(result, label):
    data = result.get('data', {})
    require(data.get('serviceState') == 'RUNNING' and data.get('engineState') == 'RUNNING', label + '_NOT_RUNNING')
    require(data.get('operationBusy') is True and data.get('nodeInstanceId') and data.get('attemptId'), label + '_IDENTITY_MISSING')
    require(data.get('runtimeReadiness') in ['READY', 'NOT_READY', 'UNKNOWN'], label + '_READINESS_FAILED')
    require(data.get('globalConsensus') == 'NOT_CHECKED', label + '_UNEXPECTED_QUORUM_CLAIM')
    return data


def stopped(result, label):
    data = result.get('data', {})
    require(result.get('outcome') == 'SUCCEEDED' and data.get('reason') == 'SERVICE_STOPPED_VERIFIED', label + '_NOT_VERIFIED')
    require(data.get('engineState') == 'STOPPED' and data.get('operationBusy') is False, label + '_NOT_STOPPED')
