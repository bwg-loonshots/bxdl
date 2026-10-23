//! Integrated preparation accepts an explicit name and saves only approved
//! configuration work. It must not install, initialize, or start a node.
#![cfg(unix)]

use bxdl::cli;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
        self.run("new", name, "y\n:cancel\n").assert_code(5);
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
        "nigo.protocol.chain-id": "11578",
        "nigo.protocol.consensus.profile-id": "TEST_QBFT",
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

/// A small verified archive permits testing preparation publication without
/// starting a process or requiring a vendor runtime download.
fn prepare_package(fixture: &Fixture, sources: &Path) -> (PathBuf, PathBuf) {
    let archive = sources.join("package.tar.gz");
    let lock = sources.join("engine.lock.json");
    let identity: Value = serde_json::from_slice(include_bytes!(
        "../contracts/nigo/development-clean-2026-09-18/evidence/engine-info.json"
    ))
    .unwrap();
    let jar = b"fixture jar never executed";
    let java = b"fixture java never executed";
    let digest = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    write(
        &lock,
        serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "jarSha256": digest(jar),
            "jarSizeBytes": jar.len(),
            "javaSha256": digest(java),
            "expected": identity,
        }))
        .unwrap(),
    );
    let stage = fixture.root.join("test-package-stage");
    for directory in ["bin", "engine", "runtime/bin", "licenses"] {
        fs::create_dir_all(stage.join(directory)).unwrap();
    }
    for (name, raw, mode) in [
        ("bin/bxdl", b"fixture CLI never executed".as_slice(), 0o755),
        ("engine/nigo-node.jar", jar.as_slice(), 0o644),
        ("runtime/bin/java", java.as_slice(), 0o755),
        (
            "licenses/THIRD_PARTY_NOTICES",
            b"test only".as_slice(),
            0o644,
        ),
        ("licenses/SBOM.json", b"{}".as_slice(), 0o644),
    ] {
        write(&stage.join(name), raw);
        fs::set_permissions(stage.join(name), fs::Permissions::from_mode(mode)).unwrap();
    }
    let spec = fixture.root.join("test-package-spec.json");
    write(
        &spec,
        serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "kind": "bxdl-package",
            "product": {"name":"BXDL", "version":"0.1.0-dev", "revision":"development"},
            "channel": "development",
            "platform": {"os":"darwin", "arch":"arm64", "libc":"none", "minGlibc":"none", "javaMajor":21, "backend":"rocksdb"},
            "engine": {"revision":identity["source"]["commit"], "jarSha256":digest(jar), "contractStatus":"proposed", "contractRevision":identity["contract"]["fingerprint"]},
            "runtime": {"vendor":"test-only", "version":"21-test", "javaSha256":digest(java)},
        }))
        .unwrap(),
    );
    bxdl::artifact::build(&bxdl::artifact::BuildOptions {
        root: stage,
        spec_path: spec,
        output: archive.clone(),
        signing_key_path: None,
        allow_unsigned_development: true,
    })
    .unwrap();
    (archive, lock)
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
    for (input, expected) in [(":cancel\n", 5), ("", 5), (":help\n:cancel\n", 5)] {
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
        fixture.run("resume", "demo", ":cancel\n").assert_code(5);
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
    fixture
        .run("resume", "console-demo", ":cancel\n")
        .assert_code(5);
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
    let (archive, lock) = prepare_package(&fixture, &sources);
    let before = snapshot(&sources);
    let mut arguments = fixture.arguments("new", "imported");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    let input = format!(
        "y\n{}\n2\ny\n{}\n1\n{}\n\n\n\n\n\nn\n",
        archive.display(),
        lock.display(),
        sources.join("network.json").display()
    );
    let result = invoke(&arguments, &mut Cursor::new(input), true);
    result.assert_code(0);
    assert!(
        !result.text().contains("제품 설정 채우기·수정"),
        "{}",
        result.text()
    );
    assert!(
        !result.text().contains("수정할 항목 번호"),
        "{}",
        result.text()
    );
    assert_eq!(snapshot(&sources), before);
    assert!(!fixture.root.join("must-not-create-data").exists());
    let before_session = snapshot(&fixture.home);
    fixture
        .run("resume", "imported", ":help\nn\n")
        .assert_code(0);
    assert_eq!(snapshot(&fixture.home), before_session);
    assert_eq!(snapshot(&sources), before);
}

