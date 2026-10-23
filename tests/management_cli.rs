//! The management CLI has its own storage and explicit noninteractive commands.
//! Parser/help/completion must not authorize node operations or read stdin.
#![cfg(unix)]

use bxdl::cli;
use serde_json::Value;
use std::{collections::BTreeMap, fs, io, os::unix::fs::MetadataExt, path::Path};

struct NoInput;
impl io::Read for NoInput {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("management commands must not read stdin")
    }
}
impl io::BufRead for NoInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        panic!("management commands must not read stdin")
    }
    fn consume(&mut self, _: usize) {
        panic!("management commands must not read stdin")
    }
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}
impl Run {
    fn json(&self) -> Value {
        assert!(self.stderr.is_empty(), "{}", self.stderr);
        serde_json::from_str(&self.stdout).expect("one complete JSON envelope")
    }
    fn candidates(&self, kind: &str) -> Vec<&str> {
        assert_eq!(self.code, 0, "{}{}", self.stdout, self.stderr);
        assert!(self.stderr.is_empty());
        let mut lines = self.stdout.lines();
        assert_eq!(lines.next(), Some(kind));
        lines.collect()
    }
}
fn invoke(words: &[&str], tty: bool) -> Run {
    let args: Vec<String> = words.iter().map(|word| (*word).into()).collect();
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = cli::run_with_input(&args, &mut NoInput, tty, &mut stdout, &mut stderr);
    Run {
        code,
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    }
}
fn complete(cursor: usize, words: &[&str]) -> Run {
    let cursor = cursor.to_string();
    let mut args = vec!["completion", "__complete", "--cursor", &cursor, "--"];
    args.extend(words.iter().copied());
    invoke(&args, false)
}

type Snapshot = BTreeMap<String, (u64, u64, u32, i64, i64, i64, i64, Vec<u8>)>;
fn snapshot(root: &Path) -> Snapshot {
    fn visit(root: &Path, path: &Path, state: &mut Snapshot) {
        let m = fs::symlink_metadata(path).unwrap();
        state.insert(
            path.strip_prefix(root).unwrap().to_string_lossy().into(),
            (
                m.dev(),
                m.ino(),
                m.mode(),
                m.mtime(),
                m.mtime_nsec(),
                m.ctime(),
                m.ctime_nsec(),
                if m.is_file() {
                    fs::read(path).unwrap()
                } else {
                    Vec::new()
                },
            ),
        );
        if m.is_dir() {
            for child in fs::read_dir(path).unwrap() {
                visit(root, &child.unwrap().path(), state);
            }
        }
    }
    let mut state = BTreeMap::new();
    visit(root, root, &mut state);
    state
}

#[test]
fn exact_help_forms_cover_management_and_bundle_commands_without_input() {
    for topic in [
        vec!["cli"],
        vec!["cli", "install"],
        vec!["cli", "update"],
        vec!["cli", "info"],
        vec!["cli", "rollback"],
        vec!["cli", "bundle"],
        vec!["cli", "bundle", "build"],
        vec!["cli", "bundle", "verify"],
    ] {
        let mut help = vec!["help"];
        help.extend(topic.iter().copied());
        let reference = invoke(&help, false);
        assert_eq!(reference.code, 0, "{}", reference.stderr);
        assert!(reference.stdout.contains("bxdl cli"));
        for suffix in ["--help", "-h"] {
            let mut words = topic.clone();
            words.push(suffix);
            let result = invoke(&words, true);
            assert_eq!(result.code, 0);
            assert_eq!(result.stdout, reference.stdout);
            assert!(result.stderr.is_empty());
            words.push("--json");
            let json = invoke(&words, false);
            assert_eq!(json.code, 0);
            assert_eq!(json.json()["data"]["usage"], reference.stdout);
        }
    }
}

