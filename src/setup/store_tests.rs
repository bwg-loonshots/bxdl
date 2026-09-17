use super::*;
use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};
use std::sync::{Arc, Barrier};

fn directory() -> (tempfile::TempDir, PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temporary.path()).unwrap();
    (temporary, root)
}
fn put(path: &Path, raw: &[u8]) {
    fs::write(path, raw).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn orphan_name() -> &'static str {
    ".bxdl-tmp-1-2-00000000000000000000000000000003"
}

#[test]
fn checkpoints_are_private_append_only_and_resume_last_complete_bytes() {
    let (_temporary, root) = directory();
    let path = root.join("state");
    let mut store = Store::create(&path).unwrap();
    assert_eq!(store.path(), path);
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o7777, 0o700);
    assert_eq!(store.read().unwrap(), None);
    store.save(b"first raw bytes").unwrap();
    store.save(b"second raw bytes").unwrap();
    assert_eq!(
        fs::read(path.join(revision_name(1))).unwrap(),
        b"first raw bytes"
    );
    assert_eq!(
        Store::open(&path).unwrap().read().unwrap().unwrap(),
        b"second raw bytes"
    );
    for revision in 1..=2 {
        let m = fs::metadata(path.join(revision_name(revision))).unwrap();
        assert_eq!(m.mode() & 0o7777, 0o600);
        assert_eq!(m.nlink(), 1);
    }
    assert_eq!(
        Store::create(&path).err().unwrap().code,
        "SETUP_STATE_EXISTS"
    );
}

#[test]
fn stale_writer_conflicts_instead_of_overwriting() {
    let (_temporary, root) = directory();
    let path = root.join("state");
    let mut first = Store::create(&path).unwrap();
    let mut second = Store::open(&path).unwrap();
    first.save(b"first").unwrap();
    assert_eq!(second.save(b"second").unwrap_err().code, "SETUP_CONFLICT");
    assert_eq!(second.read().unwrap_err().code, "SETUP_CONFLICT");
    assert_eq!(first.read().unwrap().unwrap(), b"first");
}

#[test]
fn simultaneous_publication_has_one_explicit_winner() {
    let (_temporary, root) = directory();
    let path = root.join("state");
    let a = Store::create(&path).unwrap();
    let b = Store::open(&path).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [a, b]
        .into_iter()
        .enumerate()
        .map(|(i, mut store)| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                store.save(if i == 0 { b"A" } else { b"B" })
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results.iter().find_map(|r| r.as_ref().err()).unwrap().code,
        "SETUP_CONFLICT"
    );
    assert_eq!(
        Store::open(&path).unwrap().read().unwrap().unwrap().len(),
        1
    );
    assert_eq!(fs::read_dir(path).unwrap().count(), 1);
}

#[test]
fn crash_before_publish_ignores_private_bytes_and_after_publish_resumes() {
    let (_temporary, root) = directory();
    let path = root.join("state");
    let mut store = Store::create(&path).unwrap();
    store.save(b"complete").unwrap();
    let orphan = path.join(orphan_name());
    put(&orphan, b"not JSON; interrupted partial write");
    assert_eq!(
        Store::open(&path).unwrap().read().unwrap().unwrap(),
        b"complete"
    );
    fs::remove_file(&orphan).unwrap();
    fs::hard_link(path.join(revision_name(1)), &orphan).unwrap();
    // The only extra link is the exact private publication name in this store.
    let mut resumed = Store::open(&path).unwrap();
    assert_eq!(resumed.read().unwrap().unwrap(), b"complete");
    resumed.save(b"continued").unwrap();
    assert_eq!(resumed.read().unwrap().unwrap(), b"continued");
}

#[test]
fn corrupt_published_state_never_rolls_back() {
    let (_temporary, root) = directory();
    let path = root.join("state");
    let mut store = Store::create(&path).unwrap();
    store.save(b"valid").unwrap();
    put(&path.join(revision_name(3)), b"gap");
    assert_eq!(
        Store::open(&path).err().unwrap().code,
        "SETUP_STATE_CORRUPT"
    );
    fs::remove_file(path.join(revision_name(3))).unwrap();
    put(&path.join("revision00000002.json.partial"), b"ambiguous");
    assert!(Store::open(&path).is_err());
    fs::remove_file(path.join("revision00000002.json.partial")).unwrap();
    put(&path.join(revision_name(2)), b"not JSON");
    // JSON validation belongs to the caller: do not hide a bad latest snapshot.
    assert_eq!(
        Store::open(&path).unwrap().read().unwrap().unwrap(),
        b"not JSON"
    );
}

#[test]
fn mutation_of_loaded_checkpoint_is_detected() {
    let (_temporary, root) = directory();
    let path = root.join("state");
    let mut store = Store::create(&path).unwrap();
    store.save(b"old").unwrap();
    put(&path.join(revision_name(1)), b"new");
    assert_eq!(store.read().unwrap_err().code, "SETUP_STATE_CORRUPT");
    assert_eq!(store.save(b"next").unwrap_err().code, "SETUP_STATE_CORRUPT");
    assert!(!path.join(revision_name(2)).exists());
}

