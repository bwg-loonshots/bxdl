//! Direct name linking is a catalog operation, never registration or runtime work.
#![cfg(unix)]

use bxdl::{cli, setup::Session};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor},
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
};

const CANARY: &str = "PRIVATE-NODES-CANARY-81365";

struct NoInput;
impl io::Read for NoInput {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("direct nodes commands must not read standard input")
    }
}
impl io::BufRead for NoInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        panic!("direct nodes commands must not read standard input")
    }
    fn consume(&mut self, _: usize) {
        panic!("direct nodes commands must not consume standard input")
    }
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}
impl Run {
    fn text(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
    fn assert_code(&self, code: i32) {
        assert_eq!(self.code, code, "{}", self.text());
        assert!(!self.text().contains(CANARY), "{}", self.text());
    }
    fn json(&self) -> Value {
        assert!(self.stderr.is_empty(), "{}", self.stderr);
        serde_json::from_str(&self.stdout).expect("one complete JSON envelope")
    }
}
fn invoke(args: &[String], input: &mut dyn io::BufRead, tty: bool) -> Run {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = cli::run_with_input(args, input, tty, &mut stdout, &mut stderr);
    Run {
        code,
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    }
}
fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}
fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn directory(path: &Path) {
    fs::create_dir(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn identity(path: &Path) -> Value {
    let metadata = fs::symlink_metadata(path).unwrap();
    json!({"device": metadata.dev(), "inode": metadata.ino()})
}

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    control: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let home = root.join("management parent/관리 홈");
        let control = root.join(format!("{CANARY} control"));
        instance_at(&control, &root);
        Self {
            _temporary: temporary,
            root,
            home,
            control,
        }
    }
    fn args(&self, tail: &[&str]) -> Vec<String> {
        let mut args = words(&["nodes"]);
        args.extend(words(tail));
        args.extend(words(&["--home", self.home.to_str().unwrap()]));
        args
    }
    fn add_at(&self, name: &str, control: &Path, json: bool) -> Run {
        let mut args = self.args(&["add", name, "--instance", control.to_str().unwrap()]);
        if json {
            args.push("--json".into());
        }
        invoke(&args, &mut NoInput, false)
    }
    fn add(&self, name: &str) -> Run {
        self.add_at(name, &self.control, true)
    }
    fn list(&self, json: bool) -> Run {
        let mut args = self.args(&[]);
        if json {
            args.push("--json".into());
        }
        invoke(&args, &mut NoInput, false)
    }
    fn save_legacy(&self, label: &str) -> Value {
        let args = words(&[
            "console",
            "--home",
            self.home.to_str().unwrap(),
            "--instance",
            self.control.to_str().unwrap(),
        ]);
        let result = invoke(
            &args,
            &mut Cursor::new(format!("2\n{label}\ny\n0\n0\n")),
            true,
        );
        result.assert_code(0);
        let record = catalog(&self.home);
        assert_eq!(record["schemaVersion"], 1);
        record
    }
}