#[test]
fn public_network_selection_updates_product_and_retains_selected_policy_on_resume() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let (archive, lock) = prepare_package(&fixture, &sources);
    let before = snapshot(&sources);
    let mut arguments = fixture.arguments("new", "network-demo");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    let input = format!(
        "y\n{}\n2\ny\n{}\n1\n{}\nJKS\nPKCS12\njoin-network\nno\nyes\nn\n",
        archive.display(),
        lock.display(),
        sources.join("network.json").display()
    );
    invoke(&arguments, &mut Cursor::new(input), true).assert_code(0);
    {
        let session = bxdl::setup::preparation::Preparation::resume(
            &fixture.home.join("sessions/network-demo"),
        )
        .unwrap();
        let options = session.inputs().runtime_options.as_ref().unwrap();
        assert_eq!(
            options.key_store_type,
            bxdl::config::network::StoreType::Jks
        );
        assert_eq!(
            options.trust_store_type,
            bxdl::config::network::StoreType::Pkcs12
        );
        assert_eq!(
            options.mode,
            bxdl::config::network::NetworkMode::JoinNetwork
        );
        assert!(!options.consensus_auto_start);
        assert!(options.archive_enabled);
    }
    let prepared_state = snapshot(&fixture.home);
    fixture.run("resume", "network-demo", "n\n").assert_code(0);
    assert_eq!(snapshot(&fixture.home), prepared_state);
    assert_eq!(snapshot(&sources), before);
    assert!(!fixture.root.join("must-not-create-data").exists());
}

#[test]
fn accepted_network_policy_is_checkpointed_before_a_later_cancel() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let (archive, lock) = prepare_package(&fixture, &sources);
    let before = snapshot(&sources);
    let mut arguments = fixture.arguments("new", "paused-policy");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    let input = format!(
        "y\n{}\n2\ny\n{}\n1\n{}\nJKS\n:cancel\n",
        archive.display(),
        lock.display(),
        sources.join("network.json").display()
    );
    invoke(&arguments, &mut Cursor::new(input), true).assert_code(5);
    {
        let session = bxdl::setup::preparation::Preparation::resume(
            &fixture.home.join("sessions/paused-policy"),
        )
        .unwrap();
        assert_eq!(session.inputs().runtime_answers.len(), 1);
        assert!(
            session
                .inputs()
                .runtime_answers
                .values()
                .any(|v| v == "JKS")
        );
        assert!(session.inputs().runtime_options.is_none());
    }
    // The first policy and all supplied paths must be skipped on resume.
    fixture
        .run(
            "resume",
            "paused-policy",
            "PKCS12\njoin-network\nno\nyes\nn\n",
        )
        .assert_code(0);
    {
        let session = bxdl::setup::preparation::Preparation::resume(
            &fixture.home.join("sessions/paused-policy"),
        )
        .unwrap();
        assert_eq!(session.inputs().runtime_answers.len(), 5);
        assert_eq!(
            session
                .inputs()
                .runtime_options
                .as_ref()
                .unwrap()
                .key_store_type,
            bxdl::config::network::StoreType::Jks
        );
    }
    assert_eq!(snapshot(&sources), before);
    assert!(!fixture.root.join("must-not-create-data").exists());
}

