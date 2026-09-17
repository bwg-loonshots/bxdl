use super::*;
use std::{fs, io::Cursor};

fn input(root: &Path) -> PathBuf {
    let path = root.join("source.json");
    fs::write(
        &path,
        include_bytes!("../../config/examples/instance.development.json"),
    )
    .unwrap();
    path
}

#[test]
fn import_export_pins_relative_references_and_keeps_inputs_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let source = input(root.as_path());
    let original = fs::read(&source).unwrap();
    let workspace = root.as_path().join("setup");
    let session = Session::create(&workspace, Some(&source)).unwrap();
    assert!(session.complete());
    let report = session.preflight().unwrap();
    assert_eq!(report.outcome, "FAIL"); // intentionally missing input files
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.name == "nigoCanonicalConfiguration" && c.status == "NOT_CHECKED")
    );
    assert!(!root.as_path().join("data").exists());
    let output = workspace.join("instance.json");
    session.export(&output).unwrap();
    let value: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(
        value["chainDescription"],
        root.as_path()
            .join("public-chain-description.json")
            .to_str()
            .unwrap()
    );
    assert_eq!(fs::read(&source).unwrap(), original);
    assert!(Session::resume(&workspace).unwrap().complete());
    assert!(session.export(&output).is_err());
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn accepted_fields_checkpoint_and_invalid_input_does_not_replace_them() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let workspace = root.as_path().join("setup");
    let mut session = Session::create(&workspace, None).unwrap();
    session.set("instanceId", "node-a").unwrap();
    assert!(session.set("instanceId", "PRIVATE-CANARY-Invalid").is_err());
    let restored = Session::resume(&workspace).unwrap();
    assert_eq!(restored.next_index(), 1);
    assert_eq!(restored.draft.answers["instanceId"], "node-a");
    assert!(restored.config_bytes().is_err());
    assert!(
        !serde_json::to_string(&restored.summary())
            .unwrap()
            .contains("CANARY")
    );
    assert!(Session::create(&workspace, None).is_err());
}

#[test]
fn resume_rejects_unknown_or_duplicate_fields_and_wrong_draft_kind() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    for (name, answers, kind) in [
        (
            "duplicate",
            r#"{"instanceId":"node-a","instanceId":"node-b"}"#,
            "BXDL_SETUP_DRAFT",
        ),
        (
            "unknown",
            r#"{"password":"PRIVATE-CANARY"}"#,
            "BXDL_SETUP_DRAFT",
        ),
        ("kind", "{}", "ENGINE_INITIALIZED"),
    ] {
        let path = root.as_path().join(name);
        let mut store = Store::create(&path).unwrap();
        let raw = format!(
            r#"{{"schemaVersion":1,"kind":"{kind}","inputBase":{},"answers":{answers}}}"#,
            serde_json::to_string(root.as_path().to_str().unwrap()).unwrap()
        );
        store.save(raw.as_bytes()).unwrap();
        let failure = match Session::resume(&path) {
            Ok(_) => panic!("invalid checkpoint accepted"),
            Err(e) => e,
        };
        assert_eq!(failure.code, "SETUP_DRAFT_INVALID");
        assert!(!failure.message.contains("PRIVATE-CANARY"));
    }
}

#[test]
fn draft_workspace_and_output_cannot_be_database_or_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let source = input(root.as_path());
    let workspace = root.as_path().join("setup");
    let mut session = Session::create(&workspace, Some(&source)).unwrap();
    for path in [&workspace, &workspace.join("nested"), root.as_path()] {
        assert_eq!(
            session
                .set("dataDirectory", path.to_str().unwrap())
                .unwrap_err()
                .code,
            "SETUP_PATH_CONFLICT"
        );
    }
    assert!(
        session
            .set(
                "validatorPasswordFile",
                workspace.join("key").to_str().unwrap()
            )
            .is_err()
    );
    for path in [
        root.as_path().join("data/config.json"),
        root.as_path().join("data"),
        workspace.join("revision99999999.json"),
        root.as_path().join("public-chain-description.json"),
    ] {
        assert!(session.export(&path).is_err());
        assert!(!path.exists());
    }
    assert!(!root.as_path().join("data").exists());
}

#[test]
fn invalid_import_has_no_workspace_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let source = root.as_path().join("bad.json");
    fs::write(&source, br#"{"password":"PRIVATE-CANARY"}"#).unwrap();
    let workspace = root.as_path().join("setup");
    assert!(Session::create(&workspace, Some(&source)).is_err());
    assert!(!workspace.exists());
}

#[test]
fn wizard_invalid_answer_back_cancel_eof_and_resume_preserve_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let workspace = root.as_path().join("setup");
    let mut session = Session::create(&workspace, None).unwrap();
    let mut output = Vec::new();
    let mut lines = Cursor::new(b"PRIVATE-CANARY-Invalid\nnode-a\n:back\nnode-b\n:cancel\n");
    assert!(matches!(
        interact(&mut session, &mut lines, &mut output, None).unwrap(),
        Finish::Paused
    ));
    assert!(
        !String::from_utf8(output)
            .unwrap()
            .contains("PRIVATE-CANARY")
    );
    let mut resumed = Session::resume(&workspace).unwrap();
    assert_eq!(resumed.draft.answers["instanceId"], "node-b");
    assert!(matches!(
        interact(&mut resumed, &mut Cursor::new(b""), &mut Vec::new(), None).unwrap(),
        Finish::Paused
    ));
    assert_eq!(resumed.next_index(), 1);
}

#[test]
fn wizard_export_requires_explicit_confirmation_and_leaves_existing_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let source = input(root.as_path());
    let workspace = root.as_path().join("setup");
    let mut session = Session::create(&workspace, Some(&source)).unwrap();
    let output = root.as_path().join("output.json");
    let mut prompts = Vec::new();
    assert!(matches!(
        interact(
            &mut session,
            &mut Cursor::new(b"export\nn\nsave\n"),
            &mut prompts,
            Some(&output)
        )
        .unwrap(),
        Finish::Saved
    ));
    assert!(!output.exists());
    assert!(matches!(
        interact(
            &mut session,
            &mut Cursor::new(b"export\ny\n"),
            &mut prompts,
            Some(&output)
        )
        .unwrap(),
        Finish::Exported(_)
    ));
    let before = fs::read(&output).unwrap();
    assert!(matches!(
        interact(
            &mut session,
            &mut Cursor::new(b"export\ny\n:cancel\n"),
            &mut prompts,
            Some(&output)
        )
        .unwrap(),
        Finish::Paused
    ));
    assert_eq!(before, fs::read(output).unwrap());
}

#[test]
fn wizard_large_or_control_input_does_not_persist_or_echo_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    for (name, value) in [
        ("large", "CANARY".repeat(1000)),
        ("control", "CANARY\u{1b}[31m\n".into()),
    ] {
        let mut session = Session::create(&root.as_path().join(name), None).unwrap();
        let mut prompts = Vec::new();
        assert!(
            interact(
                &mut session,
                &mut Cursor::new(value.as_bytes()),
                &mut prompts,
                None
            )
            .is_err()
        );
        assert_eq!(session.next_index(), 0);
        assert!(!String::from_utf8(prompts).unwrap().contains("CANARY"));
    }
}
