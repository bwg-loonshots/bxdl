//! Console navigation may keep a private index, but opening or indexing an
//! existing target must never modify that target or authorize engine actions.
use bxdl::{cli, setup::Session};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
};

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    draft: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let home = root.join("management parent/관리 홈");
        let draft = root.join("PRIVATE-TARGET-CANARY 초안");
        draft_at(&draft, &root);
        Self {
            _temporary: temporary,
            root,
            home,
            draft,
        }
    }

    fn args(&self) -> Vec<String> {
        vec![
            "console".into(),
            "--home".into(),
            self.home.to_str().unwrap().into(),
        ]
    }

    fn run(&self, input: &str) -> Run {
        run(&self.args(), input, true)
    }

    fn open(&self, tail: &str) -> String {
        format!("4\n3\n{}\n{tail}", self.draft.display())
    }

    fn save(&self, label: &str) {
        let result = self.run(&self.open(&format!("2\n{label}\ny\n0\n0\n")));
        assert_eq!(result.code, 0, "{}", result.text);
        assert_eq!(catalog(&self.home)["entries"].as_array().unwrap().len(), 1);
    }
}

fn draft_at(path: &Path, references: &Path) {
    let mut draft = Session::create(path, None).unwrap();
    draft.set("instanceId", "public-node").unwrap();
    draft.set("nodeId", "public-node-id").unwrap();
    draft
        .set(
            "validatorPasswordFile",
            references
                .join("PRIVATE-CREDENTIAL-CANARY.pass")
                .to_str()
                .unwrap(),
        )
        .unwrap();
}

struct Run {
    code: i32,
    text: String,
    consumed: u64,
}
fn run(args: &[String], input: &str, interactive: bool) -> Run {
    let mut input = Cursor::new(input.as_bytes());
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = cli::run_with_input(args, &mut input, interactive, &mut out, &mut err);
    Run {
        code,
        text: format!(
            "{}{}",
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&err)
        ),
        consumed: input.position(),
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
fn catalog_path(home: &Path) -> PathBuf {
    let mut paths: Vec<_> = fs::read_dir(home.join("catalog"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();
    paths.pop().expect("saved catalog revision")
}
fn catalog(home: &Path) -> Value {
    serde_json::from_slice(&fs::read(catalog_path(home)).unwrap()).unwrap()
}

#[test]
fn opening_home_and_empty_lists_does_not_prepare_management_directories() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    for input in ["2\n0\n", "3\n0\n"] {
        let result = fixture.run(input);
        assert_eq!(result.code, 0, "{}", result.text);
        assert!(!fixture.home.exists());
        assert_eq!(snapshot(&fixture.root), before);
    }
}

#[test]
fn enter_exits_and_cancel_or_eof_pause_without_writing_anything() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    for (input, expected) in [("\n", 0), (":cancel\n", 5), ("", 5)] {
        let result = fixture.run(input);
        assert_eq!(result.code, expected, "{}", result.text);
        assert_eq!(snapshot(&fixture.root), before);
    }
}

#[test]
fn nonterminal_and_bad_options_refuse_without_consuming_input() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    let result = run(&fixture.args(), "PRIVATE-INPUT-CANARY\n", false);
    assert_eq!(result.code, 2, "{}", result.text);
    assert_eq!(result.consumed, 0);
    assert!(!result.text.contains("PRIVATE-INPUT-CANARY"));
    for tail in [
        vec!["--json"],
        vec!["--unknown", "PRIVATE-ARGUMENT-CANARY"],
        vec!["--home", "PRIVATE-ARGUMENT-CANARY"],
        vec!["--instance"],
        vec!["--instance", "one", "--instance", "PRIVATE-ARGUMENT-CANARY"],
        vec!["PRIVATE-ARGUMENT-CANARY"],
    ] {
        let mut args = fixture.args();
        args.extend(tail.into_iter().map(str::to_owned));
        let result = run(&args, "PRIVATE-INPUT-CANARY\n", true);
        assert_eq!(result.code, 2, "{}", result.text);
        assert_eq!(result.consumed, 0);
        assert!(!result.text.contains("PRIVATE-INPUT-CANARY"));
        assert!(!result.text.contains("PRIVATE-ARGUMENT-CANARY"));
    }
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn console_help_and_preparation_menu_navigation_have_no_side_effects() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    let result = run(
        &["console".into(), "--help".into()],
        "PRIVATE-INPUT-CANARY\n",
        false,
    );
    assert_eq!(result.code, 0, "{}", result.text);
    assert_eq!(result.consumed, 0);
    assert!(result.text.contains("console"));
    let result = fixture.run("1\n0\n0\n");
    assert_eq!(result.code, 0, "{}", result.text);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn opening_a_supported_draft_is_read_only_and_default_output_is_public() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    let result = fixture.run(&fixture.open("1\n0\n0\n"));
    assert_eq!(result.code, 0, "{}", result.text);
    assert!(result.text.contains("public-node"), "{}", result.text);
    assert!(!result.text.contains("PRIVATE-TARGET-CANARY"));
    assert!(!result.text.contains("PRIVATE-CREDENTIAL-CANARY"));
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn explicit_details_reveal_target_location_but_not_credential_references() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    let result = fixture.run(&fixture.open("3\n0\n0\n0\n"));
    assert_eq!(result.code, 0, "{}", result.text);
    assert!(
        result.text.contains("PRIVATE-TARGET-CANARY"),
        "{}",
        result.text
    );
    assert!(result.text.contains("setup"), "{}", result.text);
    assert!(!result.text.contains("PRIVATE-CREDENTIAL-CANARY"));
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn save_needs_explicit_approval_and_cancel_or_eof_cannot_create_home() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    for (tail, expected) in [
        ("2\n\n\n0\n0\n", 0),
        ("2\n\nno\n0\n0\n", 0),
        ("2\n\n:cancel\n", 5),
        ("2\n\n", 5),
    ] {
        let result = fixture.run(&fixture.open(tail));
        assert_eq!(result.code, expected, "{}", result.text);
        assert_eq!(snapshot(&fixture.root), before);
    }
}

