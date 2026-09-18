//! One-shot mutations use durable workspaces. No SIGKILL or automatic cleanup.
use super::{
    fail,
    files::{Binary, Workspace},
};
use crate::error::Result;
use std::{
    fs::File,
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

const LIMIT: usize = 131_072;
pub(super) struct Output {
    pub pid: u32,
    pub exit: i32,
    pub stdout: Vec<u8>,
}

fn capture(
    mut input: UnixStream,
    done: Arc<AtomicBool>,
    exceeded: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    let mut draining = None;
    loop {
        if done.load(Ordering::Acquire)
            && draining.get_or_insert_with(Instant::now).elapsed() > Duration::from_millis(150)
        {
            return Err(ErrorKind::TimedOut.into());
        }
        match input.read(&mut buffer) {
            Ok(0) => return Ok(bytes),
            Ok(n) => {
                if bytes.len() + n > LIMIT {
                    exceeded.store(true, Ordering::Release);
                } else if !exceeded.load(Ordering::Acquire) {
                    bytes.extend_from_slice(&buffer[..n]);
                }
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

pub(super) struct Request<'a> {
    pub java: &'a Binary,
    pub jar: &'a Path,
    pub workspace: &'a Workspace,
    pub command: &'a str,
    pub config: &'a Path,
    pub attempt: &'a str,
    pub lock: File,
    pub timeout: Duration,
}
pub(super) fn run(request: Request<'_>) -> Result<Output> {
    let Request {
        java,
        jar,
        workspace,
        command,
        config,
        attempt,
        lock,
        timeout,
    } = request;
    if !matches!(command, "init" | "resume-init")
        || attempt.is_empty()
        || attempt.len() > 80
        || !attempt
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        || timeout.is_zero()
        || timeout > Duration::from_secs(600)
    {
        return Err(unknown());
    }
    java.recheck()?;
    workspace.recheck()?;
    let streams = || -> std::io::Result<(UnixStream, Stdio)> {
        let (read, write) = UnixStream::pair()?;
        read.set_read_timeout(Some(Duration::from_millis(50)))?;
        Ok((read, Stdio::from(OwnedFd::from(write))))
    };
    let (out, out_child) = streams().map_err(|_| unknown())?;
    let (err, err_child) = streams().map_err(|_| unknown())?;
    let mut process = Command::new(&java.path);
    process
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("HOME", &workspace.path)
        .env("TMPDIR", &workspace.path)
        .current_dir(&workspace.path)
        // Inherited duplicate retains the same flock after a controller crash.
        // NIGO managed commands do not consume stdin.
        .stdin(Stdio::from(lock))
        .stdout(out_child)
        .stderr(err_child)
        .arg(format!("-Djava.io.tmpdir={}", workspace.path.display()))
        .arg("-jar")
        .arg(jar)
        .arg(command)
        .arg(format!("--config={}", config.display()))
        .arg(format!(
            "--report={}",
            workspace.path.join("report.jsonl").display()
        ))
        .arg(format!("--attempt-id={attempt}"));
    let mut child = process.spawn().map_err(|_| unknown())?;
    let pid = child.id();
    drop(process);
    let done = Arc::new(AtomicBool::new(false));
    let exceeded = Arc::new(AtomicBool::new(false));
    let out_thread = {
        let done = done.clone();
        let exceeded = exceeded.clone();
        thread::spawn(move || capture(out, done, exceeded))
    };
    let err_thread = {
        let done = done.clone();
        let exceeded = exceeded.clone();
        thread::spawn(move || capture(err, done, exceeded))
    };
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
        if !interrupted && (started.elapsed() >= timeout || exceeded.load(Ordering::Acquire)) {
            interrupted = true;
            // The exact child is still owned and unreaped; never signal a PID
            // loaded from a previous journal. No escalation to SIGKILL.
            let _ = Command::new("/bin/kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .env_clear()
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            grace = Some(Instant::now());
        }
        if grace.is_some_and(|start| start.elapsed() >= Duration::from_secs(5)) {
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    if status.is_none() {
        // Preserve a live mutation and its inherited lock while reaping this
        // exact child when it eventually exits. This must not block a caller
        // whose initialization result is already unknown.
        thread::spawn(move || {
            let _ = child.wait();
        });
    }
    done.store(true, Ordering::Release);
    let stdout = out_thread
        .join()
        .map_err(|_| unknown())?
        .map_err(|_| unknown());
    let stderr = err_thread
        .join()
        .map_err(|_| unknown())?
        .map_err(|_| unknown());
    if interrupted || exceeded.load(Ordering::Acquire) {
        return Err(unknown());
    }
    let exit = status
        .and_then(|status| status.code())
        .ok_or_else(unknown)?;
    let stdout = stdout?;
    stderr?;
    java.recheck()?;
    workspace.recheck()?;
    Ok(Output { pid, exit, stdout })
}
fn unknown() -> crate::error::BxdlError {
    fail(
        "INSTANCE_INITIALIZATION_UNKNOWN",
        "초기화 종료를 확정하지 못했습니다. 작업 기록과 데이터를 보존했습니다. instance show로 확인하세요.",
    )
}

#[cfg(test)]
#[path = "managed_tests.rs"]
mod tests;
