use super::*;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    process::Command,
};

const INITIAL: &[u8] = b"{\"phase\":\"SELECT_INPUTS\"}";
const INITIAL_DRAFT: &[u8] = b"{\"draft\":1}";

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    (temp, root.join("session"))
}
fn create(path: &Path) -> WorkflowStore {
    WorkflowStore::create(path, INITIAL, INITIAL_DRAFT).unwrap()
}
fn code<T>(result: Result<T>) -> String {
    match result {
        Ok(_) => panic!("unexpected success"),
        Err(e) => e.code,
    }
}
fn private_file(path: &Path, raw: &[u8]) {
    fs::write(path, raw).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn create_initializes_separate_stores_and_explicit_resume_preserves_bytes() {
    let (_temp, path) = fixture();
    let mut store = create(&path);
    assert_eq!(store.path(), path);
    assert_eq!(store.draft_path(), path.join(DRAFT));
    assert_eq!(store.generated_path(), path.join(GENERATED));
    assert_eq!(store.read().unwrap(), INITIAL);
    assert_eq!(
        Store::open(store.draft_path())
            .unwrap()
            .read()
            .unwrap()
            .unwrap(),
        INITIAL_DRAFT
    );
    store.require_capacity(2).unwrap();
    store.save(b"{\"phase\":\"PLAN\"}").unwrap();
    assert_eq!(
        code(WorkflowStore::create(&path, INITIAL, INITIAL_DRAFT)),
        "SETUP_WORKFLOW_EXISTS"
    );
    for name in [JOURNAL, DRAFT, GENERATED] {
        assert_eq!(
            fs::metadata(path.join(name)).unwrap().permissions().mode() & 0o7777,
            0o700
        );
    }
    for name in [
        LOCK,
        RECORD,
        "workflow/revision00000001.json",
        "draft/revision00000001.json",
    ] {
        assert_eq!(
            fs::metadata(path.join(name)).unwrap().permissions().mode() & 0o7777,
            0o600
        );
    }
    drop(store);
    let resumed = WorkflowStore::open(&path).unwrap();
    assert_eq!(resumed.read().unwrap(), b"{\"phase\":\"PLAN\"}");
    assert_eq!(
        fs::read(path.join("workflow/revision00000001.json")).unwrap(),
        INITIAL
    );
}

#[test]
fn exclusive_lock_covers_whole_object_lifetime_and_independent_process() {
    let (_temp, path) = fixture();
    let store = create(&path);
    assert_eq!(code(WorkflowStore::open(&path)), "SETUP_WORKFLOW_BUSY");
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "setup::workflow_store::tests::lock_probe",
            "--nocapture",
        ])
        .env("BXDL_TEST_WORKFLOW_LOCK_PROBE", &path)
        .output()
        .unwrap();
    assert!(output.status.success(), "child lock probe failed");
    store.recheck().unwrap();
    drop(store);
    WorkflowStore::open(&path).unwrap();
}

#[test]
fn lock_probe() {
    let Some(path) = std::env::var_os("BXDL_TEST_WORKFLOW_LOCK_PROBE") else {
        return;
    };
    assert_eq!(
        code(WorkflowStore::open(Path::new(&path))),
        "SETUP_WORKFLOW_BUSY"
    );
}

#[test]
fn incomplete_create_missing_marker_or_initial_checkpoint_is_never_adopted() {
    for missing in [
        RECORD,
        LOCK,
        "workflow/revision00000001.json",
        "draft/revision00000001.json",
    ] {
        let (_temp, path) = fixture();
        drop(create(&path));
        fs::remove_file(path.join(missing)).unwrap();
        assert!(WorkflowStore::open(&path).is_err());
        assert!(!path.join(missing).exists());
        assert_eq!(
            code(WorkflowStore::create(&path, INITIAL, INITIAL_DRAFT)),
            "SETUP_WORKFLOW_EXISTS"
        );
    }
    let (_temp, path) = fixture();
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(WorkflowStore::open(&path).is_err());
    assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
}

#[test]
fn completed_journal_retains_store_crash_recovery_but_partial_container_marker_does_not() {
    let (_temp, path) = fixture();
    drop(create(&path));
    let temp_name = ".bxdl-tmp-1-1-00000000000000000000000000000001";
    fs::hard_link(
        path.join("workflow/revision00000001.json"),
        path.join(JOURNAL).join(temp_name),
    )
    .unwrap();
    private_file(
        &path
            .join(JOURNAL)
            .join(".bxdl-tmp-1-2-00000000000000000000000000000002"),
        b"unpublished incomplete bytes",
    );
    let store = WorkflowStore::open(&path).unwrap();
    assert_eq!(store.read().unwrap(), INITIAL);
    drop(store);
    fs::hard_link(path.join(RECORD), path.join(PENDING_RECORD)).unwrap();
    assert!(WorkflowStore::open(&path).is_err());
    assert!(path.join(PENDING_RECORD).exists());
}

#[test]
fn marker_is_strict_and_cannot_be_replaced_by_byte_identical_copy() {
    for variant in [0, 1, 2, 3, 4] {
        let (_temp, path) = fixture();
        drop(create(&path));
        let marker = path.join(RECORD);
        let raw = fs::read(&marker).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        let changed = match variant {
            0 => {
                value["schemaVersion"] = 2.into();
                serde_json::to_vec(&value).unwrap()
            }
            1 => {
                value["unknown"] = true.into();
                serde_json::to_vec(&value).unwrap()
            }
            2 => {
                value.as_object_mut().unwrap().remove("lock");
                serde_json::to_vec(&value).unwrap()
            }
            3 => {
                let s = String::from_utf8(raw).unwrap();
                s.replacen('{', "{\"schemaVersion\":1,", 1).into_bytes()
            }
            _ => {
                value["lock"] = serde_json::Value::Null;
                serde_json::to_vec(&value).unwrap()
            }
        };
        fs::write(&marker, changed).unwrap();
        assert!(WorkflowStore::open(&path).is_err());
    }
    let (_temp, path) = fixture();
    let store = create(&path);
    let marker = path.join(RECORD);
    let raw = fs::read(&marker).unwrap();
    fs::rename(&marker, path.parent().unwrap().join("old-marker")).unwrap();
    private_file(&marker, &raw);
    assert!(store.recheck().is_err());
    drop(store);
    assert!(WorkflowStore::open(&path).is_err());
}