#[test]
fn file_preparation_requires_confirmation_and_leaves_a_saved_draft_without_approval() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let (archive, lock) = prepare_package(&fixture, &sources);
    let mut arguments = fixture.arguments("new", "unapproved");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    let input = format!(
        "y\n{}\n2\ny\n{}\n1\n{}\n\n\n\n\n\nn\n",
        archive.display(),
        lock.display(),
        sources.join("network.json").display()
    );
    invoke(&arguments, &mut Cursor::new(input), true).assert_code(0);
    let generated = fixture.home.join("sessions/unapproved/generated");
    assert_eq!(fs::read_dir(&generated).unwrap().count(), 0);
    let before = snapshot(&fixture.root);
    for (input, expected) in [
        ("\n", 0),
        ("n\n", 0),
        (":help\nn\n", 0),
        (":cancel\n", 5),
        ("", 5),
    ] {
        fixture
            .run("resume", "unapproved", input)
            .assert_code(expected);
        assert_eq!(snapshot(&fixture.root), before);
    }
    fixture.run("resume", "unapproved", "y\n").assert_code(0);
    assert_eq!(fs::read_dir(&generated).unwrap().count(), 2);
    let after_prepare = snapshot(&fixture.root);
    fixture.run("resume", "unapproved", "n\n").assert_code(0);
    assert_eq!(snapshot(&fixture.root), after_prepare);
    assert!(!fixture.root.join("must-not-create-data").exists());
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
fn back_at_first_question_saves_and_back_at_later_question_revisits_previous_answer() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let (archive, _) = prepare_package(&fixture, &sources);
    fixture.create_saved("back-demo");
    let before = snapshot(&fixture.root);
    fixture.run("resume", "back-demo", ":back\n").assert_code(0);
    assert_eq!(snapshot(&fixture.root), before);
    let input = format!("{}\n:back\n\n:cancel\n", archive.display());
    fixture.run("resume", "back-demo", &input).assert_code(5);
    let session =
        bxdl::setup::preparation::Preparation::resume(&fixture.home.join("sessions/back-demo"))
            .unwrap();
    assert_eq!(session.inputs().archive.as_ref(), Some(&archive));
    assert!(session.inputs().unsigned.is_none());
    assert!(session.inputs().lock.is_none());
}

#[test]
fn switching_to_existing_native_keeps_sources_and_clears_public_network_policy() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let (archive, lock) = prepare_package(&fixture, &sources);
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
        "y\n{}\n2\ny\n{}\n1\n{}\n\n\n\n\n\nn\n",
        archive.display(),
        lock.display(),
        sources.join("network.json").display()
    );
    invoke(&arguments, &mut Cursor::new(input), true).assert_code(0);
    fixture
        .run(
            "resume",
            "native-demo",
            &format!("edit native\n{}\nn\n", native.display()),
        )
        .assert_code(0);
    let session =
        bxdl::setup::preparation::Preparation::resume(&fixture.home.join("sessions/native-demo"))
            .unwrap();
    assert_eq!(session.inputs().native.as_ref(), Some(&native));
    assert!(session.inputs().network.is_none());
    assert!(session.inputs().runtime_options.is_none());
    assert!(session.inputs().runtime_answers.is_empty());
    assert_eq!(snapshot(&sources), before);
    assert!(!fixture.root.join("must-not-create-data").exists());
}

#[test]
fn a_single_existing_field_can_be_edited_while_other_required_fields_are_missing() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let chain = sources.join("chain.json");
    let source_snapshot = snapshot(&sources);
    fixture.create_saved("partial-edit");
    for (field, value) in [
        ("chainDescription", chain.to_str().unwrap()),
        ("httpPort", "18081"),
    ] {
        let mut args = fixture.arguments("edit", "partial-edit");
        args.extend(strings(&["--field", field]));
        invoke(&args, &mut Cursor::new(format!("{value}\n")), true).assert_code(0);
    }
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
    let mut args = fixture.arguments("edit", "partial-edit");
    args.extend(strings(&["--field", "chainDescription"]));
    invoke(
        &args,
        &mut Cursor::new(format!("{}\n", chain.display())),
        true,
    )
    .assert_code(0);
    assert_eq!(snapshot(&fixture.root), before);
    assert_eq!(snapshot(&sources), source_snapshot);
}

#[test]
fn initial_draft_does_not_warn_about_an_invalid_saved_preparation() {
    let fixture = Fixture::new();
    let result = fixture.run("new", "new-draft", "y\n:cancel\n");
    result.assert_code(5);
    let text = result.text();
    assert!(!text.contains("SETUP_PREPARATION_INCOMPLETE"), "{text}");
    assert!(
        !text.contains("저장 기록을 현재 유효한 준비 결과로 사용하지 마세요"),
        "{text}"
    );
    assert!(!fixture.home.join("data/new-draft").exists());
    let preparation =
        bxdl::setup::preparation::Preparation::resume(&fixture.home.join("sessions/new-draft"))
            .unwrap();
    assert_eq!(preparation.summary().preparation, "DRAFT");
}

