//! Setup safety at the public Session and CLI boundaries. All referenced
//! material is temporary fake content; no engine, keystore or DB is opened.
#![cfg(unix)]

use bxdl::{cli, setup::Session};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Cursor,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

struct Fixture {
    _temporary: TempDir,
    root: PathBuf,
    source: PathBuf,
}

fn write(path: &Path, bytes: &[u8], mode: u32) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn directory(path: &Path) {
    fs::create_dir(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn fixture(with_references: bool) -> Fixture {
    let temporary = tempfile::tempdir().unwrap();
    // Avoid macOS /var and /tmp aliases in tests of product path policy.
    let root = fs::canonicalize(temporary.path()).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let source = root.join("source.json");
    let config = json!({
        "schemaVersion": 1, "instanceId": "node-a", "role": "validator", "nodeId": "node-a",
        "chainDescription": "chain.json",
        "storage": {"backend": "rocksdb", "dataDirectory": "Data"},
        "secrets": {
            "validatorKeystore": "secrets/validator.p12",
            "validatorPasswordFile": "secrets/validator-password",
            "tlsKeyStore": "secrets/tls.p12",
            "tlsKeyPasswordFile": "secrets/tls-password",
            "tlsTrustStore": "secrets/trust.p12",
            "tlsTrustPasswordFile": "secrets/trust-password"
        },
        "http": {"address": "127.0.0.1", "port": 18080},
        "p2p": {"address": "127.0.0.1", "port": 19090}
    });
    write(&source, &serde_json::to_vec(&config).unwrap(), 0o600);
    if with_references {
        directory(&root.join("Data"));
        directory(&root.join("secrets"));
        write(&root.join("chain.json"), b"fake chain description", 0o644);
        for name in [
            "validator.p12",
            "validator-password",
            "tls.p12",
            "tls-password",
            "trust.p12",
            "trust-password",
        ] {
            write(
                &root.join("secrets").join(name),
                b"FAKE_PRIVATE_CONTENT_CANARY",
                0o600,
            );
        }
    }
    Fixture {
        _temporary: temporary,
        root,
        source,
    }
}

#[test]
fn missing_chain_or_secret_ancestor_cannot_be_created_as_a_workspace_parent() {
    for reference in [
        "chain.json",
        "secrets/validator.p12",
        "secrets/validator-password",
    ] {
        let f = fixture(false);
        let original = fs::read(&f.source).unwrap();
        let referenced_path = f.root.join(reference);
        let workspace = referenced_path.join("setup");
        assert!(Session::create(&workspace, Some(&f.source)).is_err());
        assert!(
            !referenced_path.exists(),
            "created referenced path {reference}"
        );
        assert!(!workspace.exists());
        assert!(!f.root.join("secrets").exists());
        assert_eq!(fs::read(&f.source).unwrap(), original);
        assert_eq!(fs::read_dir(&f.root).unwrap().count(), 1);
    }
}

#[test]
fn stale_session_cannot_preflight_or_export_after_another_writer_advances() {
    let f = fixture(false);
    let workspace = f.root.join("setup");
    let first = Session::create(&workspace, Some(&f.source)).unwrap();
    let mut second = Session::resume(&workspace).unwrap();
    second.set("nodeId", "node-b").unwrap();
    let output = f.root.join("stale-output.json");
    assert_eq!(first.preflight().unwrap_err().code, "SETUP_CONFLICT");
    assert_eq!(first.export(&output).unwrap_err().code, "SETUP_CONFLICT");
    assert!(!output.exists());
    // A freshly resumed session observes the committed value, proving this was
    // a revision conflict rather than a generally invalid imported config.
    let current = Session::resume(&workspace).unwrap();
    let config: Value = serde_json::from_slice(&current.config_bytes().unwrap()).unwrap();
    assert_eq!(config["nodeId"], "node-b");
}

#[test]
fn replacing_workspace_inode_invalidates_the_loaded_session_without_output() {
    let f = fixture(false);
    let workspace = f.root.join("setup");
    let session = Session::create(&workspace, Some(&f.source)).unwrap();
    let original = f.root.join("original-setup");
    fs::rename(&workspace, &original).unwrap();
    directory(&workspace);
    let output = f.root.join("replaced-output.json");
    assert_eq!(session.preflight().unwrap_err().code, "SETUP_STATE_UNSAFE");
    assert_eq!(
        session.export(&output).unwrap_err().code,
        "SETUP_STATE_UNSAFE"
    );
    assert!(!output.exists());
    assert_eq!(fs::read_dir(&workspace).unwrap().count(), 0);
    assert!(original.join("revision00000001.json").is_file());
}

#[cfg(target_os = "macos")]
fn same_inode(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

#[test]
#[cfg(target_os = "macos")]
fn mac_case_aliases_cannot_export_into_future_history_or_existing_data() {
    let f = fixture(true);
    let workspace = f.root.join("Workspace");
    let session = Session::create(&workspace, Some(&f.source)).unwrap();
    let workspace_alias = f.root.join("workspace");
    let data = f.root.join("Data");
    let data_alias = f.root.join("dATA");
    // A case-sensitive volume does not provide the aliases under test. Never
    // assume the filesystem behavior merely from the host OS name.
    if !same_inode(&workspace, &workspace_alias) || !same_inode(&data, &data_alias) {
        return;
    }
    for output in [
        workspace_alias.join("revision00000002.json"),
        workspace_alias.join("REVISION00000002.JSON"),
        data_alias.join("exported-config.json"),
    ] {
        assert_eq!(
            session.export(&output).unwrap_err().code,
            "SETUP_OUTPUT_CONFLICT"
        );
        assert!(!output.exists());
    }
    assert!(Session::resume(&workspace).unwrap().complete());
    assert_eq!(fs::read_dir(&data).unwrap().count(), 0);
}

#[test]
fn cli_export_reports_written_config_metadata_and_exact_output_hash() {
    let f = fixture(true);
    let workspace = f.root.join("setup");
    let output = f.root.join("exported-config.json");
    let args = [
        "setup",
        "--workspace",
        workspace.to_str().unwrap(),
        "--from",
        f.source.to_str().unwrap(),
        "--non-interactive",
        "--output",
        output.to_str().unwrap(),
        "--json",
    ]
    .map(str::to_owned);
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let exit = cli::run_with_input(
        &args,
        &mut Cursor::new(b"MUST_NOT_BE_READ"),
        false,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&stdout));
    assert!(stderr.is_empty());
    let report: Value = serde_json::from_slice(&stdout).expect("one JSON envelope");
    assert_eq!(report["reasonCode"], "SETUP_CONFIG_WRITTEN");
    assert_eq!(report["data"]["configPath"], output.to_str().unwrap());
    assert_eq!(report["data"]["installation"], "NOT_PERFORMED");
    assert_eq!(report["data"]["engineValidation"], "NOT_CHECKED");
    let preflight = &report["data"]["preflight"];
    assert_eq!(preflight["outcome"], "INCOMPLETE");
    let config_check = preflight["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "configMetadata")
        .unwrap();
    assert_eq!(config_check["status"], "PASS");
    assert_ne!(config_check["reasonCode"], "DRAFT_NOT_WRITTEN");
    assert_eq!(
        preflight["configSha256"],
        hex::encode(Sha256::digest(fs::read(&output).unwrap()))
    );
    assert!(
        !String::from_utf8(stdout)
            .unwrap()
            .contains("FAKE_PRIVATE_CONTENT_CANARY")
    );
}