#[test]
fn replacing_root_child_directory_or_lock_is_detected_during_and_after_session() {
    for name in [JOURNAL, DRAFT, GENERATED, LOCK] {
        let (_temp, path) = fixture();
        let mut store = create(&path);
        let target = path.join(name);
        fs::rename(&target, path.parent().unwrap().join("old-object")).unwrap();
        if name == LOCK {
            private_file(&target, b"");
        } else {
            fs::create_dir(&target).unwrap();
            fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
        }
        assert!(store.recheck().is_err());
        assert!(store.save(b"replacement").is_err());
        drop(store);
        assert!(WorkflowStore::open(&path).is_err());
    }
    let (_temp, path) = fixture();
    let store = create(&path);
    fs::rename(&path, path.parent().unwrap().join("old-root")).unwrap();
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(store.recheck().is_err());
}

#[test]
fn symlinks_hardlinks_unsafe_permissions_and_unknown_root_entries_are_rejected() {
    for name in [
        LOCK,
        RECORD,
        "workflow/revision00000001.json",
        "draft/revision00000001.json",
    ] {
        let (_temp, path) = fixture();
        drop(create(&path));
        fs::hard_link(path.join(name), path.parent().unwrap().join("linked-file")).unwrap();
        assert!(WorkflowStore::open(&path).is_err());
    }
    for name in [LOCK, RECORD, JOURNAL, DRAFT, GENERATED] {
        let (_temp, path) = fixture();
        let store = create(&path);
        let target = path.join(name);
        let mode = if target.is_dir() { 0o755 } else { 0o644 };
        fs::set_permissions(&target, fs::Permissions::from_mode(mode)).unwrap();
        assert!(store.recheck().is_err());
    }
    let (_temp, path) = fixture();
    let store = create(&path);
    private_file(&path.join("unexpected.json"), b"secret");
    assert!(store.recheck().is_err());
    drop(store);
    fs::remove_file(path.join("unexpected.json")).unwrap();
    let alias = path.parent().unwrap().join("alias");
    symlink(&path, &alias).unwrap();
    assert!(WorkflowStore::open(&alias).is_err());
    assert!(WorkflowStore::create(&alias.join("new"), INITIAL, INITIAL_DRAFT).is_err());
    let target = path.join(LOCK);
    fs::rename(&target, path.parent().unwrap().join("real-lock")).unwrap();
    symlink(path.parent().unwrap().join("real-lock"), &target).unwrap();
    assert!(WorkflowStore::open(&path).is_err());
}

#[test]
fn generated_outputs_are_separate_private_flat_files_without_links_or_special_files() {
    let (_temp, path) = fixture();
    let store = create(&path);
    let output = store.generated_path().join("plan-a.json");
    super::super::store::write_new(&output, b"{}").unwrap();
    store.recheck().unwrap();
    fs::hard_link(&output, store.generated_path().join("alias.json")).unwrap();
    assert!(store.recheck().is_err());
    fs::remove_file(store.generated_path().join("alias.json")).unwrap();
    fs::remove_file(&output).unwrap();
    symlink(path.join(RECORD), &output).unwrap();
    assert!(store.recheck().is_err());
    fs::remove_file(&output).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&output)
            .status()
            .unwrap()
            .success()
    );
    assert!(store.recheck().is_err());
}

#[test]
fn bounded_inputs_revision_capacity_and_published_corruption_fail_closed() {
    let (_temp, path) = fixture();
    assert!(WorkflowStore::create(&path, b"", INITIAL_DRAFT).is_err());
    assert!(!path.exists());
    assert!(WorkflowStore::create(&path, INITIAL, &vec![b'x'; MAX_BYTES + 1]).is_err());
    assert!(!path.exists());
    let mut store = create(&path);
    assert!(store.save(&vec![b'x'; MAX_BYTES + 1]).is_err());
    assert!(store.require_capacity(u32::MAX).is_err());
    store.require_capacity(4095).unwrap();
    assert!(store.require_capacity(4096).is_err());
    store.save(b"second").unwrap();
    fs::write(path.join("workflow/revision00000002.json"), b"changed").unwrap();
    assert!(store.read().is_err());
    assert!(store.save(b"third").is_err());
    assert!(!path.join("workflow/revision00000003.json").exists());
}

#[test]
fn missing_parents_and_noncanonical_paths_do_not_create_or_echo_input() {
    let (_temp, path) = fixture();
    let missing = path.join("PRIVATE_PATH_CANARY/session");
    let error = WorkflowStore::create(&missing, INITIAL, INITIAL_DRAFT)
        .err()
        .unwrap();
    assert!(!error.message.contains("PRIVATE_PATH_CANARY"));
    assert!(!path.exists());
    for bad in [
        PathBuf::from("relative"),
        PathBuf::from("/"),
        path.join("../other"),
        path.join("./child"),
    ] {
        assert!(WorkflowStore::create(&bad, INITIAL, INITIAL_DRAFT).is_err());
    }
}
