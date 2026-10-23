use super::*;
use serde_json::json;
use std::os::unix::fs::{PermissionsExt, symlink};

const CANARY: &str = "PRIVATE_PREPARATION_CREDENTIAL_CANARY";
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    source: PathBuf,
    network: PathBuf,
    archive: PathBuf,
    lock: PathBuf,
}
fn write(path: &Path, raw: &[u8], mode: u32) {
    fs::write(path, raw).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn write_json(path: &Path, value: &Value) {
    write(path, &serde_json::to_vec(value).unwrap(), 0o600);
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let source = root.join("product.json");
        let network = root.join("network.json");
        let archive = root.join("package.tar.gz");
        let lock = root.join("engine.lock.json");
        let mut product: Value = serde_json::from_slice(include_bytes!(
            "../../../config/examples/instance.development.json"
        ))
        .unwrap();
        product["nodeId"] = json!(format!("0x{}", "a".repeat(64)));
        product["chainDescription"] = json!("chain.json");
        product["storage"]["dataDirectory"] = json!("data/node");
        product["p2p"]["address"] = json!("127.0.0.1");
        for (index, field) in super::super::FIELDS[8..].iter().enumerate() {
            let name = format!("credential-{index}");
            write(&root.join(&name), CANARY.as_bytes(), 0o600);
            product["secrets"][field.name] = json!(name);
        }
        write_json(&source, &product);
        write_json(
            &root.join("chain.json"),
            &json!({
                "nigo.protocol.chain-id":"11578", "nigo.protocol.consensus.profile-id":"TEST_QBFT",
                "nigo.protocol.consensus.protocol":"QBFT", "nigo.protocol.consensus.qbft.validators[0]":format!("0x{}", "b".repeat(40))
            }),
        );
        write_json(
            &network,
            &json!({"schemaVersion":1,"kind":"BXDL_QBFT_NETWORK","nodes":[{
                "nodeId":format!("0x{}", "a".repeat(64)),"validatorId":format!("0x{}", "b".repeat(40)),
                "host":"127.0.0.1","port":19090,"transactionPermission":"RELAY",
                "credentialFingerprints":[format!("0x{}", "c".repeat(64))]
            }]}),
        );
        let identity: Value = serde_json::from_slice(include_bytes!(
            "../../../contracts/nigo/development-clean-2026-09-18/evidence/engine-info.json"
        ))
        .unwrap();
        let jar = b"fixture jar never executed";
        let java = b"fixture java never executed";
        write_json(
            &lock,
            &json!({"schemaVersion":1,"jarSha256":digest(jar),"jarSizeBytes":jar.len(),"javaSha256":digest(java),"expected":identity}),
        );
        let stage = root.join("stage");
        for directory in ["bin", "engine", "runtime/bin", "licenses"] {
            fs::create_dir_all(stage.join(directory)).unwrap();
        }
        write(
            &stage.join("bin/bxdl"),
            b"fixture CLI deliberately mismatched",
            0o755,
        );
        write(&stage.join("engine/nigo-node.jar"), jar, 0o644);
        write(&stage.join("runtime/bin/java"), java, 0o755);
        write(
            &stage.join("licenses/THIRD_PARTY_NOTICES"),
            b"test only",
            0o644,
        );
        write(&stage.join("licenses/SBOM.json"), b"{}", 0o644);
        let spec = root.join("spec.json");
        write_json(
            &spec,
            &json!({"schemaVersion":1,"kind":"bxdl-package",
            "product":{"name":"BXDL","version":"0.1.0-dev","revision":"development"},"channel":"development",
            "platform":{"os":"darwin","arch":"arm64","libc":"none","minGlibc":"none","javaMajor":21,"backend":"rocksdb"},
            "engine":{"revision":identity["source"]["commit"],"jarSha256":digest(jar),"contractStatus":"proposed","contractRevision":identity["contract"]["fingerprint"]},
            "runtime":{"vendor":"test-only","version":"21-test","javaSha256":digest(java)}}),
        );
        artifact::build(&artifact::BuildOptions {
            root: stage,
            spec_path: spec,
            output: archive.clone(),
            signing_key_path: None,
            allow_unsigned_development: true,
        })
        .unwrap();
        Self {
            _temp: temp,
            root,
            source,
            network,
            archive,
            lock,
        }
    }
    fn inputs(&self) -> Inputs {
        Inputs {
            archive: Some(self.archive.clone()),
            unsigned: Some(true),
            lock: Some(self.lock.clone()),
            network: Some(self.network.clone()),
            destination: Some(self.root.join("packages/node")),
            instance: Some(self.root.join("instances/node")),
            runtime_options: Some(RuntimeOptions::default()),
            ..Inputs::default()
        }
    }
    fn session(&self) -> Preparation {
        let mut p =
            Preparation::create(&self.root.join("preparation"), Some(&self.source)).unwrap();
        p.set_inputs(self.inputs()).unwrap();
        p
    }
    fn assert_no_operations(&self) {
        for name in ["packages", "instances", "data"] {
            assert!(!self.root.join(name).exists());
        }
    }
}

