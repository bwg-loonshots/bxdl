//! Integrated preparation accepts an explicit name and saves only approved
//! configuration work. It must not install, initialize, or start a node.
#![cfg(unix)]

use bxdl::cli;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

const CANARY: &str = "PRIVATE-PREPARE-CANARY-98342";

struct NoInput;
impl io::Read for NoInput {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("rejected or help-only command must not read input")
    }
}
impl io::BufRead for NoInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        panic!("rejected or help-only command must not read input")
    }
    fn consume(&mut self, _: usize) {
        panic!("rejected or help-only command must not consume input")
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

    fn assert_code(&self, expected: i32) {
        assert_eq!(self.code, expected, "{}", self.text());
        assert!(!self.text().contains(CANARY), "{}", self.text());
    }
}

fn invoke(args: &[String], input: &mut dyn io::BufRead, interactive: bool) -> Run {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = cli::run_with_input(args, input, interactive, &mut stdout, &mut stderr);
    Run {
        code,
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    }
}

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let home = root.join("management parent/관리 홈");
        Self {
            _temporary: temporary,
            root,
            home,
        }
    }

    fn arguments(&self, action: &str, name: &str) -> Vec<String> {
        strings(&["setup", action, name, "--home", self.home.to_str().unwrap()])
    }

    fn run(&self, action: &str, name: &str, input: &str) -> Run {
        invoke(&self.arguments(action, name), &mut Cursor::new(input), true)
    }

    fn create_saved(&self, name: &str) {
        self.run("new", name, "y\n0\n").assert_code(0);
        assert!(self.home.join("sessions").join(name).is_dir());
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Entry {
    directory: bool,
    mode: u32,
    device: u64,
    inode: u64,
    bytes: Vec<u8>,
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn visit(base: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, Entry>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        assert!(!metadata.file_type().is_symlink());
        entries.insert(
            path.strip_prefix(base).unwrap().to_owned(),
            Entry {
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
                visit(base, &child.unwrap().path(), entries);
            }
        }
    }
    let mut result = BTreeMap::new();
    if root.exists() {
        visit(root, root, &mut result);
    }
    result
}

fn write(path: &Path, contents: impl AsRef<[u8]>) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn prepare_sources(fixture: &Fixture) -> PathBuf {
    let sources = fixture.root.join("sources");
    fs::create_dir(&sources).unwrap();
    fs::set_permissions(&sources, fs::Permissions::from_mode(0o700)).unwrap();
    let node = format!("0x{}", "11".repeat(32));
    let validator = format!("0x{}", "22".repeat(20));
    let mut product: Value = serde_json::from_slice(include_bytes!(
        "../config/examples/instance.development.json"
    ))
    .unwrap();
    product["nodeId"] = node.clone().into();
    product["chainDescription"] = "chain.json".into();
    product["p2p"]["address"] = "127.0.0.1".into();
    product["storage"]["dataDirectory"] = fixture
        .root
        .join("must-not-create-data")
        .to_str()
        .unwrap()
        .into();
    for name in [
        "validatorKeystore",
        "validatorPasswordFile",
        "tlsKeyStore",
        "tlsKeyPasswordFile",
        "tlsTrustStore",
        "tlsTrustPasswordFile",
    ] {
        let filename = format!("{CANARY}-{name}");
        // Intentionally invalid credential contents: configuration preparation
        // observes metadata only, and engine validation belongs to installation.
        write(&sources.join(&filename), CANARY);
        product["secrets"][name] = filename.into();
    }
    let chain = json!({
        "nigo.protocol.consensus.protocol": "QBFT",
        "nigo.protocol.consensus.qbft.validators[0]": validator,
    });
    let network = json!({
        "schemaVersion": 1,
        "kind": "BXDL_QBFT_NETWORK",
        "nodes": [{
            "nodeId": node,
            "validatorId": validator,
            "host": "127.0.0.1",
            "port": 19090,
            "transactionPermission": "NONE",
            "credentialFingerprints": [format!("0x{}", "33".repeat(32))],
        }],
    });
    for (name, value) in [
        ("instance.json", product),
        ("chain.json", chain),
        ("network.json", network),
    ] {
        write(
            &sources.join(name),
            serde_json::to_vec_pretty(&value).unwrap(),
        );
    }
    sources
}

#[test]
fn direct_setup_rejects_nonterminal_and_json_without_reading_or_writing() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    for action in ["new", "resume"] {
        let mut args = fixture.arguments(action, "demo");
        if action == "new" {
            args.extend(strings(&["--from", CANARY]));
        }
        let result = invoke(&args, &mut NoInput, false);
        result.assert_code(2);
        assert!(result.stdout.is_empty());
        assert!(result.stderr.contains("INTERACTIVE_TERMINAL_REQUIRED"));

        args.push("--json".into());
        let result = invoke(&args, &mut NoInput, true);
        result.assert_code(2);
        assert!(result.stderr.is_empty(), "{}", result.stderr);
        let json: Value = serde_json::from_str(&result.stdout).unwrap();
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["reasonCode"], "INVALID_ARGUMENTS");
    }
    assert_eq!(snapshot(&fixture.root), before);
    assert!(!fixture.home.exists());
}

