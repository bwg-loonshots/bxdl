use super::*;
use serde_json::{Value, json};
use std::{
    fs,
    io::Cursor,
    os::unix::fs::{PermissionsExt, symlink},
};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    product: PathBuf,
    network: PathBuf,
    chain: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let chain = root.join("chain.json");
        write(
            &chain,
            &json!({"nigo.protocol.consensus.protocol":"QBFT", "nigo.protocol.consensus.qbft.validators[0]":format!("0x{}", "2".repeat(40))}),
        );
        let secrets = root.join("secrets");
        fs::create_dir(&secrets).unwrap();
        fs::set_permissions(&secrets, fs::Permissions::from_mode(0o700)).unwrap();
        let mut secret_refs = serde_json::Map::new();
        for name in [
            "validatorKeystore",
            "validatorPasswordFile",
            "tlsKeyStore",
            "tlsKeyPasswordFile",
            "tlsTrustStore",
            "tlsTrustPasswordFile",
        ] {
            let path = secrets.join(format!("PRIVATE_PATH_CANARY-{name}"));
            fs::write(&path, b"PRIVATE_CONTENT_CANARY").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            secret_refs.insert(name.into(), json!(path));
        }
        let product = root.join("instance.json");
        write(
            &product,
            &json!({"schemaVersion":1,"instanceId":"validator-one","role":"validator","nodeId":format!("0x{}","1".repeat(64)),"chainDescription":chain,"storage":{"backend":"rocksdb","dataDirectory":root.join("data")},"secrets":secret_refs,"http":{"address":"127.0.0.1","port":18080},"p2p":{"address":"127.0.0.1","port":19090}}),
        );
        let network = root.join("network.json");
        write(
            &network,
            &json!({"schemaVersion":1,"kind":"BXDL_QBFT_NETWORK","nodes":[{"nodeId":format!("0x{}","1".repeat(64)),"validatorId":format!("0x{}","2".repeat(40)),"host":"127.0.0.1","port":19090,"transactionPermission":"NONE","credentialFingerprints":[format!("0x{}","3".repeat(64))]}]}),
        );
        Self {
            _temp: temp,
            root,
            product,
            network,
            chain,
        }
    }
    fn session(&self) -> Session {
        Session::create(&self.root.join("prepare"), &self.product, &self.network).unwrap()
    }
}
fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn complete(session: &mut Session) {
    for field in FIELDS {
        session.set(field.name, field.default.unwrap()).unwrap();
    }
}

#[test]
fn native_export_requires_explicit_options_and_stays_private_without_runtime_work() {
    let f = Fixture::new();
    let mut session = f.session();
    let output = f.root.join("node.json");
    assert_eq!(
        session.export(&output).unwrap_err().code,
        "SETUP_NETWORK_INCOMPLETE"
    );
    complete(&mut session);
    let summary = serde_json::to_string(&session.summary()).unwrap();
    assert!(summary.contains("NOT_CHECKED"));
    assert!(!summary.contains("PRIVATE_PATH_CANARY"));
    assert!(!summary.contains("PRIVATE_CONTENT_CANARY"));
    assert!(!summary.contains("sha256"));
    assert_eq!(session.export(&output).unwrap(), output);
    assert_eq!(
        fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!f.root.join("data").exists());
    let native: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(native["backend"], "rocksdb");
    assert_eq!(native["chainFile"], json!(f.chain));
    assert!(
        !fs::read_to_string(&output)
            .unwrap()
            .contains("PRIVATE_CONTENT_CANARY")
    );
    assert_eq!(session.export(&output).unwrap_err().code, "OUTPUT_EXISTS");
}

#[test]
fn cancel_and_eof_save_each_choice_and_resume_without_export() {
    let f = Fixture::new();
    let mut session = f.session();
    let mut out = Vec::new();
    let result = interact(
        &mut session,
        &mut Cursor::new(b"JKS\nPKCS12\n:cancel\n"),
        &mut out,
        None,
    )
    .unwrap();
    assert!(matches!(result, Finish::Paused));
    assert_eq!(session.summary().completed_fields.len(), 2);
    assert!(
        !String::from_utf8(out)
            .unwrap()
            .contains("PRIVATE_PATH_CANARY")
    );
    drop(session);
    let mut session = Session::resume(&f.root.join("prepare")).unwrap();
    assert_eq!(session.next_index(), 2);
    let result = interact(
        &mut session,
        &mut Cursor::new(b"join-network\n"),
        &mut Vec::new(),
        None,
    )
    .unwrap();
    assert!(matches!(result, Finish::Paused));
    assert_eq!(session.next_index(), 3);
    assert!(!f.root.join("prepare-node.json").exists());
    assert!(!f.root.join("data").exists());
}

#[test]
fn export_is_confirmed_after_safe_summary_and_can_be_declined() {
    let f = Fixture::new();
    let mut session = f.session();
    let output = f.root.join("node.json");
    let mut out = Vec::new();
    let result = interact(
        &mut session,
        &mut Cursor::new(b"\n\n\n\n\nexport\nn\nsave\n"),
        &mut out,
        Some(&output),
    )
    .unwrap();
    assert!(matches!(result, Finish::Saved));
    assert!(!output.exists());
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("공개 validator ID"));
    assert!(text.contains("거래 수신 정책: NONE"));
    assert!(text.contains("키·인증서 실제 identity: 미검사"));
    assert!(!text.contains("PRIVATE_PATH_CANARY"));
    let result = interact(
        &mut session,
        &mut Cursor::new(b"export\ny\n"),
        &mut Vec::new(),
        Some(&output),
    )
    .unwrap();
    assert!(matches!(result, Finish::Exported(path) if path == output));
    assert!(output.exists());
}