#[test]
fn import_prepare_resume_checks_real_archive_without_creating_operational_roots() {
    let f = Fixture::new();
    let original = fs::read(&f.source).unwrap();
    let mut preparation = f.session();
    let summary = preparation.prepare().unwrap();
    assert_eq!(summary.preparation, "PREPARED_RECORDED");
    assert_eq!(summary.installation, "NOT_PERFORMED");
    assert_eq!(summary.runtime_readiness, "NOT_CHECKED");
    assert_eq!(summary.cli_match, "MISMATCH");
    preparation.check().unwrap();
    assert!(preparation.product_path(1).exists());
    assert!(preparation.native_path(1).exists());
    let safe = serde_json::to_string(&summary).unwrap();
    assert!(!safe.contains(CANARY));
    assert!(!safe.contains("credential-"));
    assert!(!safe.contains("sha256"));
    let path = preparation.path().to_owned();
    drop(preparation);
    let resumed = Preparation::resume(&path).unwrap();
    resumed.check().unwrap();
    assert_eq!(resumed.summary().plan_revision, 1);
    assert_eq!(fs::read(&f.source).unwrap(), original);
    f.assert_no_operations();
}

#[test]
fn accepted_edit_invalidates_review_preserving_other_answers_and_old_snapshots() {
    let f = Fixture::new();
    let mut p = f.session();
    p.prepare().unwrap();
    let product_before = fs::read(p.product_path(1)).unwrap();
    let native_before = fs::read(p.native_path(1)).unwrap();
    let same = p.inputs().clone();
    p.set_inputs(same).unwrap();
    p.set_field("httpPort", "18080").unwrap();
    assert!(p.set_field("httpPort", "PRIVATE_INVALID_CANARY").is_err());
    assert_eq!(p.summary().preparation, "PREPARED_RECORDED");
    p.set_field("httpPort", "18081").unwrap();
    assert_eq!(p.summary().preparation, "DRAFT");
    assert!(p.draft().complete());
    assert!(p.check().is_err());
    p.prepare().unwrap();
    assert_eq!(p.summary().plan_revision, 2);
    assert_eq!(fs::read(p.product_path(1)).unwrap(), product_before);
    assert_eq!(fs::read(p.native_path(1)).unwrap(), native_before);
    let second: Value = serde_json::from_slice(&fs::read(p.product_path(2)).unwrap()).unwrap();
    assert_eq!(second["http"]["port"], 18081);
    assert_eq!(second["instanceId"], "validator-one");
    f.assert_no_operations();
}

#[test]
fn partial_draft_can_select_validated_public_identity_as_one_checkpoint() {
    let f = Fixture::new();
    let path = f.root.join("selection");
    let mut p = Preparation::create(&path, None).unwrap();
    p.set_field(
        "chainDescription",
        f.root.join("chain.json").to_str().unwrap(),
    )
    .unwrap();
    p.set_inputs(Inputs {
        network: Some(f.network.clone()),
        ..Inputs::default()
    })
    .unwrap();
    let choices = p.node_choices().unwrap();
    assert_eq!(choices.len(), 1);
    p.select_node(&choices[0].node_id).unwrap();
    assert!(!p.draft().complete());
    drop(p);
    let resumed = Preparation::resume(&path).unwrap();
    let rows = resumed.review_rows();
    assert!(rows.iter().any(|(_, value)| value == &choices[0].node_id));
    assert!(rows.iter().any(|(_, value)| value == "127.0.0.1"));
    assert!(rows.iter().any(|(_, value)| value == "19090"));
    assert_eq!(fs::read_dir(path.join("generated")).unwrap().count(), 0);
}

