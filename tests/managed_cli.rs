//! Manager command routing preserves explicit targets, offline contracts and trust boundaries.
#![cfg(unix)]

use bxdl::cli;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs, io,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

const CANARY: &str = "PRIVATE-MANAGER-CANARY-47210";

struct NoInput;
impl io::Read for NoInput {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("manager commands must not read standard input")
    }
}
impl io::BufRead for NoInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        panic!("manager commands must not read standard input")
    }
    fn consume(&mut self, _: usize) {
        panic!("manager commands must not consume standard input")
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
    fn run(&self, args: &[&str]) -> Run {
        let mut args = words(args);
        args.push("--json".into());
        invoke(&args, &mut NoInput, false)
    }
    fn add(&self) {
        self.run(&[
            "nodes",
            "add",
            "local",
            "--instance",
            self.control.to_str().unwrap(),
            "--home",
            self.home.to_str().unwrap(),
        ])
        .assert_code(0);
    }
    fn named(&self, command: &str, extra: &[&str]) -> Run {
        let mut args = vec![command, "local", "--home", self.home.to_str().unwrap()];
        args.extend_from_slice(extra);
        self.run(&args)
    }
    fn explicit(&self, command: &str, extra: &[&str]) -> Run {
        let mut args = vec![command, "--instance", self.control.to_str().unwrap()];
        args.extend_from_slice(extra);
        self.run(&args)
    }
    fn fake_package_cli(&self) -> PathBuf {
        let package = self.root.join("missing-package");
        directory(&package);
        directory(&package.join("bin"));
        let cli = package.join("bin/bxdl");
        write(
            &cli,
            b"#!/bin/sh\ntouch \"$0.executed\"\nprintf 'PRIVATE-MANAGER-CANARY-47210'\n",
        );
        fs::set_permissions(&cli, fs::Permissions::from_mode(0o700)).unwrap();
        PathBuf::from(format!("{}.executed", cli.display()))
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

#[test]
fn named_operation_help_is_exact_available_without_a_terminal_and_matches_json() {
    for command in ["start", "status", "stop", "logs", "diagnose"] {
        let human = invoke(&words(&[command, "--help"]), &mut NoInput, false);
        human.assert_code(0);
        assert!(human.stdout.contains("--home"), "{}", human.stdout);
        assert!(human.stdout.contains("--instance"), "{}", human.stdout);
        let machine = invoke(&words(&["help", command, "--json"]), &mut NoInput, false);
        machine.assert_code(0);
        assert_eq!(machine.json()["data"]["usage"], human.stdout);
    }
}

#[test]
fn ambiguous_targets_unknown_flags_and_missing_diagnostic_output_are_usage_errors_without_writes() {
    let fixture = Fixture::new();
    let marker = fixture.fake_package_cli();
    fixture.add();
    let before = snapshot(&fixture.root);
    let control = fixture.control.to_str().unwrap();
    let home = fixture.home.to_str().unwrap();
    for command in ["start", "status", "stop", "logs", "diagnose"] {
        for tail in [
            vec![],
            vec!["local", "other", "--home", home],
            vec!["local", "--instance", control, "--home", home],
            vec!["--instance", control, "--home", home],
            vec!["--instance", control, "--instance", control],
            vec!["local", "--home", home, "--home", home],
            vec!["local", "--home", home, "--unknown", CANARY],
            vec!["local", "--home", home, "--timeout-seconds", "0"],
        ] {
            let mut args = vec![command];
            args.extend(tail);
            fixture.run(&args).assert_code(2);
        }
    }
    for command in ["start", "stop", "logs", "diagnose"] {
        fixture.named(command, &["--wait"]).assert_code(2);
    }
    fixture.named("diagnose", &[]).assert_code(2);
    for command in ["logs", "diagnose"] {
        for tail in ["0", "201", "-1"] {
            fixture.named(command, &["--tail", tail]).assert_code(2);
        }
        for bytes in ["1", "1048577"] {
            fixture
                .named(command, &["--max-bytes", bytes])
                .assert_code(2);
        }
    }
    assert!(!marker.exists());
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn invalid_or_unknown_names_never_fall_back_to_the_only_saved_node() {
    let fixture = Fixture::new();
    fixture.add();
    let before = snapshot(&fixture.root);
    let home = fixture.home.to_str().unwrap();
    for command in ["start", "status", "stop", "logs", "diagnose"] {
        for name in ["Local", "node_1", "../local", CANARY] {
            let mut args = vec![command, name, "--home", home];
            let report = fixture.root.join("report.json");
            if command == "diagnose" {
                args.extend(["--output", report.to_str().unwrap()]);
            }
            fixture.run(&args).assert_code(2);
        }
        let mut args = vec![command, "unknown", "--home", home];
        let report = fixture.root.join("report.json");
        if command == "diagnose" {
            args.extend(["--output", report.to_str().unwrap()]);
        }
        let missing = fixture.run(&args);
        missing.assert_code(3);
        assert_eq!(missing.json()["reasonCode"], "NODE_NAME_MISSING");
    }
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn named_offline_logs_match_explicit_paths_without_mutation_or_executor_launch() {
    let fixture = Fixture::new();
    let marker = fixture.fake_package_cli();
    fixture.add();
    let before = snapshot(&fixture.root);
    let direct = fixture.explicit("logs", &[]);
    let named = fixture.named("logs", &[]);
    direct.assert_code(0);
    named.assert_code(direct.code);
    assert_eq!(named.json()["data"], direct.json()["data"]);
    assert_eq!(named.json()["reasonCode"], direct.json()["reasonCode"]);
    assert_eq!(named.json()["data"]["runtimeReadiness"], "NOT_OBSERVED");
    assert!(!marker.exists());
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn a_different_management_root_cannot_select_a_name_from_another_catalog() {
    let fixture = Fixture::new();
    fixture.add();
    let before = snapshot(&fixture.root);
    let other = fixture.root.join("another-home");
    let result = fixture.run(&["logs", "local", "--home", other.to_str().unwrap()]);
    result.assert_code(3);
    assert_eq!(result.json()["reasonCode"], "NODE_NAME_MISSING");
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn damaged_catalog_blocks_name_resolution_but_preserves_explicit_offline_access() {
    let fixture = Fixture::new();
    fixture.add();
    let latest = latest_catalog(&fixture.home);
    let mut value: Value = serde_json::from_slice(&fs::read(&latest).unwrap()).unwrap();
    value["schemaVersion"] = 99.into();
    write(&latest, serde_json::to_vec(&value).unwrap());
    let before = snapshot(&fixture.root);
    let result = fixture.named("logs", &[]);
    result.assert_code(3);
    assert_eq!(result.json()["reasonCode"], "CONSOLE_CATALOG_CORRUPT");
    fixture.explicit("logs", &[]).assert_code(0);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn a_replaced_control_cannot_receive_a_previously_saved_name() {
    let fixture = Fixture::new();
    fixture.add();
    fs::rename(&fixture.control, fixture.root.join("original-control")).unwrap();
    instance_at(&fixture.control, &fixture.root);
    let before = snapshot(&fixture.root);
    let result = fixture.named("logs", &[]);
    result.assert_code(3);
    assert_eq!(result.json()["reasonCode"], "CONSOLE_TARGET_CHANGED");
    fixture.explicit("logs", &[]).assert_code(0);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn changed_binding_identity_is_rejected_even_when_the_control_inode_is_unchanged() {
    let fixture = Fixture::new();
    fixture.add();
    let binding_path = fixture.control.join("binding.json");
    let mut binding: Value = serde_json::from_slice(&fs::read(&binding_path).unwrap()).unwrap();
    binding["archiveSha256"] = "d".repeat(64).into();
    let bytes = serde_json::to_vec(&binding).unwrap();
    write(&binding_path, &bytes);
    let journal_path = fixture.control.join("journal/revision00000001.json");
    let mut journal: Value = serde_json::from_slice(&fs::read(&journal_path).unwrap()).unwrap();
    journal["bindingSha256"] = hex::encode(Sha256::digest(bytes)).into();
    write(&journal_path, serde_json::to_vec(&journal).unwrap());
    let before = snapshot(&fixture.root);
    let result = fixture.named("logs", &[]);
    result.assert_code(3);
    assert_eq!(result.json()["reasonCode"], "CONSOLE_TARGET_CHANGED");
    fixture.explicit("logs", &[]).assert_code(0);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn unsupported_package_profiles_never_execute_a_user_supplied_package_cli() {
    let fixture = Fixture::new();
    let marker = fixture.fake_package_cli();
    fixture.add();
    let before = snapshot(&fixture.root);
    for command in ["logs", "status", "start", "stop"] {
        let direct = fixture.explicit(command, &["--timeout-seconds", "1"]);
        let named = fixture.named(command, &["--timeout-seconds", "1"]);
        assert_ne!(direct.code, 2, "{}", direct.text());
        named.assert_code(direct.code);
        assert_eq!(direct.json()["reasonCode"], named.json()["reasonCode"]);
    }
    for (command, extra) in [
        ("init", vec!["--confirm-initialize"]),
        ("resume-init", vec!["--confirm-resume"]),
        ("preflight", vec![]),
    ] {
        let result = fixture.explicit(command, &extra);
        assert_ne!(result.code, 2, "{}", result.text());
        assert!(!result.text().contains(CANARY));
    }
    assert!(!marker.exists());
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn named_diagnose_writes_only_the_requested_report_and_preserves_offline_meaning() {
    let fixture = Fixture::new();
    let marker = fixture.fake_package_cli();
    fixture.add();
    let control_before = snapshot(&fixture.control);
    let home_before = snapshot(&fixture.home);
    let report = fixture.root.join("report.json");
    let result = fixture.named("diagnose", &["--output", report.to_str().unwrap()]);
    result.assert_code(0);
    assert_eq!(result.json()["data"]["outputCreated"], true);
    assert_eq!(result.json()["data"]["runtimeReadiness"], "NOT_OBSERVED");
    assert!(!fs::read_to_string(&report).unwrap().contains(CANARY));
    let before = snapshot(&fixture.root);
    fixture
        .named("diagnose", &["--output", report.to_str().unwrap()])
        .assert_code(3);
    assert_eq!(snapshot(&fixture.root), before);
    assert_eq!(snapshot(&fixture.control), control_before);
    assert_eq!(snapshot(&fixture.home), home_before);
    assert!(!marker.exists());
}

#[test]
fn explicit_logs_preserve_partial_salvage_when_registration_cannot_identify_an_executor() {
    let fixture = Fixture::new();
    let marker = fixture.fake_package_cli();
    fixture.add();
    write(
        &fixture.control.join("binding.json"),
        b"{broken-private-binding",
    );
    let before = snapshot(&fixture.root);
    let result = fixture.explicit("logs", &[]);
    result.assert_code(5);
    assert_eq!(result.json()["reasonCode"], "LOGS_PARTIAL");
    assert_eq!(result.json()["data"]["partial"], true);
    assert_eq!(result.json()["data"]["runtimeReadiness"], "NOT_OBSERVED");
    assert!(!result.text().contains("broken-private-binding"));
    fixture.named("logs", &[]).assert_code(3);
    assert!(!marker.exists());
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn identified_rc3_authentication_failures_never_fall_back_or_run_the_package_cli() {
    for failure in ["unsigned", "manifest", "missing-source"] {
        let fixture = Fixture::new();
        let marker = fixture.fake_package_cli();
        let binding_path = fixture.control.join("binding.json");
        let mut binding: Value = serde_json::from_slice(&fs::read(&binding_path).unwrap()).unwrap();
        binding["archiveSha256"] =
            "8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017".into();
        binding["manifestSha256"] =
            "2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123".into();
        binding["allowUnsignedDevelopment"] = false.into();
        binding["publicKey"] = fixture
            .root
            .join("missing-public-key.pem")
            .to_str()
            .unwrap()
            .into();
        match failure {
            "unsigned" => binding["allowUnsignedDevelopment"] = true.into(),
            "manifest" => binding["manifestSha256"] = "b".repeat(64).into(),
            "missing-source" => (),
            _ => unreachable!(),
        }
        let bytes = serde_json::to_vec(&binding).unwrap();
        write(&binding_path, &bytes);
        let journal_path = fixture.control.join("journal/revision00000001.json");
        let mut journal: Value = serde_json::from_slice(&fs::read(&journal_path).unwrap()).unwrap();
        journal["bindingSha256"] = hex::encode(Sha256::digest(bytes)).into();
        write(&journal_path, serde_json::to_vec(&journal).unwrap());
        fixture.add();
        let before = snapshot(&fixture.root);
        for command in ["logs", "status", "start", "stop"] {
            let direct = fixture.explicit(command, &["--timeout-seconds", "1"]);
            let named = fixture.named(command, &["--timeout-seconds", "1"]);
            direct.assert_code(3);
            named.assert_code(direct.code);
            assert_eq!(direct.json()["reasonCode"], named.json()["reasonCode"]);
            if failure != "missing-source" {
                assert_eq!(direct.json()["reasonCode"], "EXECUTOR_PROFILE_UNSUPPORTED");
            }
        }
        assert!(!marker.exists());
        assert_eq!(snapshot(&fixture.root), before);
    }
}
