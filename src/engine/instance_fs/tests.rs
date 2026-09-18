use super::*;
use std::{
    fs,
    io::Read,
    os::unix::fs::{PermissionsExt, symlink},
    process::{Command, Stdio},
};

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path())
        .unwrap()
        .join("private-control");
    (temp, root)
}
fn code<T>(result: Result<T>) -> String {
    result.err().expect("expected failure").code
}

#[test]
fn private_control_and_attempt_are_new_and_durable_without_report_precreation() {
    let (_temp, root) = fixture();
    let control = Control::create(&root).unwrap();
    assert_eq!(control.path(), root);
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o7777,
        0o700
    );
    assert_eq!(
        fs::metadata(root.join(LOCK)).unwrap().permissions().mode() & 0o7777,
        0o600
    );
    assert_eq!(code(Control::create(&root)), "INSTANCE_CONTROL_EXISTS");
    let _guard = control.lock().unwrap();
    let attempt = control.attempt("init_123-ABC").unwrap();
    assert_eq!(attempt, root.join("operations/init_123-ABC"));
    assert_eq!(
        fs::metadata(&attempt).unwrap().permissions().mode() & 0o7777,
        0o700
    );
    assert_eq!(fs::read_dir(&attempt).unwrap().count(), 0);
    assert_eq!(
        code(control.attempt("init_123-ABC")),
        "INSTANCE_CONTROL_EXISTS"
    );
    for bad in ["", "../escaped", "contains.dot", "has space", "nested/name"] {
        assert_eq!(code(control.attempt(bad)), "INSTANCE_ATTEMPT_INVALID");
    }
    assert!(!root.join("escaped").exists());
    Control::open(&root).unwrap().recheck().unwrap();
}

#[test]
fn exclusive_lock_is_held_by_cloned_child_descriptor_after_parent_guard_drop() {
    let (_temp, root) = fixture();
    let control = Control::create(&root).unwrap();
    let guard = control.lock().unwrap();
    assert_eq!(code(control.lock()), "INSTANCE_BUSY");
    let mut child_fd = guard.child_stdin().unwrap();
    let mut contents = Vec::new();
    child_fd.read_to_end(&mut contents).unwrap();
    assert!(contents.is_empty());
    drop(guard);
    assert_eq!(code(Control::open(&root).unwrap().lock()), "INSTANCE_BUSY");
    drop(child_fd);
    control.lock().unwrap();
}

#[test]
fn spawned_child_retains_lock_until_exit_without_explicit_unlock() {
    let (_temp, root) = fixture();
    let control = Control::create(&root).unwrap();
    let guard = control.lock().unwrap();
    let mut command = Command::new("/bin/sleep");
    command
        .arg("0.3")
        .stdin(Stdio::from(guard.child_stdin().unwrap()));
    let mut child = command.spawn().unwrap();
    drop(command);
    drop(guard);
    assert_eq!(code(control.lock()), "INSTANCE_BUSY");
    child.wait().unwrap();
    control.lock().unwrap();
}

#[test]
fn missing_or_substituted_lock_is_never_recreated_or_adopted() {
    let (_temp, root) = fixture();
    let control = Control::create(&root).unwrap();
    fs::rename(root.join(LOCK), root.join("original.lock")).unwrap();
    assert_eq!(code(control.lock()), "INSTANCE_CONTROL_UNSAFE");
    assert_eq!(code(Control::open(&root)), "INSTANCE_CONTROL_UNSAFE");
    assert!(!root.join(LOCK).exists());
    fs::write(root.join(LOCK), b"").unwrap();
    fs::set_permissions(root.join(LOCK), fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(code(control.lock()), "INSTANCE_CONTROL_UNSAFE");
    assert_eq!(code(Control::open(&root)), "INSTANCE_CONTROL_UNSAFE");
}

#[test]
fn missing_or_copied_identity_record_is_not_adopted() {
    let (_temp, root) = fixture();
    Control::create(&root).unwrap();
    fs::rename(root.join(RECORD), root.join("old-record")).unwrap();
    assert_eq!(code(Control::open(&root)), "INSTANCE_CONTROL_UNSAFE");
    fs::copy(root.join("old-record"), root.join(RECORD)).unwrap();
    assert_eq!(code(Control::open(&root)), "INSTANCE_CONTROL_UNSAFE");
}

#[test]
fn symlink_hardlink_contents_and_permissions_of_lock_are_rejected() {
    for variant in ["symlink", "hardlink", "contents", "mode"] {
        let (_temp, root) = fixture();
        let control = Control::create(&root).unwrap();
        let lock = root.join(LOCK);
        match variant {
            "symlink" => {
                fs::rename(&lock, root.join("original.lock")).unwrap();
                symlink("original.lock", &lock).unwrap();
            }
            "hardlink" => fs::hard_link(&lock, root.join("alias.lock")).unwrap(),
            "contents" => fs::write(&lock, b"SECRET_CANARY").unwrap(),
            "mode" => fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).unwrap(),
            _ => unreachable!(),
        }
        let failure = control.lock().err().unwrap();
        assert_eq!(failure.code, "INSTANCE_CONTROL_UNSAFE");
        assert!(!failure.message.contains("SECRET_CANARY"));
        assert!(!failure.message.contains(root.to_str().unwrap()));
        assert_eq!(code(Control::open(&root)), "INSTANCE_CONTROL_UNSAFE");
    }
}

