//! Linux dispatcher coverage runs as the ordinary CI user. These cases do not
//! execute a JRE, sudo, root installer or systemd, and do not certify runtime
//! acceptance. No platform override or fake root ownership is used.
#![cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]

use bxdl::{cli, setup::linux::Preparation};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor},
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
};

const NAME: &str = "linux-draft";
const CANARY: &str = "PRIVATE-LINUX-SOURCE-94852";

struct NoInput;
impl io::Read for NoInput {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("read-only or refused Linux command read input")
    }
}
impl io::BufRead for NoInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        panic!("read-only or refused Linux command read input")
    }
    fn consume(&mut self, _: usize) {
        panic!("read-only or refused Linux command consumed input")
    }
}
struct Run {
    code: i32,
    out: String,
    err: String,
}
impl Run {
    fn expect(&self, code: i32) {
        assert_eq!(self.code, code, "{}{}", self.out, self.err);
    }
    fn json(&self) -> Value {
        assert!(self.err.is_empty(), "{}", self.err);
        serde_json::from_str(&self.out).unwrap()
    }
}
fn run(args: &[String], input: &mut dyn io::BufRead, interactive: bool) -> Run {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = cli::run_with_input(args, input, interactive, &mut out, &mut err);
    Run {
        code,
        out: String::from_utf8(out).unwrap(),
        err: String::from_utf8(err).unwrap(),
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        assert_ne!(
            rustix::process::getuid().as_raw(),
            0,
            "Linux preparation CLI tests require an ordinary non-root test user"
        );
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            home: root.join("사용자 관리 홈"),
            root,
            _temp: temp,
        }
    }
    fn args(&self, words: &[&str]) -> Vec<String> {
        words
            .iter()
            .map(|word| (*word).to_owned())
            .chain(["--home".into(), self.home.to_str().unwrap().into()])
            .collect()
    }
    fn create(&self) {
        run(
            &self.args(&["setup", "new", NAME]),
            &mut Cursor::new("y\n:cancel\n"),
            true,
        )
        .expect(5);
    }
    fn workspace(&self) -> PathBuf {
        self.home.join("sessions").join(NAME)
    }
    fn query(&self, words: &[&str]) -> Run {
        run(&self.args(words), &mut NoInput, false)
    }
    fn edit(&self, field: &str, input: &mut dyn io::BufRead) -> Run {
        run(
            &self.args(&["setup", "edit", NAME, "--field", field]),
            input,
            true,
        )
    }
}
#[derive(Debug, PartialEq, Eq)]
struct FileState {
    mode: u32,
    uid: u32,
    device: u64,
    inode: u64,
    modified: (i64, i64),
    bytes: Vec<u8>,
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, FileState> {
    fn visit(base: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, FileState>) {
        let m = fs::symlink_metadata(path).unwrap();
        entries.insert(
            path.strip_prefix(base).unwrap().into(),
            FileState {
                mode: m.mode(),
                uid: m.uid(),
                device: m.dev(),
                inode: m.ino(),
                modified: (m.mtime(), m.mtime_nsec()),
                bytes: if m.is_file() {
                    fs::read(path).unwrap()
                } else if m.file_type().is_symlink() {
                    fs::read_link(path)
                        .unwrap()
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec()
                } else {
                    Vec::new()
                },
            },
        );
        if m.is_dir() {
            for child in fs::read_dir(path).unwrap() {
                visit(base, &child.unwrap().path(), entries);
            }
        }
    }
    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}

#[test]
fn default_linux_route_saves_fixed_identity_and_sources_then_resumes_without_rewriting() {
    let f = Fixture::new();
    let runtime = f.root.join("unavailable-runtime");
    let jar = f.root.join("unavailable-engine.jar");
    let mut input = Cursor::new(format!(
        "y\n{}\n{}\n:cancel\n",
        runtime.display(),
        jar.display()
    ));
    let created = run(&f.args(&["setup", "new", NAME]), &mut input, true);
    created.expect(5);
    assert!(created.err.contains("LINUX_SETUP_PAUSED"));
    assert!(!created.out.contains("LINUX_SETUP_PAUSED"));
    let s = Preparation::open(&f.workspace()).unwrap();
    assert_eq!(s.field_value("instanceId").as_deref(), Some(NAME));
    assert_eq!(
        s.field_value("dataDirectory").as_deref(),
        Some("/var/lib/bxdl/instances/linux-draft/data")
    );
    assert_eq!(s.sources().runtime_root.as_ref(), Some(&runtime));
    assert_eq!(s.sources().engine_jar.as_ref(), Some(&jar));
    assert!(s.sources().native_config.is_none());
    let summary = s.summary().unwrap();
    assert_eq!(summary.platform, "linux-amd64");
    assert_eq!(summary.installation, "NOT_STARTED");
    assert_eq!(summary.engine_validation, "NOT_RUN");
    assert_eq!(summary.missing_inputs, ["nativeConfig"]);
    assert_eq!(summary.revision, 0);
    drop(s);
    assert!(
        fs::read_dir(f.workspace().join("generated"))
            .unwrap()
            .next()
            .is_none()
    );
    let before = snapshot(&f.root);
    for answer in ["", ":cancel\n", ":back\n:cancel\n"] {
        let result = run(
            &f.args(&["setup", "resume", NAME]),
            &mut Cursor::new(answer),
            true,
        );
        result.expect(5);
        assert_eq!(before, snapshot(&f.root));
    }
    assert!(!runtime.exists());
    assert!(!jar.exists());
    for (path, state) in snapshot(&f.home) {
        let m = fs::symlink_metadata(f.home.join(path)).unwrap();
        assert_eq!(state.uid, rustix::process::getuid().as_raw());
        assert_eq!(state.mode & 0o7777, if m.is_dir() { 0o700 } else { 0o600 });
    }
}

#[test]
fn list_and_show_redact_private_sources_and_report_draft_without_runtime_observation() {
    let f = Fixture::new();
    f.create();
    let private = f.root.join(CANARY);
    let mut session = Preparation::open(&f.workspace()).unwrap();
    session.set_source("runtimeRoot", &private).unwrap();
    session
        .set_field("validatorPasswordFile", private.to_str().unwrap())
        .unwrap();
    session.set_field("httpPort", "28080").unwrap();
    drop(session);
    let before = snapshot(&f.root);
    let listed = f.query(&["setup", "list", "--json"]);
    listed.expect(0);
    let list = listed.json();
    assert_eq!(list["data"]["runtimeObservation"], "NOT_CHECKED");
    assert_eq!(list["data"]["entries"][0]["name"], NAME);
    assert_eq!(list["data"]["entries"][0]["preparation"], "DRAFT");
    let shown = f.query(&["setup", "show", NAME, "--json"]);
    shown.expect(0);
    let show = shown.json();
    assert_eq!(show["data"]["reviewedInputs"], "NOT_REVIEWED");
    assert_eq!(show["data"]["installation"], "NOT_STARTED");
    assert_eq!(show["data"]["runtimeReadiness"], "NOT_CHECKED");
    assert_eq!(
        show["data"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| field["name"] == "httpPort")
            .unwrap()["value"],
        "28080"
    );
    for result in [&listed, &shown] {
        assert!(!result.out.contains(CANARY));
        assert!(!result.err.contains(CANARY));
    }
    assert_eq!(before, snapshot(&f.root));
}

#[test]
fn list_preserves_corrupt_entries_and_never_follows_preparation_links() {
    let f = Fixture::new();
    f.create();
    let sessions = f.home.join("sessions");
    let broken = sessions.join("broken");
    fs::create_dir(&broken).unwrap();
    fs::set_permissions(&broken, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(sessions.join("plain-file"), "not a preparation").unwrap();
    symlink(f.workspace(), sessions.join("linked")).unwrap();
    let before = snapshot(&f.root);
    let result = f.query(&["setup", "list", "--json"]);
    result.expect(0);
    let data = result.json();
    let entries = data["data"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 4);
    for entry in entries {
        assert_eq!(
            entry["preparation"],
            if entry["name"] == NAME {
                "DRAFT"
            } else {
                "UNAVAILABLE"
            }
        );
    }
    f.query(&["setup", "show", "linked", "--json"]).expect(3);
    assert_eq!(before, snapshot(&f.root));
}

#[test]
fn show_respects_active_writer_while_list_keeps_read_only_summary() {
    let f = Fixture::new();
    f.create();
    let writer = Preparation::open(&f.workspace()).unwrap();
    let before = snapshot(&f.root);
    let shown = f.query(&["setup", "show", NAME, "--json"]);
    shown.expect(3);
    assert_eq!(shown.json()["reasonCode"], "SETUP_WORKFLOW_BUSY");
    let listed = f.query(&["setup", "list", "--json"]);
    listed.expect(0);
    assert_eq!(listed.json()["data"]["entries"][0]["preparation"], "DRAFT");
    assert!(Preparation::open(&f.workspace()).is_err());
    assert_eq!(before, snapshot(&f.root));
    drop(writer);
    f.query(&["setup", "show", NAME, "--json"]).expect(0);
    assert_eq!(before, snapshot(&f.root));
}

#[test]
fn edit_changes_only_one_field_and_invalid_cancel_or_same_value_preserve_bytes() {
    let f = Fixture::new();
    f.create();
    let source = f.root.join("unavailable-engine.jar");
    let mut session = Preparation::open(&f.workspace()).unwrap();
    session.set_source("engineJar", &source).unwrap();
    session.set_field("p2pPort", "19090").unwrap();
    drop(session);
    f.edit("httpPort", &mut Cursor::new("28080\n")).expect(0);
    let s = Preparation::open(&f.workspace()).unwrap();
    assert_eq!(s.field_value("httpPort").as_deref(), Some("28080"));
    assert_eq!(s.field_value("p2pPort").as_deref(), Some("19090"));
    assert_eq!(s.field_value("instanceId").as_deref(), Some(NAME));
    assert_eq!(s.sources().engine_jar.as_ref(), Some(&source));
    drop(s);
    let before = snapshot(&f.root);
    f.edit("httpPort", &mut Cursor::new("28080\n")).expect(0);
    assert_eq!(before, snapshot(&f.root));
    let mut invalid = Cursor::new("0\n29090\n");
    f.edit("httpPort", &mut invalid).expect(3);
    assert_eq!(
        invalid.position(),
        2,
        "one edit consumes only its one answer"
    );
    assert_eq!(before, snapshot(&f.root));
    for answer in ["", "\n", ":cancel\n", ":back\n", ":help\n"] {
        f.edit("httpPort", &mut Cursor::new(answer)).expect(5);
        assert_eq!(before, snapshot(&f.root));
    }
}

#[test]
fn fixed_identity_fields_and_mac_package_options_are_refused_before_input_or_home_creation() {
    let f = Fixture::new();
    let before = snapshot(&f.root);
    for field in ["instanceId", "dataDirectory", "password"] {
        f.edit(field, &mut NoInput).expect(2);
    }
    for flag in ["--archive", "--package", "--lock", "--network"] {
        run(
            &f.args(&["setup", "new", NAME, flag, "/unavailable/macos-package"]),
            &mut NoInput,
            true,
        )
        .expect(2);
    }
    assert!(!f.home.exists());
    assert_eq!(before, snapshot(&f.root));
}

#[test]
fn unreviewed_apply_refuses_before_confirmation_and_retains_journal_without_install_intent() {
    let f = Fixture::new();
    f.create();
    let before = snapshot(&f.root);
    let result = run(&f.args(&["setup", "apply", NAME]), &mut NoInput, true);
    result.expect(3);
    assert!(result.err.contains("LINUX_PREPARATION_NOT_REVIEWED"));
    assert_eq!(before, snapshot(&f.root));
    let s = Preparation::open(&f.workspace()).unwrap();
    assert_eq!(s.summary().unwrap().installation, "NOT_STARTED");
    assert!(s.installed().is_none());
}

#[test]
fn user_owned_linux_descriptor_cannot_be_linked_or_used_as_a_privileged_worker() {
    let f = Fixture::new();
    let control = f.root.join("untrusted-control");
    fs::create_dir(&control).unwrap();
    fs::set_permissions(&control, fs::Permissions::from_mode(0o755)).unwrap();
    let worker = control.join("worker");
    fs::write(&worker, b"untrusted worker must never execute").unwrap();
    fs::set_permissions(&worker, fs::Permissions::from_mode(0o755)).unwrap();
    let m = fs::metadata(&control).unwrap();
    assert_eq!(m.uid(), rustix::process::getuid().as_raw());
    let descriptor = json!({
        "schemaVersion": 1,
        "protocol": "BXDL_LINUX_DEVELOPMENT_EXECUTOR_V1",
        "instanceId": NAME,
        "controlDirectory": control,
        "controlDevice": m.dev(),
        "controlInode": m.ino(),
        "workerSha256": "a".repeat(64),
        "workerSize": fs::metadata(&worker).unwrap().len(),
    });
    fs::write(
        control.join("management.json"),
        serde_json::to_vec(&descriptor).unwrap(),
    )
    .unwrap();
    fs::set_permissions(
        control.join("management.json"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let before = snapshot(&f.root);
    for words in [
        vec![
            "nodes",
            "add",
            NAME,
            "--instance",
            control.to_str().unwrap(),
            "--json",
        ],
        vec!["status", "--instance", control.to_str().unwrap(), "--json"],
    ] {
        let args = if words[0] == "nodes" {
            f.args(&words)
        } else {
            // Explicit instance operations reject --home by design.
            words.iter().map(|word| (*word).to_owned()).collect()
        };
        let result = run(&args, &mut NoInput, false);
        result.expect(3);
        assert_eq!(
            result.json()["reasonCode"],
            "LINUX_SERVICE_AUTHORITY_UNSAFE"
        );
        assert_eq!(before, snapshot(&f.root));
        assert!(!f.home.exists());
    }
}