#[test]
fn workspace_and_output_overlap_are_refused_before_creation() {
    let f = Fixture::new();
    let unsafe_workspace = f.root.join("data/prepare");
    assert!(Session::create(&unsafe_workspace, &f.product, &f.network).is_err());
    assert!(!f.root.join("data").exists());
    let mut session = f.session();
    complete(&mut session);
    for output in [
        &f.product,
        &f.network,
        &f.chain,
        &f.root.join("data/node.json"),
        &f.root.join("prepare/node.json"),
    ] {
        assert!(session.export(output).is_err());
    }
    let alias = f.root.join("product-hardlink.json");
    fs::hard_link(&f.product, &alias).unwrap();
    assert!(session.export(&alias).is_err());
    let link = f.root.join("link");
    symlink(&f.root, &link).unwrap();
    assert!(session.export(&link.join("node.json")).is_err());
    assert!(!f.root.join("node.json").exists());
    assert!(!f.root.join("data").exists());
}

#[test]
fn changed_public_document_invalidates_resume_and_export_even_if_semantics_match() {
    let f = Fixture::new();
    let mut session = f.session();
    complete(&mut session);
    let mut raw = fs::read(&f.chain).unwrap();
    raw.push(b'\n');
    fs::write(&f.chain, raw).unwrap();
    assert!(session.export(&f.root.join("node.json")).is_err());
    assert!(!f.root.join("node.json").exists());
    drop(session);
    assert!(Session::resume(&f.root.join("prepare")).is_err());
}

#[test]
fn invalid_answers_are_not_saved_and_relative_base_survives_resume() {
    let f = Fixture::new();
    let mut session = f.session();
    let base = session.draft.input_base.clone();
    assert!(
        session
            .set("archiveEnabled", "PRIVATE_CONTENT_CANARY")
            .is_err()
    );
    assert!(!session.has_answer("archiveEnabled"));
    assert_eq!(
        session.resolve(Path::new("relative-node.json")).unwrap(),
        base.join("relative-node.json")
    );
    drop(session);
    let session = Session::resume(&f.root.join("prepare")).unwrap();
    assert_eq!(session.next_index(), 0);
    assert_eq!(
        session.resolve(Path::new("relative-node.json")).unwrap(),
        base.join("relative-node.json")
    );
}

#[test]
fn checkpoint_rejects_duplicate_answers_and_other_setup_kinds() {
    let f = Fixture::new();
    let mut session = f.session();
    assert!(super::super::Session::resume(&f.root.join("prepare")).is_err());
    let raw = serde_json::to_string(&session.draft).unwrap().replace(
        "\"answers\":{}",
        "\"answers\":{\"mode\":\"new-network\",\"mode\":\"join-network\"}",
    );
    session.store.save(raw.as_bytes()).unwrap();
    drop(session);
    assert!(Session::resume(&f.root.join("prepare")).is_err());
}

#[test]
fn concurrent_checkpoint_update_blocks_stale_export() {
    let f = Fixture::new();
    let mut first = f.session();
    complete(&mut first);
    let mut second = Session::resume(&f.root.join("prepare")).unwrap();
    second.set("archiveEnabled", "yes").unwrap();
    assert!(first.export(&f.root.join("node.json")).is_err());
    assert!(!f.root.join("node.json").exists());
}

#[test]
fn storage_failure_cannot_be_followed_by_successful_save() {
    let f = Fixture::new();
    let mut session = f.session();
    complete(&mut session);
    // Storage cannot open this missing parent. write_new failures other than
    // OUTPUT_EXISTS are conservatively uncertain because that API may also
    // fail after its exclusive link or directory fsync.
    let output = f.root.join("missing-parent/node.json");
    let mut input = Cursor::new(b"export\ny\nsave\n");
    let failure = match interact(&mut session, &mut input, &mut Vec::new(), Some(&output)) {
        Err(failure) => failure,
        Ok(_) => panic!("storage failure must not become a saved draft result"),
    };
    assert_eq!(failure.code, "SETUP_NETWORK_PUBLICATION_UNCERTAIN");
    assert!(failure.message.contains("이미 생성되었을 수"));
    assert_eq!(input.position(), b"export\ny\n".len() as u64);
    assert!(!output.exists());
    assert!(!f.root.join("missing-parent").exists());
    assert!(Session::resume(&f.root.join("prepare")).is_ok());
}

#[test]
fn known_existing_output_can_be_declined_without_overwrite() {
    let f = Fixture::new();
    let mut session = f.session();
    complete(&mut session);
    let output = f.root.join("already-present.json");
    fs::write(&output, b"existing output").unwrap();
    let result = interact(
        &mut session,
        &mut Cursor::new(b"export\ny\nsave\n"),
        &mut Vec::new(),
        Some(&output),
    )
    .unwrap();
    assert!(matches!(result, Finish::Saved));
    assert_eq!(fs::read(&output).unwrap(), b"existing output");
}
