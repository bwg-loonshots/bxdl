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
            startup_stage: None,
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

fn observation(engine: &str, readiness: &str, outcome: &str, reason: &str) -> ServiceSummary {
    ServiceSummary {
        instance_id: "service-test".into(),
        outcome: outcome.into(),
        service_state: "RUNNING".into(),
        engine_state: engine.into(),
        runtime_readiness: readiness.into(),
        reason: reason.into(),
        last_observation_reason: None,
        startup_stage: None,
        startup_failure_reason: None,
        operation_busy: true,
        development_only: true,
        login_auto_start: false,
        automatic_restart: false,
        service_profile: "MACOS_USER_LAUNCH_AGENT",
        global_consensus: "NOT_CHECKED",
        attempt_id: Some("run-attempt".into()),
        pid: Some(31234),
        node_instance_id: Some("node-test".into()),
        health: None,
    }
}
fn waiting(kind: WaitKind) -> ReadinessWait {
    ReadinessWait {
        attempt: None,
        kind,
        last: None,
    }
}
#[test]
fn delayed_readiness_keeps_the_same_attempt_until_ready() {
    for kind in [WaitKind::Start, WaitKind::Status] {
        let mut wait = waiting(kind);
        assert!(
            wait.accept(
                observation("STARTING", "UNKNOWN", "UNKNOWN", "ENGINE_STARTING"),
                false
            )
            .unwrap()
            .is_none()
        );
        assert!(
            wait.accept(
                observation("RUNNING", "NOT_READY", "INCOMPLETE", "STARTUP_IN_PROGRESS"),
                false
            )
            .unwrap()
            .is_none()
        );
        let ready = wait
            .accept(
                observation("RUNNING", "READY", "SUCCEEDED", "VALIDATOR_READY"),
                true,
            )
            .unwrap()
            .unwrap();
        assert_eq!(ready.reason, "VALIDATOR_READY");
        assert_eq!(ready.outcome, "SUCCEEDED");
        assert_eq!(ready.attempt_id.as_deref(), Some("run-attempt"));
        assert!(ready.last_observation_reason.is_none());
    }
}
#[test]
fn expired_wait_keeps_unknown_or_incomplete_and_the_observation_reason() {
    for kind in [WaitKind::Start, WaitKind::Status] {
        for (engine, readiness, outcome, reason) in [
            ("STARTING", "UNKNOWN", "UNKNOWN", "ENGINE_STARTING"),
            ("RUNNING", "NOT_READY", "INCOMPLETE", "STARTUP_IN_PROGRESS"),
        ] {
            let result = waiting(kind)
                .accept(observation(engine, readiness, outcome, reason), true)
                .unwrap()
                .unwrap();
            assert_eq!(result.reason, kind.expired_reason());
            assert_eq!(result.outcome, outcome);
            assert_eq!(result.engine_state, engine);
            assert_eq!(result.runtime_readiness, readiness);
            assert_eq!(result.attempt_id.as_deref(), Some("run-attempt"));
            assert_eq!(result.pid, Some(31234));
            let json = serde_json::to_value(result).unwrap();
            assert_eq!(json["lastObservationReason"], reason);
        }
    }
}
#[test]
fn failed_and_stopped_observations_are_not_relabelled_as_wait_expiry() {
    for kind in [WaitKind::Start, WaitKind::Status] {
        for (engine, readiness, reason) in [
            ("FAILED", "UNKNOWN", "ENGINE_RUNTIME_FAILED"),
            ("RUNNING", "FAILED", "VALIDATOR_START_FAILED"),
            ("GATE_REFUSED", "UNKNOWN", "SERVICE_GATE_REFUSED"),
        ] {
            let result = waiting(kind)
                .accept(observation(engine, readiness, "FAILED", reason), true)
                .unwrap()
                .unwrap();
            assert_eq!(result.outcome, "FAILED");
            assert_eq!(result.reason, reason);
            assert!(result.last_observation_reason.is_none());
        }
        let result = waiting(kind)
            .accept(
                observation(
                    "STOPPED",
                    "STOPPED",
                    "SUCCEEDED",
                    "SERVICE_STOPPED_VERIFIED",
                ),
                true,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            result.reason,
            match kind {
                WaitKind::Start => "SERVICE_EXITED_DURING_START",
                WaitKind::Status => "SERVICE_STOPPED_VERIFIED",
            }
        );
        assert_eq!(
            result.outcome,
            match kind {
                WaitKind::Start => "INCOMPLETE",
                WaitKind::Status => "SUCCEEDED",
            }
        );
        assert!(result.last_observation_reason.is_none());
    }
}
#[test]
fn waiting_never_adopts_ready_from_a_replaced_or_missing_attempt() {
    for replacement in [Some("another-attempt".into()), None] {
        let mut wait = waiting(WaitKind::Status);
        wait.accept(
            observation("STARTING", "UNKNOWN", "UNKNOWN", "ENGINE_STARTING"),
            false,
        )
        .unwrap();
        let mut next = observation("RUNNING", "READY", "SUCCEEDED", "VALIDATOR_READY");
        next.attempt_id = replacement;
        let error = wait.accept(next, true).err().unwrap();
        assert_eq!(error.code, "SERVICE_ATTEMPT_CHANGED");
        assert_eq!(wait.attempt.as_deref(), Some("run-attempt"));
    }
}
#[test]
fn unstarted_instance_returns_immediately_and_does_not_adopt_a_future_attempt() {
    let mut wait = waiting(WaitKind::Status);
    let mut result = observation(
        "NOT_STARTED",
        "NOT_CHECKED",
        "INCOMPLETE",
        "SERVICE_NOT_STARTED",
    );
    result.attempt_id = None;
    result.pid = None;
    let result = wait.accept(result, false).unwrap().unwrap();
    assert_eq!(result.reason, "SERVICE_NOT_STARTED");
    assert!(wait.attempt.is_none());
    assert!(wait.last.is_none());
}
#[test]
fn final_query_expiry_preserves_starting_but_does_not_hide_identity_errors() {
    let mut wait = waiting(WaitKind::Status);
    wait.accept(
        observation("STARTING", "UNKNOWN", "UNKNOWN", "ENGINE_STARTING"),
        false,
    )
    .unwrap();
    let partial = expired_summary(
        observation(
            "UNKNOWN",
            "UNKNOWN",
            "UNKNOWN",
            "SERVICE_OBSERVATION_UNKNOWN",
        ),
        "SERVICE_STATUS_QUERY_EXPIRED",
    );
    let result = wait.accept(partial, true).unwrap().unwrap();
    assert_eq!(result.reason, "SERVICE_STATUS_WAIT_EXPIRED");
    assert_eq!(result.engine_state, "STARTING");
    assert_eq!(
        result.last_observation_reason.as_deref(),
        Some("ENGINE_STARTING")
    );

    let mut wait = waiting(WaitKind::Start);
    wait.accept(
        observation("STARTING", "UNKNOWN", "UNKNOWN", "ENGINE_STARTING"),
        false,
    )
    .unwrap();
    let result = wait
        .accept(
            observation(
                "RUNNING",
                "UNKNOWN",
                "UNKNOWN",
                "SERVICE_HTTP_IDENTITY_UNKNOWN",
            ),
            true,
        )
        .unwrap()
        .unwrap();
    assert_eq!(result.engine_state, "RUNNING");
    assert_eq!(
        result.last_observation_reason.as_deref(),
        Some("SERVICE_HTTP_IDENTITY_UNKNOWN")
    );
}
#[test]
fn deadline_before_next_poll_finishes_from_last_observation_without_a_fresh_budget() {
    let now = Instant::now();
    let deadline = now + Duration::from_secs(20);
    assert_eq!(
        observation_deadline(deadline, now),
        now + Duration::from_secs(8)
    );
    assert_eq!(
        observation_deadline(deadline, deadline - Duration::from_nanos(1)),
        deadline
    );
    assert_eq!(
        observation_deadline(deadline, deadline + Duration::from_secs(1)),
        deadline
    );
    assert_eq!(
        query_remaining(now - Duration::from_millis(1))
            .err()
            .unwrap()
            .code,
        "SERVICE_STATUS_QUERY_EXPIRED"
    );
    let mut wait = waiting(WaitKind::Status);
    wait.accept(
        observation("RUNNING", "NOT_READY", "INCOMPLETE", "STARTUP_IN_PROGRESS"),
        false,
    )
    .unwrap();
    let result = wait.finish().unwrap();
    assert_eq!(result.reason, "SERVICE_STATUS_WAIT_EXPIRED");
    assert_eq!(
        result.last_observation_reason.as_deref(),
        Some("STARTUP_IN_PROGRESS")
    );
    assert_eq!(result.outcome, "INCOMPLETE");
}

