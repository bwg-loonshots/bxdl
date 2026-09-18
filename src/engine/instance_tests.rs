use super::*;
use serde_json::{Value, json};
use std::os::unix::fs::{PermissionsExt, symlink};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    control: Control,
    binding: Binding,
    state: State,
    journal: Store,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let control = Control::create(&root.join("control")).unwrap();
        let binding = Binding {
            schema_version: 1,
            instance_id: "test-validator".into(),
            package: root.join("package"),
            archive: root.join("bundle.tar.gz"),
            public_key: None,
            allow_unsigned_development: true,
            product: root.join("product.json"),
            native: root.join("node.json"),
            lock: root.join("trusted.lock.json"),
            archive_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            identity: serde_json::from_slice(include_bytes!(
                "../../contracts/nigo/development-clean-2026-09-18/evidence/engine-info.json"
            ))
            .unwrap(),
            backend: "rocksdb".into(),
            chain_fingerprint: "c".repeat(64),
            node_identity: format!("0x{}:0x{}", "1".repeat(64), "2".repeat(40)),
            data_directory: root.join("data"),
            pins: vec![],
        };
        let raw = serde_json::to_vec(&binding).unwrap();
        store::write_new(&control.path().join("binding.json"), &raw).unwrap();
        let mut journal = Store::create(&control.path().join("journal")).unwrap();
        let state = State {
            schema_version: 1,
            instance_id: binding.instance_id.clone(),
            binding_sha256: files::digest(&raw),
            phase: Phase::Registered,
            attempt: None,
            genesis_hash: None,
            reason: "REGISTERED_COLD_CHECKED".into(),
        };
        state.save(&mut journal).unwrap();
        Self {
            _temp: temp,
            root,
            control,
            binding,
            state,
            journal,
        }
    }
    fn pending(&mut self) {
        self.state.phase = Phase::InitIntent;
        self.state.attempt = Some(Attempt {
            id: "known-attempt".into(),
            command: "init".into(),
        });
        self.state.reason = "INITIALIZATION_ATTEMPT_UNRESOLVED".into();
        self.state.save(&mut self.journal).unwrap();
    }
    fn engine_journal(&self, status: &str) {
        fs::create_dir_all(self.binding.data_directory.join("ledger")).unwrap();
        let v = json!({"status":status,"backend":"rocksdb","chainFingerprint":self.binding.chain_fingerprint,"nodeIdentity":self.binding.node_identity,"dataDirectory":self.binding.data_directory,"genesisHash":if status=="INITIALIZING"{"".into()}else{format!("0x{}","3".repeat(64))}});
        put(
            &self.binding.data_directory.join("engine-instance.json"),
            &serde_json::to_vec(&v).unwrap(),
        );
        put(&self.binding.data_directory.join("engine.lock"), b"");
    }
}
fn put(path: &Path, raw: &[u8]) {
    fs::write(path, raw).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
#[test]
fn unresolved_intent_stays_unknown_and_inherited_lock_is_observed() {
    let mut f = Fixture::new();
    assert_eq!(
        show(f.control.path()).unwrap().initialization,
        "NOT_STARTED"
    );
    f.pending();
    let lock = f.control.lock().unwrap();
    let report = show(f.control.path()).unwrap();
    assert_eq!(report.initialization, "UNKNOWN");
    assert_eq!(report.operation_busy, Some(true));
    assert_eq!(report.attempt_id.as_deref(), Some("known-attempt"));
    let child = lock.child_stdin().unwrap();
    drop(lock);
    assert_eq!(show(f.control.path()).unwrap().operation_busy, Some(true));
    drop(child);
    assert_eq!(show(f.control.path()).unwrap().operation_busy, Some(false));
    assert_eq!(show(f.control.path()).unwrap().initialization, "UNKNOWN");
}
#[test]
fn journal_terminal_state_must_have_attempt_and_genesis() {
    let mut f = Fixture::new();
    f.state.phase = Phase::Initialized;
    f.state.genesis_hash = Some(format!("0x{}", "3".repeat(64)));
    f.state.save(&mut f.journal).unwrap();
    assert_eq!(
        show(f.control.path()).err().unwrap().code,
        "INSTANCE_STATE_INVALID"
    );
}
#[test]
fn changed_binding_is_not_adopted_and_summary_does_not_expose_paths_or_hashes() {
    let f = Fixture::new();
    let report = serde_json::to_string(&show(f.control.path()).unwrap()).unwrap();
    assert!(!report.contains(f.root.to_str().unwrap()));
    assert!(!report.contains(&f.state.binding_sha256));
    let path = f.control.path().join("binding.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["instanceId"] = json!("changed");
    put(&path, &serde_json::to_vec(&value).unwrap());
    assert_eq!(
        show(f.control.path()).err().unwrap().code,
        "INSTANCE_STATE_INVALID"
    );
}
#[test]
fn input_rotation_blocks_without_echoing_private_contents() {
    let mut f = Fixture::new();
    let path = f.root.join("secret");
    put(&path, b"CANARY-original-secret");
    f.binding.pins.push(Pin {
        path: path.clone(),
        sha256: files::digest(b"CANARY-original-secret"),
    });
    assert!(f.binding.check_pins().is_ok());
    put(&path, b"CANARY-rotated-secret");
    let error = f.binding.check_pins().err().unwrap();
    assert_eq!(error.code, "INSTANCE_INPUT_CHANGED");
    assert!(!error.message.contains("CANARY"));
    assert!(!error.message.contains(f.root.to_str().unwrap()));
}
#[test]
fn init_rejects_nonempty_symlink_and_missing_parent_without_changes() {
    let mut f = Fixture::new();
    assert!(data_precondition(&f.binding, false).is_ok());
    assert!(!f.binding.data_directory.exists());
    fs::create_dir(&f.binding.data_directory).unwrap();
    put(&f.binding.data_directory.join("sentinel"), b"keep");
    assert!(data_precondition(&f.binding, false).is_err());
    assert_eq!(
        fs::read(f.binding.data_directory.join("sentinel")).unwrap(),
        b"keep"
    );
    f.binding.data_directory = f.root.join("link");
    symlink(f.root.join("data"), &f.binding.data_directory).unwrap();
    assert!(data_precondition(&f.binding, false).is_err());
    f.binding.data_directory = f.root.join("missing-parent/data");
    assert!(data_precondition(&f.binding, false).is_err());
    assert!(!f.root.join("missing-parent").exists());
}
#[test]
fn resume_requires_initializing_same_identity_and_existing_ledger() {
    let f = Fixture::new();
    f.engine_journal("INITIALIZING");
    assert!(data_precondition(&f.binding, true).is_err());
    put(
        &f.binding.data_directory.join("ledger/CURRENT"),
        b"MANIFEST-00001\n",
    );
    assert!(data_precondition(&f.binding, true).is_ok());
    f.engine_journal("INITIALIZED");
    let before = fs::read(f.binding.data_directory.join("engine-instance.json")).unwrap();
    assert_eq!(
        data_precondition(&f.binding, true).unwrap_err().code,
        "INSTANCE_RESUME_NOT_ELIGIBLE"
    );
    assert_eq!(
        before,
        fs::read(f.binding.data_directory.join("engine-instance.json")).unwrap()
    );
    f.engine_journal("INITIALIZING");
    let path = f.binding.data_directory.join("engine-instance.json");
    let mut journal: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    journal["chainFingerprint"] = json!("f".repeat(64));
    put(&path, &serde_json::to_vec(&journal).unwrap());
    assert!(data_precondition(&f.binding, true).is_err());
}
#[test]
fn input_and_trust_paths_cannot_overlap_control_data_or_package() {
    let mut f = Fixture::new();
    assert!(disjoint(f.control.path(), &f.binding).is_ok());
    f.binding.lock = f.binding.package.join("docs/lock.json");
    assert_eq!(
        disjoint(f.control.path(), &f.binding).unwrap_err().code,
        "INSTANCE_TRUST_PATH_OVERLAP"
    );
    f.binding.lock = f.root.join("lock");
    f.binding.data_directory = f.control.path().join("data");
    assert!(disjoint(f.control.path(), &f.binding).is_err());
    f.binding.data_directory = f.root.join("data");
    f.binding.pins.push(Pin {
        path: f.binding.data_directory.join("key"),
        sha256: "a".repeat(64),
    });
    assert!(disjoint(f.control.path(), &f.binding).is_err());
}
