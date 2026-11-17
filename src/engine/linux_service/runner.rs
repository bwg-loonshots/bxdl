//! Narrow privilege boundary for the development Linux service controller.
//! No shell, caller-supplied arguments or root JVM. setpriv replaces itself,
//! retaining the gate/owned-child PID, and refuses execution if dropping any
//! requested privilege fails. All other root descriptors must be CLOEXEC.
use crate::{
    engine::fail,
    error::{BxdlError, Result},
};
use std::{
    fs::{self, File},
    io::{ErrorKind, Read},
    os::{
        fd::OwnedFd,
        unix::{fs::MetadataExt, net::UnixStream, process::CommandExt},
    },
    path::{Component, Path},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const SETPRIV: &str = "/usr/bin/setpriv";
const LIMIT: usize = 131_072;

pub(super) struct Request<'a> {
    pub java: &'a Path,
    pub jar: &'a Path,
    pub command: &'a str,
    pub config: Option<&'a Path>,
    pub report: Option<&'a Path>,
    pub attempt: Option<&'a str>,
    pub work: &'a Path,
    pub uid: u32,
    pub gid: u32,
    pub timeout: Duration,
    /// A readonly descriptor holding the controller's flock, never a writable
    /// journal descriptor. The engine does not consume stdin.
    pub lock: Option<&'a File>,
}

pub(super) struct Output {
    pub pid: u32,
    pub exit: i32,
    pub stdout: Vec<u8>,
}

fn invalid() -> BxdlError {
    fail(
        "LINUX_SERVICE_EXECUTION_INVALID",
        "Linux 엔진 실행 경로·계정·명령을 확인하세요.",
    )
}
fn unknown() -> BxdlError {
    fail(
        "LINUX_SERVICE_EXECUTION_UNKNOWN",
        "엔진 실행 종료 또는 출력 수집을 확정하지 못했습니다. 시도와 데이터를 보존했습니다.",
    )
}
fn valid_path(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
        && path
            .to_str()
            .is_some_and(|s| s.len() <= 4096 && !s.chars().any(char::is_control))
}
fn valid_attempt(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
fn validate(request: &Request<'_>) -> Result<()> {
    let r = request;
    if r.uid == 0
        || r.gid == 0
        || r.uid == u32::MAX
        || r.gid == u32::MAX
        || r.timeout.is_zero()
        || r.timeout > Duration::from_secs(600)
        || [r.java, r.jar, r.work].iter().any(|p| !valid_path(p))
        || r.config.is_some_and(|p| !valid_path(p))
        || r.report.is_some_and(|p| {
            !valid_path(p)
                || p.parent() != Some(r.work)
                || p.file_name().is_none_or(|n| n != "report.jsonl")
        })
        || !matches!(
            (
                r.command,
                r.config.is_some(),
                r.report.is_some(),
                r.attempt.is_some()
            ),
            ("engine-info", false, false, false)
                | ("preflight", true, false, false)
                | ("init" | "resume-init" | "run", true, true, true)
        )
        || r.attempt.is_some_and(|a| !valid_attempt(a))
    {
        return Err(invalid());
    }
    Ok(())
}

/// Root-owned immutable executable/input paths: no user-writable ancestors,
/// symlinks, special files, hardlinks or set-id files. Actual content pins are
/// verified by the controller immediately before this independent boundary.
fn trusted_file(path: &Path, executable: bool) -> Result<()> {
    if !valid_path(path) {
        return Err(invalid());
    }
    for ancestor in path.ancestors().skip(1) {
        let m = fs::symlink_metadata(ancestor).map_err(|_| invalid())?;
        if !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
            return Err(invalid());
        }
    }
    let m = fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !m.is_file()
        || m.uid() != 0
        || m.nlink() != 1
        || m.mode() & 0o7022 != 0
        || (executable && m.mode() & 0o111 == 0)
    {
        return Err(invalid());
    }
    Ok(())
}
fn prepare(r: &Request<'_>) -> Result<Command> {
    validate(r)?;
    if !cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        return Err(fail(
            "LINUX_SERVICE_PLATFORM_UNSUPPORTED",
            "실제 Linux GNU x86_64 호스트에서 실행하세요.",
        ));
    }
    if rustix::process::getuid().as_raw() != 0 || rustix::process::geteuid().as_raw() != 0 {
        return Err(fail(
            "LINUX_SERVICE_ROOT_REQUIRED",
            "시스템 서비스 제어는 관리자 권한이 필요합니다.",
        ));
    }
    trusted_file(Path::new(SETPRIV), true)?;
    trusted_file(r.java, true)?;
    trusted_file(r.jar, false)?;
    if let Some(config) = r.config {
        trusted_file(config, false)?;
    }
    for ancestor in r.work.ancestors().skip(1) {
        let m = fs::symlink_metadata(ancestor).map_err(|_| invalid())?;
        if !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
            return Err(invalid());
        }
    }
    let m = fs::symlink_metadata(r.work).map_err(|_| invalid())?;
    if !m.is_dir() || m.uid() != r.uid || m.gid() != r.gid || m.mode() & 0o7777 != 0o700 {
        return Err(invalid());
    }
    if let Some(report) = r.report {
        if fs::symlink_metadata(report).is_ok() {
            return Err(invalid());
        }
    }
    let mut command = command(r)?;
    if let Some(lock) = r.lock {
        let mode = rustix::fs::fcntl_getfl(lock).map_err(|_| invalid())?;
        if mode & rustix::fs::OFlags::ACCMODE != rustix::fs::OFlags::RDONLY {
            return Err(invalid());
        }
        command.stdin(Stdio::from(lock.try_clone().map_err(|_| invalid())?));
    }
    Ok(command)
}
fn command(r: &Request<'_>) -> Result<Command> {
    validate(r)?;
    let mut command = Command::new(SETPRIV);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("HOME", r.work)
        .env("TMPDIR", r.work)
        .current_dir(r.work)
        .stdin(Stdio::null())
        .args([format!("--reuid={}", r.uid), format!("--regid={}", r.gid)])
        .args([
            "--clear-groups",
            "--inh-caps=-all",
            "--ambient-caps=-all",
            "--bounding-set=-all",
            "--no-new-privs",
            "--",
        ])
        .arg(r.java)
        .arg(format!("-Djava.io.tmpdir={}", r.work.display()))
        .arg("-jar")
        .arg(r.jar)
        .arg(r.command);
    if let Some(config) = r.config {
        command.arg(format!("--config={}", config.display()));
    }
    if let Some(report) = r.report {
        command.arg(format!("--report={}", report.display()));
    }
    if let Some(attempt) = r.attempt {
        command.arg(format!("--attempt-id={attempt}"));
    }
    Ok(command)
}

