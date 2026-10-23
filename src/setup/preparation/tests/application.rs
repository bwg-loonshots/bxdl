use super::*;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::setup::workflow::Workflow;

#[test]
fn approved_binding_survives_missing_supply_but_requires_immutable_local_snapshots() {
    let f = Fixture::new();
    let mut preparation = f.session();
    preparation.prepare().unwrap();
    let source = preparation.application_source().unwrap();
    assert_eq!(source.revision, 1);
    fs::remove_file(&f.archive).unwrap();
    fs::remove_file(&f.lock).unwrap();
    assert_eq!(source, preparation.application_binding().unwrap());
    assert!(preparation.application_source().is_err());
    let raw = fs::read(&source.native).unwrap();
    let replacement = f.root.join("replacement.json");
    write(&replacement, &raw, 0o600);
    fs::rename(&replacement, &source.native).unwrap();
    assert_eq!(
        preparation.application_binding().unwrap_err().code,
        "SETUP_APPLICATION_SOURCE_CHANGED"
    );
    f.assert_no_operations();
}

#[test]
fn draft_edits_and_new_revision_invalidate_the_old_binding() {
    let f = Fixture::new();
    let mut preparation = f.session();
    preparation.prepare().unwrap();
    let old = preparation.application_source().unwrap();
    preparation.set_field("httpPort", "18081").unwrap();
    assert!(old.recheck(false).is_err());
    preparation.prepare().unwrap();
    let new = preparation.application_source().unwrap();
    assert_ne!(old, new);
    assert_eq!(new.revision, 2);
    assert!(old.recheck(false).is_err());
    new.recheck(false).unwrap();
    f.assert_no_operations();
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn execution_is_bound_initially_and_inspection_is_readonly_even_while_locked() {
    let f = Fixture::new();
    let mut preparation = f.session();
    preparation.prepare().unwrap();
    let source = preparation.application_source().unwrap();
    let operation = f.root.join("operation");
    let flow = Workflow::create_prepared(&operation, &source).unwrap();
    let before = tree(&operation);
    let inspected = Workflow::inspect_prepared(&operation, &source, true).unwrap();
    assert_eq!(inspected.verification, "PASSED");
    assert_eq!(inspected.source_match, "MATCH");
    assert_eq!(inspected.installation, "PENDING");
    assert!(!inspected.registration_confirmed);
    assert_eq!(inspected.instance, source.instance());
    assert_eq!(tree(&operation), before);
    let without_source = Workflow::inspect_application(&operation, None, true).unwrap();
    assert_eq!(without_source.source_match, "UNAVAILABLE");
    assert_eq!(without_source.verification, "FAILED");
    assert_eq!(without_source.installation, "PENDING");
    assert_eq!(tree(&operation), before);
    let safe = serde_json::to_string(&inspected).unwrap();
    for private in ["sha256", CANARY, "credential-", "reviewSha256", "rootInode"] {
        assert!(!safe.contains(private));
    }
    drop(flow);
    Workflow::resume_prepared(&operation, &source).unwrap();
    f.assert_no_operations();
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn old_revision_cannot_resume_or_adopt_an_unbound_legacy_execution() {
    let f = Fixture::new();
    let mut preparation = f.session();
    preparation.prepare().unwrap();
    let source = preparation.application_source().unwrap();
    let operation = f.root.join("operation");
    drop(Workflow::create_prepared(&operation, &source).unwrap());
    preparation.set_field("httpPort", "18081").unwrap();
    preparation.prepare().unwrap();
    let changed = preparation.application_source().unwrap();
    assert_eq!(
        Workflow::resume_prepared(&operation, &changed)
            .err()
            .unwrap()
            .code,
        "SETUP_APPLICATION_SOURCE_CHANGED"
    );
    let mismatch = Workflow::inspect_prepared(&operation, &changed, true).unwrap();
    assert_eq!(mismatch.source_match, "MISMATCH");
    assert_eq!(mismatch.preparation_revision, 1);
    assert_eq!(mismatch.verification, "FAILED");
    let legacy = f.root.join("legacy");
    drop(Workflow::create(&legacy, Some(&f.source)).unwrap());
    let before = tree(&legacy);
    assert_eq!(
        Workflow::resume_prepared(&legacy, &changed)
            .err()
            .unwrap()
            .code,
        "SETUP_APPLICATION_SOURCE_CHANGED"
    );
    assert_eq!(tree(&legacy), before);
    f.assert_no_operations();
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn rejected_apply_guard_leaves_all_journals_and_outputs_unchanged() {
    let f = Fixture::new();
    let mut preparation = f.session();
    preparation.prepare().unwrap();
    let source = preparation.application_source().unwrap();
    let operation = f.root.join("operation");
    let mut flow = Workflow::create_prepared(&operation, &source).unwrap();
    let before = tree(&operation);
    let error = flow
        .apply_prepared(&source, || Err(error("GUARD_TEST", "changed")))
        .unwrap_err();
    assert_eq!(error.code, "GUARD_TEST");
    assert_eq!(tree(&operation), before);
    f.assert_no_operations();
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn tree(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(tree(&path));
        } else {
            out.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    out
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn application_registers_once_and_reapplies_from_owned_supply_without_writes() {
    let f = Fixture::with_executor(true);
    let mut preparation = f.session();
    preparation.prepare().unwrap();
    let source = preparation.application_source().unwrap();
    for directory in ["packages", "instances", "data"] {
        fs::create_dir(f.root.join(directory)).unwrap();
    }
    let operation = f.root.join("operation");
    let mut flow = Workflow::create_prepared(&operation, &source).unwrap();
    let applied = flow.apply_prepared(&source, || Ok(())).unwrap();
    assert_eq!(applied.verification, "PASSED");
    assert_eq!(applied.installation, "CONFIRMED");
    assert_eq!(applied.registration, "CONFIRMED");
    assert_eq!(applied.initialization, "PENDING");
    assert!(applied.registration_confirmed);
    assert!(!source.data_directory.exists());
    let calls = fs::read_to_string(f.root.join("fixture-calls")).unwrap();
    assert!(calls.contains("preflight"));
    assert!(!calls.contains("init"));
    assert!(!calls.contains("run"));
    drop(flow);
    fs::remove_file(&f.archive).unwrap();
    fs::remove_file(&f.lock).unwrap();
    assert!(preparation.check().is_err());
    let binding = preparation.application_binding().unwrap();
    assert_eq!(source, binding);
    let before = tree(&operation);
    let owned = tree(source.instance());
    let inspected = Workflow::inspect_prepared(&operation, &binding, true).unwrap();
    assert_eq!(inspected.verification, "PASSED");
    assert_eq!(tree(&operation), before);
    let mut resumed = Workflow::resume_prepared(&operation, &binding).unwrap();
    let repeated = resumed.apply_prepared(&binding, || Ok(())).unwrap();
    assert_eq!(repeated.verification, "PASSED");
    assert!(repeated.registration_confirmed);
    assert_eq!(tree(&operation), before);
    assert_eq!(tree(source.instance()), owned);
    assert_eq!(
        fs::read_to_string(f.root.join("fixture-calls")).unwrap(),
        calls
    );
    assert!(!source.data_directory.exists());
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn partial_apply_reuses_its_owned_installation_and_never_overwrites_changed_outputs() {
    let f = Fixture::with_executor(true);
    let mut preparation = f.session();
    preparation.prepare().unwrap();
    let source = preparation.application_source().unwrap();
    for directory in ["packages", "instances", "data"] {
        fs::create_dir(f.root.join(directory)).unwrap();
    }
    let operation = f.root.join("operation");
    let mut flow = Workflow::create_prepared(&operation, &source).unwrap();
    let error = flow
        .apply_prepared(&source, || {
            if source.destination().exists() {
                Err(error("GUARD_TEST", "changed before registration"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert_eq!(error.code, "GUARD_TEST");
    assert!(source.destination().exists());
    assert!(!source.instance().exists());
    assert!(!f.root.join("fixture-calls").exists());
    let inspected = Workflow::inspect_prepared(&operation, &source, true).unwrap();
    assert_eq!(inspected.verification, "PASSED");
    assert_eq!(inspected.installation, "CONFIRMED");
    assert_eq!(inspected.registration, "PENDING");
    let installed = tree(source.destination());
    drop(flow);
    // Simulate a crash after reserving/writing the owned installation but before
    // publishing its final confirmation. Read-only inspection never promotes it.
    let mut store = WorkflowStore::open(&operation).unwrap();
    let mut checkpoint: Value = serde_json::from_slice(&store.read().unwrap()).unwrap();
    checkpoint["installation"]["phase"] = json!("UNKNOWN");
    store
        .save(&serde_json::to_vec(&checkpoint).unwrap())
        .unwrap();
    drop(store);
    let uncertain = tree(&operation);
    let inspected = Workflow::inspect_prepared(&operation, &source, true).unwrap();
    assert_eq!(inspected.installation, "UNKNOWN");
    assert_eq!(inspected.verification, "FAILED");
    assert_eq!(tree(&operation), uncertain);
    let package_file = source.destination().join("engine/nigo-node.jar");
    let original_jar = fs::read(&package_file).unwrap();
    write(&package_file, b"interrupted package content", 0o644);
    let mut flow = Workflow::resume_prepared(&operation, &source).unwrap();
    assert!(flow.apply_prepared(&source, || Ok(())).is_err());
    assert_eq!(tree(&operation), uncertain);
    assert_eq!(
        fs::read(&package_file).unwrap(),
        b"interrupted package content"
    );
    assert!(!source.instance().exists());
    assert!(!f.root.join("fixture-calls").exists());
    // Restoring the test fixture makes this an already complete owned result;
    // the adapter may now reconcile it, but still performs no extraction.
    write(&package_file, &original_jar, 0o644);
    let completed = flow.apply_prepared(&source, || Ok(())).unwrap();
    assert!(completed.registration_confirmed);
    assert_eq!(tree(source.destination()), installed);
    let calls = fs::read(f.root.join("fixture-calls")).unwrap();
    let package_file = source.destination().join("engine/nigo-node.jar");
    write(&package_file, b"changed retained package", 0o644);
    let operation_before = tree(&operation);
    assert!(flow.apply_prepared(&source, || Ok(())).is_err());
    assert_eq!(tree(&operation), operation_before);
    assert_eq!(
        fs::read(&package_file).unwrap(),
        b"changed retained package"
    );
    let changed = Workflow::inspect_prepared(&operation, &source, true).unwrap();
    assert_eq!(changed.verification, "FAILED");
    assert_eq!(changed.installation, "CONFIRMED");
    assert_eq!(changed.registration, "CONFIRMED");
    assert_eq!(fs::read(f.root.join("fixture-calls")).unwrap(), calls);
}