impl Fixture {
    fn bind_initialized(&mut self) {
        let root = self.control.path().parent().unwrap();
        let binding = Binding {
            schema_version: 1,
            supply: None,
            instance_id: self.state.instance_id.clone(),
            package: root.join("package"),
            archive: root.join("bundle.tar.gz"),
            public_key: None,
            allow_unsigned_development: true,
            product: root.join("product.json"),
            native: root.join("node.json"),
            lock: root.join("trusted.lock.json"),
            archive_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            identity: serde_json::from_slice(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/contracts/nigo/engine-rc1-2026-10-19/evidence/engine-info.json"
            )))
            .unwrap(),
            backend: "rocksdb".into(),
            chain_fingerprint: "c".repeat(64),
            node_identity: format!("0x{}:0x{}", "1".repeat(64), "2".repeat(40)),
            data_directory: root.join("data"),
            pins: vec![],
        };
        let raw = serde_json::to_vec(&binding).unwrap();
        store::write_new(&self.control.path().join("binding.json"), &raw).unwrap();
        self.state.binding_sha256 = files::digest(&raw);
        let mut journal = Store::create(&self.control.path().join("journal")).unwrap();
        self.state.save(&mut journal).unwrap();
        self.runtime.binding_sha256 = self.state.binding_sha256.clone();
        self.runtime.save(&mut self.store).unwrap();
        self.control.attempt(&self.runtime.attempt_id).unwrap();
    }
}
fn tree_bytes(path: &Path) -> std::collections::BTreeMap<PathBuf, (u32, Vec<u8>)> {
    fn walk(
        root: &Path,
        path: &Path,
        result: &mut std::collections::BTreeMap<PathBuf, (u32, Vec<u8>)>,
    ) {
        let metadata = fs::symlink_metadata(path).unwrap();
        let bytes = if metadata.is_dir() {
            vec![]
        } else {
            fs::read(path).unwrap()
        };
        result.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            (metadata.permissions().mode(), bytes),
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                walk(root, &entry.unwrap().path(), result);
            }
        }
    }
    let mut result = std::collections::BTreeMap::new();
    walk(path, path, &mut result);
    result
}
#[test]
fn expired_status_wait_reads_existing_attempt_without_writing_or_starting() {
    let mut f = Fixture::new();
    f.bind_initialized();
    let before = tree_bytes(f.control.path());
    let result =
        wait_observations(f.control.path(), None, Duration::ZERO, WaitKind::Status).unwrap();
    assert_eq!(result.reason, "SERVICE_STATUS_WAIT_EXPIRED");
    assert_eq!(
        result.last_observation_reason.as_deref(),
        Some("SERVICE_OBSERVATION_UNKNOWN")
    );
    assert_eq!(result.attempt_id.as_deref(), Some("run-attempt"));
    assert_eq!(result.outcome, "UNKNOWN");
    assert_eq!(before, tree_bytes(f.control.path()));
    let result = status_until(f.control.path(), Instant::now()).unwrap();
    assert_eq!(result.reason, "SERVICE_STATUS_QUERY_EXPIRED");
    assert_eq!(result.attempt_id.as_deref(), Some("run-attempt"));
    assert_eq!(before, tree_bytes(f.control.path()));
}
#[test]
fn expired_query_does_not_hide_invalid_runtime_identity() {
    let mut f = Fixture::new();
    f.bind_initialized();
    f.runtime.instance_id = "different-instance".into();
    f.runtime.save(&mut f.store).unwrap();
    let before = tree_bytes(f.control.path());
    let error = status_until(f.control.path(), Instant::now())
        .err()
        .unwrap();
    assert_eq!(error.code, "SERVICE_STATE_INVALID");
    assert_eq!(before, tree_bytes(f.control.path()));
}

