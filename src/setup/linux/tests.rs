//! Synthetic user-journal/CLI tests. These never supply a JRE/JAR, invoke sudo,
//! create an account, register a systemd service, or assert Linux acceptance.
use super::*;
use crate::operator::{
    catalog::Catalog,
    linux_setup::{self, Options},
};
use std::{
    fs,
    io::Cursor,
    os::unix::fs::{MetadataExt, symlink},
};

struct ControlBase(Option<PathBuf>);
impl Drop for ControlBase {
    fn drop(&mut self) {
        TEST_CONTROL_BASE.with(|base| *base.borrow_mut() = self.0.take());
    }
}
struct Fixture {
    _control: ControlBase,
    _temporary: tempfile::TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        assert!(
            user().is_ok(),
            "user-journal tests require an ordinary non-root test user"
        );
        let temporary = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let previous =
            TEST_CONTROL_BASE.with(|base| base.replace(Some(root.join("system-instances"))));
        Self {
            _control: ControlBase(previous),
            _temporary: temporary,
            root,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
struct FileRecord {
    path: PathBuf,
    inode: u64,
    mode: u32,
    uid: u32,
    bytes: Option<Vec<u8>>,
}
fn recorded(root: &Path) -> Vec<FileRecord> {
    fn visit(root: &Path, path: &Path, entries: &mut Vec<FileRecord>) {
        for item in fs::read_dir(path).unwrap() {
            let path = item.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            entries.push(FileRecord {
                path: path.strip_prefix(root).unwrap().into(),
                inode: meta.ino(),
                mode: meta.mode(),
                uid: meta.uid(),
                bytes: if meta.is_file() {
                    Some(fs::read(&path).unwrap())
                } else {
                    None
                },
            });
            if meta.is_dir() {
                visit(root, &path, entries);
            }
        }
    }
    let mut entries = Vec::new();
    visit(root, root, &mut entries);
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries
}
fn options<'a>(home: &'a Path, name: &'a str, resume: bool) -> Options<'a> {
    Options {
        home,
        name,
        resume,
        from: None,
        runtime: None,
        jar: None,
        native: None,
    }
}
fn complete_product(preparation: &mut Preparation, root: &Path) {
    for field in super::super::FIELDS {
        let value = match field.name {
            "instanceId" | "dataDirectory" => continue,
            "nodeId" => format!("0x{}", "11".repeat(32)),
            "httpAddress" | "p2pAddress" => "127.0.0.1".into(),
            "httpPort" => "18080".into(),
            "p2pPort" => "19090".into(),
            name => root.join("sources").join(name).to_str().unwrap().into(),
        };
        preparation.set_field(field.name, &value).unwrap();
    }
}
fn synthetic_review(preparation: &mut Preparation) {
    // Exercise journal transitions without pretending a real supply was reviewed.
    preparation.record.revision = 1;
    preparation.record.review = Some(Reviewed {
        revision: 1,
        request: preparation.request_path(1),
        sha256: "a".repeat(64),
        product_sha256: "b".repeat(64),
    });
    preparation.save().unwrap();
}

#[test]
fn default_control_is_fixed_and_names_cannot_escape_it() {
    assert_eq!(
        control("node-a").unwrap(),
        Path::new("/var/lib/bxdl/instances/node-a")
    );
    for name in ["", "../other", "a/b", "A", "-node", "node_1", ".", ".."] {
        assert!(control(name).is_err(), "{name}");
    }
}

#[test]
fn declining_or_eof_before_creation_leaves_no_management_files() {
    let f = Fixture::new();
    for (index, answer) in ["", "n\n", ":cancel\n", ":back\n"].into_iter().enumerate() {
        let home = f.root.join(format!("home-{index}"));
        let mut out = Vec::new();
        let result = linux_setup::run(
            options(&home, "local", false),
            &mut Cursor::new(answer),
            &mut out,
        )
        .unwrap();
        assert_eq!(result.code, "LINUX_SETUP_NOT_CREATED");
        assert_eq!(result.exit, 5);
        assert!(!home.exists());
    }
    assert!(recorded(&f.root).is_empty());
}

#[test]
fn initial_checkpoint_contains_fixed_identity_and_resume_is_read_only() {
    let f = Fixture::new();
    let path = f.root.join("preparation");
    Preparation::validate_new(&path, "node-a", None).unwrap();
    assert!(!path.exists());
    let preparation = Preparation::create(&path, "node-a", None).unwrap();
    assert_eq!(
        preparation.field_value("instanceId").as_deref(),
        Some("node-a")
    );
    assert_eq!(
        preparation.field_value("dataDirectory"),
        Some(
            control("node-a")
                .unwrap()
                .join("data")
                .to_str()
                .unwrap()
                .into()
        )
    );
    assert_eq!(fs::read_dir(path.join("draft")).unwrap().count(), 1);
    let summary = preparation.summary().unwrap();
    assert_eq!(summary.platform, "linux-amd64");
    assert_eq!(summary.preparation, "DRAFT");
    assert_eq!(summary.installation, "NOT_STARTED");
    assert_eq!(summary.engine_validation, "NOT_RUN");
    assert_eq!(summary.runtime_readiness, "NOT_CHECKED");
    assert_eq!(
        summary.missing_inputs,
        ["runtimeRoot", "engineJar", "nativeConfig"]
    );
    assert!(
        !summary
            .missing_fields
            .iter()
            .any(|f| f == "instanceId" || f == "dataDirectory")
    );
    assert!(!summary.control.exists());
    drop(preparation);
    let before = recorded(&f.root);
    assert_eq!(Preparation::inspect(&path).unwrap().revision, 0);
    drop(Preparation::open(&path).unwrap());
    assert_eq!(recorded(&f.root), before);
}

#[test]
fn cancellation_preserves_answers_and_list_show_do_not_run_or_write() {
    let f = Fixture::new();
    let home = f.root.join("home");
    let source = f.root.join("PRIVATE_SOURCE_PATH");
    let answers = format!("y\n{}\n:cancel\n", source.display());
    let mut out = Vec::new();
    let result = linux_setup::run(
        options(&home, "local", false),
        &mut Cursor::new(answers),
        &mut out,
    )
    .unwrap();
    assert_eq!(result.code, "LINUX_SETUP_PAUSED");
    let path = home.join("sessions/local");
    let preparation = Preparation::open(&path).unwrap();
    assert_eq!(preparation.sources().runtime_root.as_ref(), Some(&source));
    drop(preparation);
    let before = recorded(&f.root);
    let resumed = linux_setup::run(
        options(&home, "local", true),
        &mut Cursor::new(":cancel\n"),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(resumed.code, "LINUX_SETUP_PAUSED");
    let listing = linux_setup::list(&home).unwrap();
    assert_eq!(listing["entries"][0]["name"], "local");
    let shown = linux_setup::show(&home, "local").unwrap();
    assert_eq!(shown["reviewedInputs"], "NOT_REVIEWED");
    assert_eq!(shown["installation"], "NOT_STARTED");
    assert!(!shown.to_string().contains("PRIVATE_SOURCE_PATH"));
    assert_eq!(recorded(&f.root), before);
    assert!(Catalog::open(&home).unwrap().unwrap().entries().is_empty());
    assert!(!source.exists());
    assert!(!control("local").unwrap().exists());
}

#[test]
fn invalid_edits_and_unchanged_answers_preserve_prior_review_and_files() {
    let f = Fixture::new();
    let path = f.root.join("preparation");
    let mut preparation = Preparation::create(&path, "node-a", None).unwrap();
    preparation.set_field("httpPort", "18080").unwrap();
    let source = f.root.join("runtime");
    preparation.set_source("runtimeRoot", &source).unwrap();
    synthetic_review(&mut preparation);
    let before = recorded(&f.root);
    for (field, value) in [
        ("httpPort", "invalid"),
        ("instanceId", "other"),
        ("dataDirectory", "/other"),
        ("unknown", "value"),
    ] {
        assert!(preparation.set_field(field, value).is_err());
        assert_eq!(recorded(&f.root), before);
    }
    for (field, value) in [
        ("runtimeRoot", PathBuf::new()),
        ("unknown", f.root.join("other")),
        ("engineJar", path.join("input.jar")),
        ("engineJar", control("node-a").unwrap().join("input.jar")),
    ] {
        assert!(preparation.set_source(field, &value).is_err());
        assert_eq!(recorded(&f.root), before);
    }
    preparation.set_field("httpPort", "18080").unwrap();
    preparation.set_source("runtimeRoot", &source).unwrap();
    assert_eq!(recorded(&f.root), before);
    assert!(preparation.record.review.is_some());
    preparation.set_field("httpPort", "18081").unwrap();
    assert!(preparation.record.review.is_none());
    drop(preparation);
    assert_eq!(
        Preparation::open(&path)
            .unwrap()
            .field_value("httpPort")
            .as_deref(),
        Some("18081")
    );
}

#[test]
fn missing_product_or_sources_do_not_publish_a_plan_or_advance_revision() {
    let f = Fixture::new();
    let path = f.root.join("preparation");
    let mut preparation = Preparation::create(&path, "node-a", None).unwrap();
    let before = recorded(&f.root);
    assert!(preparation.prepare().is_err());
    assert_eq!(recorded(&f.root), before);
    complete_product(&mut preparation, &f.root);
    let before = recorded(&f.root);
    assert_eq!(
        preparation.prepare().err().unwrap().code,
        "LINUX_PREPARATION_INCOMPLETE"
    );
    assert_eq!(recorded(&f.root), before);
    assert_eq!(preparation.summary().unwrap().revision, 0);
    assert!(
        fs::read_dir(path.join("generated"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(!control("node-a").unwrap().exists());
}

#[test]
fn unresolved_apply_freezes_inputs_even_when_request_is_unavailable() {
    let f = Fixture::new();
    let path = f.root.join("preparation");
    let mut preparation = Preparation::create(&path, "node-a", None).unwrap();
    synthetic_review(&mut preparation);
    preparation.record.apply_intent = Some("a".repeat(64));
    preparation.save().unwrap();
    drop(preparation);
    let before = recorded(&f.root);
    let mut preparation = Preparation::open(&path).unwrap();
    assert_eq!(preparation.summary().unwrap().installation, "UNRESOLVED");
    assert_eq!(
        preparation.summary().unwrap().engine_validation,
        "NOT_OBSERVED"
    );
    assert_eq!(
        preparation.set_field("httpPort", "18080").unwrap_err().code,
        "LINUX_PREPARATION_APPLY_UNRESOLVED"
    );
    assert_eq!(
        preparation
            .set_source("runtimeRoot", &f.root.join("other"))
            .unwrap_err()
            .code,
        "LINUX_PREPARATION_APPLY_UNRESOLVED"
    );
    assert!(preparation.begin_apply().is_err());
    assert_eq!(recorded(&f.root), before);
}

#[test]
fn changed_fixed_identity_is_rejected_without_repair() {
    let f = Fixture::new();
    for (index, field) in ["instanceId", "dataDirectory"].into_iter().enumerate() {
        let path = f.root.join(format!("preparation-{index}"));
        let mut preparation = Preparation::create(&path, "node-a", None).unwrap();
        let other = f.root.join("other-data");
        let value = if field == "instanceId" {
            "other"
        } else {
            other.to_str().unwrap()
        };
        preparation.draft.set(field, value).unwrap();
        assert_eq!(
            preparation.recheck().unwrap_err().code,
            "LINUX_PREPARATION_INVALID"
        );
        drop(preparation);
        let before = recorded(&f.root);
        assert!(Preparation::open(&path).is_err());
        assert!(Preparation::inspect(&path).is_err());
        assert_eq!(recorded(&f.root), before);
    }
}

#[test]
fn symlinked_sessions_cannot_redirect_preparation_creation() {
    let f = Fixture::new();
    let home = f.root.join("home");
    Catalog::create(&home).unwrap();
    let outside = f.root.join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, home.join("sessions")).unwrap();
    let before = recorded(&f.root);
    assert!(
        linux_setup::run(
            options(&home, "local", false),
            &mut Cursor::new("y\n"),
            &mut Vec::new()
        )
        .is_err()
    );
    assert!(fs::read_dir(outside).unwrap().next().is_none());
    assert_eq!(recorded(&f.root), before);
}