#[test]
fn choices_reject_membership_endpoint_pin_and_secret_alias_conflicts() {
    let f = Fixture::new();
    let mut p = f.session();
    let mut network: Value = serde_json::from_slice(&fs::read(&f.network).unwrap()).unwrap();
    network["nodes"][0]["validatorId"] = json!(format!("0x{}", "d".repeat(40)));
    write_json(&f.network, &network);
    assert_eq!(
        p.node_choices().unwrap_err().code,
        "SETUP_PREPARATION_MATERIAL_INVALID"
    );
    // A public-chain selection must not open a credential also named in draft.
    p.set_field(
        "chainDescription",
        f.root.join("credential-0").to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(
        p.node_choices().unwrap_err().code,
        "SETUP_PREPARATION_PATH_CONFLICT"
    );
    assert_eq!(fs::read_dir(p.path().join("generated")).unwrap().count(), 0);
}

#[test]
fn native_import_creates_absolute_snapshot_and_tracks_original_identity() {
    let f = Fixture::new();
    let native = f.root.join("native.json");
    let generated = network::load(&f.source, &f.network, RuntimeOptions::default()).unwrap();
    let mut value: Value = serde_json::from_slice(&generated.native_bytes).unwrap();
    value["chainFile"] = json!("chain.json");
    value["dataDirectory"] = json!("data/node");
    write_json(&native, &value);
    let original = fs::read(&native).unwrap();
    let mut p = f.session();
    let mut inputs = p.inputs().clone();
    inputs.network = None;
    inputs.runtime_options = None;
    inputs.native = Some(native.clone());
    p.set_inputs(inputs).unwrap();
    p.prepare().unwrap();
    let snapshot: Value = serde_json::from_slice(&fs::read(p.native_path(1)).unwrap()).unwrap();
    assert!(Path::new(snapshot["chainFile"].as_str().unwrap()).is_absolute());
    assert_eq!(fs::read(&native).unwrap(), original);
    // Byte-identical source replacement must still invalidate reviewed identity.
    fs::rename(&native, f.root.join("old-native.json")).unwrap();
    write(&native, &original, 0o600);
    assert_eq!(
        p.check().unwrap_err().code,
        "SETUP_PREPARATION_INPUT_CHANGED"
    );
    let path = p.path().to_owned();
    drop(p);
    let mut p = Preparation::resume(&path).unwrap();
    assert!(p.draft().complete());
    assert!(p.check().is_err());
    p.prepare().unwrap();
    assert_eq!(p.summary().plan_revision, 2);
    f.assert_no_operations();
}

#[test]
fn changed_or_missing_materials_keep_recorded_answers_and_fail_without_source_paths() {
    let f = Fixture::new();
    let mut p = f.session();
    p.prepare().unwrap();
    fs::remove_file(&f.lock).unwrap();
    let failure = p.check().unwrap_err();
    assert_eq!(failure.code, "SETUP_PREPARATION_INPUT_CHANGED");
    assert!(!failure.message.contains(f.root.to_str().unwrap()));
    let path = p.path().to_owned();
    drop(p);
    let p = Preparation::resume(&path).unwrap();
    assert!(p.draft().complete());
    assert!(p.check().is_err());
    f.assert_no_operations();
}

#[test]
fn invalid_archive_and_unapproved_runtime_options_publish_no_snapshot() {
    let f = Fixture::new();
    let mut p = f.session();
    let mut inputs = p.inputs().clone();
    inputs.runtime_options = None;
    p.set_inputs(inputs).unwrap();
    assert_eq!(
        p.prepare().unwrap_err().code,
        "SETUP_PREPARATION_INCOMPLETE"
    );
    p.set_inputs(f.inputs()).unwrap();
    write(&f.archive, b"not an archive", 0o600);
    assert_eq!(
        p.prepare().unwrap_err().code,
        "SETUP_PREPARATION_MATERIAL_INVALID"
    );
    assert_eq!(fs::read_dir(p.path().join("generated")).unwrap().count(), 0);
    f.assert_no_operations();
}

#[test]
fn outputs_and_materials_cannot_consume_data_credentials_or_container() {
    let f = Fixture::new();
    let mut p = f.session();
    for path in [
        f.root.join("data"),
        f.root.join("credential-0"),
        p.path().join("generated"),
        f.archive.clone(),
    ] {
        let mut inputs = p.inputs().clone();
        inputs.destination = Some(path);
        assert!(p.set_inputs(inputs).is_err());
    }
    let mut inputs = p.inputs().clone();
    inputs.network = Some(f.root.join("credential-0"));
    assert!(p.set_inputs(inputs).is_err());
    assert!(
        p.set_field("dataDirectory", p.path().join("nested").to_str().unwrap())
            .is_err()
    );
    assert_eq!(fs::read_dir(p.path().join("generated")).unwrap().count(), 0);
    f.assert_no_operations();
}

#[test]
fn existing_operational_root_is_rejected_before_generated_write() {
    let f = Fixture::new();
    let mut p = f.session();
    fs::create_dir_all(f.root.join("data/node")).unwrap();
    assert_eq!(
        p.prepare().unwrap_err().code,
        "SETUP_PREPARATION_DESTINATION_EXISTS"
    );
    assert_eq!(fs::read_dir(p.path().join("generated")).unwrap().count(), 0);
}

#[test]
fn new_import_preflight_and_partial_resume_never_adopt_legacy_or_replaced_roots() {
    let f = Fixture::new();
    let forbidden = f.root.join("data/node/preparation");
    assert!(Preparation::validate_new(&forbidden, Some(&f.source)).is_err());
    assert!(!forbidden.exists());
    let path = f.root.join("preparation");
    let mut p = Preparation::create(&path, None).unwrap();
    p.set_field("instanceId", "saved-node").unwrap();
    assert!(Preparation::resume(&path).is_err()); // wizard lock held
    let moved = f.root.join("moved");
    fs::rename(&path, &moved).unwrap();
    symlink(&moved, &path).unwrap();
    assert!(p.set_field("httpPort", "18081").is_err());
    drop(p);
    assert!(Preparation::resume(&path).is_err());
    fs::remove_file(&path).unwrap();
    fs::rename(&moved, &path).unwrap();
    let p = Preparation::resume(&path).unwrap();
    assert!(
        p.review_rows()
            .iter()
            .any(|(_, value)| value == "saved-node")
    );
    drop(p);
    assert!(super::super::workflow::Workflow::resume(&path).is_err());
}

#[test]
fn runtime_answers_resume_individually_and_defaults_do_not_approve_remaining_policy() {
    let f = Fixture::new();
    let mut p = f.session();
    let mut inputs = p.inputs().clone();
    inputs.runtime_options = None;
    p.set_inputs(inputs).unwrap();
    p.set_runtime_answer(0, "jks").unwrap();
    p.set_runtime_answer(1, "PKCS12").unwrap();
    assert_eq!(p.inputs().runtime_answers["keyStoreType"], "JKS");
    assert!(p.inputs().runtime_options.is_none());
    let path = p.path().to_owned();
    drop(p);
    let mut p = Preparation::resume(&path).unwrap();
    assert_eq!(p.inputs().runtime_answers.len(), 2);
    assert!(p.inputs().runtime_options.is_none());
    assert!(p.set_runtime_answer(2, "guess").is_err());
    assert_eq!(p.inputs().runtime_answers.len(), 2);
    p.set_runtime_answer(2, "join-network").unwrap();
    p.set_runtime_answer(3, "no").unwrap();
    assert!(p.inputs().runtime_options.is_none());
    p.set_runtime_answer(4, "yes").unwrap();
    let options = p.inputs().runtime_options.as_ref().unwrap();
    assert_eq!(options.mode, network::NetworkMode::JoinNetwork);
    assert!(!options.consensus_auto_start);
    assert!(options.archive_enabled);
    p.prepare().unwrap();
    p.set_runtime_answer(4, "no").unwrap();
    assert_eq!(p.summary().preparation, "DRAFT");
    assert_eq!(p.inputs().runtime_answers["keyStoreType"], "JKS");
    assert_eq!(
        p.inputs().runtime_options.as_ref().unwrap().mode,
        network::NetworkMode::JoinNetwork
    );
}

#[test]
fn cli_prepares_verified_package_and_resume_edit_publishes_new_revision_only() {
    use std::io::Cursor;
    let f = Fixture::new();
    let home = f.root.join("home");
    let original = fs::read(&f.source).unwrap();
    let args = vec![
        "setup".into(),
        "new".into(),
        "local".into(),
        "--home".into(),
        home.to_str().unwrap().into(),
        "--from".into(),
        f.source.to_str().unwrap().into(),
    ];
    let answers = format!(
        "y\n2\n1\n{}\n2\n2\n2\ny\n2\n3\n{}\n3\n1\n{}\n1\nPKCS12\nPKCS12\nnew-network\nyes\nno\n5\ny\n0\n",
        f.archive.display(),
        f.lock.display(),
        f.network.display()
    );
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code =
        crate::cli::run_with_input(&args, &mut Cursor::new(answers), true, &mut out, &mut err);
    let text = format!(
        "{}{}",
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap()
    );
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("PREPARED_RECORDED"), "{text}");
    assert!(text.contains("설정 파일 준비를 마쳤습니다"), "{text}");
    assert!(
        text.contains("준비 당시 package의 CLI가 현재 도구와 다릅니다"),
        "{text}"
    );
    assert!(!text.contains(CANARY));
    assert!(!text.contains("credential-0"));
    let workspace = home.join("sessions/local");
    let p = Preparation::resume(&workspace).unwrap();
    p.check().unwrap();
    let old_product = fs::read(p.product_path(1)).unwrap();
    let old_native = fs::read(p.native_path(1)).unwrap();
    drop(p);
    let args = vec![
        "setup".into(),
        "resume".into(),
        "local".into(),
        "--home".into(),
        home.to_str().unwrap().into(),
    ];
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = crate::cli::run_with_input(
        &args,
        &mut Cursor::new("1\n5\n18081\n5\ny\n0\n"),
        true,
        &mut out,
        &mut err,
    );
    let text = format!(
        "{}{}",
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap()
    );
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("설정 파일 준비를 마쳤습니다"), "{text}");
    let p = Preparation::resume(&workspace).unwrap();
    p.check().unwrap();
    assert_eq!(p.summary().plan_revision, 2);
    assert_eq!(fs::read(p.product_path(1)).unwrap(), old_product);
    assert_eq!(fs::read(p.native_path(1)).unwrap(), old_native);
    assert_eq!(fs::read(&f.source).unwrap(), original);
    for path in [
        home.join("packages"),
        home.join("instances"),
        f.root.join("data"),
    ] {
        assert!(!path.exists());
    }
}