#[test]
fn new_draft_selects_network_before_only_missing_product_questions() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let (archive, lock) = prepare_package(&fixture, &sources);
    let before = snapshot(&sources);
    let credentials = [
        "validatorKeystore",
        "validatorPasswordFile",
        "tlsKeyStore",
        "tlsKeyPasswordFile",
        "tlsTrustStore",
        "tlsTrustPasswordFile",
    ]
    .map(|field| format!("{}\n", sources.join(format!("{CANARY}-{field}")).display()))
    .concat();
    let input = format!(
        "y\n{}\n2\ny\n{}\n1\n{}\n{}\n1\n\ninvalid-port\n18081\n{}\n\n\n\n\nn\n",
        archive.display(),
        lock.display(),
        sources.join("network.json").display(),
        sources.join("chain.json").display(),
        credentials,
    );
    let result = fixture.run("new", "fresh-node", &input);
    result.assert_code(0);
    let preparation =
        bxdl::setup::preparation::Preparation::resume(&fixture.home.join("sessions/fresh-node"))
            .unwrap();
    assert!(preparation.draft().complete());
    assert_eq!(preparation.summary().preparation, "DRAFT");
    assert!(
        preparation
            .review_rows()
            .iter()
            .any(|(_, value)| value == "18081")
    );
    assert!(
        preparation
            .review_rows()
            .iter()
            .any(|(_, value)| value == &format!("0x{}", "11".repeat(32)))
    );
    assert_eq!(
        fs::read_dir(preparation.path().join("generated"))
            .unwrap()
            .count(),
        0
    );
    for path in [
        "data/fresh-node",
        "instances/fresh-node",
        "packages/fresh-node",
    ] {
        assert!(!fixture.home.join(path).exists());
    }
    assert_eq!(snapshot(&sources), before);
}

#[test]
fn review_edits_one_field_and_publishes_a_new_revision_without_overwriting_previous_files() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let (archive, lock) = prepare_package(&fixture, &sources);
    let source_snapshot = snapshot(&sources);
    let mut arguments = fixture.arguments("new", "revisions");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    let input = format!(
        "y\n{}\n2\ny\n{}\n1\n{}\n\n\n\n\n\ny\n",
        archive.display(),
        lock.display(),
        sources.join("network.json").display()
    );
    invoke(&arguments, &mut Cursor::new(input), true).assert_code(0);
    let generated = fixture.home.join("sessions/revisions/generated");
    let first_product = generated.join("instance-0001.json");
    let first_native = generated.join("node-0001.json");
    let original_product = fs::read(&first_product).unwrap();
    let original_native = fs::read(&first_native).unwrap();
    let original_inode = fs::metadata(&first_product).unwrap().ino();
    fixture
        .run("resume", "revisions", "edit httpPort\n18081\ny\n")
        .assert_code(0);
    assert_eq!(fs::read(&first_product).unwrap(), original_product);
    assert_eq!(fs::read(&first_native).unwrap(), original_native);
    assert_eq!(fs::metadata(&first_product).unwrap().ino(), original_inode);
    let mut old: Value = serde_json::from_slice(&original_product).unwrap();
    let second: Value =
        serde_json::from_slice(&fs::read(generated.join("instance-0002.json")).unwrap()).unwrap();
    old["http"]["port"] = 18081.into();
    assert_eq!(second, old);
    let preparation =
        bxdl::setup::preparation::Preparation::resume(&fixture.home.join("sessions/revisions"))
            .unwrap();
    assert_eq!(preparation.summary().plan_revision, 2);
    preparation.check().unwrap();
    assert_eq!(snapshot(&sources), source_snapshot);
    assert!(!fixture.root.join("must-not-create-data").exists());
}