#[test]
fn invalid_management_arguments_do_not_read_input_or_create_storage() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let install_root = root.join("CLI root");
    let bin = root.join("entry directory");
    let install_root = install_root.to_str().unwrap();
    let bin = bin.to_str().unwrap();
    let before = snapshot(&root);
    for mut args in [
        vec!["cli"],
        vec!["cli", "unknown"],
        vec!["cli", "bundle"],
        vec!["cli", "bundle", "unknown"],
        vec!["cli", "update", "--root", install_root, "--bin-dir", bin],
        vec!["cli", "update", "archive", "--root", install_root],
        vec![
            "cli",
            "install",
            "--root",
            install_root,
            "--bin-dir",
            bin,
            "--public-key",
            "KEY-CANARY",
            "--allow-unsigned-development",
        ],
        vec![
            "cli",
            "install",
            "--root",
            install_root,
            "--root",
            install_root,
        ],
        vec!["cli", "install", "--from", "--help"],
        vec!["cli", "install", "--from="],
        vec!["cli", "info", "--from", "KEY-CANARY"],
        vec!["cli", "info", "--home", install_root],
        vec!["cli", "rollback", "--allow-unsigned-development"],
        vec![
            "cli",
            "bundle",
            "build",
            "--root",
            install_root,
            "--output",
            bin,
        ],
        vec!["cli", "bundle", "verify", "KEY-CANARY"],
        vec![
            "cli",
            "bundle",
            "verify",
            "KEY-CANARY",
            "--public-key",
            "KEY-CANARY",
            "--allow-unsigned-development",
        ],
    ] {
        args.push("--json");
        let result = invoke(&args, true);
        assert_eq!(
            result.code, 2,
            "{args:?}: {}{}",
            result.stdout, result.stderr
        );
        assert_eq!(result.json()["reasonCode"], "INVALID_ARGUMENTS");
        assert!(!result.stdout.contains("KEY-CANARY"));
        assert_eq!(snapshot(&root), before);
    }
}

#[test]
fn info_for_absent_installation_is_read_only_in_human_and_json_modes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let install_root = root.join("CLI root");
    let bin = root.join("bin");
    fs::write(root.join("existing-data"), "unchanged").unwrap();
    let before = snapshot(&root);
    let args = [
        "cli",
        "info",
        "--root",
        install_root.to_str().unwrap(),
        "--bin-dir",
        bin.to_str().unwrap(),
    ];
    let human = invoke(&args, false);
    assert_eq!(human.code, 0, "{}{}", human.stdout, human.stderr);
    assert!(human.stderr.is_empty());
    assert!(human.stdout.contains("설치 기록 없음"));
    assert!(human.stdout.contains("실행 진입점"));
    assert!(!human.stdout.contains("source <("));
    let mut args = args.to_vec();
    args.push("--json");
    let json = invoke(&args, true);
    assert_eq!(json.code, 0, "{}{}", json.stdout, json.stderr);
    let json = json.json();
    assert_eq!(json["command"], "cli info");
    assert_eq!(json["reasonCode"], "CLI_NOT_INSTALLED");
    assert_eq!(json["data"]["status"], "NOT_INSTALLED");
    assert_eq!(json["data"]["changed"], false);
    assert_eq!(json["data"]["root"], install_root.to_str().unwrap());
    assert!(json["data"]["current"].is_null());
    assert_eq!(snapshot(&root), before);
}

#[test]
fn bundle_verification_of_missing_supply_is_noninteractive_and_read_only() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let archive = root.join("missing-cli.tar.gz");
    let before = snapshot(&root);
    let result = invoke(
        &[
            "cli",
            "bundle",
            "verify",
            archive.to_str().unwrap(),
            "--allow-unsigned-development",
            "--json",
        ],
        false,
    );
    assert_eq!(result.code, 3, "{}{}", result.stdout, result.stderr);
    assert_eq!(result.json()["command"], "cli bundle verify");
    assert_eq!(result.json()["outcome"], "FAILED");
    assert_eq!(snapshot(&root), before);
}

#[test]
fn unavailable_install_update_and_rollback_do_not_prompt_or_create_storage() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let install_root = root.join("CLI root");
    let bin = root.join("bin");
    let missing = root.join("missing-cli.tar.gz");
    let before = snapshot(&root);
    let checkout = invoke(
        &[
            "cli",
            "install",
            "--root",
            install_root.to_str().unwrap(),
            "--bin-dir",
            bin.to_str().unwrap(),
            "--allow-unsigned-development",
            "--json",
        ],
        true,
    );
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        assert_eq!(checkout.code, 3, "{}{}", checkout.stdout, checkout.stderr);
        assert_eq!(checkout.json()["reasonCode"], "CLI_BUNDLE_REQUIRED");
    } else {
        assert_eq!(checkout.code, 4);
    }
    assert_eq!(snapshot(&root), before);
    for action in ["install", "update", "rollback"] {
        let mut words = vec![
            "cli",
            action,
            "--root",
            install_root.to_str().unwrap(),
            "--bin-dir",
            bin.to_str().unwrap(),
            "--json",
        ];
        if action != "rollback" {
            words.extend([
                "--from",
                missing.to_str().unwrap(),
                "--allow-unsigned-development",
            ]);
        }
        for tty in [false, true] {
            let result = invoke(&words, tty);
            let expected_exit = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                3
            } else {
                4
            };
            assert_eq!(
                result.code, expected_exit,
                "{}{}",
                result.stdout, result.stderr
            );
            assert_eq!(result.json()["command"], format!("cli {action}"));
            assert_eq!(snapshot(&root), before);
        }
    }
}