#[test]
fn invalid_names_and_options_are_rejected_before_input_or_filesystem_changes() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    let mut cases = Vec::new();
    for action in ["new", "resume"] {
        for name in [
            "",
            "Upper",
            "1node",
            "node_one",
            "a b",
            "../escape",
            "a/b",
            "a\\b",
            "-node",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            cases.push(fixture.arguments(action, name));
        }
        for tail in [
            vec!["--home", CANARY],
            vec!["--unknown", CANARY],
            vec!["--instance", CANARY],
            vec!["--workspace", CANARY],
            vec!["--non-interactive"],
            vec!["--from"],
            vec![CANARY],
        ] {
            let mut args = fixture.arguments(action, "demo");
            args.extend(strings(&tail));
            cases.push(args);
        }
        cases.push(strings(&["setup", action]));
    }
    let mut duplicate_from = fixture.arguments("new", "demo");
    duplicate_from.extend(strings(&["--from", CANARY, "--from", CANARY]));
    cases.push(duplicate_from);
    let mut resume_from = fixture.arguments("resume", "demo");
    resume_from.extend(strings(&["--from", CANARY]));
    cases.push(resume_from);
    for args in cases {
        let result = invoke(&args, &mut NoInput, true);
        result.assert_code(2);
        assert!(result.stdout.is_empty());
        assert!(
            result.stderr.contains("INVALID_ARGUMENTS"),
            "{}",
            result.stderr
        );
    }
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn help_only_forms_are_read_only_and_need_no_terminal() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    for action in ["new", "resume"] {
        for args in [
            strings(&["setup", action, "--help"]),
            strings(&["help", "setup", action]),
        ] {
            let result = invoke(&args, &mut NoInput, false);
            result.assert_code(0);
            assert!(result.stdout.contains(&format!("bxdl setup {action}")));
            assert!(result.stderr.is_empty());
        }
    }
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn new_session_creation_requires_explicit_approval_and_preserves_absence_on_pause() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    for input in [
        "\n",
        "n\n",
        "YES\n",
        ":back\n",
        ":cancel\n",
        "",
        ":help\n\n",
    ] {
        let result = fixture.run("new", "demo", input);
        result.assert_code(5);
        assert!(!fixture.home.exists());
        assert_eq!(snapshot(&fixture.root), before);
    }
}

#[test]
fn approved_session_uses_named_private_layout_and_does_not_create_node_outputs() {
    let fixture = Fixture::new();
    fixture.create_saved("demo-one");
    assert!(fixture.home.join(".bxdl-home.json").is_file());
    assert!(fixture.home.join("sessions/demo-one").is_dir());
    for path in ["data/demo-one", "instances/demo-one", "packages/demo-one"] {
        assert!(
            !fixture.home.join(path).exists(),
            "unexpected output: {path}"
        );
    }
    for (path, entry) in snapshot(&fixture.home) {
        assert_eq!(
            entry.mode & 0o777,
            if entry.directory { 0o700 } else { 0o600 },
            "{}",
            path.display()
        );
    }
}