#[test]
fn a_failed_preparation_is_not_retried_or_published_after_eof() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let (_, lock) = prepare_package(&fixture, &sources);
    let invalid_archive = sources.join("invalid-package.tar.gz");
    write(&invalid_archive, b"not an archive");
    let source_snapshot = snapshot(&sources);
    let mut arguments = fixture.arguments("new", "bad-package");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    let input = format!(
        "y\n{}\n2\ny\n{}\n1\n{}\n\n\n\n\n\ny\n",
        invalid_archive.display(),
        lock.display(),
        sources.join("network.json").display()
    );
    let result = invoke(&arguments, &mut Cursor::new(input), true);
    result.assert_code(5);
    let preparation =
        bxdl::setup::preparation::Preparation::resume(&fixture.home.join("sessions/bad-package"))
            .unwrap();
    assert_eq!(preparation.summary().preparation, "DRAFT");
    assert_eq!(preparation.summary().plan_revision, 0);
    assert_eq!(
        fs::read_dir(preparation.path().join("generated"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(snapshot(&sources), source_snapshot);
}

#[test]
fn read_only_queries_revalidate_prepared_inputs_and_edit_preserves_published_files() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let (archive, lock) = prepare_package(&fixture, &sources);
    let mut arguments = fixture.arguments("new", "inspectable");
    arguments.extend(strings(&[
        "--from",
        sources.join("instance.json").to_str().unwrap(),
    ]));
    let input = format!(
        "y\n{}\n2\ny\n{}\n1\n{}\n\n\n\n\n\ny\n",
        archive.display(),
        lock.display(),
        sources.join("network.json").display()
    );
    invoke(&arguments, &mut Cursor::new(input), true).assert_code(0);
    let generated = fixture.home.join("sessions/inspectable/generated");
    let immutable_files = snapshot(&generated);
    let mut show = fixture.arguments("show", "inspectable");
    show.push("--json".into());
    let list = strings(&[
        "setup",
        "list",
        "--home",
        fixture.home.to_str().unwrap(),
        "--json",
    ]);
    let before_queries = snapshot(&fixture.root);
    let result = invoke(&show, &mut NoInput, false);
    result.assert_code(0);
    let report: Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(report["data"]["inspection"]["verification"], "VERIFIED");
    assert_eq!(
        report["data"]["inspection"]["summary"]["preparation"],
        "PREPARED_RECORDED"
    );
    let result = invoke(&list, &mut NoInput, false);
    result.assert_code(0);
    let report: Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(
        report["data"]["entries"][0]["inspection"]["verification"],
        "NOT_CHECKED"
    );
    assert_eq!(snapshot(&fixture.root), before_queries);

    // Equal bytes under a new inode are still changed inputs requiring review.
    let replacement = sources.join("replacement.tar.gz");
    write(&replacement, fs::read(&archive).unwrap());
    fs::rename(&replacement, &archive).unwrap();
    let before_changed_query = snapshot(&fixture.root);
    let result = invoke(&show, &mut NoInput, false);
    result.assert_code(5);
    let report: Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(report["reasonCode"], "PREPARATION_CHANGED");
    assert_eq!(report["data"]["inspection"]["verification"], "CHANGED");
    assert_eq!(
        report["data"]["inspection"]["summary"]["preparation"],
        "PREPARED_RECORDED"
    );
    assert_eq!(snapshot(&fixture.root), before_changed_query);

    let mut edit = fixture.arguments("edit", "inspectable");
    edit.extend(strings(&["--field", "httpPort"]));
    invoke(&edit, &mut Cursor::new("18081\n"), true).assert_code(0);
    assert_eq!(snapshot(&generated), immutable_files);
    let after_edit = snapshot(&fixture.root);
    let result = invoke(&show, &mut NoInput, false);
    result.assert_code(0);
    let report: Value = serde_json::from_str(&result.stdout).unwrap();
    let inspection = &report["data"]["inspection"];
    assert_eq!(inspection["verification"], "NOT_PREPARED");
    assert_eq!(inspection["summary"]["preparation"], "DRAFT");
    assert!(
        inspection["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field["name"] == "httpPort" && field["value"] == "18081")
    );
    assert_eq!(snapshot(&fixture.root), after_edit);
    assert!(!fixture.root.join("must-not-create-data").exists());
}

#[test]
fn replacing_a_checkpoint_while_answering_cannot_complete_using_stale_state() {
    struct ReplacingInput {
        input: Cursor<Vec<u8>>,
        trigger_after: u64,
        checkpoint: PathBuf,
        root: PathBuf,
        after_replacement: Option<BTreeMap<PathBuf, Entry>>,
    }
    impl ReplacingInput {
        fn replace_once(&mut self) {
            if self.after_replacement.is_none() && self.input.position() >= self.trigger_after {
                let replacement = self.root.join("checkpoint-replacement");
                write(&replacement, fs::read(&self.checkpoint).unwrap());
                fs::rename(replacement, &self.checkpoint).unwrap();
                self.after_replacement = Some(snapshot(&self.root));
            }
        }
    }
    impl io::Read for ReplacingInput {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.replace_once();
            io::Read::read(&mut self.input, buffer)
        }
    }
    impl io::BufRead for ReplacingInput {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            self.replace_once();
            io::BufRead::fill_buf(&mut self.input)
        }
        fn consume(&mut self, amount: usize) {
            io::BufRead::consume(&mut self.input, amount);
        }
    }
    for checkpoint_store in ["draft", "workflow"] {
        for (answer, trigger_after) in [("n\n", 0), (":back\n\n", 6)] {
            let fixture = Fixture::new();
            let sources = prepare_sources(&fixture);
            let (archive, lock) = prepare_package(&fixture, &sources);
            let mut arguments = fixture.arguments("new", "checkpoint-race");
            arguments.extend(strings(&[
                "--from",
                sources.join("instance.json").to_str().unwrap(),
            ]));
            let input = format!(
                "y\n{}\n2\ny\n{}\n1\n{}\n\n\n\n\n\nn\n",
                archive.display(),
                lock.display(),
                sources.join("network.json").display()
            );
            invoke(&arguments, &mut Cursor::new(input), true).assert_code(0);
            let store = fixture
                .home
                .join("sessions/checkpoint-race")
                .join(checkpoint_store);
            let checkpoint = fs::read_dir(store)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .max()
                .unwrap();
            let mut input = ReplacingInput {
                input: Cursor::new(answer.as_bytes().to_vec()),
                trigger_after,
                checkpoint,
                root: fixture.root.clone(),
                after_replacement: None,
            };
            let result = invoke(
                &fixture.arguments("resume", "checkpoint-race"),
                &mut input,
                true,
            );
            result.assert_code(3);
            assert!(
                result.text().contains("SETUP_STATE_CORRUPT"),
                "{}",
                result.text()
            );
            assert_eq!(snapshot(&fixture.root), input.after_replacement.unwrap());
        }
    }
}

