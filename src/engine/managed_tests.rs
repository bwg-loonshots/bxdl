use super::super::{files, instance_fs::Control};
use super::*;
use std::{fs, io::Write, os::unix::fs::PermissionsExt, path::PathBuf};

const CANARY: &str = "PRIVATE_MANAGED_STDERR_CANARY";

struct Fixture {
    _temp: tempfile::TempDir,
    control: Control,
    workspace: Workspace,
    java: Binary,
    jar: PathBuf,
    config: PathBuf,
}

impl Fixture {
    fn new(script: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let control = Control::create(&root.join("control")).unwrap();
        let guard = control.lock().unwrap();
        let workspace = Workspace::persistent(&control.attempt("attempt-1").unwrap()).unwrap();
        drop(guard);
        workspace
            .write("fixture.jar", b"pinned fake fixture, not NIGO")
            .unwrap();
        workspace.write("node.json", b"{}").unwrap();
        workspace
            .write("preserve.marker", b"durable operation input")
            .unwrap();
        let executable = root.join("fake java");
        fs::write(&executable, script).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let java = Binary::open(&executable, &files::digest(script.as_bytes())).unwrap();
        let jar = workspace.path.join("fixture.jar");
        let config = workspace.path.join("node.json");
        Self {
            _temp: temp,
            control,
            workspace,
            java,
            jar,
            config,
        }
    }

    fn request(&self, lock: File, timeout: Duration) -> Request<'_> {
        Request {
            java: &self.java,
            jar: &self.jar,
            workspace: &self.workspace,
            command: "init",
            config: &self.config,
            attempt: "attempt-1",
            lock,
            timeout,
        }
    }

    fn assert_preserved(&self) {
        assert!(self.workspace.path.is_dir());
        assert_eq!(
            fs::read(self.workspace.path.join("preserve.marker")).unwrap(),
            b"durable operation input"
        );
        assert_eq!(
            fs::read(&self.jar).unwrap(),
            b"pinned fake fixture, not NIGO"
        );
        assert_eq!(fs::read(&self.config).unwrap(), b"{}");
    }
}

fn unknown_result(result: Result<Output>) {
    let error = match result {
        Ok(_) => panic!("expected an unknown operation result"),
        Err(error) => error,
    };
    assert_eq!(error.code, "INSTANCE_INITIALIZATION_UNKNOWN");
    assert!(!error.to_string().contains(CANARY));
}