#[test]
fn links_special_files_and_unsafe_permissions_are_rejected() {
    let (_temporary, root) = directory();
    let path = root.join("state");
    let mut store = Store::create(&path).unwrap();
    store.save(b"data").unwrap();
    let snapshot = path.join(revision_name(1));
    let outside = root.join("outside");
    fs::hard_link(&snapshot, &outside).unwrap();
    assert!(Store::open(&path).is_err());
    fs::remove_file(&outside).unwrap();
    fs::set_permissions(&snapshot, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Store::open(&path).is_err());
    fs::remove_file(&snapshot).unwrap();
    put(&outside, b"untouched");
    symlink(&outside, &snapshot).unwrap();
    assert!(Store::open(&path).is_err());
    fs::remove_file(&snapshot).unwrap();
    let socket = std::os::unix::net::UnixListener::bind(&snapshot).unwrap();
    assert!(Store::open(&path).is_err());
    drop(socket);
    fs::remove_file(&snapshot).unwrap();
    symlink(&outside, path.join(orphan_name())).unwrap();
    assert!(Store::open(&path).is_err());
    fs::remove_file(path.join(orphan_name())).unwrap();
    fs::hard_link(&outside, path.join(orphan_name())).unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"untouched");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Store::open(&path).is_err());
}

#[test]
fn symlink_ancestors_and_replaced_store_anchor_are_rejected() {
    let (_temporary, root) = directory();
    let path = root.join("state");
    let mut store = Store::create(&path).unwrap();
    symlink(&path, root.join("alias")).unwrap();
    assert!(Store::open(&root.join("alias")).is_err());
    symlink(&root, root.join("parent-alias")).unwrap();
    assert!(Store::create(&root.join("parent-alias/new")).is_err());
    fs::rename(&path, root.join("moved")).unwrap();
    Store::create(&path).unwrap();
    assert!(store.save(b"must not publish").is_err());
    assert_eq!(fs::read_dir(root.join("moved")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(path).unwrap().count(), 0);
}

#[test]
fn byte_and_revision_limits_do_not_publish_extra_state() {
    let (_temporary, root) = directory();
    let path = root.join("state");
    let mut store = Store::create(&path).unwrap();
    assert_eq!(
        store.save(&vec![0; MAX_BYTES + 1]).unwrap_err().code,
        "SETUP_LIMIT"
    );
    store.save(&vec![0; MAX_BYTES]).unwrap();
    assert_eq!(store.read().unwrap().unwrap().len(), MAX_BYTES);
    for revision in 2..=MAX_REVISIONS {
        put(&path.join(revision_name(revision)), b"x");
    }
    let mut full = Store::open(&path).unwrap();
    assert_eq!(full.save(b"overflow").unwrap_err().code, "SETUP_LIMIT");
    assert!(!path.join(revision_name(MAX_REVISIONS + 1)).exists());
}

#[test]
fn output_publication_is_private_exclusive_and_requires_existing_parent() {
    let (_temporary, root) = directory();
    let output = root.join("config.json");
    write_new(&output, b"complete config").unwrap();
    let m = fs::metadata(&output).unwrap();
    assert_eq!(m.mode() & 0o7777, 0o600);
    assert_eq!(m.nlink(), 1);
    assert_eq!(
        write_new(&output, b"replace").unwrap_err().code,
        "OUTPUT_EXISTS"
    );
    assert_eq!(fs::read(&output).unwrap(), b"complete config");
    assert!(write_new(&root.join("missing/config"), b"new").is_err());
    assert!(!root.join("missing").exists());
    symlink(&output, root.join("alias")).unwrap();
    assert_eq!(
        write_new(&root.join("alias"), b"replace").unwrap_err().code,
        "OUTPUT_EXISTS"
    );
    assert_eq!(
        write_new(&root.join("large"), &vec![0; MAX_BYTES + 1])
            .unwrap_err()
            .code,
        "SETUP_LIMIT"
    );
    assert!(!root.join("large").exists());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
}

#[test]
fn errors_do_not_include_paths_or_raw_secrets() {
    let (_temporary, root) = directory();
    let path = root.join("SECRET_PATH_CANARY");
    let mut store = Store::create(&path).unwrap();
    store.save(b"SECRET_RAW_CANARY").unwrap();
    put(&path.join(revision_name(1)), b"SECRET_CHANGED_CANARY");
    let e = store.read().unwrap_err();
    let text = format!("{e:?} {e}");
    assert!(!text.contains("SECRET_"));
    assert!(!text.contains(root.to_str().unwrap()));
}

#[test]
fn creation_makes_only_missing_private_ancestors_and_accepts_final_export() {
    let (_temporary, root) = directory();
    let parent = root.join("existing");
    fs::create_dir(&parent).unwrap();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
    let path = parent.join("private/nested/state");
    let mut store = Store::create(&path).unwrap();
    assert_eq!(fs::metadata(&parent).unwrap().mode() & 0o7777, 0o755);
    for directory in [
        parent.join("private"),
        parent.join("private/nested"),
        path.clone(),
    ] {
        assert_eq!(fs::metadata(directory).unwrap().mode() & 0o7777, 0o700);
    }
    store.save(b"checkpoint").unwrap();
    write_new(&path.join("instance.json"), b"config export").unwrap();
    assert_eq!(
        Store::open(&path).unwrap().read().unwrap().unwrap(),
        b"checkpoint"
    );
    store.save(b"after export").unwrap();
    assert_eq!(
        fs::read(path.join("instance.json")).unwrap(),
        b"config export"
    );
    // A crash after publishing the optional export has the same recoverable
    // private-link interval as a checkpoint publication.
    fs::hard_link(path.join("instance.json"), path.join(orphan_name())).unwrap();
    assert_eq!(
        Store::open(&path).unwrap().read().unwrap().unwrap(),
        b"after export"
    );
}
