//! Bounded, shell-free capture. On supervision failure the request is never
//! retried and the child is allowed to finish: killing an init controller could
//! abandon its still-running JVM. A detached waiter reaps it in long-lived use.
use super::*;
use std::{
    io::{ErrorKind, Read},
    os::{fd::OwnedFd, unix::net::UnixStream},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Instant,
};
const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;
pub(super) struct Output {
    pub exit: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

fn capture(
    mut input: UnixStream,
    done: Arc<AtomicBool>,
    exceeded: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        // Stop even when a child/descendant continuously retains this socket.
        if done.load(Ordering::Acquire) {
            return Ok(data);
        }
        match input.read(&mut buffer) {
            Ok(0) => return Ok(data),
            Ok(n) => {
                if data.len() + n > OUTPUT_LIMIT {
                    exceeded.store(true, Ordering::Release);
                    return Err(std::io::Error::new(ErrorKind::InvalidData, "output limit"));
                }
                data.extend_from_slice(&buffer[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e),
        }
    }
}
fn abandon(mut child: Child) {
    thread::spawn(move || {
        let _ = child.wait();
    });
}
fn streams() -> std::io::Result<(UnixStream, Stdio)> {
    let (reader, writer) = UnixStream::pair()?;
    reader.set_read_timeout(Some(Duration::from_millis(25)))?;
    Ok((reader, Stdio::from(OwnedFd::from(writer))))
}
pub(super) fn run(
    cli: &files::Input,
    cwd: &Path,
    args: &[OsString],
    deadline: Duration,
) -> Result<Output> {
    let (out, child_out) = streams().map_err(|_| spawn_failed())?;
    let (err, child_err) = streams().map_err(|_| spawn_failed())?;
    cli.recheck()?;
    let mut command = Command::new(&cli.path);
    command
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("HOME", cwd)
        .env("TMPDIR", "/private/tmp")
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(child_out)
        .stderr(child_err);
    let mut child = command.spawn().map_err(|_| spawn_failed())?;
    drop(command);
    let done = Arc::new(AtomicBool::new(false));
    let exceeded = Arc::new(AtomicBool::new(false));
    let out_done = done.clone();
    let out_limit = exceeded.clone();
    let out_thread = thread::spawn(move || capture(out, out_done, out_limit));
    let err_done = done.clone();
    let err_limit = exceeded.clone();
    let err_thread = thread::spawn(move || capture(err, err_done, err_limit));
    let start = Instant::now();
    let status = loop {
        if exceeded.load(Ordering::Acquire) {
            abandon(child);
            break Err(error("EXECUTOR_OUTPUT_LIMIT"));
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => (),
            Err(_) => {
                abandon(child);
                break Err(error("EXECUTOR_OBSERVATION_FAILED"));
            }
        }
        if start.elapsed() >= deadline {
            abandon(child);
            break Err(error("EXECUTOR_TIMEOUT"));
        }
        thread::sleep(Duration::from_millis(10));
    };
    // Allow buffered output to drain after normal exit, but never wait for a
    // descendant that inherited stdout/stderr to close it indefinitely.
    if status.is_ok() {
        let drain = Instant::now();
        while !(out_thread.is_finished() && err_thread.is_finished())
            && drain.elapsed() < Duration::from_millis(150)
        {
            thread::sleep(Duration::from_millis(5));
        }
    }
    let complete = out_thread.is_finished() && err_thread.is_finished();
    done.store(true, Ordering::Release);
    let stdout = out_thread
        .join()
        .map_err(|_| error("EXECUTOR_CAPTURE_FAILED"))?
        .map_err(|_| error("EXECUTOR_CAPTURE_FAILED"));
    let stderr = err_thread
        .join()
        .map_err(|_| error("EXECUTOR_CAPTURE_FAILED"))?
        .map_err(|_| error("EXECUTOR_CAPTURE_FAILED"));
    let status = status?;
    if !complete || exceeded.load(Ordering::Acquire) {
        return Err(error("EXECUTOR_CAPTURE_FAILED"));
    }
    let exit = status
        .code()
        .ok_or_else(|| error("EXECUTOR_EXIT_UNKNOWN"))?;
    Ok(Output {
        exit,
        stdout: stdout?,
        stderr: stderr?,
    })
}
fn spawn_failed() -> BxdlError {
    BxdlError::new(
        "EXECUTOR_SPAWN_FAILED",
        "검증한 패키지 실행기를 시작하지 못했습니다.",
    )
}
fn error(code: &str) -> BxdlError {
    BxdlError::new(
        code,
        "패키지 실행기의 종료와 제한된 결과 출력을 확인하지 못했습니다. 실행한 작업은 계속될 수 있습니다.",
    )
}