#[test]
fn query_next_input_follows_network_dependencies_before_product_and_policy_fields() {
    let fixture = Fixture::new();
    let sources = prepare_sources(&fixture);
    let (archive, lock) = prepare_package(&fixture, &sources);
    fixture.create_saved("dependency-order");
    let workspace = fixture.home.join("sessions/dependency-order");
    {
        let mut session = bxdl::setup::preparation::Preparation::resume(&workspace).unwrap();
        let mut inputs = session.inputs().clone();
        inputs.archive = Some(archive);
        inputs.lock = Some(lock);
        inputs.unsigned = Some(true);
        inputs.network = Some(sources.join("network.json"));
        session.set_inputs(inputs).unwrap();
    }
    let show = |next: &str, node: &str| {
        let before = snapshot(&fixture.root);
        let mut args = fixture.arguments("show", "dependency-order");
        args.push("--json".into());
        let result = invoke(&args, &mut NoInput, false);
        result.assert_code(0);
        let report: Value = serde_json::from_str(&result.stdout).unwrap();
        assert_eq!(report["data"]["inspection"]["nextInput"], next);
        assert_eq!(report["data"]["inspection"]["nodeSelection"], node);
        assert_eq!(snapshot(&fixture.root), before);
    };
    show("chainDescription", "MISSING");
    {
        let mut session = bxdl::setup::preparation::Preparation::resume(&workspace).unwrap();
        session
            .set_field(
                "chainDescription",
                sources.join("chain.json").to_str().unwrap(),
            )
            .unwrap();
    }
    show("node", "MISSING");
    {
        let mut session = bxdl::setup::preparation::Preparation::resume(&workspace).unwrap();
        session
            .select_node(&format!("0x{}", "11".repeat(32)))
            .unwrap();
    }
    show("httpAddress", "MATCH");
    {
        let mut session = bxdl::setup::preparation::Preparation::resume(&workspace).unwrap();
        session.set_field("httpAddress", "127.0.0.1").unwrap();
    }
    show("httpPort", "MATCH");
    fs::rename(
        sources.join("network.json"),
        sources.join("network-offline.json"),
    )
    .unwrap();
    let before = snapshot(&fixture.root);
    let list = strings(&[
        "setup",
        "list",
        "--home",
        fixture.home.to_str().unwrap(),
        "--json",
    ]);
    let result = invoke(&list, &mut NoInput, false);
    result.assert_code(0);
    let report: Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(
        report["data"]["entries"][0]["inspection"]["nodeSelection"],
        "NOT_CHECKED"
    );
    assert_eq!(snapshot(&fixture.root), before);
    show("node", "UNVERIFIED");
}
