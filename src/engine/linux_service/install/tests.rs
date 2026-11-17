use super::*;
fn fixture() -> Request {
    let inputs = Inputs {
        runtime_root: "/home/operator/jre".into(),
        engine_jar: "/home/operator/node.jar".into(),
        product_config: "/home/operator/product.json".into(),
        native_config: "/home/operator/native.json".into(),
    };
    let mut product = serde_json::json!({"schemaVersion":1,"instanceId":"alpha","role":"validator","nodeId":format!("0x{}","11".repeat(32)),"chainDescription":"/home/operator/chain.json","storage":{"backend":"rocksdb","dataDirectory":"/var/lib/bxdl/instances/alpha/data"},"secrets":{},"http":{"address":"127.0.0.1","port":18080},"p2p":{"address":"127.0.0.1","port":19090}});
    for (field, _) in SECRET_MAPPING {
        product["secrets"][*field] = Value::String(format!("/home/operator/{field}"));
    }
    let mut native = serde_json::json!({"chainFile":"/home/operator/chain.json","dataDirectory":"/var/lib/bxdl/instances/alpha/data","backend":"rocksdb","node":{}});
    native["node"][format!("{QBFT}role")] = Value::String("VALIDATOR".into());
    native["node"][format!("{QBFT}transport-security-scheme")] = Value::String("MTLS".into());
    native["node"][format!("{QBFT}peers")] = Value::String("keep-this-policy".into());
    let control = control_path("alpha").unwrap();
    derive("alpha", &control, &mut product, &mut native).unwrap();
    let mut paths = reference_paths(&product, &native).unwrap();
    paths.extend([
        inputs.engine_jar.clone(),
        inputs.product_config.clone(),
        inputs.native_config.clone(),
    ]);
    let engine = lock().unwrap();
    let pins = paths
        .into_iter()
        .map(|path| {
            if path == inputs.engine_jar {
                Pin {
                    path,
                    sha256: engine.jar_sha256.clone(),
                    size: engine.jar_size_bytes,
                }
            } else {
                Pin {
                    path,
                    sha256: "a".repeat(64),
                    size: 8,
                }
            }
        })
        .collect();
    Request {
        schema_version: 1,
        name: "alpha".into(),
        control,
        operator_uid: 1000,
        inputs,
        pins,
        product,
        native,
        runtime_lock_sha256: files::digest(RUNTIME_LOCK),
        engine_lock_sha256: files::digest(ENGINE_LOCK),
    }
}
#[test]
fn approved_contract_round_trips_and_preserves_unrelated_native_policy() {
    let request = fixture();
    validate(&request).unwrap();
    let raw = encoded(&request).unwrap();
    let decoded: Request = json::decode(&raw).unwrap();
    assert_eq!(digest(&request).unwrap(), digest(&decoded).unwrap());
    assert_eq!(
        request.native["node"][format!("{QBFT}peers")],
        "keep-this-policy"
    );
}
#[test]
fn reviewed_contract_rejects_mismatched_native_and_unselected_supply() {
    for change in 0..9 {
        let mut r = fixture();
        match change {
            0 => r.native["node"]["server.port"] = Value::from(18081),
            1 => r.native["chainFile"] = Value::String("/home/operator/another-chain".into()),
            2 => {
                r.native["node"][format!("{QBFT}keystore-path")] =
                    Value::String("/home/operator/another-key".into())
            }
            3 => {
                r.native["node"][format!("{QBFT}transport-security-scheme")] =
                    Value::String("NONE".into())
            }
            4 => r.operator_uid = 0,
            5 => r.control = "/tmp/alpha".into(),
            6 => r.engine_lock_sha256 = "b".repeat(64),
            7 => {
                r.pins
                    .iter_mut()
                    .find(|p| p.path == r.inputs.engine_jar)
                    .unwrap()
                    .sha256 = "a".repeat(64)
            }
            8 => r.pins.push(r.pins[0].clone()),
            _ => unreachable!(),
        }
        assert!(validate(&r).is_err(), "mutation {change} admitted");
    }
}
#[test]
fn root_journal_actual_store_round_trip_at_crash_boundaries() {
    let temporary = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temporary.path()).unwrap();
    let mut journal = Store::create(&root.join("journal")).unwrap();
    for phase in [
        InstallPhase::SnapshotIntent,
        InstallPhase::SnapshotsReady,
        InstallPhase::AccountIntent,
        InstallPhase::RegisterIntent,
        InstallPhase::Reserved,
    ] {
        let state = InstallState {
            schema_version: 1,
            request_sha256: "a".repeat(64),
            phase: phase.clone(),
            account: accounts::State::default(),
            control_identity: if phase == InstallPhase::Reserved {
                Some(RootIdentity {
                    device: 1,
                    inode: 9,
                })
            } else {
                None
            },
            receipt: None,
        };
        save(&mut journal, &state).unwrap();
        let raw = journal.read().unwrap().unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("null"));
        let loaded: InstallState = json::decode(&raw).unwrap();
        assert_eq!(loaded.phase, phase);
        assert_eq!(loaded.control_identity, state.control_identity);
    }
}
#[test]
fn source_size_and_request_identity_are_bound_before_root_execution() {
    let mut r = fixture();
    let original = digest(&r).unwrap();
    r.product["http"]["port"] = Value::from(18082);
    r.native["node"]["server.port"] = Value::from(18082);
    assert_ne!(original, digest(&r).unwrap());
    let nonjar = r
        .pins
        .iter_mut()
        .find(|p| p.path != r.inputs.engine_jar)
        .unwrap();
    nonjar.size = 4 * 1024 * 1024 + 1;
    assert!(validate(&r).is_err());
    for name in ["", "../alpha", "Alpha", "-alpha", "alpha/other", "alpha\n"] {
        assert!(control_path(name).is_err());
    }
}
#[test]
fn bounded_collector_returns_only_stdout_and_rejects_oversized_output() {
    let mut small = Command::new("/usr/bin/printf");
    small.arg("ok");
    assert_eq!(
        capture_command(small, Duration::from_secs(2)).unwrap(),
        (0, b"ok".to_vec())
    );
    // Fixed benign command exercises a byte cap without any privileged helper.
    let mut large = Command::new("/usr/bin/head");
    large.args(["-c", "1048577", "/dev/zero"]);
    assert!(capture_command(large, Duration::from_secs(2)).is_err());
}