#[test]
fn management_completion_handles_nested_commands_and_suffix_constraints() {
    assert_eq!(complete(0, &["cl"]).candidates("values"), ["cli"]);
    assert_eq!(
        complete(1, &["cli", ""]).candidates("values"),
        ["bundle", "info", "install", "rollback", "update"]
    );
    assert_eq!(
        complete(2, &["cli", "bundle", ""]).candidates("values"),
        ["build", "verify"]
    );
    assert_eq!(
        complete(3, &["help", "cli", "bundle", ""]).candidates("values"),
        ["build", "verify"]
    );
    assert_eq!(
        complete(1, &["cli", "", "--from", "archive"]).candidates("values"),
        ["install", "update"]
    );
    assert_eq!(
        complete(1, &["cli", "", "build", "--version", "1.0.0"]).candidates("values"),
        ["bundle"]
    );
    assert_eq!(
        complete(2, &["--json", "cli", "", "--from", "archive"]).candidates("values"),
        ["install", "update"]
    );
    assert_eq!(
        complete(2, &["cli", "bundle", "", "--json"]).candidates("values"),
        ["build", "verify"]
    );
    assert_eq!(
        complete(2, &["cli", "bundle", "", "--signing-key", "key"]).candidates("values"),
        ["build"]
    );
    assert_eq!(
        complete(2, &["cli", "bundle", "", "--public-key", "key"]).candidates("values"),
        ["verify"]
    );
    for (line, kind) in [
        (vec!["cli", "install", "--root", ""], "directories"),
        (vec!["cli", "update", "--from", ""], "files"),
        (vec!["cli", "bundle", "build", "--root", ""], "directories"),
        (vec!["cli", "bundle", "build", "--version", ""], "none"),
        (vec!["cli", "bundle", "verify", ""], "files"),
    ] {
        assert!(complete(line.len() - 1, &line).candidates(kind).is_empty());
    }
    let update = complete(2, &["cli", "update", "--"]);
    for option in [
        "--from",
        "--root",
        "--bin-dir",
        "--public-key",
        "--allow-unsigned-development",
        "--json",
    ] {
        assert!(update.candidates("values").contains(&option));
    }
    let info = complete(2, &["cli", "info", "--"]);
    for option in [
        "--from",
        "--public-key",
        "--allow-unsigned-development",
        "--home",
    ] {
        assert!(!info.candidates("values").contains(&option));
    }
    let unsigned = complete(3, &["cli", "install", "--allow-unsigned-development", "--"]);
    assert!(!unsigned.candidates("values").contains(&"--public-key"));
    let signed = complete(5, &["cli", "bundle", "build", "--signing-key", "key", "--"]);
    assert!(
        !signed
            .candidates("values")
            .contains(&"--allow-unsigned-development")
    );
    for line in [
        vec!["cli", "bundle", "build", "--unknown", ""],
        vec!["cli", "info", "--from", "archive", ""],
        vec!["cli", "rollback", "--home", "root", ""],
    ] {
        assert!(
            complete(line.len() - 1, &line)
                .candidates("none")
                .is_empty()
        );
    }
}

#[test]
fn management_capabilities_are_separate_from_runtime_installation() {
    let result = invoke(&["version", "--json"], false);
    assert_eq!(result.code, 0);
    let result = result.json();
    let capabilities = result["data"]["capabilities"].as_array().unwrap();
    for capability in [
        "management-cli.v1",
        "management.install.macos",
        "management.update.macos",
        "management.info",
        "management.rollback.macos",
        "management.bundle.build",
        "management.bundle.verify",
        "install.macos",
        "completion.zsh",
    ] {
        assert!(capabilities.iter().any(|value| value == capability));
    }
    let runtime = invoke(&["install", "--help"], false);
    assert!(runtime.stdout.contains("bxdl install <tar.gz>"));
    assert!(!runtime.stdout.contains("cli install"));
}