#[test]
fn saved_session_resume_is_read_only_until_an_explicit_action() {
    let fixture = Fixture::new();
    fixture.create_saved("demo");
    let before = snapshot(&fixture.root);
    for (input, expected) in [("0\n", 0), (":cancel\n", 5), ("", 5), (":help\n0\n", 0)] {
        fixture.run("resume", "demo", input).assert_code(expected);
        assert_eq!(snapshot(&fixture.root), before);
    }
}

#[test]
fn cancellation_after_approved_creation_preserves_a_resumable_session() {
    for input in ["y\n:cancel\n", "yes\n"] {
        let fixture = Fixture::new();
        fixture.run("new", "demo", input).assert_code(5);
        assert!(fixture.home.join("sessions/demo").is_dir());
        let before = snapshot(&fixture.root);
        fixture.run("resume", "demo", "0\n").assert_code(0);
        assert_eq!(snapshot(&fixture.root), before);
    }
}

#[test]
fn existing_session_and_unmarked_home_are_not_overwritten_or_adopted() {
    let fixture = Fixture::new();
    fixture.create_saved("demo");
    let before = snapshot(&fixture.root);
    fixture.run("new", "demo", "y\n0\n").assert_code(3);
    assert_eq!(snapshot(&fixture.root), before);

    let unmarked = Fixture::new();
    fs::create_dir_all(&unmarked.home).unwrap();
    fs::set_permissions(&unmarked.home, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(unmarked.home.join("existing-data"), CANARY).unwrap();
    let before = snapshot(&unmarked.root);
    unmarked.run("new", "demo", "y\n0\n").assert_code(3);
    assert_eq!(snapshot(&unmarked.root), before);
}

#[test]
fn missing_resume_never_creates_a_management_home_or_fallback_session() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    let result = invoke(&fixture.arguments("resume", "missing"), &mut NoInput, true);
    result.assert_code(3);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn direct_and_console_share_named_creation_and_resume() {
    let fixture = Fixture::new();
    let arguments = strings(&["console", "--home", fixture.home.to_str().unwrap()]);
    let result = invoke(
        &arguments,
        &mut Cursor::new("1\n1\nconsole-demo\n\ny\n0\n0\n"),
        true,
    );
    result.assert_code(0);
    assert!(fixture.home.join("sessions/console-demo").is_dir());
    let before = snapshot(&fixture.root);
    fixture.run("resume", "console-demo", "0\n").assert_code(0);
    assert_eq!(snapshot(&fixture.root), before);

    fixture.create_saved("direct-demo");
    let before = snapshot(&fixture.root);
    let result = invoke(
        &arguments,
        &mut Cursor::new("1\n2\ndirect-demo\n0\n0\n"),
        true,
    );
    result.assert_code(0);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn input_io_failure_has_existing_io_exit_code_and_does_not_create_a_home() {
    struct FailingInput;
    impl io::Read for FailingInput {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other(CANARY))
        }
    }
    impl io::BufRead for FailingInput {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            Err(io::Error::other(CANARY))
        }
        fn consume(&mut self, _: usize) {}
    }
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    let result = invoke(&fixture.arguments("new", "demo"), &mut FailingInput, true);
    result.assert_code(7);
    assert!(result.stderr.contains("SETUP_INPUT_FAILED"));
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn importing_existing_product_keeps_references_private_and_skips_repeated_questions() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let before = snapshot(&sources);
    let mut arguments = fixture.arguments("new", "imported");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    let result = invoke(&arguments, &mut Cursor::new("y\n1\n:back\n0\n"), true);
    result.assert_code(0);
    assert!(
        result.stderr.contains("수정할 항목 번호"),
        "{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("값 입력 / :back 검토"),
        "{}",
        result.stderr
    );
    assert!(result.text().contains(&format!("0x{}", "11".repeat(32))));
    assert_eq!(snapshot(&sources), before);
    assert!(!fixture.root.join("must-not-create-data").exists());

    let before_session = snapshot(&fixture.home);
    fixture
        .run("resume", "imported", "1\n:help\n:back\n0\n")
        .assert_code(0);
    assert_eq!(snapshot(&fixture.home), before_session);
    assert_eq!(snapshot(&sources), before);
}

