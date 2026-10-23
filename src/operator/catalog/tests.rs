use super::*;
use crate::operator::target::Kind;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temporary.path()).unwrap();
    (temporary, root)
}
fn draft(root: &Path, name: &str) -> Snapshot {
    let path = root.join(name);
    crate::setup::Session::create(&path, None).unwrap();
    target::inspect(Kind::DraftSession, &path).unwrap()
}
fn latest(home: &Path) -> PathBuf {
    let mut files: Vec<_> = fs::read_dir(home.join("catalog"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    files.pop().unwrap()
}
fn overwrite(path: &Path, value: &serde_json::Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

#[test]
fn browsing_absent_and_existing_catalog_is_read_only() {
    let (_temporary, root) = fixture();
    let home = root.join("missing parents/한글 home");
    assert!(Catalog::open(&home).unwrap().is_none());
    assert!(!root.join("missing parents").exists());
    let created = Catalog::create(&home).unwrap();
    assert!(created.entries().is_empty());
    assert_eq!(
        fs::metadata(&home).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(latest(&home)).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let before = fs::read(latest(&home)).unwrap();
    let path = latest(&home);
    drop(created);
    assert!(Catalog::open(&home).unwrap().unwrap().entries().is_empty());
    assert_eq!(latest(&home), path);
    assert_eq!(fs::read(path).unwrap(), before);
    for unused in ["sessions", "packages", "instances", "data", "reports"] {
        assert!(!home.join(unused).exists());
    }
}

#[test]
fn distinct_targets_can_share_names_and_have_opaque_ids() {
    let (_temporary, root) = fixture();
    let first = draft(&root, "초안 한 곳");
    let second = draft(&root, "초안 다른 곳");
    let home = root.join("home");
    let mut catalog = Catalog::create(&home).unwrap();
    let first_id = catalog.add("동일 표시 이름", first.clone()).unwrap();
    let second_id = catalog.add("동일 표시 이름", second).unwrap();
    assert!(valid_id(&first_id));
    assert!(valid_id(&second_id));
    assert_ne!(first_id, second_id);
    assert_eq!(
        catalog.add("다른 이름", first).unwrap_err().code,
        "CONSOLE_TARGET_EXISTS"
    );
    assert_eq!(Catalog::open(&home).unwrap().unwrap().entries().len(), 2);
}

#[test]
fn writers_do_not_hold_prompt_locks_and_stale_updates_conflict() {
    let (_temporary, root) = fixture();
    let first = draft(&root, "first");
    let second = draft(&root, "second");
    let home = root.join("home");
    let mut a = Catalog::create(&home).unwrap();
    let mut b = Catalog::open(&home).unwrap().unwrap();
    let id = a.add("first", first).unwrap();
    assert_eq!(
        b.add("second", second).unwrap_err().code,
        "CONSOLE_CATALOG_CONFLICT"
    );
    let mut c = Catalog::open(&home).unwrap().unwrap();
    c.remove(&id).unwrap();
    assert_eq!(a.remove(&id).unwrap_err().code, "CONSOLE_CATALOG_CONFLICT");
    assert!(Catalog::open(&home).unwrap().unwrap().entries().is_empty());
}

#[test]
fn busy_lock_rejects_only_the_mutation_and_is_released() {
    let (_temporary, root) = fixture();
    let target = draft(&root, "draft");
    let home = root.join("home");
    let mut catalog = Catalog::create(&home).unwrap();
    let other = Home::open(&home).unwrap().unwrap();
    let guard = other.lock().unwrap();
    assert!(Catalog::open(&home).unwrap().is_some());
    assert_eq!(
        catalog.add("draft", target.clone()).unwrap_err().code,
        "CONSOLE_CATALOG_BUSY"
    );
    drop(guard);
    assert!(catalog.add("draft", target).is_ok());
}

#[test]
fn root_symlink_and_inode_replacement_never_redirect_writes() {
    let (_temporary, root) = fixture();
    let target = draft(&root, "draft");
    let home = root.join("home");
    let mut catalog = Catalog::create(&home).unwrap();
    let moved = root.join("moved");
    fs::rename(&home, &moved).unwrap();
    fs::create_dir(&home).unwrap();
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(catalog.add("draft", target.clone()).is_err());
    assert!(fs::read_dir(&home).unwrap().next().is_none());
    fs::remove_dir(&home).unwrap();
    symlink(&moved, &home).unwrap();
    assert!(Catalog::open(&home).is_err());
    assert!(catalog.add("draft", target).is_err());
    assert_eq!(fs::read_dir(moved.join("catalog")).unwrap().count(), 1);
}

#[test]
fn target_replacement_is_rejected_but_missing_target_can_be_hidden() {
    let (_temporary, root) = fixture();
    let target = draft(&root, "draft");
    let home = root.join("home");
    let mut catalog = Catalog::create(&home).unwrap();
    let id = catalog.add("draft", target.clone()).unwrap();
    fs::rename(&target.path, root.join("old-draft")).unwrap();
    let replacement = draft(&root, "draft");
    assert!(catalog.refresh(&id, target).is_err());
    assert!(catalog.refresh(&id, replacement).is_err());
    fs::rename(root.join("draft"), root.join("new-draft")).unwrap();
    assert_eq!(Catalog::open(&home).unwrap().unwrap().entries().len(), 1);
    catalog.remove(&id).unwrap();
    assert!(root.join("old-draft").exists());
    assert!(root.join("new-draft").exists());
}

#[test]
fn overlap_is_rejected_before_new_home_creation() {
    let (_temporary, root) = fixture();
    let target = draft(&root, "draft");
    let home = target.path.join("management");
    assert_eq!(
        check_target_location(&home, &target).unwrap_err().code,
        "CONSOLE_TARGET_OVERLAP"
    );
    assert!(!home.exists());
    assert!(check_target_location(&root, &target).is_err());
}

#[test]
fn unrelated_partial_roots_and_wrong_permissions_are_not_adopted() {
    let (_temporary, root) = fixture();
    let home = root.join("existing");
    fs::create_dir(&home).unwrap();
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(home.join("unrelated"), "preserve").unwrap();
    assert!(Catalog::open(&home).is_err());
    assert!(Catalog::create(&home).is_err());
    assert_eq!(
        fs::read_to_string(home.join("unrelated")).unwrap(),
        "preserve"
    );
    let home = root.join("managed");
    Catalog::create(&home).unwrap();
    fs::set_permissions(&home, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Catalog::open(&home).is_err());
    assert_eq!(
        fs::metadata(&home).unwrap().permissions().mode() & 0o777,
        0o755
    );
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(latest(&home), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Catalog::open(&home).is_err());
}

#[test]
fn missing_marker_lock_or_checkpoint_is_never_repaired() {
    let (_temporary, root) = fixture();
    for (index, file) in [
        ".bxdl-home.json",
        ".catalog.lock",
        "catalog/revision00000001.json",
    ]
    .iter()
    .enumerate()
    {
        let home = root.join(format!("home-{index}"));
        Catalog::create(&home).unwrap();
        fs::remove_file(home.join(file)).unwrap();
        assert!(Catalog::open(&home).is_err());
        assert!(!home.join(file).exists());
    }
}

#[test]
fn corrupt_duplicate_unknown_and_oversize_records_are_rejected() {
    let (_temporary, root) = fixture();
    let target = draft(&root, "draft");
    let home = root.join("home");
    let mut catalog = Catalog::create(&home).unwrap();
    catalog.add("draft", target).unwrap();
    drop(catalog);
    let path = latest(&home);
    let original: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let mut duplicate = original.clone();
    let entry = duplicate["entries"][0].clone();
    duplicate["entries"].as_array_mut().unwrap().push(entry);
    overwrite(&path, &duplicate);
    assert!(Catalog::open(&home).is_err());
    let mut unknown = original.clone();
    unknown["secretPath"] = "should never be accepted".into();
    overwrite(&path, &unknown);
    assert!(Catalog::open(&home).is_err());
    let mut version = original.clone();
    version["schemaVersion"] = 99.into();
    overwrite(&path, &version);
    assert!(Catalog::open(&home).is_err());
    fs::write(&path, vec![b' '; MAX_BYTES + 1]).unwrap();
    assert!(Catalog::open(&home).is_err());
    fs::write(&path, b"{\"schemaVersion\":1,\"schemaVersion\":1}").unwrap();
    assert!(Catalog::open(&home).is_err());
    overwrite(&path, &original);
    assert!(Catalog::open(&home).is_ok());
}

#[test]
fn refresh_preserves_identity_and_remove_never_modifies_target() {
    let (_temporary, root) = fixture();
    let target = draft(&root, "draft");
    let before = fs::read_dir(&target.path).unwrap().count();
    let home = root.join("home");
    let mut catalog = Catalog::create(&home).unwrap();
    let id = catalog.add("node", target.clone()).unwrap();
    catalog.refresh(&id, target.clone()).unwrap();
    assert_eq!(catalog.entries()[0].target.identity, target.identity);
    catalog.remove(&id).unwrap();
    assert_eq!(fs::read_dir(&target.path).unwrap().count(), before);
    assert!(crate::setup::Session::resume(&target.path).is_ok());
}

#[test]
fn schema_bounds_ids_labels_and_timestamps() {
    let (_temporary, root) = fixture();
    let target = draft(&root, "draft");
    let mut record = Record::empty();
    for index in 0..MAX_ENTRIES {
        let mut snapshot = target.clone();
        snapshot.identity.inode += index as u64;
        snapshot.path = root.join(format!("missing-{index}"));
        record.entries.push(Entry {
            id: format!("{index:032x}"),
            label: "same label".into(),
            target: snapshot,
            recorded_at: now().unwrap(),
            alias: None,
            binding_sha256: None,
        });
    }
    assert!(record.validate().is_ok());
    record.entries.push(record.entries[0].clone());
    assert_eq!(record.validate().unwrap_err().code, "CONSOLE_CATALOG_LIMIT");
    for label in ["", " leading", "trailing ", "control\nvalue"] {
        assert!(!valid_label(label));
    }
    assert!(!valid_label(&"가".repeat(81)));
    assert!(valid_label(&"가".repeat(80)));
    assert!(!valid_time("2026-02-30T01:02:03Z"));
    assert!(!valid_time("2026-10-22T25:02:03Z"));
    assert!(valid_time("2026-10-22T01:02:03Z"));
}

#[test]
fn failed_catalog_write_leaves_target_available_to_existing_cli() {
    let (_temporary, root) = fixture();
    let target = draft(&root, "draft");
    let home = root.join("home");
    let mut catalog = Catalog::create(&home).unwrap();
    fs::write(home.join(".catalog.lock"), b"invalid lock").unwrap();
    assert!(catalog.add("draft", target.clone()).is_err());
    assert!(crate::setup::Session::resume(&target.path).is_ok());
    assert_eq!(fs::read_dir(home.join("catalog")).unwrap().count(), 1);
}

#[test]
fn symlink_checkpoint_and_hardlinked_marker_are_rejected() {
    let (_temporary, root) = fixture();
    let home = root.join("home");
    Catalog::create(&home).unwrap();
    let path = latest(&home);
    let moved = root.join("checkpoint");
    fs::rename(&path, &moved).unwrap();
    symlink(&moved, &path).unwrap();
    assert!(Catalog::open(&home).is_err());
    fs::remove_file(&path).unwrap();
    fs::rename(&moved, &path).unwrap();
    fs::hard_link(home.join(".bxdl-home.json"), root.join("marker-alias")).unwrap();
    assert!(Catalog::open(&home).is_err());
}

#[test]
fn managed_session_namespace_requires_a_complete_home_and_direct_child() {
    let (_temporary, root) = fixture();
    let home = root.join("home");
    let mut catalog = Catalog::create(&home).unwrap();
    let sessions = home.join("sessions");
    fs::create_dir(&sessions).unwrap();
    fs::set_permissions(&sessions, fs::Permissions::from_mode(0o700)).unwrap();
    let target = draft(&sessions, "operation");
    assert!(check_target_location(&home, &target).is_ok());
    catalog.add("managed draft", target.clone()).unwrap();
    assert_eq!(Catalog::open(&home).unwrap().unwrap().entries().len(), 1);
    let nested = draft(&target.path, "nested");
    assert_eq!(
        check_target_location(&home, &nested).unwrap_err().code,
        "CONSOLE_TARGET_OVERLAP"
    );
    let arbitrary = draft(&home, "arbitrary");
    assert!(check_target_location(&home, &arbitrary).is_err());
}

#[test]
fn unsafe_namespace_and_unmarked_home_are_not_adopted() {
    let (_temporary, root) = fixture();
    let home = root.join("home");
    fs::create_dir(&home).unwrap();
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
    let sessions = home.join("sessions");
    fs::create_dir(&sessions).unwrap();
    fs::set_permissions(&sessions, fs::Permissions::from_mode(0o700)).unwrap();
    let target = draft(&sessions, "operation");
    assert!(check_target_location(&home, &target).is_err());
    assert!(!home.join(".bxdl-home.json").exists());
    let home = root.join("managed");
    Catalog::create(&home).unwrap();
    fs::create_dir(home.join("sessions")).unwrap();
    fs::set_permissions(home.join("sessions"), fs::Permissions::from_mode(0o755)).unwrap();
    let target = draft(&home.join("sessions"), "operation");
    assert!(check_target_location(&home, &target).is_err());
    assert!(Catalog::open(&home).is_err());
}

#[test]
fn private_reference_overlap_blocks_preparation_and_catalog_save() {
    let (_temporary, root) = fixture();
    let home = root.join("home");
    let draft_path = root.join("draft");
    let mut session = crate::setup::Session::create(&draft_path, None).unwrap();
    session
        .set(
            "validatorPasswordFile",
            home.join("catalog").to_str().unwrap(),
        )
        .unwrap();
    let target = target::inspect(Kind::DraftSession, &draft_path).unwrap();
    assert!(check_target_location(&home, &target).is_err());
    assert!(!home.exists());
    let mut catalog = Catalog::create(&home).unwrap();
    let before = fs::read(latest(&home)).unwrap();
    let error = catalog.add("safe name", target).unwrap_err();
    assert!(!error.to_string().contains(home.to_str().unwrap()));
    assert_eq!(fs::read(latest(&home)).unwrap(), before);
    assert!(crate::setup::Session::resume(&draft_path).is_ok());
}

#[test]
fn managed_preparation_round_trip_preserves_kind_and_never_installs() {
    let (_temporary, root) = fixture();
    let home = root.join("home");
    let mut catalog = Catalog::create(&home).unwrap();
    let sessions = home.join("sessions");
    fs::create_dir(&sessions).unwrap();
    fs::set_permissions(&sessions, fs::Permissions::from_mode(0o700)).unwrap();
    let path = sessions.join("local");
    drop(crate::setup::preparation::Preparation::create(&path, None).unwrap());
    let target = target::inspect(Kind::PreparationSession, &path).unwrap();
    assert!(check_target_location(&home, &target).is_ok());
    let id = catalog.add("설치 준비", target.clone()).unwrap();
    let reopened = Catalog::open(&home).unwrap().unwrap();
    assert_eq!(reopened.entries()[0].target, target);
    assert_eq!(reopened.entries()[0].target.kind, Kind::PreparationSession);
    assert_eq!(reopened.entries()[0].id, id);
    catalog.remove(&id).unwrap();
    let preparation = crate::setup::preparation::Preparation::resume(&path).unwrap();
    assert_eq!(preparation.summary().preparation, "DRAFT");
    assert_eq!(preparation.summary().installation, "NOT_PERFORMED");
    assert_eq!(fs::read_dir(path.join("generated")).unwrap().count(), 0);
    for namespace in ["packages", "instances", "data"] {
        assert!(!home.join(namespace).exists());
    }
}

#[test]
fn command_names_and_versioned_binding_pairs_are_strict() {
    for name in ["a", "node-01", &"a".repeat(32)] {
        assert!(valid_name(name));
    }
    for name in [
        "",
        "A",
        "-node",
        "1node",
        " node",
        "node ",
        "node_1",
        "한글",
        "../local",
        &"a".repeat(33),
    ] {
        assert!(!valid_name(name), "{name}");
    }
    let (_temporary, root) = fixture();
    let mut node = draft(&root, "draft");
    node.kind = Kind::Instance;
    node.state = target::RecordState::Registered;
    node.cli = target::CliMatch::Unavailable;
    let mut record = Record::empty();
    record.entries.push(Entry {
        id: "a".repeat(32),
        label: "label".into(),
        target: node,
        recorded_at: now().unwrap(),
        alias: Some("local".into()),
        binding_sha256: Some("b".repeat(64)),
    });
    assert!(record.validate().is_err());
    record.schema_version = 2;
    assert!(record.validate().is_ok());
    let original = serde_json::to_value(&record).unwrap();
    for (field, bad) in [
        ("alias", serde_json::json!("BAD")),
        ("bindingSha256", serde_json::json!("bad")),
        ("alias", serde_json::Value::Null),
        ("bindingSha256", serde_json::Value::Null),
    ] {
        let mut value = original.clone();
        value["entries"][0][field] = bad;
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let mut duplicate = record.entries[0].clone();
    duplicate.id = "c".repeat(32);
    duplicate.target.identity.inode += 1;
    duplicate.target.path = root.join("second");
    record.entries.push(duplicate);
    assert!(record.validate().is_err());
    record.entries[1].alias = Some("other".into());
    assert!(record.validate().is_ok());
    record.entries[1].target.kind = Kind::DraftSession;
    record.entries[1].target.state = target::RecordState::DraftIncomplete;
    record.entries[1].target.cli = target::CliMatch::NotApplicable;
    assert!(record.validate().is_err());
}