#[test]
fn stalled_http_query_preserves_attempt_and_report_without_claiming_identity_failure() {
    use std::{io::Read, net::TcpListener, sync::mpsc};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (release, released) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request);
        // Keep the accepted connection open without a response until the client
        // has observed its socket deadline. No fixed test sleep is necessary.
        let _ = released.recv_timeout(Duration::from_secs(2));
    });
    let error = runtime_http::get(
        address,
        "/monitor/api/console/bootstrap",
        Duration::from_millis(50),
    )
    .err()
    .unwrap();
    let _ = release.send(());
    server.join().unwrap();
    assert_eq!(error.code, runtime_http::WAIT_EXPIRED);
    let result = observation_failure(
        observation("RUNNING", "UNKNOWN", "UNKNOWN", "ENGINE_RUNNING"),
        &error,
        "SERVICE_HTTP_IDENTITY_UNKNOWN",
    );
    assert_eq!(result.reason, "SERVICE_STATUS_QUERY_EXPIRED");
    assert_eq!(
        result.last_observation_reason.as_deref(),
        Some("ENGINE_RUNNING")
    );
    assert_eq!(result.attempt_id.as_deref(), Some("run-attempt"));
    assert_eq!(result.pid, Some(31234));
    assert_eq!(result.engine_state, "RUNNING");
    assert_eq!(result.runtime_readiness, "UNKNOWN");
    assert_eq!(result.outcome, "UNKNOWN");
}
#[test]
fn adapter_timeout_and_actual_identity_failure_remain_distinct_in_status() {
    let partial = || observation("RUNNING", "UNKNOWN", "UNKNOWN", "ENGINE_RUNNING");
    let launchd = fail(
        super::super::super::launchd::WAIT_EXPIRED,
        "bounded helper expired",
    );
    let result = observation_failure(partial(), &launchd, "SERVICE_OBSERVATION_UNKNOWN");
    assert_eq!(result.reason, "SERVICE_STATUS_QUERY_EXPIRED");
    assert_eq!(
        result.last_observation_reason.as_deref(),
        Some("ENGINE_RUNNING")
    );

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
    let error = validate_health_role(
        &health,
        &format!("0x{}:0x{}", "1".repeat(64), "2".repeat(40)),
    )
    .unwrap_err();
    let result = observation_failure(partial(), &error, "SERVICE_HTTP_IDENTITY_UNKNOWN");
    assert_eq!(result.reason, "SERVICE_HTTP_IDENTITY_UNKNOWN");
    assert!(result.last_observation_reason.is_none());
    assert_eq!(result.runtime_readiness, "UNKNOWN");
    let result = observation_failure(
        partial(),
        &fail("SERVICE_HTTP_UNKNOWN", "malformed response"),
        "SERVICE_HTTP_IDENTITY_UNKNOWN",
    );
    assert_eq!(result.reason, "SERVICE_HTTP_IDENTITY_UNKNOWN");
    assert!(result.last_observation_reason.is_none());
}