// Fixture a previously completed registration. Package, native inputs and DB
// intentionally do not exist: catalog metadata must not execute or rebuild them.
// Existing instance show validates this private on-disk fixture before use.
fn instance_at(control: &Path, references: &Path) {
    directory(control);
    directory(&control.join("operations"));
    directory(&control.join("journal"));
    write(&control.join(".operation.lock"), b"");
    write(&control.join(".control.json"), b"");
    let marker = json!({
        "version": 1,
        "root": identity(control),
        "lock": identity(&control.join(".operation.lock")),
        "operations": identity(&control.join("operations")),
        "record": identity(&control.join(".control.json")),
    });
    write(
        &control.join(".control.json"),
        serde_json::to_vec(&marker).unwrap(),
    );
    let engine_identity: Value = serde_json::from_slice(include_bytes!(
        "../contracts/nigo/development-clean-2026-09-18/evidence/engine-info.json"
    ))
    .unwrap();
    let binding = json!({
        "schemaVersion": 1,
        "instanceId": "public-validator",
        "package": references.join("missing-package"),
        "archive": references.join("missing-archive.tar.gz"),
        "allowUnsignedDevelopment": true,
        "product": references.join("missing-product.json"),
        "native": references.join("missing-node.json"),
        "lock": references.join("missing-lock.json"),
        "archiveSha256": "a".repeat(64),
        "manifestSha256": "b".repeat(64),
        "identity": engine_identity,
        "backend": "rocksdb",
        "chainFingerprint": "c".repeat(64),
        "nodeIdentity": format!("0x{}:0x{}", "1".repeat(64), "2".repeat(40)),
        "dataDirectory": references.join("must-not-create-data"),
        "pins": [],
    });
    let raw = serde_json::to_vec(&binding).unwrap();
    write(&control.join("binding.json"), &raw);
    let state = json!({
        "schemaVersion": 1,
        "instanceId": "public-validator",
        "bindingSha256": hex::encode(Sha256::digest(&raw)),
        "phase": "REGISTERED",
        "reason": "REGISTERED_COLD_CHECKED",
    });
    write(
        &control.join("journal/revision00000001.json"),
        serde_json::to_vec(&state).unwrap(),
    );
    let result = invoke(
        &words(&[
            "instance",
            "show",
            "--instance",
            control.to_str().unwrap(),
            "--json",
        ]),
        &mut NoInput,
        false,
    );
    result.assert_code(0);
    assert_eq!(result.json()["data"]["initialization"], "NOT_STARTED");
}