fn capture(
    mut stream: UnixStream,
    done: Arc<AtomicBool>,
    overflow: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut raw = Vec::new();
    let mut buffer = [0; 8192];
    let mut draining = None;
    loop {
        if done.load(Ordering::Acquire)
            && draining.get_or_insert_with(Instant::now).elapsed() >= Duration::from_millis(150)
        {
            return Err(ErrorKind::TimedOut.into());
        }
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(raw),
            Ok(n) if raw.len() + n <= LIMIT && !overflow.load(Ordering::Acquire) => {
                raw.extend_from_slice(&buffer[..n])
            }
            Ok(_) => {
                overflow.store(true, Ordering::Release);
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if done.load(Ordering::Acquire) {
                    return Err(ErrorKind::TimedOut.into());
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => (),
            Err(e) => return Err(e),
        }
    }
}

pub(super) fn run(request: Request<'_>) -> Result<Output> {
    if request.command == "run" {
        return Err(invalid());
    }
    let mut command = prepare(&request)?;
    let streams = || -> std::io::Result<(UnixStream, Stdio)> {
        let (read, write) = UnixStream::pair()?;
        read.set_read_timeout(Some(Duration::from_millis(50)))?;
        Ok((read, Stdio::from(OwnedFd::from(write))))
    };
    let (stdout, out) = streams().map_err(|_| unknown())?;
    let (stderr, err) = streams().map_err(|_| unknown())?;
    command.stdout(out).stderr(err);
    let mut child = command.spawn().map_err(|_| unknown())?;
    let pid = child.id();
    drop(command);
    let done = Arc::new(AtomicBool::new(false));
    let overflow = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = [stdout, stderr]
        .into_iter()
        .map(|stream| {
            let done = done.clone();
            let overflow = overflow.clone();
            thread::spawn(move || capture(stream, done, overflow))
        })
        .collect();
    let started = Instant::now();
    let mut interrupted = false;
    let mut grace = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Err(_) => {
                interrupted = true;
                break None;
            }
            Ok(None) => (),
        }
        if !interrupted
            && (started.elapsed() >= request.timeout || overflow.load(Ordering::Acquire))
        {
            interrupted = true;
            // The Child remains unreaped: the PID cannot be reused here. Never
            // kill a journal-loaded PID or force-kill a storage mutation.
            if let Some(pid) = rustix::process::Pid::from_raw(pid as i32) {
                let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
            }
            grace = Some(Instant::now());
        }
        if grace.is_some_and(|t| t.elapsed() >= Duration::from_secs(5)) {
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    if status.is_none() {
        thread::spawn(move || {
            let _ = child.wait();
        });
    }
    done.store(true, Ordering::Release);
    let results: Vec<_> = readers
        .into_iter()
        .map(|t| t.join().map_err(|_| unknown())?.map_err(|_| unknown()))
        .collect();
    let mut results = results.into_iter();
    let stdout = results.next().ok_or_else(unknown)??;
    results.next().ok_or_else(unknown)??;
    if interrupted || overflow.load(Ordering::Acquire) {
        return Err(unknown());
    }
    let exit = status.and_then(|s| s.code()).ok_or_else(unknown)?;
    Ok(Output { pid, exit, stdout })
}

pub(super) fn exec(request: Request<'_>) -> Result<()> {
    if request.command != "run" {
        return Err(invalid());
    }
    let mut command = prepare(&request)?;
    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let _error = command.exec();
    Err(unknown())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Request<'static> {
        Request {
            java: Path::new("/opt/bxdl/runtime/bin/java"),
            jar: Path::new("/opt/bxdl/node.jar"),
            command: "run",
            config: Some(Path::new("/var/lib/bxdl/one/config/native.json")),
            report: Some(Path::new("/var/lib/bxdl/one/attempt/output/report.jsonl")),
            attempt: Some("attempt_1"),
            work: Path::new("/var/lib/bxdl/one/attempt/output"),
            uid: 991,
            gid: 991,
            timeout: Duration::from_secs(10),
            lock: None,
        }
    }
    #[test]
    fn command_drops_all_privilege_before_fixed_java_without_shell() {
        let c = command(&fixture()).unwrap();
        assert_eq!(c.get_program(), SETPRIV);
        let args: Vec<_> = c.get_args().map(|x| x.to_str().unwrap()).collect();
        assert_eq!(
            &args[..9],
            &[
                "--reuid=991",
                "--regid=991",
                "--clear-groups",
                "--inh-caps=-all",
                "--ambient-caps=-all",
                "--bounding-set=-all",
                "--no-new-privs",
                "--",
                "/opt/bxdl/runtime/bin/java"
            ]
        );
        assert_eq!(args.last(), Some(&"--attempt-id=attempt_1"));
        let env: Vec<_> = c.get_envs().map(|(k, _)| k.to_str().unwrap()).collect();
        assert_eq!(env, ["HOME", "LANG", "LC_ALL", "TMPDIR"]);
    }
    #[test]
    fn refuses_root_identity_and_argument_injection() {
        let mut r = fixture();
        r.uid = 0;
        assert!(command(&r).is_err());
        r = fixture();
        r.gid = 0;
        assert!(command(&r).is_err());
        r = fixture();
        r.command = "--help";
        assert!(command(&r).is_err());
        r = fixture();
        r.attempt = Some("../two");
        assert!(command(&r).is_err());
        r = fixture();
        r.report = Some(Path::new("/etc/report.jsonl"));
        assert!(command(&r).is_err());
        r = fixture();
        r.java = Path::new("/opt/../java");
        assert!(command(&r).is_err());
        r = fixture();
        r.config = None;
        assert!(command(&r).is_err());
    }
    #[test]
    fn cold_commands_cannot_receive_managed_arguments() {
        let mut r = fixture();
        r.command = "engine-info";
        assert!(command(&r).is_err());
        r.config = None;
        r.report = None;
        r.attempt = None;
        assert!(command(&r).is_ok());
        r.command = "preflight";
        assert!(command(&r).is_err());
        r.config = Some(Path::new("/opt/bxdl/native.json"));
        assert!(command(&r).is_ok());
    }
    #[test]
    fn foreign_host_never_executes_privilege_helper() {
        if !cfg!(all(
            target_os = "linux",
            target_arch = "x86_64",
            target_env = "gnu"
        )) {
            assert_eq!(
                exec(fixture()).unwrap_err().code,
                "LINUX_SERVICE_PLATFORM_UNSUPPORTED"
            );
        }
    }
    #[test]
    fn bounded_capture_rejects_descendant_holding_output_open() {
        let (reader, _writer) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_millis(10)))
            .unwrap();
        let start = Instant::now();
        assert!(
            capture(
                reader,
                Arc::new(AtomicBool::new(true)),
                Arc::new(AtomicBool::new(false))
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