#[test]
fn startup_steps_are_durable_before_work_and_survive_refusal() {
    let mut f = Fixture::new();
    f.store.require_capacity(START_REVISIONS).unwrap();
    f.runtime.save(&mut f.store).unwrap();
    f.store.require_capacity(GATE_REVISIONS).unwrap();
    f.runtime.phase = RuntimePhase::GateConsumed;
    f.runtime.pid = Some(31234);
    f.runtime.save(&mut f.store).unwrap();
    let mut stage_count = 0;
    for stage in [
        StartupStage::InputPins,
        StartupStage::PackageVerification,
        StartupStage::ColdPreflight,
        StartupStage::NativeConfiguration,
        StartupStage::SnapshotVerification,
        StartupStage::FinalRecheck,
        StartupStage::EngineExec,
    ] {
        f.runtime.enter_stage(&mut f.store, stage).unwrap();
        // A separate observer sees the step even before its operation returns.
        let raw = f.store.read().unwrap().unwrap();
        let record = diagnostic_record(&f.control, &f.state, &raw).unwrap();
        assert_eq!(record.startup_stage.as_deref(), Some(stage.code()));
        assert_eq!(record.startup_failure_reason, None);
        assert_eq!(record.attempt, "run-attempt");
        assert_eq!(record.phase, "GATE_CONSUMED");
        stage_count += 1;
    }
    assert_eq!(START_REVISIONS, 1 + 1 + stage_count + 1 + 1);
    f.runtime
        .refuse(&mut f.store, "SERVICE_EXEC_FAILED")
        .unwrap();
    let raw = f.store.read().unwrap().unwrap();
    let record = diagnostic_record(&f.control, &f.state, &raw).unwrap();
    assert_eq!(record.startup_stage.as_deref(), Some("ENGINE_EXEC"));
    assert_eq!(
        record.startup_failure_reason.as_deref(),
        Some("SERVICE_EXEC_FAILED")
    );
    assert_eq!(record.phase, "GATE_REFUSED");
    assert!(
        f.runtime
            .enter_stage(&mut f.store, StartupStage::InputPins)
            .is_err()
    );
}