#[test]
fn failed_generation_consumes_revision_without_claiming_preparation_or_overwriting() {
    let f = Fixture::new();
    let network = fs::read(&f.network).unwrap();
    let mut p = f.session();
    write_json(
        &f.network,
        &json!({"schemaVersion":1,"kind":"BXDL_QBFT_NETWORK","nodes":[]}),
    );
    assert!(p.prepare().is_err());
    assert_eq!(p.summary().preparation, "DRAFT");
    assert_eq!(p.summary().plan_revision, 1);
    let first = fs::read(p.product_path(1)).unwrap();
    assert!(!p.native_path(1).exists());
    write(&f.network, &network, 0o600);
    p.prepare().unwrap();
    assert_eq!(p.summary().plan_revision, 2);
    assert_eq!(fs::read(p.product_path(1)).unwrap(), first);
    assert!(!p.native_path(1).exists());
    p.check().unwrap();
    f.assert_no_operations();
}

#[test]
fn stale_checkpoint_and_review_corruption_are_rejected_without_repair() {
    let f = Fixture::new();
    let mut p = f.session();
    p.prepare().unwrap();
    let path = p.path().to_owned();
    let mut store = store::Store::open(&path.join("workflow")).unwrap();
    let mut journal: Value = serde_json::from_slice(&store.read().unwrap().unwrap()).unwrap();
    journal["review"]["cliMatch"] = json!("MATCH");
    store.save(&serde_json::to_vec(&journal).unwrap()).unwrap();
    assert!(p.set_field("httpPort", "18081").is_err());
    drop(p);
    assert!(Preparation::resume(&path).is_err());
    assert_eq!(
        store.read().unwrap().unwrap(),
        serde_json::to_vec(&journal).unwrap()
    );
    f.assert_no_operations();
}