#[test]
fn approved_save_creates_only_private_catalog_and_keeps_target_untouched() {
    let fixture = Fixture::new();
    let before = snapshot(&fixture.draft);
    fixture.save("검증 노드");
    assert_eq!(snapshot(&fixture.draft), before);
    assert!(!fixture.root.join("PRIVATE-CREDENTIAL-CANARY.pass").exists());
    let record = catalog(&fixture.home);
    let entries = record["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["label"], "검증 노드");
    let id = entries[0]["id"].as_str().unwrap();
    assert_eq!(id.len(), 32);
    assert!(id.bytes().all(|value| value.is_ascii_hexdigit()));
    assert!(!record.to_string().contains("PRIVATE-CREDENTIAL-CANARY"));
    for (path, entry) in snapshot(&fixture.home) {
        if entry.directory {
            assert_eq!(entry.mode & 0o777, 0o700, "{}", path.display());
        } else {
            assert_eq!(entry.mode & 0o777, 0o600, "{}", path.display());
        }
    }
}

#[test]
fn same_display_name_can_identify_two_independent_targets() {
    let fixture = Fixture::new();
    fixture.save("같은 이름");
    let other = fixture.root.join("다른 초안");
    draft_at(&other, &fixture.root);
    let before = snapshot(&other);
    let result = fixture.run(&format!(
        "4\n3\n{}\n2\n같은 이름\ny\n0\n0\n",
        other.display()
    ));
    assert_eq!(result.code, 0, "{}", result.text);
    let record = catalog(&fixture.home);
    let entries = record["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["label"], entries[1]["label"]);
    assert_ne!(entries[0]["id"], entries[1]["id"]);
    assert_eq!(snapshot(&other), before);
    let result = fixture.run("2\n0\n0\n");
    assert_eq!(result.code, 0, "{}", result.text);
    assert!(!result.text.contains("PRIVATE-TARGET-CANARY"));
    assert!(!result.text.contains("PRIVATE-CREDENTIAL-CANARY"));
}

#[test]
fn reading_a_changed_draft_does_not_refresh_the_index_without_approval() {
    let fixture = Fixture::new();
    fixture.save("유지할 별칭");
    let record = catalog(&fixture.home);
    let original_id = record["entries"][0]["id"].clone();
    Session::resume(&fixture.draft)
        .unwrap()
        .set("instanceId", "changed-public-node")
        .unwrap();
    let before = snapshot(&fixture.root);
    let result = fixture.run("2\n1\n1\n0\n0\n0\n");
    assert_eq!(result.code, 0, "{}", result.text);
    assert_eq!(snapshot(&fixture.root), before);
    assert_eq!(catalog(&fixture.home), record);
    let target_before = snapshot(&fixture.draft);
    let result = fixture.run("2\n1\n2\ny\n0\n0\n0\n");
    assert_eq!(result.code, 0, "{}", result.text);
    let record = catalog(&fixture.home);
    assert_eq!(record["entries"][0]["id"], original_id);
    assert_eq!(record["entries"][0]["label"], "유지할 별칭");
    assert_eq!(
        record["entries"][0]["target"]["name"],
        "changed-public-node"
    );
    assert_eq!(snapshot(&fixture.draft), target_before);
}

#[test]
fn hidden_catalog_entry_does_not_delete_the_draft_or_its_checkpoints() {
    let fixture = Fixture::new();
    fixture.save("지울 수 있는 목록 항목");
    let before = snapshot(&fixture.draft);
    let result = fixture.run("2\n1\n4\ny\n0\n0\n0\n");
    assert_eq!(result.code, 0, "{}", result.text);
    assert!(
        catalog(&fixture.home)["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(snapshot(&fixture.draft), before);
    assert!(Session::resume(&fixture.draft).is_ok());
}

#[test]
fn corrupt_catalog_preserves_evidence_and_still_allows_explicit_target_details() {
    let fixture = Fixture::new();
    fixture.save("기존 초안");
    fs::write(
        catalog_path(&fixture.home),
        b"PRIVATE-RAW-REASON-CANARY invalid JSON",
    )
    .unwrap();
    let before = snapshot(&fixture.root);
    let result = fixture.run(&fixture.open("3\n0\n0\n0\n"));
    assert_eq!(result.code, 0, "{}", result.text);
    assert!(
        result.text.contains("PRIVATE-TARGET-CANARY"),
        "{}",
        result.text
    );
    assert!(!result.text.contains("PRIVATE-RAW-REASON-CANARY"));
    assert!(!result.text.contains("PRIVATE-CREDENTIAL-CANARY"));
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn missing_catalog_target_is_not_recreated_or_removed_from_the_index_implicitly() {
    let fixture = Fixture::new();
    fixture.save("없어진 초안");
    fs::remove_dir_all(&fixture.draft).unwrap();
    let before = snapshot(&fixture.root);
    let result = fixture.run("2\n1\n0\n0\n0\n");
    assert_eq!(result.code, 0, "{}", result.text);
    assert!(!fixture.draft.exists());
    assert_eq!(snapshot(&fixture.root), before);
    assert_eq!(
        catalog(&fixture.home)["entries"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn replacement_at_the_same_path_cannot_refresh_the_saved_identity() {
    let fixture = Fixture::new();
    fixture.save("교체된 초안");
    fs::rename(&fixture.draft, fixture.root.join("original draft")).unwrap();
    draft_at(&fixture.draft, &fixture.root);
    let before = snapshot(&fixture.root);
    // Re-querying and attempting a saved record update may report the identity
    // mismatch, but must not silently adopt the replacement at the same name.
    let result = fixture.run("2\n1\n1\n2\ny\n0\n0\n0\n");
    assert_eq!(result.code, 0, "{}", result.text);
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn symlink_target_is_rejected_without_following_it_or_preparing_home() {
    let fixture = Fixture::new();
    let link = fixture.root.join("link draft");
    symlink(&fixture.draft, &link).unwrap();
    let before = snapshot(&fixture.draft);
    let result = fixture.run(&format!("4\n3\n{}\n:back\n0\n", link.display()));
    assert_eq!(result.code, 0, "{}", result.text);
    assert_eq!(snapshot(&fixture.draft), before);
    assert!(!fixture.home.exists());
    assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
}

#[test]
fn public_home_is_not_silently_chmodded_or_adopted() {
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.home).unwrap();
    fs::set_permissions(&fixture.home, fs::Permissions::from_mode(0o755)).unwrap();
    let before = snapshot(&fixture.root);
    let result = fixture.run(&fixture.open("3\n0\n0\n0\n"));
    assert_eq!(result.code, 0, "{}", result.text);
    assert_eq!(snapshot(&fixture.root), before);
    assert!(
        result.text.contains("PRIVATE-TARGET-CANARY"),
        "{}",
        result.text
    );
}

#[test]
fn input_and_prompt_io_failures_preserve_exit_seven_and_create_nothing() {
    struct BrokenInput;
    impl std::io::Read for BrokenInput {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("PRIVATE-IO-CANARY"))
        }
    }
    impl std::io::BufRead for BrokenInput {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            Err(std::io::Error::other("PRIVATE-IO-CANARY"))
        }
        fn consume(&mut self, _: usize) {}
    }
    struct BrokenOutput;
    impl std::io::Write for BrokenOutput {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("PRIVATE-IO-CANARY"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let fixture = Fixture::new();
    let before = snapshot(&fixture.root);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(
        cli::run_with_input(&fixture.args(), &mut BrokenInput, true, &mut out, &mut err),
        7
    );
    assert!(!String::from_utf8_lossy(&err).contains("PRIVATE-IO-CANARY"));
    assert_eq!(
        cli::run_with_input(
            &fixture.args(),
            &mut Cursor::new(b"0\n"),
            true,
            &mut Vec::new(),
            &mut BrokenOutput
        ),
        7
    );
    assert_eq!(snapshot(&fixture.root), before);
}