#[test]
fn old_v1_runtime_without_startup_stage_remains_compatible_and_unknown_stages_are_rejected() {
    let f = Fixture::new();
    let raw = serde_json::to_vec(&f.runtime).unwrap();
    assert!(!String::from_utf8_lossy(&raw).contains("startupStage"));
    assert!(
        decode_runtime(&f.control, &f.state, &raw)
            .unwrap()
            .startup_stage
            .is_none()
    );
    for stage in ["INPUT_PINS", "PRIVATE_CANARY"] {
        let mut value = serde_json::to_value(&f.runtime).unwrap();
        value["startupStage"] = stage.into();
        assert!(
            decode_runtime(&f.control, &f.state, &serde_json::to_vec(&value).unwrap()).is_err()
        );
    }
    let mut consumed = f.runtime;
    consumed.phase = RuntimePhase::GateConsumed;
    consumed.pid = Some(31234);
    let record = diagnostic_record(
        &f.control,
        &f.state,
        &serde_json::to_vec(&consumed).unwrap(),
    )
    .unwrap();
    assert!(record.startup_stage.is_none());
    assert!(record.startup_failure_reason.is_none());
}

#[test]
fn startup_failure_projection_withholds_unknown_reasons_and_never_infers_failure_from_stage() {
    let mut f = Fixture::new();
    f.runtime.phase = RuntimePhase::GateConsumed;
    f.runtime.pid = Some(31234);
    f.runtime.startup_stage = Some(StartupStage::PackageVerification);
    f.runtime.reason = "ENGINE_INPUT_IO".into();
    assert!(f.runtime.startup_failure().is_none());
    f.runtime
        .refuse(&mut f.store, "PRIVATE_CANARY /customer/path")
        .unwrap();
    let record =
        diagnostic_record(&f.control, &f.state, &f.store.read().unwrap().unwrap()).unwrap();
    assert_eq!(
        record.startup_stage.as_deref(),
        Some("PACKAGE_VERIFICATION")
    );
    assert_eq!(
        record.startup_failure_reason.as_deref(),
        Some("DETAIL_WITHHELD")
    );
    assert!(!String::from_utf8_lossy(&f.store.read().unwrap().unwrap()).contains("PRIVATE_CANARY"));
    assert_eq!(startup_failure_code("ENGINE_INPUT_IO"), "ENGINE_INPUT_IO");
}

