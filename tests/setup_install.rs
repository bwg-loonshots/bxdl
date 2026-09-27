//! Public installation-wizard integration with signed throwaway packages.
//! Java is a bounded shell fixture. No JVM, database, network or launchd starts.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use bxdl::{
    artifact, cli,
    setup::workflow::{self, Workflow},
};
use ed25519_dalek::{
    SigningKey,
    pkcs8::{EncodePrivateKey, EncodePublicKey},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Cursor,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

const CANARY: &str = "PRIVATE_SETUP_INSTALL_CANARY";
const Q: &str = "nigo.protocol.consensus.qbft.node.";

// These scenarios share one libtest process. Another scenario's fork can
// transiently inherit a workflow flock before exec closes CLOEXEC descriptors,
// even after its owning scenario drops the last local handle. Serialize the
// unrelated scenarios, not the explicit same-workspace lock checks below.
static SCENARIOS: Mutex<()> = Mutex::new(());

fn scenario() -> MutexGuard<'static, ()> {
    SCENARIOS.lock().expect("another setup scenario failed")
}

fn hash(raw: &[u8]) -> String {
    hex::encode(Sha256::digest(raw))
}
fn write(path: &Path, raw: &[u8], mode: u32) {
    fs::write(path, raw).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn json_file(path: &Path, value: &Value) {
    write(path, &serde_json::to_vec(value).unwrap(), 0o600);
}
fn path(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    workspace: PathBuf,
    source: PathBuf,
    archive: PathBuf,
    public_key: PathBuf,
    lock: PathBuf,
    native: PathBuf,
    destination: PathBuf,
    instance: PathBuf,
    data: PathBuf,
    calls: PathBuf,
}

impl Fixture {
    fn new(same_cli: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let workspace = root.join("setup workspace");
        let source = root.join("source-product.json");
        let native = root.join("native-node.json");
        let data = root.join("new data");
        let calls = root.join("fixture-calls");
        let identity: Value = serde_json::from_slice(include_bytes!(
            "../contracts/nigo/development-clean-2026-09-18/evidence/engine-info.json"
        ))
        .unwrap();
        let cold = json!({
            "command":"preflight","status":"INCOMPLETE","reason":"RUNTIME_CHECKS_REQUIRED",
            "contractStatus":"PROPOSED","backend":"rocksdb","chainFingerprint":"c".repeat(64),
            "nodeIdentity":format!("0x{}:0x{}", "a".repeat(64), "b".repeat(40)),
            "checks":[{"check":"CONFIGURATION","status":"PASS"},{"check":"KEY_MATERIAL","status":"PASS"},
                {"check":"DATABASE_AND_WAL","status":"NOT_CHECKED","reason":"REQUIRES_EXCLUSIVE_OPEN"},
                {"check":"PORTS_AND_PEERS","status":"NOT_CHECKED","reason":"NETWORK_NOT_ACCESSED"},
                {"check":"NATIVE_RUNTIME","status":"NOT_CHECKED","reason":"REQUIRES_RUNTIME_LOAD"}]
        });
        // Absolute, test-owned marker; all interpolated JSON and paths are fixture-controlled.
        let java = format!(
            "#!/bin/sh\nfor argument in \"$@\"; do\ncase \"$argument\" in\nengine-info) printf 'engine-info\\n' >> '{}'; printf '%s\\n' '{}'; exit 0;;\npreflight) printf 'preflight\\n' >> '{}'; printf '%s\\n' '{}'; exit 3;;\ninit|resume-init|run) printf '%s\\n' \"$argument\" >> '{}'; printf '%s\\n' '{{\"status\":\"FAILED\",\"reason\":\"PRECONDITION_OR_IO_FAILURE\"}}'; exit 74;;\nesac\ndone\nexit 77\n",
            calls.display(),
            identity,
            calls.display(),
            cold,
            calls.display()
        );
        let jar = b"fake development JAR; never executed";
        let lock = root.join("trusted-engine.lock.json");
        json_file(
            &lock,
            &json!({"schemaVersion":1,"jarSha256":hash(jar),"jarSizeBytes":jar.len(),
            "javaSha256":hash(java.as_bytes()),"expected":identity}),
        );
        json_file(
            &root.join("chain.json"),
            &json!({"nigo.protocol.chain-id":"11578",
            "nigo.protocol.consensus.protocol":"QBFT","nigo.protocol.consensus.profile-id":"TEST_QBFT"}),
        );
        let mut product: Value = serde_json::from_slice(include_bytes!(
            "../config/examples/instance.development.json"
        ))
        .unwrap();
        product["nodeId"] = json!(format!("0x{}", "a".repeat(64)));
        product["chainDescription"] = json!("chain.json");
        product["storage"]["dataDirectory"] = json!(path(&data));
        let mut node = json!({"server.address":"127.0.0.1","server.port":"18080"});
        for (key, value) in [
            ("node-id", format!("0x{}", "a".repeat(64))),
            ("validator-id", format!("0x{}", "b".repeat(40))),
            ("role", "VALIDATOR".into()),
            ("transport-security-scheme", "MTLS".into()),
            ("listen-host", "192.0.2.10".into()),
            ("listen-port", "19090".into()),
        ] {
            node[format!("{Q}{key}")] = json!(value);
        }
        for (i, (product_key, property)) in [
            ("validatorKeystore", "keystore-path"),
            ("validatorPasswordFile", "keystore-password-file"),
            ("tlsKeyStore", "mtls-key-store-path"),
            ("tlsKeyPasswordFile", "mtls-key-store-password-file"),
            ("tlsTrustStore", "mtls-trust-store-path"),
            ("tlsTrustPasswordFile", "mtls-trust-store-password-file"),
        ]
        .iter()
        .enumerate()
        {
            let name = format!("private-credential-{i}");
            write(&root.join(&name), CANARY.as_bytes(), 0o600);
            product["secrets"][product_key] = json!(name);
            node[format!("{Q}{property}")] = json!(name);
        }
        json_file(&source, &product);
        json_file(
            &native,
            &json!({"chainFile":"chain.json","dataDirectory":path(&data),"backend":"rocksdb","node":node}),
        );
        let stage = root.join("stage");
        for dir in ["bin", "engine", "runtime/bin", "licenses"] {
            fs::create_dir_all(stage.join(dir)).unwrap();
        }
        let executable = if same_cli {
            fs::read(std::env::current_exe().unwrap()).unwrap()
        } else {
            b"wrong test CLI".to_vec()
        };
        write(&stage.join("bin/bxdl"), &executable, 0o755);
        write(&stage.join("engine/nigo-node.jar"), jar, 0o644);
        write(&stage.join("runtime/bin/java"), java.as_bytes(), 0o755);
        write(
            &stage.join("licenses/THIRD_PARTY_NOTICES"),
            b"test only",
            0o644,
        );
        write(&stage.join("licenses/SBOM.json"), b"{}", 0o644);
        let spec = root.join("spec.json");
        json_file(
            &spec,
            &json!({"schemaVersion":1,"kind":"bxdl-package",
            "product":{"name":"BXDL","version":"0.1.0-dev","revision":"development"},"channel":"development",
            "platform":{"os":"darwin","arch":"arm64","libc":"none","minGlibc":"none","javaMajor":21,"backend":"rocksdb"},
            "engine":{"revision":identity["source"]["commit"],"jarSha256":hash(jar),"contractStatus":"proposed","contractRevision":identity["contract"]["fingerprint"]},
            "runtime":{"vendor":"test-only","version":"21-test","javaSha256":hash(java.as_bytes())}}),
        );
        let key = SigningKey::from_bytes(&[83; 32]);
        let private_key = root.join("throwaway-signing.pem");
        let public_key = root.join("trusted-public.pem");
        write(
            &private_key,
            key.to_pkcs8_pem(Default::default()).unwrap().as_bytes(),
            0o600,
        );
        write(
            &public_key,
            key.verifying_key()
                .to_public_key_pem(Default::default())
                .unwrap()
                .as_bytes(),
            0o644,
        );
        let archive = root.join("signed-package.tar.gz");
        artifact::build(&artifact::BuildOptions {
            root: stage,
            spec_path: spec,
            output: archive.clone(),
            signing_key_path: Some(private_key),
            allow_unsigned_development: false,
        })
        .unwrap();
        Self {
            _temp: temp,
            root: root.clone(),
            workspace,
            source,
            archive,
            public_key,
            lock,
            native,
            destination: root.join("installed package"),
            instance: root.join("instance control"),
            data,
            calls,
        }
    }
    fn inputs(&self, tail: &str) -> String {
        [
            path(&self.archive),
            "signed".into(),
            path(&self.public_key),
            path(&self.lock),
            path(&self.native),
            path(&self.destination),
            path(&self.instance),
            tail.to_owned(),
        ]
        .join("\n")
    }
    fn create(&self) -> Workflow {
        Workflow::create(&self.workspace, Some(&self.source)).unwrap()
    }
    fn no_outputs(&self) {
        assert!(!self.destination.exists());
        assert!(!self.instance.exists());
        assert!(!self.data.exists());
        assert!(!self.calls.exists());
    }
    fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.calls)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

fn interact(workflow: &mut Workflow, text: &str) -> (bool, String) {
    let mut output = Vec::new();
    let finished = workflow::interact(workflow, &mut Cursor::new(text), &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(!output.contains(CANARY));
    assert!(!output.contains("private-credential-"));
    assert!(!output.contains("BEGIN PRIVATE KEY"));
    (finished, output)
}

#[test]
fn planned_install_pauses_and_resumes_without_creating_customer_outputs() {
    let _scenario = scenario();
    let f = Fixture::new(true);
    let source = fs::read(&f.source).unwrap();
    let mut workflow = f.create();
    let (finished, text) = interact(&mut workflow, &f.inputs(":cancel\n"));
    assert!(!finished);
    assert!(text.contains("설치 계획 #1"));
    assert_eq!(workflow.summary().plan_revision, 1);
    f.no_outputs();
    assert_eq!(
        Workflow::resume(&f.workspace).err().unwrap().code,
        "SETUP_WORKFLOW_BUSY"
    );
    drop(workflow);
    let mut resumed = Workflow::resume(&f.workspace).unwrap();
    assert!(interact(&mut resumed, "finish\n").0);
    assert_eq!(resumed.summary().plan_revision, 1);
    assert_eq!(resumed.summary().runtime_readiness, "NOT_CHECKED");
    assert_eq!(source, fs::read(&f.source).unwrap());
    f.no_outputs();
}

#[test]
fn apply_registers_once_and_declined_init_stays_uninitialized_after_resume() {
    let _scenario = scenario();
    let f = Fixture::new(true);
    let mut workflow = f.create();
    assert!(interact(&mut workflow, &f.inputs("apply\ny\ninit\nn\nfinish\n")).0);
    let summary = workflow.summary();
    assert_eq!(summary.installation, "CONFIRMED");
    assert_eq!(summary.registration, "CONFIRMED");
    assert_eq!(summary.initialization, "NOT_STARTED");
    assert_eq!(summary.global_consensus, "NOT_CHECKED");
    assert!(!f.data.exists());
    let calls = f.calls();
    assert_eq!(calls, ["engine-info", "preflight"]);
    let receipt = fs::read(f.destination.join(".bxdl-install.json")).unwrap();
    drop(workflow);
    let mut resumed = Workflow::resume(&f.workspace).unwrap();
    assert!(interact(&mut resumed, "finish\n").0);
    assert_eq!(resumed.summary().initialization, "NOT_STARTED");
    assert_eq!(f.calls(), calls);
    assert_eq!(
        receipt,
        fs::read(f.destination.join(".bxdl-install.json")).unwrap()
    );
    assert!(!f.data.exists());
}

#[test]
fn cli_mismatch_is_rejected_before_fourteen_answers_or_installation() {
    let _scenario = scenario();
    let f = Fixture::new(false);
    let mut workflow = Workflow::create(&f.workspace, None).unwrap();
    let (_, output) = interact(&mut workflow, &f.inputs("finish\n"));
    assert!(output.contains("SETUP_CLI_MISMATCH"));
    assert!(!output.contains("[1/14]"));
    assert_eq!(workflow.summary().plan_revision, 0);
    f.no_outputs();
}

#[test]
fn changed_credential_invalidates_plan_and_never_reaches_engine_or_install() {
    let _scenario = scenario();
    let f = Fixture::new(true);
    let mut workflow = f.create();
    interact(&mut workflow, &f.inputs(":cancel\n"));
    drop(workflow);
    write(
        &f.root.join("private-credential-0"),
        b"ROTATED_PRIVATE_CANARY",
        0o600,
    );
    let mut resumed = Workflow::resume(&f.workspace).unwrap();
    let (_, output) = interact(&mut resumed, "apply\ny\nfinish\n");
    assert!(output.contains("ENGINE_INPUT_CHANGED"));
    assert!(!output.contains("ROTATED_PRIVATE_CANARY"));
    assert_eq!(resumed.summary().installation, "NOT_STARTED");
    f.no_outputs();
}

#[test]
fn declined_apply_and_eof_confirmation_do_not_authorize_writes() {
    let _scenario = scenario();
    let f = Fixture::new(true);
    let mut workflow = f.create();
    let (finished, _) = interact(&mut workflow, &f.inputs("apply\nn\napply\n"));
    assert!(!finished);
    assert_eq!(workflow.summary().installation, "NOT_STARTED");
    f.no_outputs();
}

#[test]
fn existing_data_is_never_adopted_or_cleared_by_planning() {
    let _scenario = scenario();
    let f = Fixture::new(true);
    fs::create_dir(&f.data).unwrap();
    fs::set_permissions(&f.data, fs::Permissions::from_mode(0o700)).unwrap();
    write(&f.data.join("existing-ledger-sentinel"), b"KEEP", 0o600);
    let mut workflow = f.create();
    let (_, output) = interact(&mut workflow, &f.inputs("apply\ny\nfinish\n"));
    assert!(output.contains("SETUP_TARGET_EXISTS"));
    assert_eq!(
        fs::read(f.data.join("existing-ledger-sentinel")).unwrap(),
        b"KEEP"
    );
    assert!(!f.destination.exists());
    assert!(!f.instance.exists());
    assert!(!f.calls.exists());
}

#[test]
fn failed_fake_init_stays_unknown_and_resume_does_not_retry_or_start() {
    let _scenario = scenario();
    let f = Fixture::new(true);
    let mut workflow = f.create();
    let (_, output) = interact(&mut workflow, &f.inputs("apply\ny\ninit\ny\n:cancel\n"));
    assert!(output.contains("UNKNOWN"));
    assert_eq!(workflow.summary().initialization, "UNKNOWN");
    assert!(f.data.is_dir());
    assert_eq!(fs::read_dir(&f.data).unwrap().count(), 0);
    let calls = f.calls();
    assert_eq!(
        calls
            .iter()
            .filter(|command| command.as_str() == "init")
            .count(),
        1
    );
    assert!(
        !calls
            .iter()
            .any(|command| matches!(command.as_str(), "run" | "resume-init"))
    );
    drop(workflow);
    let mut resumed = Workflow::resume(&f.workspace).unwrap();
    interact(&mut resumed, "finish\n");
    assert_eq!(resumed.summary().initialization, "UNKNOWN");
    assert_eq!(f.calls(), calls);
}

#[test]
fn cli_install_mode_is_explicitly_interactive_and_pause_is_exit_five() {
    let _scenario = scenario();
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let workspace = root.join("workspace");
    let base = vec![
        "setup".into(),
        "--install".into(),
        "--workspace".into(),
        path(&workspace),
    ];
    for extra in [
        vec!["--json"],
        vec!["--non-interactive"],
        vec!["--output", "private-canary"],
    ] {
        let mut args = base.clone();
        args.extend(extra.into_iter().map(str::to_owned));
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        assert_eq!(
            cli::run_with_input(&args, &mut Cursor::new(b""), true, &mut stdout, &mut stderr),
            2
        );
        assert!(!workspace.exists());
        assert!(!String::from_utf8_lossy(&stdout).contains("private-canary"));
        assert!(!String::from_utf8_lossy(&stderr).contains("private-canary"));
    }
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    assert_eq!(
        cli::run_with_input(
            &base,
            &mut Cursor::new(b""),
            false,
            &mut stdout,
            &mut stderr
        ),
        2
    );
    assert!(!workspace.exists());
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    assert_eq!(
        cli::run_with_input(
            &base,
            &mut Cursor::new(b":cancel\n"),
            true,
            &mut stdout,
            &mut stderr
        ),
        5
    );
    assert!(workspace.is_dir());
    assert!(String::from_utf8_lossy(&stderr).contains("SETUP_INSTALL_PAUSED"));
    let saved = Workflow::resume(&workspace).unwrap();
    assert_eq!(saved.summary().plan_revision, 0);
    assert_eq!(saved.summary().initialization, "NOT_STARTED");
}

fn remove_registration_confirmation(f: &Fixture) -> PathBuf {
    let mut checkpoints = fs::read_dir(f.workspace.join("workflow"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("revision")
        })
        .collect::<Vec<_>>();
    checkpoints.sort();
    let final_path = checkpoints.pop().unwrap();
    let final_state: Value = serde_json::from_slice(&fs::read(&final_path).unwrap()).unwrap();
    assert_eq!(final_state["registration"]["phase"], "CONFIRMED");
    let intent_path = checkpoints.pop().unwrap();
    let intent: Value = serde_json::from_slice(&fs::read(&intent_path).unwrap()).unwrap();
    assert_eq!(intent["registration"]["phase"], "IN_PROGRESS");
    assert!(intent["registration"]["root"].is_object());
    // Simulate the workflow completion checkpoint never being published. The
    // installer and registration really ran, including the reservation callback.
    fs::remove_file(final_path).unwrap();
    intent_path
}

#[test]
fn completed_owned_registration_survives_missing_workflow_completion_without_reexecution() {
    let _scenario = scenario();
    let f = Fixture::new(true);
    let mut workflow = f.create();
    interact(&mut workflow, &f.inputs("apply\ny\nfinish\n"));
    let calls = f.calls();
    drop(workflow);
    remove_registration_confirmation(&f);
    let mut resumed = Workflow::resume(&f.workspace).unwrap();
    assert_eq!(resumed.summary().registration, "UNKNOWN");
    interact(&mut resumed, "finish\n");
    assert_eq!(resumed.summary().registration, "CONFIRMED");
    assert_eq!(resumed.summary().initialization, "NOT_STARTED");
    assert_eq!(f.calls(), calls);
    assert!(!f.data.exists());
}

#[test]
fn matching_registration_without_creation_evidence_is_not_adopted() {
    let _scenario = scenario();
    let f = Fixture::new(true);
    let mut workflow = f.create();
    interact(&mut workflow, &f.inputs("apply\ny\nfinish\n"));
    let calls = f.calls();
    let binding = fs::read(f.instance.join("binding.json")).unwrap();
    drop(workflow);
    let intent_path = remove_registration_confirmation(&f);
    let mut intent: Value = serde_json::from_slice(&fs::read(&intent_path).unwrap()).unwrap();
    intent["registration"]
        .as_object_mut()
        .unwrap()
        .remove("root");
    json_file(&intent_path, &intent);
    let mut resumed = Workflow::resume(&f.workspace).unwrap();
    let (_, text) = interact(&mut resumed, "apply\ny\ninit\ny\nfinish\n");
    assert!(text.contains("SETUP_OPERATION_UNKNOWN"));
    assert_eq!(resumed.summary().registration, "UNKNOWN");
    assert_eq!(f.calls(), calls);
    assert_eq!(binding, fs::read(f.instance.join("binding.json")).unwrap());
    assert!(!f.data.exists());
}

#[test]
fn cancel_at_apply_confirmation_exits_without_consuming_later_commands() {
    let _scenario = scenario();
    let f = Fixture::new(true);
    let mut workflow = f.create();
    let (finished, _) = interact(
        &mut workflow,
        &f.inputs("apply\n:cancel\napply\ny\nfinish\n"),
    );
    assert!(!finished);
    f.no_outputs();
}
