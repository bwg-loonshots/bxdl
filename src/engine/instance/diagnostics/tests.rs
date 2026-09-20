use super::*;
use serde_json::{Value, json};
use std::os::unix::fs::{PermissionsExt, symlink};

struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
    root: Control,
    binding: Binding,
    state: State,
    journal: Store,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
        let root = Control::create(&base.join("control")).unwrap();
        let binding = Binding {
            schema_version: 1,
            instance_id: "CANARY-CUSTOMER-ID".into(),
            package: base.join("package"),
            archive: base.join("bundle.tar.gz"),
            public_key: None,
            allow_unsigned_development: true,
            product: base.join("product.json"),
            native: base.join("node.json"),
            lock: base.join("trusted.lock.json"),
            archive_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            identity: serde_json::from_slice(include_bytes!(
                "../../../../contracts/nigo/development-clean-2026-09-18/evidence/engine-info.json"
            ))
            .unwrap(),
            backend: "rocksdb".into(),
            chain_fingerprint: "c".repeat(64),
            node_identity: format!("0x{}:0x{}", "1".repeat(64), "2".repeat(40)),
            data_directory: base.join("CANARY-DATA"),
            pins: vec![Pin {
                path: base.join("CANARY-KEY"),
                sha256: "d".repeat(64),
            }],
        };
        let raw = serde_json::to_vec(&binding).unwrap();
        store::write_new(&root.path().join("binding.json"), &raw).unwrap();
        let mut journal = Store::create(&root.path().join("journal")).unwrap();
        let state = State {
            schema_version: 1,
            instance_id: binding.instance_id.clone(),
            binding_sha256: files::digest(&raw),
            phase: Phase::Registered,
            attempt: None,
            genesis_hash: None,
            reason: "CANARY-state-reason".into(),
        };
        state.save(&mut journal).unwrap();
        Self {
            _temp: temp,
            base,
            root,
            binding,
            state,
            journal,
        }
    }
    fn init(&mut self) {
        self.state.phase = Phase::Initialized;
        self.state.genesis_hash = Some(format!("0x{}", "3".repeat(64)));
        self.state.attempt = Some(Attempt {
            id: "CANARY-init-attempt".into(),
            command: "init".into(),
        });
        self.state.save(&mut self.journal).unwrap();
        let dir = self.root.attempt("CANARY-init-attempt").unwrap();
        put(
            &dir.join("report.jsonl"),
            &rows("init", "CANARY-init-attempt"),
        );
    }
    fn service(&self) -> (PathBuf, Store) {
        let attempt = "CANARY-service-attempt";
        let dir = self.root.attempt(attempt).unwrap();
        let mut journal = Store::create(&self.root.path().join("service-journal")).unwrap();
        let raw = json!({"schemaVersion":1,"instanceId":self.binding.instance_id,
            "controlDirectory":self.root.path(),"bindingSha256":self.state.binding_sha256,
            "attemptId":attempt,"phase":"STOPPED_VERIFIED","uid":rustix::process::getuid().as_raw(),
            "pid":1234,"workerSha256":"a".repeat(64),"nodeSha256":"b".repeat(64),"chainSha256":"c".repeat(64),
            "endpoint":"127.0.0.1:19100","reason":"CANARY-service-reason"});
        journal.save(&serde_json::to_vec(&raw).unwrap()).unwrap();
        put(&dir.join("report.jsonl"), &rows("run", attempt));
        put(
            &dir.join("service.stdout.json"),
            b"password=CANARY-STDOUT\x1b[31m",
        );
        put(
            &dir.join("service.stderr.private"),
            b"-----BEGIN PRIVATE KEY-----\nCANARY-SECRET\n",
        );
        (dir, journal)
    }
}
fn rows(command: &str, attempt: &str) -> Vec<u8> {
    let items = [
        (
            "CHECKING",
            "VALIDATING_CONFIGURATION",
            json!({"build":{"private":"CANARY-build"}}),
        ),
        (
            "STARTING",
            "OPENING_MANAGED_STORAGE",
            json!({"backend":"rocksdb"}),
        ),
        (
            "FAILED",
            "STARTUP_OR_INITIALIZATION_FAILED",
            json!({"resultMayBePartial":true}),
        ),
    ];
    let mut raw = Vec::new();
    for (i, (status, reason, details)) in items.into_iter().enumerate() {
        raw.extend(serde_json::to_vec(&json!({"attemptId":attempt,"command":command,"sequence":i+1,
            "pid":1234,"observedAt":0,"status":status,"reason":reason,"contractStatus":"PROPOSED","details":details})).unwrap());
        raw.push(b'\n');
    }
    raw
}
fn put(path: &Path, raw: &[u8]) {
    fs::write(path, raw).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn json_value(report: &Report) -> Value {
    serde_json::to_value(report).unwrap()
}

#[test]
fn registered_offline_instance_does_not_require_package_keys_or_engine() {
    let f = Fixture::new();
    let before = fs::read(f.root.path().join("binding.json")).unwrap();
    let report = logs(f.root.path(), &Options::default()).unwrap();
    assert!(!report.partial);
    assert_eq!(report.initialization_record, "NOT_STARTED");
    assert_eq!(report.initialization_reason, "DETAIL_WITHHELD");
    assert_eq!(report.service_record, "NO_RECORDED_ATTEMPT");
    assert_eq!(report.runtime_readiness, "NOT_OBSERVED");
    assert!(!report.output_created);
    assert!(!f.binding.package.exists());
    assert!(!f.binding.data_directory.exists());
    assert_eq!(
        fs::read(f.root.path().join("binding.json")).unwrap(),
        before
    );
}
#[test]
fn stopped_instance_exports_only_fixed_events_with_no_secret_or_current_health_claim() {
    let mut f = Fixture::new();
    f.init();
    f.service();
    let target = f.base.join("diagnose.json");
    let report = diagnose(f.root.path(), &target, &Options::default()).unwrap();
    assert!(!report.partial);
    assert!(report.output_created);
    assert_eq!(report.events.len(), 6);
    assert_eq!(report.service_record, "STOPPED_VERIFIED");
    let bytes = fs::read(&target).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(!text.contains("CANARY"));
    assert!(!text.contains(f.base.to_str().unwrap()));
    assert!(!text.contains(&f.binding.archive_sha256));
    assert!(!text.contains("PRIVATE KEY"));
    assert!(!text.contains('\u{1b}'));
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        json_value(&report)
    );
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        report
            .sources
            .iter()
            .any(|s| s.reason == "RAW_TEXT_EXCLUDED_BY_POLICY")
    );
    assert!(!f.binding.data_directory.exists());
}
#[test]
fn busy_unknown_instance_is_observed_without_waiting_or_recovery() {
    let mut f = Fixture::new();
    f.init();
    f.state.phase = Phase::Unknown;
    f.state.genesis_hash = None;
    f.state.reason = "ENGINE_INITIALIZATION_EXIT_FAILED".into();
    f.state.save(&mut f.journal).unwrap();
    let _lock = f.root.lock().unwrap();
    let report = logs(f.root.path(), &Options::default()).unwrap();
    assert_eq!(report.operation_busy, Some(true));
    assert_eq!(report.initialization_record, "UNKNOWN");
    assert_eq!(
        report.initialization_reason,
        "ENGINE_INITIALIZATION_EXIT_FAILED"
    );
    assert_eq!(
        f.journal.read().unwrap(),
        Some(serde_json::to_vec(&f.state).unwrap())
    );
}
#[test]
fn partial_report_and_tail_are_explicit_and_exportable() {
    let mut f = Fixture::new();
    f.init();
    let (dir, _) = f.service();
    let path = dir.join("report.jsonl");
    let mut raw = fs::read(&path).unwrap();
    raw.extend(b"CANARY-trailing-partial");
    put(&path, &raw);
    let options = Options {
        tail: 2,
        ..Options::default()
    };
    let report = diagnose(f.root.path(), &f.base.join("partial.json"), &options).unwrap();
    assert!(report.partial);
    assert_eq!(report.events.len(), 2);
    assert!(report.events_omitted >= 5);
    assert!(!serde_json::to_string(&report).unwrap().contains("CANARY"));
}
#[test]
fn missing_report_and_unsafe_log_are_partial_not_node_failure() {
    let mut f = Fixture::new();
    f.init();
    let (dir, _) = f.service();
    fs::remove_file(dir.join("report.jsonl")).unwrap();
    fs::remove_file(dir.join("service.stderr.private")).unwrap();
    symlink("/dev/zero", dir.join("service.stderr.private")).unwrap();
    let report = diagnose(
        f.root.path(),
        &f.base.join("partial.json"),
        &Options::default(),
    )
    .unwrap();
    assert!(report.partial);
    assert_eq!(report.service_record, "STOPPED_VERIFIED");
    assert_eq!(report.runtime_readiness, "NOT_OBSERVED");
}
#[test]
fn damaged_binding_still_allows_partial_logs_but_cannot_export_to_unknown_scope() {
    let f = Fixture::new();
    put(&f.root.path().join("binding.json"), b"CANARY invalid");
    let report = logs(f.root.path(), &Options::default()).unwrap();
    assert!(report.partial);
    let output = f.base.join("out.json");
    assert_eq!(
        diagnose(f.root.path(), &output, &Options::default())
            .err()
            .unwrap()
            .code,
        "DIAGNOSTIC_EXPORT_SCOPE_UNKNOWN"
    );
    assert!(!output.exists());
    assert!(!serde_json::to_string(&report).unwrap().contains("CANARY"));
}
#[test]
fn export_never_overwrites_or_creates_protected_paths() {
    let f = Fixture::new();
    for path in [
        f.root.path().join("export.json"),
        f.binding.data_directory.join("export.json"),
        f.binding.package.join("export.json"),
        f.binding.pins[0].path.clone(),
    ] {
        assert_eq!(
            diagnose(f.root.path(), &path, &Options::default())
                .err()
                .unwrap()
                .code,
            "DIAGNOSTIC_OUTPUT_OVERLAP"
        );
        assert!(!path.exists());
    }
    let path = f.base.join("existing.json");
    put(&path, b"keep");
    assert!(diagnose(f.root.path(), &path, &Options::default()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"keep");
    assert!(
        diagnose(
            f.root.path(),
            &f.base.join("missing/out.json"),
            &Options::default()
        )
        .is_err()
    );
    assert!(!f.base.join("missing").exists());
}
#[test]
fn bounded_reads_and_expired_budget_report_omissions() {
    let mut f = Fixture::new();
    f.init();
    let (dir, _) = f.service();
    put(&dir.join("report.jsonl"), &vec![b'x'; 1_048_576]);
    let options = Options {
        max_bytes: 16_384,
        ..Options::default()
    };
    let report = diagnose(f.root.path(), &f.base.join("bounded.json"), &options).unwrap();
    assert!(report.partial);
    assert!(report.bytes_read <= options.max_bytes);
    assert!(fs::metadata(f.base.join("bounded.json")).unwrap().len() <= options.max_bytes);
    let mut c = Collector::new(&Options::default()).unwrap();
    c.deadline = Instant::now();
    c.events(
        "service.report",
        &dir.join("report.jsonl"),
        "run",
        "attempt",
    );
    assert!(c.report.partial);
    assert_eq!(c.report.bytes_read, 0);
    assert_eq!(c.report.sources[0].reason, "TIME_BUDGET_EXCEEDED");
}
#[test]
fn cli_success_and_partial_exit_codes_are_collection_results() {
    let mut f = Fixture::new();
    for expected in [0, 5] {
        if expected == 5 {
            f.init();
            fs::remove_file(
                f.root
                    .path()
                    .join("operations/CANARY-init-attempt/report.jsonl"),
            )
            .unwrap();
        }
        let mut out = Vec::new();
        let mut err = Vec::new();
        let args = vec![
            "logs".to_owned(),
            "--instance".into(),
            f.root.path().to_str().unwrap().into(),
            "--json".into(),
        ];
        assert_eq!(crate::cli::run(&args, &mut out, &mut err), expected);
        assert!(err.is_empty());
        let value: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["data"]["partial"], expected == 5);
        assert!(!String::from_utf8(out).unwrap().contains("CANARY"));
    }
}

#[test]
fn changed_registration_cannot_redirect_export_into_original_data() {
    let f = Fixture::new();
    fs::create_dir(&f.binding.data_directory).unwrap();
    let path = f.root.path().join("binding.json");
    let mut binding: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    binding["dataDirectory"] = json!(f.base.join("different-data"));
    put(&path, &serde_json::to_vec(&binding).unwrap());
    let output = f.binding.data_directory.join("unsafe.json");
    assert!(logs(f.root.path(), &Options::default()).unwrap().partial);
    assert_eq!(
        diagnose(f.root.path(), &output, &Options::default())
            .err()
            .unwrap()
            .code,
        "DIAGNOSTIC_EXPORT_SCOPE_UNKNOWN"
    );
    assert!(!output.exists());
}
#[test]
fn checkpoints_obey_payload_limit_before_loading() {
    let f = Fixture::new();
    let path = f.root.path().join("journal");
    assert!(Store::open_bounded(&path, 1).is_err());
    let store = Store::open_bounded(&path, 4096).unwrap();
    assert!(store.read_bounded(1).is_err());
    assert!(store.read_bounded(4096).unwrap().is_some());
}
