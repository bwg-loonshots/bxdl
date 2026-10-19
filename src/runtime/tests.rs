use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    options: StageOptions,
    lock: RuntimeLock,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("Home");
        fs::create_dir(&source).unwrap();
        let mut files = Vec::new();
        for (path, content, mode) in [
            ("bin/java", "fixture Java launcher; must never run", 0o755),
            ("release", "JAVA_VERSION=21\n", 0o644),
            ("legal/java.base/LICENSE", "fixture license", 0o644),
            ("legal/java.logging/LICENSE", "fixture license", 0o644),
            ("lib/modules", "fixture full module image", 0o644),
        ] {
            let target = source.join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(&target, content).unwrap();
            fs::set_permissions(&target, fs::Permissions::from_mode(mode)).unwrap();
            files.push(artifact::FileEntry {
                path: path.into(),
                size: content.len() as u64,
                sha256: hash(content.as_bytes()),
                mode,
            });
        }
        fs::remove_file(source.join("legal/java.logging/LICENSE")).unwrap();
        symlink(
            "../java.base/LICENSE",
            source.join("legal/java.logging/LICENSE"),
        )
        .unwrap();
        let archive = Archive {
            name: "fixture.tar.gz".into(),
            url: "https://example.invalid/pinned/fixture.tar.gz".into(),
            sha256: "a".repeat(64),
            size_bytes: 42,
        };
        let lock = RuntimeLock {
            schema_version: 1,
            kind: "bxdl-runtime-lock".into(),
            vendor: "Fixture only".into(),
            version: "21-fixture".into(),
            platform: Platform {
                os: "darwin".into(),
                arch: "arm64".into(),
                java_major: 21,
            },
            archive: archive.clone(),
            source_archive: archive,
            java_sha256: files[0].sha256.clone(),
            files,
        };
        let options = StageOptions {
            source,
            lock: temp.path().join("lock.json"),
            output: temp.path().join("staged"),
        };
        let fixture = Self {
            _temp: temp,
            options,
            lock,
        };
        fixture.save();
        fixture
    }
    fn save(&self) {
        fs::write(
            &self.options.lock,
            serde_json::to_vec_pretty(&self.lock).unwrap(),
        )
        .unwrap();
    }
    fn rejected(&self) {
        assert!(stage(&self.options).is_err());
        assert!(!self.options.output.exists());
    }
}
#[test]
fn stages_full_inventory_and_materializes_legal_link_without_execution() {
    let fixture = Fixture::new();
    let report = stage(&fixture.options).unwrap();
    assert_eq!(report.files_verified, 5);
    assert_eq!(report.symlinks_materialized, 1);
    assert_eq!(report.archive_verification, "NOT_PERFORMED");
    assert_eq!(report.runtime_execution, "NOT_PERFORMED");
    assert_eq!(
        fs::read(fixture.options.output.join("docs/runtime-provenance.json")).unwrap(),
        fs::read(&fixture.options.lock).unwrap()
    );
    for file in &fixture.lock.files {
        let path = fixture.options.output.join("runtime").join(&file.path);
        let m = fs::symlink_metadata(&path).unwrap();
        assert!(m.is_file());
        assert_eq!(m.nlink(), 1);
        assert_eq!(m.mode() & 0o7777, file.mode);
        assert_eq!(hash(&fs::read(path).unwrap()), file.sha256);
    }
    assert_eq!(
        fs::metadata(&fixture.options.output).unwrap().mode() & 0o7777,
        0o700
    );
    assert_eq!(
        stage(&fixture.options).unwrap_err().code,
        "RUNTIME_OUTPUT_EXISTS"
    );
}
#[test]
fn rejects_tampered_missing_extra_and_wrong_mode_inputs_before_writing() {
    for case in 0..5 {
        let fixture = Fixture::new();
        let target = fixture.options.source.join("lib/modules");
        match case {
            0 => fs::write(&target, b"fixture tampered modules").unwrap(),
            1 => fs::remove_file(&target).unwrap(),
            2 => fs::write(fixture.options.source.join("lib/extra"), b"extra").unwrap(),
            3 => fs::set_permissions(&target, fs::Permissions::from_mode(0o666)).unwrap(),
            _ => fs::set_permissions(
                fixture.options.source.join("bin/java"),
                fs::Permissions::from_mode(0o4755),
            )
            .unwrap(),
        }
        fixture.rejected();
    }
}
#[test]
fn rejects_escaping_missing_cyclic_and_directory_symlinks() {
    for target in [
        "/etc/hosts",
        "../../../outside",
        "../java.base/missing",
        "LICENSE",
        "../java.base",
    ] {
        let fixture = Fixture::new();
        let path = fixture.options.source.join("legal/java.logging/LICENSE");
        fs::remove_file(&path).unwrap();
        symlink(target, path).unwrap();
        fixture.rejected();
    }
    let fixture = Fixture::new();
    let dir = fixture.options.source.join("legal/java.logging");
    fs::remove_file(dir.join("LICENSE")).unwrap();
    fs::remove_dir(&dir).unwrap();
    symlink("java.base", dir).unwrap();
    fixture.rejected();
}
#[test]
fn rejects_duplicate_traversal_and_mismatched_lock_identity() {
    for case in 0..6 {
        let mut fixture = Fixture::new();
        match case {
            0 => fixture.lock.files.push(fixture.lock.files[0].clone()),
            1 => fixture.lock.files[0].path = "../outside".into(),
            2 => fixture.lock.java_sha256 = "b".repeat(64),
            3 => fixture.lock.platform.arch = "amd64".into(),
            4 => fixture.lock.files[2].sha256 = "c".repeat(64),
            _ => fixture.lock.files.push(artifact::FileEntry {
                path: "lib".into(),
                size: 1,
                sha256: "a".repeat(64),
                mode: 0o644,
            }),
        }
        fixture.save();
        fixture.rejected();
    }
}
#[test]
fn rejects_source_and_output_aliases_without_replacing_existing_files() {
    let mut fixture = Fixture::new();
    fixture.options.output = fixture.options.source.join("new-stage");
    fixture.rejected();
    let fixture = Fixture::new();
    symlink(&fixture.options.source, &fixture.options.output).unwrap();
    assert_eq!(
        stage(&fixture.options).unwrap_err().code,
        "RUNTIME_OUTPUT_EXISTS"
    );
    assert!(fixture.options.output.is_symlink());
    let mut fixture = Fixture::new();
    let alias = fixture._temp.path().join("source-alias");
    symlink(&fixture.options.source, &alias).unwrap();
    fixture.options.source = alias;
    fixture.rejected();
}
#[test]
fn preserves_readonly_upstream_files_as_normalized_regular_copies() {
    let fixture = Fixture::new();
    fs::set_permissions(
        fixture.options.source.join("legal/java.base/LICENSE"),
        fs::Permissions::from_mode(0o444),
    )
    .unwrap();
    fs::set_permissions(
        fixture.options.source.join("bin/java"),
        fs::Permissions::from_mode(0o555),
    )
    .unwrap();
    stage(&fixture.options).unwrap();
    assert_eq!(
        fs::metadata(fixture.options.output.join("runtime/bin/java"))
            .unwrap()
            .mode()
            & 0o7777,
        0o755
    );
}
#[test]
fn strict_lock_rejects_duplicate_unknown_and_null_fields() {
    for raw in [
        "{\"schemaVersion\":1,\"schemaVersion\":1}",
        "null",
        "{\"files\":null}",
    ] {
        let fixture = Fixture::new();
        fs::write(&fixture.options.lock, raw).unwrap();
        fixture.rejected();
    }
    let fixture = Fixture::new();
    let mut raw: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.options.lock).unwrap()).unwrap();
    raw["unknown"] = true.into();
    fs::write(&fixture.options.lock, raw.to_string()).unwrap();
    fixture.rejected();
}
#[test]
fn source_change_after_snapshot_is_rejected() {
    let fixture = Fixture::new();
    let dirs = validate(&fixture.lock).unwrap();
    let (_, root) = open_root(&fixture.options.source).unwrap();
    let snapshot = scan(&root, &dirs).unwrap();
    fs::write(
        fixture.options.source.join("lib/modules"),
        b"fixture full module image modified",
    )
    .unwrap();
    let file = fixture
        .lock
        .files
        .iter()
        .find(|f| f.path == "lib/modules")
        .unwrap();
    assert_eq!(
        copy_file(&root, &snapshot, file, &mut std::io::sink())
            .unwrap_err()
            .code,
        "RUNTIME_INPUT_CHANGED"
    );
}
#[test]
fn cli_accepts_explicit_stage_and_does_not_echo_invalid_paths() {
    let fixture = Fixture::new();
    let args = vec![
        "runtime".into(),
        "stage".into(),
        "--source".into(),
        fixture.options.source.to_str().unwrap().into(),
        "--lock".into(),
        fixture.options.lock.to_str().unwrap().into(),
        "--output".into(),
        fixture.options.output.to_str().unwrap().into(),
        "--json".into(),
    ];
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(crate::cli::run(&args, &mut out, &mut err), 0);
    assert!(err.is_empty());
    let result: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(result["reasonCode"], "RUNTIME_STAGED");
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let args = ["runtime", "stage", "--source", "PRIVATE-CANARY", "--json"].map(str::to_owned);
    assert_eq!(crate::cli::run(&args, &mut out, &mut err), 2);
    assert!(!String::from_utf8(out).unwrap().contains("PRIVATE-CANARY"));
}

#[test]
fn rejects_special_files_and_lock_symlinks() {
    let fixture = Fixture::new();
    let path = fixture.options.source.join("lib/modules");
    fs::remove_file(&path).unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(path).unwrap();
    fixture.rejected();
    let fixture = Fixture::new();
    let original = fixture._temp.path().join("original-lock.json");
    fs::rename(&fixture.options.lock, &original).unwrap();
    symlink(original, &fixture.options.lock).unwrap();
    fixture.rejected();
}