#[test]
fn public_network_selection_updates_product_and_retains_selected_policy_on_resume() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let before = snapshot(&sources);
    let mut arguments = fixture.arguments("new", "network-demo");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    let input = format!(
        "y\n3\n1\n{}\n1\nJKS\nPKCS12\njoin-network\nno\nyes\n0\n",
        sources.join("network.json").display()
    );
    let result = invoke(&arguments, &mut Cursor::new(input), true);
    result.assert_code(0);
    assert!(result.text().contains("공개 목록 선택됨"));
    assert!(result.text().contains(&format!("0x{}", "22".repeat(20))));
    assert_eq!(snapshot(&sources), before);
    assert!(!fixture.root.join("must-not-create-data").exists());

    let result = fixture.run("resume", "network-demo", "3\n1\n\n1\n\n\n\n\n\n0\n");
    result.assert_code(0);
    for expected in [
        "Enter JKS",
        "Enter PKCS12",
        "Enter join-network",
        "Enter no",
        "Enter yes",
    ] {
        assert!(
            result.text().contains(expected),
            "missing {expected}: {}",
            result.text()
        );
    }
    assert_eq!(snapshot(&sources), before);
    assert!(!fixture.root.join("must-not-create-data").exists());
}

#[test]
fn accepted_network_policy_is_checkpointed_before_a_later_cancel() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let before = snapshot(&sources);
    let mut arguments = fixture.arguments("new", "paused-policy");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    let input = format!(
        "y\n3\n1\n{}\n1\nJKS\n:cancel\n",
        sources.join("network.json").display()
    );
    invoke(&arguments, &mut Cursor::new(input), true).assert_code(5);
    let result = fixture.run("resume", "paused-policy", "3\n1\n\n1\n:cancel\n");
    result.assert_code(5);
    assert!(result.text().contains("Enter JKS"), "{}", result.text());
    assert_eq!(snapshot(&sources), before);
    assert!(!fixture.root.join("must-not-create-data").exists());
}

#[test]
fn file_preparation_requires_confirmation_and_missing_materials_cannot_publish_outputs() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let mut arguments = fixture.arguments("new", "incomplete");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    invoke(&arguments, &mut Cursor::new("y\n0\n"), true).assert_code(0);
    let before = snapshot(&fixture.root);
    for input in [
        "5\n\n0\n",
        "5\n:back\n0\n",
        "5\n:help\n\n0\n",
        "5\n:cancel\n",
    ] {
        fixture
            .run("resume", "incomplete", input)
            .assert_code(if input.ends_with(":cancel\n") { 5 } else { 0 });
        assert_eq!(snapshot(&fixture.root), before);
    }
    let result = fixture.run("resume", "incomplete", "5\ny\n0\n");
    result.assert_code(0); // Saving the unchanged draft is independent of failed preparation.
    assert!(
        result
            .text()
            .contains("선택한 자료·필수 항목·권한·경로 충돌을 확인하세요")
    );
    assert!(!result.text().contains("설정 파일 준비를 마쳤습니다"));
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn imported_data_path_overlapping_management_writes_is_rejected_before_home_creation() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let product = sources.join("instance.json");
    let mut data: Value = serde_json::from_slice(&fs::read(&product).unwrap()).unwrap();
    data["storage"]["dataDirectory"] = fixture.home.to_str().unwrap().into();
    write(&product, serde_json::to_vec(&data).unwrap());
    let before = snapshot(&fixture.root);
    let mut arguments = fixture.arguments("new", "overlap");
    arguments.extend(strings(&["--from", product.to_str().unwrap()]));
    invoke(&arguments, &mut NoInput, true).assert_code(3);
    assert_eq!(snapshot(&fixture.root), before);
    assert!(!fixture.home.exists());
}