fn wait_until(mut condition: impl FnMut() -> bool, timeout: Duration) {
    let start = Instant::now();
    while !condition() {
        assert!(
            start.elapsed() < timeout,
            "fixture condition did not become true"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn process_exists(pid: &str) -> bool {
    Command::new("/bin/kill")
        .args(["-0", pid])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[test]
fn owned_success_returns_actual_pid_stdout_and_preserves_persistent_workspace() {
    let fixture = Fixture::new(&format!(
        "#!/bin/sh\nprintf '%s\\n' '{CANARY}' >&2\nprintf '{{\"pid\":%s,\"status\":\"INITIALIZED\"}}\\n' \"$$\"\nexit 0\n"
    ));
    let guard = fixture.control.lock().unwrap();
    let output =
        run(fixture.request(guard.child_stdin().unwrap(), Duration::from_secs(3))).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output.exit, 0);
    assert_eq!(value["pid"], output.pid);
    assert_eq!(value["status"], "INITIALIZED");
    assert!(!String::from_utf8_lossy(&output.stdout).contains(CANARY));
    assert!(!process_exists(&output.pid.to_string()));
    fixture.assert_preserved();
    let path = fixture.workspace.path.clone();
    drop(fixture.workspace);
    assert!(path.join("preserve.marker").exists());
}

#[test]
fn nonzero_exit_is_returned_for_the_caller_to_classify() {
    for exit in [64, 74] {
        let fixture = Fixture::new(&format!(
            "#!/bin/sh\nprintf '{{\"status\":\"FAILED\"}}\\n'\nexit {exit}\n"
        ));
        let guard = fixture.control.lock().unwrap();
        let output =
            run(fixture.request(guard.child_stdin().unwrap(), Duration::from_secs(3))).unwrap();
        assert_eq!(output.exit, exit);
        assert_eq!(output.stdout, b"{\"status\":\"FAILED\"}\n");
        assert!(!process_exists(&output.pid.to_string()));
        fixture.assert_preserved();
    }
}

#[test]
fn cancellation_sends_term_without_kill_preserves_work_and_child_inherited_lock() {
    // Start cancellation only after the fixture proves it installed its trap.
    // This exercises the shared timeout/overflow TERM-and-grace branch without
    // racing shell startup against an intentionally short deadline.
    let fixture = Fixture::new(&format!(
        "#!/bin/sh\ntrap 'printf term > term-seen' TERM\nprintf '%s' \"$$\" > ready-pid\nwhile [ ! -f trigger ]; do [ -f release ] && exit 0; /bin/sleep 0.02; done\ni=0\nwhile [ \"$i\" -lt 160 ]; do printf '%s\\n' '{}'; i=$((i+1)); done\nwhile [ ! -f release ]; do /bin/sleep 0.02; done\nexit 0\n",
        "x".repeat(1024)
    ));
    struct ReleaseOnDrop(PathBuf);
    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            let _ = fs::write(&self.0, b"release");
        }
    }
    let guard = fixture.control.lock().unwrap();
    let request = fixture.request(guard.child_stdin().unwrap(), Duration::from_secs(30));
    let pid = thread::scope(|scope| {
        // Unwind releases the fixture before scope waits for its worker.
        let release = ReleaseOnDrop(fixture.workspace.path.join("release"));
        let running = scope.spawn(|| run(request));
        let ready = fixture.workspace.path.join("ready-pid");
        wait_until(
            || {
                fs::read_to_string(&ready)
                    .is_ok_and(|raw| raw.parse::<u32>().is_ok_and(|pid| pid > 0))
            },
            Duration::from_secs(10),
        );
        let pid = fs::read_to_string(ready).unwrap();
        assert!(pid.parse::<u32>().unwrap() > 0);
        drop(guard);
        assert_eq!(fixture.control.lock().err().unwrap().code, "INSTANCE_BUSY");
        let start = Instant::now();
        fs::write(fixture.workspace.path.join("trigger"), b"trigger").unwrap();
        unknown_result(running.join().unwrap());
        assert!(start.elapsed() < Duration::from_secs(8));
        assert!(fixture.workspace.path.join("term-seen").exists());
        assert!(
            process_exists(&pid),
            "a TERM-ignoring child must not be killed"
        );
        assert_eq!(fixture.control.lock().err().unwrap().code, "INSTANCE_BUSY");
        fixture.assert_preserved();
        drop(release);
        pid
    });
    wait_until(|| fixture.control.lock().is_ok(), Duration::from_secs(3));
    wait_until(|| !process_exists(&pid), Duration::from_secs(3));
    fixture.assert_preserved();
}

#[test]
fn pure_timeout_is_unknown_bounded_and_preserves_operation_files() {
    let fixture = Fixture::new("#!/bin/sh\nexec /bin/sleep 3\n");
    let guard = fixture.control.lock().unwrap();
    let start = Instant::now();
    unknown_result(run(
        fixture.request(guard.child_stdin().unwrap(), Duration::from_millis(100))
    ));
    assert!(start.elapsed() < Duration::from_secs(7));
    drop(guard);
    wait_until(|| fixture.control.lock().is_ok(), Duration::from_secs(3));
    fixture.assert_preserved();
}

#[test]
fn oversized_stdout_or_stderr_is_unknown_and_keeps_all_operation_files() {
    for redirect in ["", " >&2"] {
        let fixture = Fixture::new(&format!(
            "#!/bin/sh\ni=0\nwhile [ \"$i\" -lt 160 ]; do printf '%s\\n' '{}'{}; i=$((i+1)); done\nexit 0\n",
            "x".repeat(1024),
            redirect
        ));
        let guard = fixture.control.lock().unwrap();
        let start = Instant::now();
        unknown_result(run(
            fixture.request(guard.child_stdin().unwrap(), Duration::from_secs(3))
        ));
        assert!(start.elapsed() < Duration::from_secs(4));
        fixture.assert_preserved();
    }
}

#[test]
fn capture_requires_eof_and_bounds_post_exit_trickling_output() {
    let (reader, mut writer) = UnixStream::pair().unwrap();
    reader
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    writer.write_all(b"{\"partial\":").unwrap();
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
    drop(writer);

    let (reader, mut writer) = UnixStream::pair().unwrap();
    reader
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let writer_stop = stop.clone();
    let writing = thread::spawn(move || {
        while !writer_stop.load(Ordering::Acquire) {
            if writer.write_all(b" ").is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
    });
    let start = Instant::now();
    let result = capture(
        reader,
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicBool::new(false)),
    );
    stop.store(true, Ordering::Release);
    writing.join().unwrap();
    assert!(result.is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn invalid_operation_cannot_spawn_the_pinned_executable() {
    let fixture = Fixture::new("#!/bin/sh\nprintf invoked > invoked\nexit 0\n");
    let guard = fixture.control.lock().unwrap();
    for command in ["run", "preflight", "", "init;run"] {
        let mut request = fixture.request(guard.child_stdin().unwrap(), Duration::from_secs(3));
        request.command = command;
        unknown_result(run(request));
    }
    assert!(!fixture.workspace.path.join("invoked").exists());
    fixture.assert_preserved();
}
