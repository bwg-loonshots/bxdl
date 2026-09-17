use super::{
    fail,
    files::{Binary, Workspace},
};
use crate::error::Result;
use std::{
    io::{ErrorKind, Read},
    os::{fd::OwnedFd, unix::net::UnixStream},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 131_072;
pub struct Output {
    pub exit: i32,
    pub stdout: Vec<u8>,
}

// Socket readers have timeouts: a descendant retaining an output descriptor
// cannot hold a pipe reader/join indefinitely after the owned JVM is reaped.
fn capture(
    mut input: UnixStream,
    done: Arc<AtomicBool>,
    exceeded: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut drain_started = None;
    loop {
        if done.load(Ordering::Acquire) {
            let started = drain_started.get_or_insert_with(Instant::now);
            if started.elapsed() >= Duration::from_millis(100) {
                return Err(std::io::Error::new(
                    ErrorKind::TimedOut,
                    "output has no EOF",
                ));
            }
        }
        match input.read(&mut buffer) {
            Ok(0) => return Ok(bytes),
            Ok(count) => {
                if bytes.len() + count > OUTPUT_LIMIT {
                    exceeded.store(true, Ordering::Release);
                    return Ok(Vec::new());
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if done.load(Ordering::Acquire) {
                    return Err(std::io::Error::new(
                        ErrorKind::TimedOut,
                        "output has no EOF",
                    ));
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

pub fn run(
    java: &Binary,
    jar: &Path,
    workspace: &Workspace,
    command: &str,
    config: Option<&Path>,
    timeout: Duration,
) -> Result<Output> {
    run_observed(java, jar, workspace, command, config, timeout, |_| {})
}

fn run_observed(
    java: &Binary,
    jar: &Path,
    workspace: &Workspace,
    command: &str,
    config: Option<&Path>,
    timeout: Duration,
    spawned: impl FnOnce(u32),
) -> Result<Output> {
    // Only these two commands can reach process creation, even from module code.
    if !matches!(
        (command, config.is_some()),
        ("engine-info", false) | ("preflight", true)
    ) {
        return Err(fail(
            "ENGINE_OPTIONS_INVALID",
            "허용된 cold 명령만 실행할 수 있습니다.",
        ));
    }
    java.recheck()?;
    workspace.recheck()?;
    let streams = || -> std::io::Result<(UnixStream, Stdio)> {
        let (reader, writer) = UnixStream::pair()?;
        reader.set_read_timeout(Some(Duration::from_millis(50)))?;
        Ok((reader, Stdio::from(OwnedFd::from(writer))))
    };
    let (stdout, child_stdout) = streams().map_err(|_| process_error())?;
    let (stderr, child_stderr) = streams().map_err(|_| process_error())?;
    let mut process = Command::new(&java.path);
    process
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("HOME", &workspace.path)
        .env("TMPDIR", &workspace.path)
        .current_dir(&workspace.path)
        .stdin(Stdio::null())
        .stdout(child_stdout)
        .stderr(child_stderr)
        .arg(format!("-Djava.io.tmpdir={}", workspace.path.display()))
        .arg("-jar")
        .arg(jar)
        .arg(command);
    if let Some(config) = config {
        process.arg(format!("--config={}", config.display()));
    }
    let mut child = process.spawn().map_err(|_| {
        fail(
            "ENGINE_START_FAILED",
            "고정된 Java 실행파일로 cold 명령을 시작하지 못했습니다.",
        )
    })?;
    spawned(child.id());
    // Drop Command's copies of stdout/stderr immediately so EOF is meaningful.
    drop(process);
    let done = Arc::new(AtomicBool::new(false));
    let exceeded = Arc::new(AtomicBool::new(false));
    let out_thread = {
        let done = done.clone();
        let exceeded = exceeded.clone();
        thread::spawn(move || capture(stdout, done, exceeded))
    };
    let err_thread = {
        let done = done.clone();
        let exceeded = exceeded.clone();
        thread::spawn(move || capture(stderr, done, exceeded))
    };
    let start = Instant::now();
    let mut failure = None;
    let status = loop {
        if exceeded.load(Ordering::Acquire) {
            failure = Some(fail(
                "ENGINE_OUTPUT_LIMIT",
                "엔진 출력이 허용된 한도를 초과했습니다.",
            ));
            let _ = child.kill();
            break child.wait();
        }
        if start.elapsed() >= timeout {
            failure = Some(fail(
                "ENGINE_TIMEOUT",
                "cold 명령이 시간 내에 끝나지 않아 실행을 종료했습니다. 검사 결과는 불명입니다.",
            ));
            let _ = child.kill();
            break child.wait();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                failure = Some(process_error());
                let _ = child.kill();
                break child.wait();
            }
        }
    };
    done.store(true, Ordering::Release);
    let stdout = out_thread
        .join()
        .map_err(|_| process_error())
        .and_then(|r| r.map_err(|_| process_error()));
    // Read stderr to the same bound, but never return it or interpolate it.
    let stderr = err_thread
        .join()
        .map_err(|_| process_error())
        .and_then(|r| r.map_err(|_| process_error()));
    if let Some(failure) = failure {
        return Err(failure);
    }
    if exceeded.load(Ordering::Acquire) {
        return Err(fail(
            "ENGINE_OUTPUT_LIMIT",
            "엔진 출력이 허용된 한도를 초과했습니다.",
        ));
    }
    let status = status.map_err(|_| process_error())?;
    let stdout = stdout?;
    let _ = stderr?;
    java.recheck()?;
    workspace.recheck()?;
    let exit = status.code().ok_or_else(process_error)?;
    Ok(Output { exit, stdout })
}
fn process_error() -> crate::error::BxdlError {
    fail(
        "ENGINE_PROCESS_FAILED",
        "cold 명령의 종료 또는 출력 수집을 확인하지 못했습니다.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write, os::unix::fs::PermissionsExt, sync::mpsc};

    #[test]
    fn timeout_kills_and_reaps_the_observed_spawn_pid() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let java_path = root.join("java");
        let bytes = b"#!/bin/sh\nwhile :; do :; done\n";
        fs::write(&java_path, bytes).unwrap();
        fs::set_permissions(&java_path, fs::Permissions::from_mode(0o700)).unwrap();
        let java = Binary::open(&java_path, &crate::engine::files::digest(bytes)).unwrap();
        let workspace = Workspace::create(None).unwrap();
        let jar = workspace.write("engine.jar", b"unit-test fixture").unwrap();
        let mut pid = 0;
        let start = Instant::now();
        let result = run_observed(
            &java,
            &jar,
            &workspace,
            "engine-info",
            None,
            Duration::from_millis(100),
            |id| pid = id,
        );
        assert_eq!(result.err().unwrap().code, "ENGINE_TIMEOUT");
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(pid > 0);
        assert!(
            !Command::new("/bin/kill")
                .args(["-0", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    fn active_trickle_without_eof_is_bounded_and_never_successful() {
        let (reader, mut writer) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let exceeded = Arc::new(AtomicBool::new(false));
        let stop_writer = Arc::new(AtomicBool::new(false));
        let (started, ready) = mpsc::channel();
        let stop = stop_writer.clone();
        let producer = thread::spawn(move || {
            writer.write_all(b"first bytes").unwrap();
            started.send(()).unwrap();
            let deadline = Instant::now();
            while !stop.load(Ordering::Acquire) && deadline.elapsed() < Duration::from_secs(2) {
                if writer.write_all(b"x").is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        ready.recv().unwrap(); // The inherited writer is active before exit observation.
        done.store(true, Ordering::Release);
        let start = Instant::now();
        let result = capture(reader, done, exceeded);
        let elapsed = start.elapsed();
        stop_writer.store(true, Ordering::Release);
        producer.join().unwrap();
        assert_eq!(result.unwrap_err().kind(), ErrorKind::TimedOut);
        assert!(elapsed < Duration::from_millis(500));
    }
}
