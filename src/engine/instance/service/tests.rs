use super::*;

struct Fixture {
    _temp: tempfile::TempDir,
    control: Control,
    state: State,
    runtime: Runtime,
    store: Store,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let control = Control::create(&path.join("control")).unwrap();
        let state = State {
            schema_version: 1,
            instance_id: "service-test".into(),
            binding_sha256: "b".repeat(64),
            phase: Phase::Initialized,
            attempt: Some(Attempt {
                id: "initialized-attempt".into(),
                command: "init".into(),
            }),
            genesis_hash: Some(format!("0x{}", "a".repeat(64))),
            reason: "INITIALIZATION_VERIFIED".into(),
        };
        let runtime = Runtime {
            schema_version: 1,
            instance_id: state.instance_id.clone(),
            control_directory: control.path().to_owned(),
            binding_sha256: state.binding_sha256.clone(),
            attempt_id: "run-attempt".into(),
            phase: RuntimePhase::StartPrepared,
            uid: uid().unwrap(),
            pid: None,
            worker_sha256: "a".repeat(64),
            node_sha256: "c".repeat(64),
            chain_sha256: "d".repeat(64),
            endpoint: "127.0.0.1:18880".parse().unwrap(),
            reason: "START_REQUEST_RECORDED".into(),
        };
        let store = Store::create(&control.path().join(SERVICE_JOURNAL)).unwrap();
        Self {
            _temp: temp,
            control,
            state,
            runtime,
            store,
        }
    }
}

#[test]
fn durable_attempt_progress_retains_pid_and_binding() {
    let mut f = Fixture::new();
    f.runtime.save(&mut f.store).unwrap();
    let (_, prepared) = load_runtime(&f.control, &f.state).unwrap().unwrap();
    assert_eq!(prepared.phase, RuntimePhase::StartPrepared);
    assert_eq!(prepared.pid, None);
    f.runtime.phase = RuntimePhase::GateConsumed;
    f.runtime.pid = Some(31234);
    f.runtime.save(&mut f.store).unwrap();
    let (_, consumed) = load_runtime(&f.control, &f.state).unwrap().unwrap();
    assert_eq!(consumed.pid, Some(31234));
    assert_eq!(consumed.phase, RuntimePhase::GateConsumed);
    f.runtime.phase = RuntimePhase::StoppedVerified;
    f.runtime.save(&mut f.store).unwrap();
    assert_eq!(
        load_runtime(&f.control, &f.state).unwrap().unwrap().1.pid,
        Some(31234)
    );
}
#[test]
fn journal_rejects_mismatched_instance_binding_uid_endpoint_and_pid() {
    for change in 0..8 {
        let mut f = Fixture::new();
        match change {
            0 => f.runtime.binding_sha256 = "c".repeat(64),
            1 => f.runtime.instance_id = "other".into(),
            2 => f.runtime.uid += 1,
            3 => f.runtime.endpoint = "192.0.2.1:8080".parse().unwrap(),
            4 => f.runtime.pid = Some(1234),
            5 => f.runtime.phase = RuntimePhase::GateConsumed,
            6 => f.runtime.attempt_id = "../escape".into(),
            _ => f.runtime.worker_sha256 = "invalid".into(),
        }
        f.runtime.save(&mut f.store).unwrap();
        assert!(load_runtime(&f.control, &f.state).is_err(), "{change}");
    }
}
#[test]
fn journal_conflicts_cannot_overwrite_consumed_attempt() {
    let mut f = Fixture::new();
    f.runtime.save(&mut f.store).unwrap();
    let (mut stale, old) = load_runtime(&f.control, &f.state).unwrap().unwrap();
    f.runtime.phase = RuntimePhase::GateConsumed;
    f.runtime.pid = Some(11234);
    f.runtime.save(&mut f.store).unwrap();
    assert!(old.save(&mut stale).is_err());
    assert_eq!(
        load_runtime(&f.control, &f.state).unwrap().unwrap().1.phase,
        RuntimePhase::GateConsumed
    );
}
#[test]
fn inherited_runtime_lock_remains_busy_after_controller_copy_is_dropped() {
    let f = Fixture::new();
    let lock = f.control.lock().unwrap();
    let child_stdin = lock.child_stdin().unwrap();
    drop(lock);
    assert!(busy(&f.control).unwrap());
    drop(child_stdin);
    assert!(!busy(&f.control).unwrap());
}
#[test]
fn missing_or_replaced_snapshot_never_passes_hash_guard() {
    let f = Fixture::new();
    let attempt = f.control.attempt(&f.runtime.attempt_id).unwrap();
    let workspace = Workspace::persistent(&attempt).unwrap();
    let path = workspace
        .write("node.json", b"{\"sensitive\":true}")
        .unwrap();
    assert!(checked_snapshot(&path, &"a".repeat(64), 262_144).is_err());
    assert!(checked_snapshot(&path, &files::digest(b"{\"sensitive\":true}"), 262_144).is_ok());
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink("missing", &path).unwrap();
    assert!(checked_snapshot(&path, &"a".repeat(64), 262_144).is_err());
}

#[test]
fn valid_health_from_another_role_is_not_adopted() {
    let health = runtime_result::HealthReport {
        observed_at_epoch_millis: "1789600000000".into(),
        running: false,
        lifecycle_running: false,
        readiness: Some(runtime_result::Readiness {
            status: "READY".into(),
            reason: "OBSERVER_INITIALIZED".into(),
            role: "OBSERVER".into(),
            startup_mode: "MANUAL_START".into(),
            startup_completed: true,
            sync_status: "FOLLOWING".into(),
        }),
        execution_status: None,
    };
    assert!(
        validate_health_role(
            &health,
            &format!("0x{}:0x{}", "1".repeat(64), "2".repeat(40))
        )
        .is_err()
    );
    assert!(validate_health_role(&health, &format!("0x{}:OBSERVER", "1".repeat(64))).is_ok());
    assert!(validate_health_role(&health, "INSTANT").is_err());
}
#[test]
fn consumed_deadline_never_grants_another_full_wait() {
    assert!(remaining(Instant::now() - Duration::from_millis(1)).is_err());
    let deadline = Instant::now() + Duration::from_millis(100);
    let rest = remaining(deadline).unwrap();
    assert!(rest <= Duration::from_millis(100));
}