#[derive(Debug, PartialEq, Eq)]
struct FileSnapshot {
    directory: bool,
    mode: u32,
    device: u64,
    inode: u64,
    bytes: Vec<u8>,
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, FileSnapshot> {
    fn visit(base: &Path, path: &Path, output: &mut BTreeMap<PathBuf, FileSnapshot>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        assert!(!metadata.file_type().is_symlink());
        output.insert(
            path.strip_prefix(base).unwrap().to_owned(),
            FileSnapshot {
                directory: metadata.is_dir(),
                mode: metadata.mode(),
                device: metadata.dev(),
                inode: metadata.ino(),
                bytes: if metadata.is_file() {
                    fs::read(path).unwrap()
                } else {
                    Vec::new()
                },
            },
        );
        if metadata.is_dir() {
            for child in fs::read_dir(path).unwrap() {
                visit(base, &child.unwrap().path(), output);
            }
        }
    }
    let mut output = BTreeMap::new();
    if root.exists() {
        visit(root, root, &mut output);
    }
    output
}
fn latest_catalog(home: &Path) -> PathBuf {
    let mut files: Vec<_> = fs::read_dir(home.join("catalog"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    files.pop().unwrap()
}
fn catalog(home: &Path) -> Value {
    serde_json::from_slice(&fs::read(latest_catalog(home)).unwrap()).unwrap()
}

#[test]
fn list_missing_home_is_successful_without_input_or_directory_creation() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    for json in [false, true] {
        let result = fixture.list(json);
        result.assert_code(0);
        if json {
            let report = result.json();
            assert_eq!(report["reasonCode"], "NODE_LIST");
            assert!(report["data"]["entries"].as_array().unwrap().is_empty());
            assert_eq!(report["data"]["currentStatus"], "NOT_OBSERVED");
        }
    }
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn help_is_exact_read_only_and_available_without_a_terminal() {
    for args in [
        words(&["nodes", "--help"]),
        words(&["help", "nodes"]),
        words(&["nodes", "add", "--help"]),
        words(&["help", "nodes", "add"]),
    ] {
        let result = invoke(&args, &mut NoInput, false);
        result.assert_code(0);
        assert!(result.text().contains("nodes"));
    }
}

#[test]
fn invalid_arguments_and_names_have_no_input_or_filesystem_effects() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    let control = fixture.control.to_str().unwrap();
    for tail in [
        vec!["add"],
        vec!["add", "local"],
        vec!["add", "local", "--instance"],
        vec!["add", "local", "--instance", control, "--instance", control],
        vec!["add", "local", "--instance", control, "--home", "elsewhere"],
        vec!["add", "local", "--instance", control, "--unknown", CANARY],
        vec!["add", "local", "--instance", control, "extra"],
        vec!["--instance", control],
        vec!["list"],
        vec!["--unknown", CANARY],
    ] {
        let result = invoke(&fixture.args(&tail), &mut NoInput, false);
        result.assert_code(2);
    }
    for name in [
        "",
        "Local",
        "1node",
        "two words",
        "node/name",
        "한글",
        ".",
        "..",
        "--help",
    ] {
        fixture.add(name).assert_code(2);
    }
    fixture.add(&"n".repeat(33)).assert_code(2);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn invalid_names_report_a_specific_error_without_echoing_input_or_writing() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    for name in ["Local", "node_1", "한글"] {
        let result = fixture.add(name);
        result.assert_code(2);
        assert_eq!(result.json()["reasonCode"], "NODE_NAME_INVALID");
        assert!(!result.text().contains(name), "{}", result.text());
    }
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn add_links_existing_node_without_a_menu_or_modifying_registered_state() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.control);
    let result = fixture.add("local");
    result.assert_code(0);
    let report = result.json();
    assert_eq!(report["reasonCode"], "NODE_NAME_LINKED");
    assert_eq!(report["data"]["name"], "local");
    assert_eq!(report["data"]["currentStatus"], "NOT_OBSERVED");
    assert_eq!(report["data"]["id"].as_str().unwrap().len(), 32);
    let linked_id = report["data"]["id"].clone();
    assert_eq!(snapshot(&fixture.control), before);
    assert!(!fixture.root.join("must-not-create-data").exists());
    assert!(!fixture.root.join("missing-package").exists());
    for (path, entry) in snapshot(&fixture.home) {
        assert_eq!(
            entry.mode & 0o777,
            if entry.directory { 0o700 } else { 0o600 },
            "{}",
            path.display()
        );
    }
    let before = snapshot(&fixture.root);
    let result = fixture.list(true);
    result.assert_code(0);
    let report = result.json();
    let entries = report["data"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["name"], "local");
    assert_eq!(entries[0]["instanceId"], "public-validator");
    assert_eq!(entries[0]["currentStatus"], "NOT_OBSERVED");
    assert_eq!(entries[0]["id"], linked_id);
    fixture.list(false).assert_code(0);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn same_name_and_target_is_idempotent_and_does_not_append_a_revision() {
    let fixture = Fixture::new();
    let first = fixture.add("local");
    first.assert_code(0);
    let before = snapshot(&fixture.root);
    let again = fixture.add("local");
    again.assert_code(0);
    assert_eq!(again.json()["data"]["id"], first.json()["data"]["id"]);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn names_are_unique_and_existing_target_is_not_implicitly_renamed() {
    let fixture = Fixture::new();
    fixture.add("local").assert_code(0);
    let other = fixture.root.join("other-control");
    instance_at(&other, &fixture.root);
    let before = snapshot(&fixture.root);
    let duplicate = fixture.add_at("local", &other, true);
    assert_ne!(duplicate.code, 0, "{}", duplicate.text());
    assert!(!duplicate.text().contains(CANARY));
    let rename = fixture.add("another-name");
    assert_ne!(rename.code, 0, "{}", rename.text());
    assert_eq!(snapshot(&fixture.root), before);
    // Duplicate public instanceId does not imply a duplicate target.
    fixture.add_at("other", &other, true).assert_code(0);
    assert_eq!(
        fixture.list(true).json()["data"]["entries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn legacy_catalog_reads_do_not_migrate_and_linking_preserves_id_and_label() {
    let fixture = Fixture::new();
    let legacy = fixture.save_legacy("기존 표시 이름");
    let before = snapshot(&fixture.root);
    let listed = fixture.list(true);
    listed.assert_code(0);
    assert_eq!(
        listed.json()["data"]["entries"][0]["label"],
        "기존 표시 이름"
    );
    assert!(listed.json()["data"]["entries"][0]["name"].is_null());
    assert_eq!(snapshot(&fixture.root), before);
    let old_revision = latest_catalog(&fixture.home);
    let old_bytes = fs::read(&old_revision).unwrap();
    fixture.add("local").assert_code(0);
    let current = catalog(&fixture.home);
    assert_eq!(current["schemaVersion"], 2);
    assert_eq!(current["entries"][0]["id"], legacy["entries"][0]["id"]);
    assert_eq!(
        current["entries"][0]["label"],
        legacy["entries"][0]["label"]
    );
    assert_eq!(fs::read(old_revision).unwrap(), old_bytes);
}

#[test]
fn listing_cached_nodes_does_not_reopen_missing_or_replaced_targets() {
    let fixture = Fixture::new();
    fixture.add("local").assert_code(0);
    fs::rename(&fixture.control, fixture.root.join("original-control")).unwrap();
    let before = snapshot(&fixture.root);
    let result = fixture.list(true);
    result.assert_code(0);
    assert_eq!(result.json()["data"]["entries"][0]["name"], "local");
    assert_eq!(snapshot(&fixture.root), before);
    // A same-path replacement must not change the cached list or bind the name.
    instance_at(&fixture.control, &fixture.root);
    let before = snapshot(&fixture.root);
    fixture.list(true).assert_code(0);
    assert_eq!(snapshot(&fixture.root), before);
    assert_ne!(fixture.add("local").code, 0);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn node_list_excludes_preparation_entries_without_opening_them() {
    let fixture = Fixture::new();
    let draft = fixture.root.join("draft");
    Session::create(&draft, None).unwrap();
    let result = invoke(
        &words(&["console", "--home", fixture.home.to_str().unwrap()]),
        &mut Cursor::new(format!(
            "4\n3\n{}\n2\n설정 초안\ny\n0\n0\n",
            draft.display()
        )),
        true,
    );
    result.assert_code(0);
    fs::remove_dir_all(&draft).unwrap();
    let before = snapshot(&fixture.root);
    let result = fixture.list(true);
    result.assert_code(0);
    assert!(
        result.json()["data"]["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn unsupported_missing_and_symlink_targets_do_not_create_management_home() {
    let fixture = Fixture::new();
    let draft = fixture.root.join("draft");
    Session::create(&draft, None).unwrap();
    let before = snapshot(&fixture.root);
    for target in [&draft, &fixture.root.join("missing-control")] {
        let result = fixture.add_at("local", target, true);
        assert_ne!(result.code, 0, "{}", result.text());
        assert!(!result.text().contains(CANARY));
    }
    assert_eq!(snapshot(&fixture.root), before);
    let link = fixture.root.join("symlink-control");
    symlink(&fixture.control, &link).unwrap();
    let before = snapshot(&fixture.control);
    assert_ne!(fixture.add_at("local", &link, true).code, 0);
    assert_eq!(snapshot(&fixture.control), before);
    assert!(!fixture.home.exists());
}

#[test]
fn corrupt_catalog_is_preserved_and_explicit_instance_show_still_works() {
    let fixture = Fixture::new();
    fixture.add("local").assert_code(0);
    write(
        &latest_catalog(&fixture.home),
        format!("{CANARY} invalid JSON"),
    );
    let before = snapshot(&fixture.root);
    for result in [fixture.list(true), fixture.add("other")] {
        assert_ne!(result.code, 0, "{}", result.text());
        assert!(!result.text().contains(CANARY));
    }
    let result = invoke(
        &words(&[
            "instance",
            "show",
            "--instance",
            fixture.control.to_str().unwrap(),
            "--json",
        ]),
        &mut NoInput,
        false,
    );
    result.assert_code(0);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn overlapping_management_location_is_rejected_before_any_write() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    for home in [
        fixture.control.join("management"),
        fixture.root.join("must-not-create-data/management"),
    ] {
        let result = invoke(
            &words(&[
                "nodes",
                "add",
                "local",
                "--home",
                home.to_str().unwrap(),
                "--instance",
                fixture.control.to_str().unwrap(),
                "--json",
            ]),
            &mut NoInput,
            false,
        );
        assert_ne!(result.code, 0, "{}", result.text());
        assert!(!result.text().contains(CANARY));
    }
    assert_eq!(snapshot(&fixture.root), before);
}