#[test]
fn backing_out_of_a_missing_product_or_chain_field_returns_to_review() {
    let fixture = Fixture::new();
    fixture.create_saved("back-demo");
    let before = snapshot(&fixture.root);
    for input in ["1\n:back\n0\n", "3\n1\n:back\n0\n"] {
        fixture.run("resume", "back-demo", input).assert_code(0);
        assert_eq!(snapshot(&fixture.root), before);
    }
}

#[test]
fn switching_to_existing_native_keeps_sources_and_clears_public_network_policy() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let native = sources.join("existing-native.json");
    let prepared = bxdl::config::network::load(
        &sources.join("instance.json"),
        &sources.join("network.json"),
        bxdl::config::network::RuntimeOptions::default(),
    )
    .unwrap();
    write(&native, prepared.native_bytes);
    let before = snapshot(&sources);
    let mut arguments = fixture.arguments("new", "native-demo");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    let input = format!(
        "y\n3\n1\n{}\n1\n\n\n\n\n\n0\n",
        sources.join("network.json").display()
    );
    invoke(&arguments, &mut Cursor::new(input), true).assert_code(0);
    let result = fixture.run(
        "resume",
        "native-demo",
        &format!("3\n2\n{}\n0\n", native.display()),
    );
    result.assert_code(0);
    assert!(result.text().contains("기존 native 선택됨"));
    let session =
        bxdl::setup::preparation::Preparation::resume(&fixture.home.join("sessions/native-demo"))
            .unwrap();
    assert_eq!(session.inputs().native.as_ref(), Some(&native));
    assert!(session.inputs().network.is_none());
    assert!(session.inputs().runtime_options.is_none());
    assert_eq!(snapshot(&sources), before);
    assert!(!fixture.root.join("must-not-create-data").exists());
}

#[test]
fn a_single_existing_field_can_be_edited_while_other_required_fields_are_missing() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let chain = sources.join("chain.json");
    let source_snapshot = snapshot(&sources);
    let result = fixture.run(
        "new",
        "partial-edit",
        &format!("y\n7\n8\n{}\n7\n5\n18081\n0\n", chain.display()),
    );
    result.assert_code(0);
    assert!(result.text().contains("18081"));
    {
        let preparation = bxdl::setup::preparation::Preparation::resume(
            &fixture.home.join("sessions/partial-edit"),
        )
        .unwrap();
        assert!(!preparation.draft().complete());
        assert!(preparation.draft().has_answer("chainDescription"));
        assert!(preparation.draft().has_answer("httpPort"));
        assert!(!preparation.draft().has_answer("nodeId"));
        assert!(!preparation.draft().has_answer("validatorKeystore"));
    }
    let before = snapshot(&fixture.root);
    let result = fixture.run(
        "resume",
        "partial-edit",
        &format!("7\n8\n{}\n0\n", chain.display()),
    );
    result.assert_code(0);
    assert!(result.text().contains("18081"));
    assert_eq!(snapshot(&fixture.root), before);
    assert_eq!(snapshot(&sources), source_snapshot);
}

#[test]
fn initial_draft_review_does_not_warn_about_an_invalid_saved_preparation() {
    let fixture = Fixture::new();
    let result = fixture.run("new", "new-draft", "y\n0\n");
    result.assert_code(0);
    let text = result.text();
    assert!(text.contains("저장된 준비 기록: DRAFT"), "{text}");
    assert!(text.contains("미선택"), "{text}");
    assert!(!text.contains("SETUP_PREPARATION_INCOMPLETE"), "{text}");
    assert!(
        !text.contains("저장 기록을 현재 유효한 준비 결과로 사용하지 마세요"),
        "{text}"
    );
    assert!(!fixture.home.join("data/new-draft").exists());
}