#[test]
fn root_or_operations_replacement_invalidates_open_anchors_and_child_fd_clone() {
    for operations in [false, true] {
        let (_temp, root) = fixture();
        let control = Control::create(&root).unwrap();
        let guard = control.lock().unwrap();
        if operations {
            fs::rename(root.join(OPERATIONS), root.join("old-operations")).unwrap();
            fs::create_dir(root.join(OPERATIONS)).unwrap();
            fs::set_permissions(root.join(OPERATIONS), fs::Permissions::from_mode(0o700)).unwrap();
        } else {
            fs::rename(&root, root.with_file_name("old-control")).unwrap();
            Control::create(&root).unwrap();
            assert_eq!(code(guard.child_stdin()), "INSTANCE_CONTROL_UNSAFE");
        }
        assert_eq!(code(control.recheck()), "INSTANCE_CONTROL_UNSAFE");
        assert_eq!(
            code(control.attempt("never_created")),
            "INSTANCE_CONTROL_UNSAFE"
        );
        assert!(!root.join("operations/never_created").exists());
    }
}

#[test]
fn parent_symlinks_nonabsolute_and_missing_parents_are_rejected() {
    let (temp, root) = fixture();
    let parent = root.parent().unwrap();
    symlink(parent, parent.join("alias")).unwrap();
    assert_eq!(
        code(Control::create(&parent.join("alias/new"))),
        "INSTANCE_CONTROL_UNSAFE"
    );
    assert_eq!(
        code(Control::create(Path::new("relative"))),
        "INSTANCE_CONTROL_UNSAFE"
    );
    assert_eq!(
        code(Control::create(&parent.join("missing/new"))),
        "INSTANCE_CONTROL_UNSAFE"
    );
    assert!(!temp.path().join("missing").exists());
    let control = Control::create(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(code(control.recheck()), "INSTANCE_CONTROL_UNSAFE");
    assert_eq!(code(Control::open(&root)), "INSTANCE_CONTROL_UNSAFE");
}

#[test]
fn data_preparation_creates_private_empty_directory_and_never_repairs_existing_data() {
    let (_temp, data) = fixture();
    prepare_data(&data).unwrap();
    assert_eq!(
        fs::metadata(&data).unwrap().permissions().mode() & 0o7777,
        0o700
    );
    assert_eq!(fs::read_dir(&data).unwrap().count(), 0);
    prepare_data(&data).unwrap();
    fs::write(data.join("sentinel"), b"PRESERVE_DATA").unwrap();
    assert_eq!(code(prepare_data(&data)), "INSTANCE_DATA_NOT_EMPTY");
    assert_eq!(fs::read(data.join("sentinel")).unwrap(), b"PRESERVE_DATA");
    fs::set_permissions(&data, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(code(prepare_data(&data)), "INSTANCE_CONTROL_UNSAFE");
    assert_eq!(
        fs::metadata(&data).unwrap().permissions().mode() & 0o7777,
        0o755
    );
    assert_eq!(fs::read(data.join("sentinel")).unwrap(), b"PRESERVE_DATA");
}

#[test]
fn data_preparation_rejects_symlink_and_missing_parent_without_mutation() {
    let (_temp, path) = fixture();
    let target = path.with_file_name("target");
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
    symlink(&target, &path).unwrap();
    assert_eq!(code(prepare_data(&path)), "INSTANCE_CONTROL_UNSAFE");
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    assert_eq!(
        code(prepare_data(&path.join("nested"))),
        "INSTANCE_CONTROL_UNSAFE"
    );
    assert!(!target.join("nested").exists());
    let absent = path.with_file_name("missing");
    assert_eq!(
        code(prepare_data(&absent.join("data"))),
        "INSTANCE_CONTROL_UNSAFE"
    );
    assert!(!absent.exists());
}
