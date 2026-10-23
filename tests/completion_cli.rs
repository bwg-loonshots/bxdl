//! Completion proposes syntax and advisory names without executing a command,
//! inspecting control/package contents, or changing any management record.
#![cfg(unix)]

use bxdl::cli;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor},
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
};

const CANARY: &str = "PRIVATE-COMPLETION-CANARY-73941";

struct NoInput;
impl io::Read for NoInput {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("completion must not read stdin")
    }
}
impl io::BufRead for NoInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        panic!("completion must not read stdin")
    }
    fn consume(&mut self, _: usize) {
        panic!("completion must not consume stdin")
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
        assert!(!self.stdout.contains(CANARY), "{}", self.stdout);
        assert!(!self.stderr.contains(CANARY), "{}", self.stderr);
    }
    fn candidates(&self, kind: &str) -> Vec<&str> {
        self.expect(0);
        assert!(self.stderr.is_empty(), "{}", self.stderr);
        assert!(self.stdout.ends_with('\n'));
        let mut rows = self.stdout.lines();
        assert_eq!(rows.next(), Some(kind), "{}", self.stdout);
        let candidates: Vec<_> = rows.collect();
        assert!(candidates.iter().all(|value| {
            !value.is_empty()
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        }));
        let mut unique = candidates.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), candidates.len());
        candidates
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
fn words(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
fn complete(cursor: usize, line: &[&str]) -> Run {
    let mut args = words(&[
        "completion",
        "__complete",
        "--cursor",
        &cursor.to_string(),
        "--",
    ]);
    args.extend(words(line));
    invoke(&args, &mut NoInput, false)
}
fn private_directory(path: &Path) {
    fs::create_dir(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
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
            home: root.join("관리 홈 with spaces"),
            root,
            _temp: temp,
        }
    }
    fn create_home(&self) {
        let args = words(&[
            "setup",
            "new",
            "alpha",
            "--home",
            self.home.to_str().unwrap(),
        ]);
        invoke(&args, &mut Cursor::new("y\n:cancel\n"), true).expect(5);
    }
    fn catalog_file(&self) -> PathBuf {
        fs::read_dir(self.home.join("catalog"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .max()
            .unwrap()
    }
    fn seed_catalog(&self) {
        let entries: Vec<_> = [
            (Some("local"), "local"),
            (Some("ledger"), "ledger"),
            (None, "legacy"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (alias, name))| {
            let mut entry = json!({
                "id": format!("{:032x}", index + 1),
                "label": format!("운영 표시명 {CANARY} {name}"),
                "target": {
                    "kind": "INSTANCE",
                    "path": self.root.join(format!("{CANARY}-{name}-missing-control")),
                    "identity": {"device": 1, "inode": index + 1},
                    "schemaVersion": 1,
                    "name": format!("engine-{name}"),
                    "state": "REGISTERED",
                    "cli": "MISMATCH",
                },
                "recordedAt": "2026-10-23T00:00:00Z",
            });
            if let Some(alias) = alias {
                entry["alias"] = alias.into();
                entry["bindingSha256"] = "a".repeat(64).into();
            }
            entry
        })
        .collect();
        write(
            &self.catalog_file(),
            serde_json::to_vec(&json!({
                "schemaVersion": 2, "kind": "BXDL_CONSOLE_CATALOG", "entries": entries,
            }))
            .unwrap(),
        );
    }
    fn nodes(&self, prefix: &str) -> Run {
        complete(
            1,
            &["status", prefix, "--home", self.home.to_str().unwrap()],
        )
    }
    fn preparation(&self, action: &str, prefix: &str) -> Run {
        complete(
            2,
            &[
                "setup",
                action,
                prefix,
                "--home",
                self.home.to_str().unwrap(),
            ],
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FileState {
    mode: u32,
    device: u64,
    inode: u64,
    modified: (i64, i64),
    changed: (i64, i64),
    bytes: Vec<u8>,
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, FileState> {
    fn walk(base: &Path, path: &Path, output: &mut BTreeMap<PathBuf, FileState>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        output.insert(
            path.strip_prefix(base).unwrap().to_owned(),
            FileState {
                mode: metadata.mode(),
                device: metadata.dev(),
                inode: metadata.ino(),
                modified: (metadata.mtime(), metadata.mtime_nsec()),
                changed: (metadata.ctime(), metadata.ctime_nsec()),
                bytes: if metadata.is_file() {
                    fs::read(path).unwrap()
                } else if metadata.file_type().is_symlink() {
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
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                walk(base, &entry.unwrap().path(), output);
            }
        }
    }
    let mut output = BTreeMap::new();
    walk(root, root, &mut output);
    output
}

#[test]
fn zsh_script_and_help_are_raw_or_help_only_without_input() {
    let generated = invoke(&words(&["completion", "zsh"]), &mut NoInput, false);
    generated.expect(0);
    assert!(generated.stderr.is_empty());
    assert!(generated.stdout.contains("_bxdl"));
    assert!(generated.stdout.contains("__complete"));
    assert!(!generated.stdout.contains("[SUCCEEDED]"));
    for args in [
        words(&["completion", "zsh", "--json"]),
        words(&["--json", "completion", "zsh"]),
        words(&["completion", "--json", "zsh"]),
    ] {
        let json = invoke(&args, &mut NoInput, true);
        json.expect(2);
        assert!(json.stderr.is_empty());
        assert_eq!(
            serde_json::from_str::<Value>(&json.stdout).unwrap()["reasonCode"],
            "INVALID_ARGUMENTS"
        );
    }
}

#[test]
fn completion_help_forms_match_human_and_json_usage_without_input() {
    for topic in [vec!["completion"], vec!["completion", "zsh"]] {
        let mut reference_args = topic.clone();
        reference_args.push("--help");
        let reference = invoke(&words(&reference_args), &mut NoInput, false);
        reference.expect(0);
        assert!(reference.stderr.is_empty());
        assert!(reference.stdout.contains("bxdl completion zsh"));
        for (prefix, suffix) in [(false, "--help"), (false, "-h"), (true, "")] {
            let mut args = if prefix { vec!["help"] } else { vec![] };
            args.extend(topic.iter().copied());
            if !suffix.is_empty() {
                args.push(suffix);
            }
            let human = invoke(&words(&args), &mut NoInput, false);
            human.expect(0);
            assert_eq!(human.stdout, reference.stdout);
            assert!(human.stderr.is_empty());
            args.push("--json");
            let machine = invoke(&words(&args), &mut NoInput, false);
            machine.expect(0);
            assert!(machine.stderr.is_empty());
            let document: Value = serde_json::from_str(&machine.stdout).unwrap();
            assert_eq!(document["reasonCode"], "HELP");
            assert_eq!(document["data"]["usage"], reference.stdout);
        }
    }
}

#[test]
fn unsupported_shells_and_invalid_protocols_are_rejected_before_input() {
    for args in [
        words(&["completion"]),
        words(&["completion", "bash"]),
        words(&["completion", "zsh", CANARY]),
        words(&["completion", "__complete", "--cursor", "0", "--"]),
        words(&["completion", "__complete", "--cursor", "-1", "--", ""]),
        words(&["completion", "__complete", "--cursor", "1", "--", ""]),
        words(&["completion", "__complete", "--cursor", "0", ""]),
        words(&[
            "completion",
            "__complete",
            "--cursor",
            "0",
            "--cursor",
            "0",
            "--",
            "",
        ]),
        words(&[
            "completion",
            "__complete",
            "--unknown",
            CANARY,
            "--cursor",
            "0",
            "--",
            "",
        ]),
    ] {
        invoke(&args, &mut NoInput, false).expect(2);
    }
    complete(0, &[&"x".repeat(4097)]).expect(2);
    complete(0, &[&"é".repeat(2049)]).expect(2);
    complete(0, &vec![""; 129]).expect(2);
    let large = "x".repeat(4096);
    complete(0, &[&large, &large, &large, &large, "x"]).expect(2);
    // Exact limits remain valid protocol even if the typed command is unknown.
    complete(0, &[&large]).expect(0);
    complete(0, &vec![""; 128]).expect(0);
    complete(0, &[&large, &large, &large, &large]).expect(0);
}

#[test]
fn command_option_and_field_candidates_follow_the_selected_command() {
    assert_eq!(
        complete(0, &["sta"]).candidates("values"),
        ["start", "status"]
    );
    let top = complete(0, &[""]);
    let names = top.candidates("values");
    for name in ["setup", "nodes", "start", "status", "completion"] {
        assert!(names.contains(&name));
    }
    for name in ["service-run", "__complete", "upgrade", "uninstall"] {
        assert!(!names.contains(&name));
    }
    let setup = complete(1, &["setup", ""]);
    for name in ["new", "resume", "list", "show", "edit", "apply"] {
        assert!(setup.candidates("values").contains(&name));
    }
    let status = complete(1, &["status", "--"]);
    for option in [
        "--home",
        "--instance",
        "--wait",
        "--timeout-seconds",
        "--json",
    ] {
        assert!(status.candidates("values").contains(&option));
    }
    let start = complete(1, &["start", "--"]);
    assert!(!start.candidates("values").contains(&"--wait"));
    assert!(!start.candidates("values").contains(&"--confirm-initialize"));
    let used = complete(2, &["status", "--wait", "--"]);
    assert!(!used.candidates("values").contains(&"--wait"));
    let init = complete(1, &["init", "--confirm"]);
    assert_eq!(init.candidates("values"), ["--confirm-initialize"]);
    assert_eq!(
        complete(4, &["setup", "edit", "local", "--field", "http"]).candidates("values"),
        ["httpAddress", "httpPort"]
    );
    let interactive = complete(3, &["setup", "new", "fresh", "--"]);
    assert!(!interactive.candidates("values").contains(&"--json"));
}

#[test]
fn value_positions_and_invalid_typed_lines_do_not_turn_into_commands() {
    for line in [
        vec!["status", "--home", ""],
        vec!["status", "--instance", ""],
    ] {
        assert!(complete(2, &line).candidates("directories").is_empty());
    }
    assert!(
        complete(4, &["setup", "new", "fresh", "--from", ""])
            .candidates("files")
            .is_empty()
    );
    assert!(
        complete(2, &["status", "--timeout-seconds", ""])
            .candidates("none")
            .is_empty()
    );
    for (cursor, line) in [
        (2, vec!["status", "--unknown", ""]),
        (3, vec!["status", "--instance", CANARY, "l"]),
        (
            5,
            vec!["status", "--instance", CANARY, "--home", CANARY, ""],
        ),
        (5, vec!["status", "--home", CANARY, "--home", CANARY, ""]),
        (4, vec!["status", "--", "--home", CANARY, "l"]),
        (4, vec!["setup", "new", "fresh", "--json", ""]),
        (1, vec!["not-a-command", ""]),
    ] {
        assert!(complete(cursor, &line).candidates("none").is_empty());
    }
}

#[test]
fn node_candidates_read_only_aliases_from_the_selected_catalog_without_opening_controls() {
    let f = Fixture::new();
    f.create_home();
    f.seed_catalog();
    let before = snapshot(&f.root);
    assert_eq!(f.nodes("l").candidates("values"), ["ledger", "local"]);
    for (cursor, line) in [
        (3, vec!["status", "--home", f.home.to_str().unwrap(), "l"]),
        (
            2,
            vec!["--json", "status", "l", "--home", f.home.to_str().unwrap()],
        ),
        (
            2,
            vec!["status", "--json", "l", "--home", f.home.to_str().unwrap()],
        ),
    ] {
        assert_eq!(
            complete(cursor, &line).candidates("values"),
            ["ledger", "local"]
        );
    }
    for command in [
        "init",
        "resume-init",
        "start",
        "stop",
        "status",
        "logs",
        "diagnose",
    ] {
        assert_eq!(
            complete(1, &[command, "l", "--home", f.home.to_str().unwrap()]).candidates("values"),
            ["ledger", "local"]
        );
    }
    assert!(f.nodes("legacy").candidates("values").is_empty());
    assert!(f.nodes("engine").candidates("values").is_empty());
    assert_eq!(snapshot(&f.root), before);
}

#[test]
fn missing_corrupt_duplicate_alias_and_symlink_homes_never_create_or_repair_records() {
    let f = Fixture::new();
    let before = snapshot(&f.root);
    assert!(f.nodes("").candidates("values").is_empty());
    assert_eq!(snapshot(&f.root), before);
    f.create_home();
    f.seed_catalog();
    let file = f.catalog_file();
    let valid = fs::read(&file).unwrap();
    write(&file, format!("broken {CANARY}"));
    let before = snapshot(&f.root);
    assert!(f.nodes("").candidates("values").is_empty());
    assert_eq!(snapshot(&f.root), before);
    let mut duplicate: Value = serde_json::from_slice(&valid).unwrap();
    duplicate["entries"][1]["alias"] = "local".into();
    write(&file, serde_json::to_vec(&duplicate).unwrap());
    let before = snapshot(&f.root);
    assert!(f.nodes("").candidates("values").is_empty());
    assert_eq!(snapshot(&f.root), before);
    write(&file, valid);
    let linked = f.root.join("linked-home");
    symlink(&f.home, &linked).unwrap();
    let before = snapshot(&f.root);
    assert!(
        complete(1, &["status", "", "--home", linked.to_str().unwrap()])
            .candidates("values")
            .is_empty()
    );
    assert_eq!(snapshot(&f.root), before);
}

#[test]
fn preparation_candidates_are_named_private_directories_without_loading_or_locking_sessions() {
    let f = Fixture::new();
    f.create_home();
    let sessions = f.home.join("sessions");
    private_directory(&sessions.join("beta")); // Deliberately lacks a Preparation marker.
    private_directory(&sessions.join("Upper"));
    private_directory(&sessions.join("non-private"));
    fs::set_permissions(
        sessions.join("non-private"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    write(&sessions.join("plain-file"), CANARY);
    let outside = f.root.join("outside-session");
    private_directory(&outside);
    symlink(&outside, sessions.join("linked")).unwrap();
    let operations = f.home.join("operations");
    private_directory(&operations);
    private_directory(&operations.join("orphan"));
    private_directory(&operations.join("alpha"));
    let held = bxdl::setup::preparation::Preparation::resume(&sessions.join("alpha")).unwrap();
    let before = snapshot(&f.root);
    for action in ["resume", "edit", "apply"] {
        assert_eq!(
            f.preparation(action, "").candidates("values"),
            ["alpha", "beta"]
        );
    }
    assert_eq!(
        f.preparation("show", "").candidates("values"),
        ["alpha", "beta", "orphan"]
    );
    assert_eq!(f.preparation("show", "o").candidates("values"), ["orphan"]);
    assert!(f.preparation("resume", "o").candidates("values").is_empty());
    assert_eq!(snapshot(&f.root), before);
    drop(held);
}

#[test]
fn advanced_commands_offer_only_their_options_and_respect_exclusions() {
    for (command, expected) in [
        ("config", vec!["fingerprint", "validate"]),
        ("engine", vec!["inspect", "preflight"]),
        ("instance", vec!["register", "show"]),
        ("package", vec!["build", "verify"]),
        ("runtime", vec!["stage"]),
        ("completion", vec!["zsh"]),
    ] {
        assert_eq!(complete(1, &[command, ""]).candidates("values"), expected);
    }
    for (line, expected_kind) in [
        (vec!["config", "fingerprint", "--certificate", ""], "files"),
        (vec!["engine", "inspect", "--java", ""], "files"),
        (vec!["runtime", "stage", "--source", ""], "directories"),
        (vec!["runtime", "stage", "--output", ""], "directories"),
        (vec!["instance", "register", "--package", ""], "directories"),
        (vec!["package", "build", "--spec", ""], "files"),
    ] {
        assert!(
            complete(line.len() - 1, &line)
                .candidates(expected_kind)
                .is_empty()
        );
    }
    let verify = complete(
        3,
        &["package", "verify", "--allow-unsigned-development", "--"],
    );
    assert!(!verify.candidates("values").contains(&"--public-key"));
    let build = complete(4, &["package", "build", "--signing-key", CANARY, "--"]);
    assert!(
        !build
            .candidates("values")
            .contains(&"--allow-unsigned-development")
    );
    let network = complete(3, &["setup", "--network", "--resume", "--"]);
    for excluded in [
        "--from",
        "--network-file",
        "--install",
        "--json",
        "--non-interactive",
    ] {
        assert!(!network.candidates("values").contains(&excluded));
    }
    let registered = complete(3, &["preflight", "--instance", CANARY, "--"]);
    for excluded in [
        "--config",
        "--engine-config",
        "--jar",
        "--java",
        "--lock",
        "--allow-development",
    ] {
        assert!(!registered.candidates("values").contains(&excluded));
    }
    for (cursor, line) in [
        (
            5,
            vec![
                "package",
                "verify",
                "--public-key",
                CANARY,
                "--allow-unsigned-development",
                "",
            ],
        ),
        (4, vec!["setup", "--resume", "--from", CANARY, ""]),
        (3, vec!["setup", "--install", "--network", ""]),
        (4, vec!["engine", "inspect", "--config", CANARY, ""]),
    ] {
        assert!(complete(cursor, &line).candidates("none").is_empty());
    }
}

#[test]
fn inline_values_and_literal_separator_keep_value_and_name_contexts() {
    let f = Fixture::new();
    f.create_home();
    f.seed_catalog();
    let home_option = format!("--home={}", f.home.display());
    let before = snapshot(&f.root);
    for (cursor, line) in [
        (1, vec!["status", "l", home_option.as_str()]),
        (2, vec!["status", home_option.as_str(), "l"]),
        (3, vec!["status", home_option.as_str(), "--", "l"]),
    ] {
        assert_eq!(
            complete(cursor, &line).candidates("values"),
            ["ledger", "local"]
        );
    }
    assert!(
        complete(3, &["status", &home_option, "--", "--json"])
            .candidates("values")
            .is_empty()
    );
    assert_eq!(
        complete(3, &["setup", "edit", "alpha", "--field=http"]).candidates("values"),
        ["httpAddress", "httpPort"]
    );
    assert!(
        complete(1, &["status", "--home="])
            .candidates("directories")
            .is_empty()
    );
    assert!(
        complete(2, &["install", "--", "-archive"])
            .candidates("files")
            .is_empty()
    );
    for (cursor, line) in [
        (2, vec!["status", "--wait=true", ""]),
        (2, vec!["status", "--json=true", ""]),
        (3, vec!["config", "validate", "--", "--file="]),
    ] {
        assert!(complete(cursor, &line).candidates("none").is_empty());
    }
    assert_eq!(snapshot(&f.root), before);
}

#[test]
fn oversized_preparation_namespace_is_an_empty_advisory_view_without_changes() {
    let f = Fixture::new();
    f.create_home();
    for index in 0..255 {
        private_directory(&f.home.join("sessions").join(format!("n{index:03}")));
    }
    let before = snapshot(&f.root);
    assert_eq!(f.preparation("resume", "").candidates("values").len(), 256);
    assert_eq!(snapshot(&f.root), before);
    private_directory(&f.home.join("sessions").join("overflow"));
    let before = snapshot(&f.root);
    assert!(f.preparation("resume", "").candidates("values").is_empty());
    assert_eq!(snapshot(&f.root), before);
}

#[test]
fn empty_cursor_after_or_before_an_explicit_instance_offers_compatible_options() {
    for (cursor, line) in [
        (3, vec!["status", "--instance", CANARY, ""]),
        (1, vec!["status", "", "--instance", CANARY]),
        (2, vec!["status", "--instance=/missing-control", ""]),
        (1, vec!["status", "", "--instance=/missing-control"]),
    ] {
        let result = complete(cursor, &line);
        let candidates = result.candidates("values");
        for option in ["--wait", "--timeout-seconds", "--json"] {
            assert!(candidates.contains(&option), "{}", result.stdout);
        }
        for option in ["--home", "--instance", "--help", "-h"] {
            assert!(!candidates.contains(&option), "{}", result.stdout);
        }
    }
}

#[test]
fn completed_option_values_must_match_cli_parser_without_rejecting_current_values() {
    for (cursor, line) in [
        (3, vec!["status", "--home", "--wait", ""]),
        (1, vec!["status", "", "--home", "--wait"]),
        (3, vec!["status", "--home", "", ""]),
        (1, vec!["status", "", "--home", ""]),
        (2, vec!["status", "--home=", ""]),
        (1, vec!["status", "", "--home="]),
        (4, vec!["setup", "edit", "alpha", "--field=", ""]),
    ] {
        assert!(complete(cursor, &line).candidates("none").is_empty());
    }
    for line in [
        vec!["config", "validate", "--file", "-", ""],
        vec!["config", "validate", "--file", "-private", ""],
        vec!["config", "validate", "--file=--private", ""],
    ] {
        assert!(
            complete(line.len() - 1, &line)
                .candidates("values")
                .contains(&"--json")
        );
    }
    assert!(
        complete(2, &["status", "--home", ""])
            .candidates("directories")
            .is_empty()
    );
    assert!(
        complete(1, &["status", "--home="])
            .candidates("directories")
            .is_empty()
    );
}

#[test]
fn current_subcommand_candidates_obey_completed_options_after_the_cursor() {
    assert_eq!(
        complete(1, &["setup", "", "--json"]).candidates("values"),
        ["list", "show"]
    );
    assert_eq!(
        complete(2, &["--json", "setup", "", "--home", CANARY]).candidates("values"),
        ["list", "show"]
    );
    assert_eq!(
        complete(1, &["package", "", "--public-key", CANARY]).candidates("values"),
        ["verify"]
    );
    assert_eq!(
        complete(1, &["package", "v", "--public-key", CANARY]).candidates("values"),
        ["verify"]
    );
    assert!(
        complete(1, &["package", "b", "--public-key", CANARY])
            .candidates("values")
            .is_empty()
    );
}

#[test]
fn help_options_are_offered_only_for_exact_supported_help_forms() {
    for line in [
        vec!["status", "-"],
        vec!["config", "validate", "-"],
        vec!["setup", "new", "-"],
    ] {
        let result = complete(line.len() - 1, &line);
        let candidates = result.candidates("values");
        assert!(candidates.contains(&"--help"), "{}", result.stdout);
        assert!(candidates.contains(&"-h"), "{}", result.stdout);
    }
    for line in [
        vec!["status", "local", "-"],
        vec!["setup", "new", "local", "-"],
        vec!["config", "validate", "--file", CANARY, "-"],
        vec!["config", "-"],
        vec!["package", "-"],
        vec!["runtime", "-"],
        vec!["engine", "-"],
        vec!["instance", "-"],
    ] {
        let result = complete(line.len() - 1, &line);
        let candidates = result.candidates("values");
        assert!(!candidates.contains(&"--help"), "{}", result.stdout);
        assert!(!candidates.contains(&"-h"), "{}", result.stdout);
    }
}
