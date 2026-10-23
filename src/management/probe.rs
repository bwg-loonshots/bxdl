//! Execute only the admitted candidate's side-effect-free version contract.
//! Capture is bounded even if a broken executable retains output descriptors.
use super::{Result, bundle::Manifest, error};
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
const LIMIT: usize = 65536;
fn failed() -> crate::error::BxdlError {
    error(
        "CLI_CANDIDATE_FAILED",
        "새 관리 CLI의 version 응답·버전·기능을 확인하지 못했습니다. 기존 CLI 선택을 유지했습니다.",
    )
}
fn streams() -> std::io::Result<(UnixStream, Stdio)> {
    let (reader, writer) = UnixStream::pair()?;
    reader.set_read_timeout(Some(Duration::from_millis(25)))?;
    Ok((reader, Stdio::from(OwnedFd::from(writer))))
}
fn capture(
    mut reader: UnixStream,
    stop: Arc<AtomicBool>,
    overflow: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    while !stop.load(Ordering::Acquire) {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(bytes),
            Ok(n) => {
                if bytes.len() + n > LIMIT {
                    overflow.store(true, Ordering::Release);
                    return Err(std::io::Error::from(ErrorKind::InvalidData));
                }
                bytes.extend_from_slice(&buffer[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(bytes)
}
pub(super) fn check(path: &Path, manifest: &Manifest) -> Result<()> {
    run(path, manifest, Duration::from_secs(5))
}
fn run(path: &Path, manifest: &Manifest, deadline: Duration) -> Result<()> {
    let (out, child_out) = streams().map_err(|_| failed())?;
    let (err, child_err) = streams().map_err(|_| failed())?;
    let cwd = path.parent().ok_or_else(failed)?;
    let mut command = Command::new(path);
    command
        .args(["version", "--json"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("HOME", cwd)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(child_out)
        .stderr(child_err);
    let mut child = command.spawn().map_err(|_| failed())?;
    drop(command);
    let stop = Arc::new(AtomicBool::new(false));
    let overflow = Arc::new(AtomicBool::new(false));
    let ostop = stop.clone();
    let olimit = overflow.clone();
    let output = thread::spawn(move || capture(out, ostop, olimit));
    let estop = stop.clone();
    let elimit = overflow.clone();
    let errors = thread::spawn(move || capture(err, estop, elimit));
    let start = Instant::now();
    let status = loop {
        if overflow.load(Ordering::Acquire) || start.elapsed() >= deadline {
            let _ = child.kill();
            thread::spawn(move || {
                let _ = child.wait();
            });
            break Err(failed());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.kill();
                thread::spawn(move || {
                    let _ = child.wait();
                });
                break Err(failed());
            }
        }
    };
    if status.is_ok() {
        let drain = Instant::now();
        while !(output.is_finished() && errors.is_finished())
            && drain.elapsed() < Duration::from_millis(150)
        {
            thread::sleep(Duration::from_millis(5));
        }
    }
    let complete = output.is_finished() && errors.is_finished();
    stop.store(true, Ordering::Release);
    let stdout = output.join().map_err(|_| failed())?.map_err(|_| failed())?;
    let stderr = errors.join().map_err(|_| failed())?.map_err(|_| failed())?;
    if !status?.success() || !complete || overflow.load(Ordering::Acquire) || !stderr.is_empty() {
        return Err(failed());
    }
    validate(&stdout, manifest)
}
fn validate(stdout: &[u8], manifest: &Manifest) -> Result<()> {
    let v: serde_json::Value = crate::artifact::decode_strict_json(stdout).map_err(|_| failed())?;
    let data = &v["data"];
    if v["schemaVersion"] != 1
        || v["command"] != "version"
        || v["outcome"] != "SUCCEEDED"
        || v["reasonCode"] != "VERSION"
        || data["product"] != "BXDL"
        || data["implementation"] != "rust"
        || data["version"] != manifest.version
        || data["revision"] != manifest.revision
        || data["primaryTarget"] != manifest.target
    {
        return Err(failed());
    }
    let caps = data["capabilities"].as_array().ok_or_else(failed)?;
    for required in [
        "management-cli.v1",
        "management.install.macos",
        "management.update.macos",
        "management.info",
        "management.rollback.macos",
        "completion.zsh",
        "executor.rc3-legacy-v1",
    ] {
        if !caps.iter().any(|c| c.as_str() == Some(required)) {
            return Err(failed());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn version_bytes() -> Vec<u8> {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(
            crate::cli::run(&["version".into(), "--json".into()], &mut out, &mut err),
            0
        );
        assert!(err.is_empty());
        out
    }
    fn manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            kind: "bxdl-management-cli".into(),
            version: crate::cli::VERSION.into(),
            revision: crate::cli::REVISION.into(),
            target: "darwin-arm64".into(),
            files: vec![],
        }
    }
    fn script(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("candidate");
        std::fs::write(&p, format!("#!/bin/sh\n{text}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
        (t, p)
    }
    #[test]
    fn validates_real_contract_and_rejects_mismatch_or_missing_capability() {
        let bytes = version_bytes();
        validate(&bytes, &manifest()).unwrap();
        let mut m = manifest();
        m.revision = "b".repeat(40);
        assert!(validate(&bytes, &m).is_err());
        let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        v["data"]["capabilities"] = serde_json::json!([]);
        assert!(validate(&serde_json::to_vec(&v).unwrap(), &manifest()).is_err());
        assert!(validate(b"{\"schemaVersion\":1,\"schemaVersion\":1}", &manifest()).is_err());
    }
    #[test]
    fn captures_version_without_stdin_and_rejects_stderr() {
        let bytes = String::from_utf8(version_bytes()).unwrap();
        let (t, p) = script(&format!(
            "read value && exit 3\nprintf '%s' '{}'",
            bytes.replace('\'', "'\\''")
        ));
        run(&p, &manifest(), Duration::from_secs(2)).unwrap();
        drop(t);
        let (_t, p) = script("printf 'private diagnostic' >&2; exit 1");
        assert_eq!(
            run(&p, &manifest(), Duration::from_secs(2))
                .unwrap_err()
                .code,
            "CLI_CANDIDATE_FAILED"
        );
    }
    #[test]
    fn limits_hung_and_unbounded_candidate() {
        for text in [
            "exec /bin/sleep 5",
            "while :; do printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'; done",
        ] {
            let (_t, p) = script(text);
            let start = Instant::now();
            assert!(run(&p, &manifest(), Duration::from_millis(120)).is_err());
            assert!(start.elapsed() < Duration::from_secs(2));
        }
    }
}