#[test]
fn wait_expiry_preserves_current_startup_stage_without_starting_another_attempt() {
    let mut pending = observation("UNKNOWN", "UNKNOWN", "UNKNOWN", "SERVICE_REPORT_UNKNOWN");
    pending.startup_stage = Some("PACKAGE_VERIFICATION".into());
    let attempt = pending.attempt_id.clone();
    let expired = waiting(WaitKind::Status)
        .accept(pending, true)
        .unwrap()
        .unwrap();
    assert_eq!(expired.attempt_id, attempt);
    assert_eq!(
        expired.startup_stage.as_deref(),
        Some("PACKAGE_VERIFICATION")
    );
    assert!(expired.startup_failure_reason.is_none());
    assert_eq!(expired.reason, "SERVICE_STATUS_WAIT_EXPIRED");
    assert_eq!(
        expired.last_observation_reason.as_deref(),
        Some("SERVICE_REPORT_UNKNOWN")
    );
    assert_eq!(expired.outcome, "UNKNOWN");
}

#[test]
fn advancing_gate_stage_does_not_invalidate_read_only_snapshot_or_relax_writer_conflicts() {
    let mut f = Fixture::new();
    f.runtime.phase = RuntimePhase::GateConsumed;
    f.runtime.pid = Some(31234);
    f.runtime
        .enter_stage(&mut f.store, StartupStage::InputPins)
        .unwrap();
    let mut earlier = open_runtime(&f.control).unwrap().unwrap();
    f.runtime
        .enter_stage(&mut f.store, StartupStage::PackageVerification)
        .unwrap();
    assert!(earlier.read().is_err());
    let snapshot = runtime_snapshot(&f.control, &f.state, &earlier).unwrap();
    assert_eq!(snapshot.startup_stage, Some(StartupStage::InputPins));
    assert_eq!(snapshot.attempt_id, f.runtime.attempt_id);
    // An old observer snapshot must still be unable to overwrite newer work.
    assert!(snapshot.save(&mut earlier).is_err());
    let next = observe_runtime(&f.control, &f.state).unwrap().unwrap();
    assert_eq!(next.startup_stage, Some(StartupStage::PackageVerification));
    assert_eq!(next.attempt_id, snapshot.attempt_id);
}

#[test]
fn stop_observation_follows_stage_progress_but_never_adopts_a_replacement_attempt() {
    let mut f = Fixture::new();
    f.runtime.save(&mut f.store).unwrap();
    let initial = observe_runtime(&f.control, &f.state).unwrap().unwrap();
    assert_eq!(initial.phase, RuntimePhase::StartPrepared);
    f.runtime.phase = RuntimePhase::GateConsumed;
    f.runtime.pid = Some(31234);
    f.runtime
        .enter_stage(&mut f.store, StartupStage::InputPins)
        .unwrap();
    let mut earlier = open_runtime(&f.control).unwrap().unwrap();
    f.runtime
        .enter_stage(&mut f.store, StartupStage::PackageVerification)
        .unwrap();
    assert!(earlier.read().is_err());
    let current = observe_stop_attempt(&f.control, &f.state, &initial.attempt_id).unwrap();
    assert_eq!(
        current.startup_stage,
        Some(StartupStage::PackageVerification)
    );
    assert_eq!(current.pid, Some(31234));
    // Stop observations do not relax the exclusive final confirmation writer.
    assert!(current.save(&mut earlier).is_err());
    f.runtime.attempt_id = "different-attempt".into();
    f.runtime.save(&mut f.store).unwrap();
    assert_eq!(
        observe_stop_attempt(&f.control, &f.state, &initial.attempt_id)
            .err()
            .unwrap()
            .code,
        "SERVICE_OPERATION_UNKNOWN"
    );
}
