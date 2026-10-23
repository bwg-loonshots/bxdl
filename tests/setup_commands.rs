//! Named preparation commands preserve journals and never run nodes.
#![cfg(unix)]
use bxdl::{cli, setup::preparation::Preparation};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor},
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
};

struct NoInput;
impl io::Read for NoInput {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("query/rejected command read stdin")
    }
}
impl io::BufRead for NoInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        panic!("query/rejected command read stdin")
    }
    fn consume(&mut self, _: usize) {
        panic!("query/rejected command consumed stdin")
    }
}
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}
impl Run {
    fn expect(&self, code: i32) {
        assert_eq!(self.code, code, "{}{}", self.stdout, self.stderr);
    }
    fn json(&self) -> Value {
        serde_json::from_str(&self.stdout).unwrap()
    }
}
fn run(args: &[String], input: &mut dyn io::BufRead, interactive: bool) -> Run {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = cli::run_with_input(args, input, interactive, &mut out, &mut err);
    Run {
        code,
        stdout: String::from_utf8(out).unwrap(),
        stderr: String::from_utf8(err).unwrap(),
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            home: root.join("준비 관리 홈"),
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
    fn create(&self, name: &str) {
        run(
            &self.args(&["setup", "new", name]),
            &mut Cursor::new("y\n:cancel\n"),
            true,
        )
        .expect(5);
    }
    fn workspace(&self, name: &str) -> PathBuf {
        self.home.join("sessions").join(name)
    }
    fn query(&self, words: &[&str]) -> Run {
        run(&self.args(words), &mut NoInput, false)
    }
    fn edit(&self, name: &str, field: &str, answer: &str) -> Run {
        run(
            &self.args(&["setup", "edit", name, "--field", field]),
            &mut Cursor::new(answer),
            true,
        )
    }
}
#[derive(PartialEq, Eq, Debug)]
struct FileState {
    mode: u32,
    inode: u64,
    device: u64,
    modified: (i64, i64),
    bytes: Vec<u8>,
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, FileState> {
    fn walk(base: &Path, path: &Path, result: &mut BTreeMap<PathBuf, FileState>) {
        let m = fs::symlink_metadata(path).unwrap();
        result.insert(
            path.strip_prefix(base).unwrap().to_owned(),
            FileState {
                mode: m.mode(),
                inode: m.ino(),
                device: m.dev(),
                modified: (m.mtime(), m.mtime_nsec()),
                bytes: if m.is_file() {
                    fs::read(path).unwrap()
                } else if m.file_type().is_symlink() {
                    fs::read_link(path)
                        .unwrap()
                        .to_string_lossy()
                        .as_bytes()
                        .to_vec()
                } else {
                    Vec::new()
                },
            },
        );
        if m.is_dir() {
            for child in fs::read_dir(path).unwrap() {
                walk(base, &child.unwrap().path(), result);
            }
        }
    }
    let mut result = BTreeMap::new();
    if root.exists() {
        walk(root, root, &mut result);
    }
    result
}

#[test]
fn empty_list_and_missing_show_are_read_only_without_input() {
    let f = Fixture::new();
    let before = snapshot(&f.root);
    let result = f.query(&["setup", "list", "--json"]);
    result.expect(0);
    assert_eq!(result.json()["data"]["schemaVersion"], 1);
    assert_eq!(result.json()["data"]["entries"], serde_json::json!([]));
    assert_eq!(result.json()["data"]["partial"], false);
    f.query(&["setup", "show", "local", "--json"]).expect(3);
    assert_eq!(before, snapshot(&f.root));
}

#[test]
fn queries_do_not_lock_or_write_and_redact_reference_paths() {
    let f = Fixture::new();
    f.create("local");
    let mut session = Preparation::resume(&f.workspace("local")).unwrap();
    session
        .set_field(
            "validatorPasswordFile",
            f.root
                .join("PRIVATE-CREDENTIAL-PATH-CANARY")
                .to_str()
                .unwrap(),
        )
        .unwrap();
    let before = snapshot(&f.root);
    // Exclusive mutable session is held. Read-only queries must still work.
    let listed = f.query(&["setup", "list", "--json"]);
    listed.expect(0);
    let shown = f.query(&["setup", "show", "local", "--json"]);
    shown.expect(0);
    let data = shown.json();
    assert_eq!(data["data"]["inspection"]["verification"], "NOT_PREPARED");
    assert_eq!(data["data"]["inspection"]["nextInput"], "archive");
    assert_eq!(
        data["data"]["inspection"]["summary"]["installation"],
        "NOT_PERFORMED"
    );
    assert!(!shown.stdout.contains("PRIVATE-CREDENTIAL-PATH-CANARY"));
    assert!(!listed.stdout.contains("PRIVATE-CREDENTIAL-PATH-CANARY"));
    assert_eq!(before, snapshot(&f.root));
    // Query drop must not release the writer's exclusive lock.
    assert!(Preparation::resume(&f.workspace("local")).is_err());
    drop(session);
    assert!(Preparation::resume(&f.workspace("local")).is_ok());
}

#[test]
fn list_retains_corrupt_named_entries_without_following_links() {
    let f = Fixture::new();
    f.create("valid");
    let invalid = f.workspace("broken");
    fs::create_dir(&invalid).unwrap();
    fs::set_permissions(&invalid, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(f.workspace("plain-file"), "invalid").unwrap();
    symlink(f.workspace("valid"), f.workspace("linked")).unwrap();
    let before = snapshot(&f.root);
    let result = f.query(&["setup", "list", "--json"]);
    result.expect(5);
    let data = result.json();
    assert_eq!(data["reasonCode"], "PREPARATION_LIST_PARTIAL");
    assert_eq!(data["data"]["partial"], true);
    let entries = data["data"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 4);
    for entry in entries {
        assert_eq!(
            entry["record"],
            if entry["name"] == "valid" {
                "READABLE"
            } else {
                "UNAVAILABLE"
            }
        );
    }
    assert_eq!(before, snapshot(&f.root));
}

#[test]
fn edit_only_one_field_and_retry_invalid_without_echoing_input() {
    let f = Fixture::new();
    f.create("local");
    f.edit("local", "httpPort", "not-a-port\n28080\n").expect(0);
    let shown = f.query(&["setup", "show", "local", "--json"]);
    shown.expect(0);
    let data = shown.json();
    let fields = data["data"]["inspection"]["fields"].as_array().unwrap();
    assert_eq!(
        fields
            .iter()
            .find(|field| field["name"] == "httpPort")
            .unwrap()["value"],
        "28080"
    );
    assert_eq!(
        fields
            .iter()
            .find(|field| field["name"] == "instanceId")
            .unwrap()["value"],
        "local"
    );
    assert_eq!(
        fields
            .iter()
            .find(|field| field["name"] == "nodeId")
            .unwrap()["value"],
        "미입력"
    );
    let before = snapshot(&f.root);
    for answer in ["28080\n", "\n", ":back\n", ":help\n\n"] {
        let result = f.edit("local", "httpPort", answer);
        result.expect(0);
        assert!(result.stdout.contains("PREPARATION_UNCHANGED"));
        assert_eq!(before, snapshot(&f.root));
    }
}

#[test]
fn edit_cancel_eof_and_invalid_then_eof_keep_original_bytes() {
    let f = Fixture::new();
    f.create("local");
    f.edit("local", "httpPort", "28080\n").expect(0);
    let before = snapshot(&f.root);
    for answer in ["", ":cancel\n", "0\n", ":help\n:cancel\n"] {
        let result = f.edit("local", "httpPort", answer);
        result.expect(5);
        assert_eq!(before, snapshot(&f.root));
    }
}

#[test]
fn edit_json_nonterminal_invalid_forms_reject_before_input_and_creation() {
    let f = Fixture::new();
    let before = snapshot(&f.root);
    for (words, interactive) in [
        (
            vec!["setup", "edit", "local", "--field", "httpPort", "--json"],
            true,
        ),
        (vec!["setup", "edit", "local", "--field", "httpPort"], false),
        (vec!["setup", "edit", "local", "--field", "password"], true),
        (vec!["setup", "edit", "local"], true),
        (vec!["setup", "list", "local"], false),
        (vec!["setup", "show", "../local", "--json"], false),
        (vec!["setup", "show", "local", "--field", "httpPort"], false),
    ] {
        run(&f.args(&words), &mut NoInput, interactive).expect(2);
    }
    for action in ["list", "show", "edit"] {
        let args = ["setup", action, "--help"].map(str::to_owned);
        let result = run(&args, &mut NoInput, false);
        result.expect(0);
        assert!(result.stdout.contains(&format!("bxdl setup {action}")));
    }
    assert_eq!(before, snapshot(&f.root));
}

#[test]
fn edit_busy_session_does_not_read_answer() {
    let f = Fixture::new();
    f.create("local");
    let _session = Preparation::resume(&f.workspace("local")).unwrap();
    let before = snapshot(&f.root);
    let result = run(
        &f.args(&["setup", "edit", "local", "--field", "httpPort"]),
        &mut NoInput,
        true,
    );
    result.expect(3);
    assert!(result.stderr.contains("SETUP_WORKFLOW_BUSY"));
    assert_eq!(before, snapshot(&f.root));
}

struct ReplaceHome {
    cursor: Cursor<&'static str>,
    home: PathBuf,
    moved: PathBuf,
    replaced: bool,
}
impl io::Read for ReplaceHome {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(bytes)
    }
}
impl io::BufRead for ReplaceHome {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if !self.replaced {
            fs::rename(&self.home, &self.moved)?;
            fs::create_dir(&self.home)?;
            fs::set_permissions(&self.home, fs::Permissions::from_mode(0o700))?;
            self.replaced = true;
        }
        self.cursor.fill_buf()
    }
    fn consume(&mut self, amount: usize) {
        self.cursor.consume(amount);
    }
}
#[test]
fn home_replacement_after_prompt_rejects_before_field_write() {
    let f = Fixture::new();
    f.create("local");
    let before = snapshot(&f.home);
    let moved = f.root.join("old-home");
    let mut input = ReplaceHome {
        cursor: Cursor::new("28080\n"),
        home: f.home.clone(),
        moved: moved.clone(),
        replaced: false,
    };
    run(
        &f.args(&["setup", "edit", "local", "--field", "httpPort"]),
        &mut input,
        true,
    )
    .expect(3);
    assert_eq!(before, snapshot(&moved));
    assert_eq!(fs::read_dir(&f.home).unwrap().count(), 0);
}
