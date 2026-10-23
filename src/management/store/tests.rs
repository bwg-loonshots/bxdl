use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{
    SigningKey,
    pkcs8::{EncodePrivateKey, EncodePublicKey},
};
use std::{
    collections::BTreeMap,
    fs as stdfs,
    os::unix::fs::{MetadataExt as _, PermissionsExt, symlink},
};

struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
    stage: PathBuf,
    locations: Locations,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let stage = base.join("CLI source with spaces");
        for dir in ["bin", "docs", "share/zsh/site-functions"] {
            stdfs::create_dir_all(stage.join(dir)).unwrap();
        }
        Self {
            _temp: temp,
            locations: Locations {
                root: base.join("management CLI"),
                bin_dir: base.join("entry directory"),
            },
            base,
            stage,
        }
    }
    fn bundle(&self, name: &str, version: &str, revision: char, key: Option<&Path>) -> PathBuf {
        let mut binary = vec![0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1];
        binary.resize(128, revision as u8);
        for (name, bytes, mode) in [
            ("bin/bxdl", binary, 0o755),
            (
                "docs/management-cli.md",
                format!("management {version}\n").into_bytes(),
                0o644,
            ),
            ("docs/completion.md", b"completion guide\n".to_vec(), 0o644),
            (
                "share/zsh/site-functions/_bxdl",
                format!("#compdef bxdl\n# {version}\n").into_bytes(),
                0o644,
            ),
        ] {
            stdfs::write(self.stage.join(name), bytes).unwrap();
            stdfs::set_permissions(self.stage.join(name), stdfs::Permissions::from_mode(mode))
                .unwrap();
        }
        let output = self.base.join(name);
        bundle::build(&bundle::BuildOptions {
            root: self.stage.clone(),
            output: output.clone(),
            version: version.into(),
            revision: revision.to_string().repeat(40),
            signing_key_path: key.map(Path::to_path_buf),
            allow_unsigned_development: key.is_none(),
        })
        .unwrap();
        output
    }
    fn options(&self, from: &Path) -> InstallOptions {
        InstallOptions {
            locations: self.locations.clone(),
            from: Some(from.into()),
            trust: unsigned(),
        }
    }
    fn keys(&self, seed: u8) -> (PathBuf, PathBuf) {
        let key = SigningKey::from_bytes(&[seed; 32]);
        let private = self.base.join(format!("private-{seed}.pem"));
        let public = self.base.join(format!("public-{seed}.pem"));
        let pem = |label: &str, bytes: &[u8]| {
            format!(
                "-----BEGIN {label}-----\n{}\n-----END {label}-----\n",
                STANDARD.encode(bytes)
            )
        };
        stdfs::write(
            &private,
            pem("PRIVATE KEY", key.to_pkcs8_der().unwrap().as_bytes()),
        )
        .unwrap();
        stdfs::write(
            &public,
            pem(
                "PUBLIC KEY",
                key.verifying_key().to_public_key_der().unwrap().as_bytes(),
            ),
        )
        .unwrap();
        (private, public)
    }
    fn current_link(&self) -> PathBuf {
        stdfs::read_link(self.locations.root.join("current")).unwrap()
    }
}
fn unsigned() -> VerifyOptions {
    VerifyOptions {
        public_key_path: None,
        allow_unsigned_development: true,
    }
}
fn permit(path: &Path, manifest: &bundle::Manifest) -> Result<()> {
    assert!(path.is_file());
    assert_eq!(path.file_name().unwrap(), "bxdl");
    assert!(!manifest.version.is_empty());
    Ok(())
}
fn reject(_: &Path, _: &bundle::Manifest) -> Result<()> {
    Err(error(
        "CLI_TEST_PROBE_FAILED",
        "candidate version probe failed",
    ))
}

#[derive(Debug, PartialEq)]
struct FileState {
    device: u64,
    inode: u64,
    mode: u32,
    modified: (i64, i64),
    changed: (i64, i64),
    bytes: Vec<u8>,
    link: Option<PathBuf>,
}
type Snapshot = BTreeMap<PathBuf, FileState>;
fn snapshot(path: &Path) -> Snapshot {
    fn visit(base: &Path, path: &Path, entries: &mut Snapshot) {
        let m = stdfs::symlink_metadata(path).unwrap();
        entries.insert(
            path.strip_prefix(base).unwrap().into(),
            FileState {
                device: m.dev(),
                inode: m.ino(),
                mode: m.mode(),
                modified: (m.mtime(), m.mtime_nsec()),
                changed: (m.ctime(), m.ctime_nsec()),
                bytes: if m.is_file() {
                    stdfs::read(path).unwrap()
                } else {
                    Vec::new()
                },
                link: m.is_symlink().then(|| stdfs::read_link(path).unwrap()),
            },
        );
        if m.is_dir() {
            for item in stdfs::read_dir(path).unwrap() {
                visit(base, &item.unwrap().path(), entries);
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(path, path, &mut result);
    result
}

#[test]
fn installation_noop_update_and_rollback_keep_stable_entry_and_completion_selection() {
    let f = Fixture::new();
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', None);
    let b = f.bundle("second.tar.gz", "0.1.0-dev.2", 'b', None);
    let first = install(&f.options(&a), false, permit).unwrap();
    assert_eq!(first.status, "INSTALLED");
    assert!(first.changed);
    assert_eq!(first.current.as_ref().unwrap().version, "0.1.0-dev.1");
    assert!(first.previous.is_none());
    let entry = f.locations.bin_dir.join("bxdl");
    assert_eq!(
        stdfs::read_link(&entry).unwrap(),
        f.locations.root.join("current/bin/bxdl")
    );
    let entry_inode = stdfs::symlink_metadata(&entry).unwrap().ino();
    let first_activation = f.current_link();
    let before = snapshot(&f.base);
    for update in [false, true] {
        // A same-bundle no-op neither rewrites files nor executes the candidate.
        let same = install(&f.options(&a), update, reject).unwrap();
        assert_eq!(same.status, "UNCHANGED");
        assert!(!same.changed);
        assert_eq!(snapshot(&f.base), before);
    }
    let next = install(&f.options(&b), true, permit).unwrap();
    assert_eq!(next.status, "UPDATED");
    assert_eq!(next.current.as_ref().unwrap().version, "0.1.0-dev.2");
    assert_eq!(next.previous.as_ref().unwrap().version, "0.1.0-dev.1");
    assert_ne!(f.current_link(), first_activation);
    assert_eq!(stdfs::symlink_metadata(&entry).unwrap().ino(), entry_inode);
    assert!(
        stdfs::read_to_string(
            f.locations
                .root
                .join("current/share/zsh/site-functions/_bxdl")
        )
        .unwrap()
        .contains("0.1.0-dev.2")
    );
    let rolled = rollback(&f.locations, permit).unwrap();
    assert_eq!(rolled.status, "ROLLED_BACK");
    assert_eq!(rolled.current.as_ref().unwrap().version, "0.1.0-dev.1");
    assert_eq!(rolled.previous.as_ref().unwrap().version, "0.1.0-dev.2");
    assert_eq!(stdfs::symlink_metadata(&entry).unwrap().ino(), entry_inode);
    assert!(
        stdfs::read_to_string(
            f.locations
                .root
                .join("current/share/zsh/site-functions/_bxdl")
        )
        .unwrap()
        .contains("0.1.0-dev.1")
    );
    assert_eq!(
        stdfs::read_dir(f.locations.root.join("releases"))
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn info_and_recorded_location_resolution_are_read_only_even_while_locked() {
    let f = Fixture::new();
    let before = snapshot(&f.base);
    assert_eq!(info(&f.locations).unwrap().status, "NOT_INSTALLED");
    assert_eq!(snapshot(&f.base), before);
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', None);
    install(&f.options(&a), false, permit).unwrap();
    let root = Root::open(&f.locations).unwrap().unwrap();
    let _lock = root.lock().unwrap();
    let before = snapshot(&f.base);
    let locations = super::super::default_locations(Some(&f.locations.root), None).unwrap();
    assert_eq!(locations.bin_dir, f.locations.bin_dir);
    for _ in 0..2 {
        let report = info(&locations).unwrap();
        assert_eq!(report.status, "READY");
        assert!(!report.changed);
        assert_eq!(report.current.unwrap().version, "0.1.0-dev.1");
    }
    assert_eq!(snapshot(&f.base), before);
}

#[test]
fn signed_installation_pins_external_key_and_updates_reuse_it_without_fallback() {
    let f = Fixture::new();
    let (private, public) = f.keys(42);
    let (wrong_private, wrong_public) = f.keys(43);
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', Some(&private));
    let b = f.bundle("second.tar.gz", "0.1.0-dev.2", 'b', Some(&private));
    let wrong = f.bundle("wrong.tar.gz", "0.1.0-dev.3", 'c', Some(&wrong_private));
    let plain = f.bundle("unsigned.tar.gz", "0.1.0-dev.4", 'd', None);
    let mut options = f.options(&a);
    options.trust = VerifyOptions {
        public_key_path: Some(public),
        allow_unsigned_development: false,
    };
    install(&options, false, permit).unwrap();
    let before = snapshot(&f.base);
    for (archive, trust) in [
        (&b, unsigned()),
        (
            &b,
            VerifyOptions {
                public_key_path: Some(wrong_public),
                allow_unsigned_development: false,
            },
        ),
        (&wrong, VerifyOptions::default()),
        (&plain, VerifyOptions::default()),
    ] {
        options.from = Some(archive.into());
        options.trust = trust;
        assert!(install(&options, true, permit).is_err());
        assert_eq!(snapshot(&f.base), before);
    }
    options.from = Some(b);
    options.trust = VerifyOptions::default();
    let updated = install(&options, true, permit).unwrap();
    assert_eq!(updated.status, "UPDATED");
    assert_eq!(
        updated.current.unwrap().authenticity,
        "verified-external-ed25519"
    );
}

#[test]
fn unsigned_trust_requires_explicit_opt_in_on_every_mutation() {
    let f = Fixture::new();
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', None);
    let b = f.bundle("second.tar.gz", "0.1.0-dev.2", 'b', None);
    let mut options = f.options(&a);
    options.trust = VerifyOptions::default();
    let before = snapshot(&f.base);
    assert_eq!(
        install(&options, false, permit).unwrap_err().code,
        "CLI_TRUST_POLICY"
    );
    assert_eq!(snapshot(&f.base), before);
    install(&f.options(&a), false, permit).unwrap();
    let before = snapshot(&f.base);
    for archive in [&a, &b] {
        options.from = Some(archive.into());
        assert_eq!(
            install(&options, true, permit).unwrap_err().code,
            "CLI_TRUST_POLICY"
        );
        assert_eq!(snapshot(&f.base), before);
    }
}

#[test]
fn entry_conflicts_and_unsafe_root_permissions_preserve_existing_files() {
    let f = Fixture::new();
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', None);
    stdfs::create_dir(&f.locations.bin_dir).unwrap();
    let entry = f.locations.bin_dir.join("bxdl");
    stdfs::write(&entry, b"existing user executable").unwrap();
    let before = snapshot(&f.base);
    assert_eq!(
        install(&f.options(&a), false, permit).unwrap_err().code,
        "CLI_ENTRY_CONFLICT"
    );
    assert_eq!(snapshot(&f.base), before);
    stdfs::remove_file(&entry).unwrap();
    install(&f.options(&a), false, permit).unwrap();
    stdfs::set_permissions(&f.locations.root, stdfs::Permissions::from_mode(0o755)).unwrap();
    let before = snapshot(&f.base);
    assert_eq!(info(&f.locations).unwrap_err().code, "CLI_STORAGE_UNSAFE");
    assert_eq!(
        install(&f.options(&a), true, permit).unwrap_err().code,
        "CLI_STORAGE_UNSAFE"
    );
    assert_eq!(snapshot(&f.base), before);
    stdfs::set_permissions(&f.locations.root, stdfs::Permissions::from_mode(0o700)).unwrap();
    stdfs::remove_file(&entry).unwrap();
    symlink(&a, &entry).unwrap();
    let before = snapshot(&f.base);
    assert_eq!(info(&f.locations).unwrap_err().code, "CLI_ENTRY_CONFLICT");
    assert_eq!(
        install(&f.options(&a), true, permit).unwrap_err().code,
        "CLI_ENTRY_CONFLICT"
    );
    assert_eq!(snapshot(&f.base), before);
}

#[test]
fn installation_refuses_known_node_package_and_preparation_descendants() {
    let f = Fixture::new();
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', None);
    for marker in [
        ".bxdl-home.json",
        ".bxdl-install.json",
        ".workflow.json",
        "binding.json",
        "engine-instance.json",
    ] {
        let node = f.base.join(format!("node-{}", marker.replace('.', "")));
        stdfs::create_dir(&node).unwrap();
        stdfs::write(node.join(marker), b"preserve node record").unwrap();
        for use_bin in [false, true] {
            let mut options = f.options(&a);
            if use_bin {
                options.locations.bin_dir = node.join("nested/bin");
            } else {
                options.locations.root = node.join("nested/management");
            }
            let before = snapshot(&f.base);
            assert_eq!(
                install(&options, false, permit).unwrap_err().code,
                "CLI_LOCATION_CONFLICT"
            );
            assert_eq!(snapshot(&f.base), before);
        }
    }
}

#[test]
fn probe_failure_keeps_previous_selection_and_existing_releases_untouched() {
    let f = Fixture::new();
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', None);
    let b = f.bundle("second.tar.gz", "0.1.0-dev.2", 'b', None);
    install(&f.options(&a), false, permit).unwrap();
    let current = f.current_link();
    let releases = snapshot(&f.locations.root.join("releases"));
    let activations = snapshot(&f.locations.root.join("activations"));
    assert_eq!(
        install(&f.options(&b), true, reject).unwrap_err().code,
        "CLI_TEST_PROBE_FAILED"
    );
    assert_eq!(f.current_link(), current);
    assert_eq!(snapshot(&f.locations.root.join("releases")), releases);
    assert_eq!(snapshot(&f.locations.root.join("activations")), activations);
    assert_eq!(
        stdfs::read_dir(f.locations.root.join("staging"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        info(&f.locations).unwrap().current.unwrap().version,
        "0.1.0-dev.1"
    );
    install(&f.options(&b), true, permit).unwrap();
    let before = snapshot(&f.base);
    assert_eq!(
        rollback(&f.locations, reject).unwrap_err().code,
        "CLI_TEST_PROBE_FAILED"
    );
    assert_eq!(snapshot(&f.base), before);
}

#[test]
fn failed_first_probe_can_be_retried_without_publishing_an_entrypoint() {
    let f = Fixture::new();
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', None);
    assert_eq!(
        install(&f.options(&a), false, reject).unwrap_err().code,
        "CLI_TEST_PROBE_FAILED"
    );
    assert!(!f.locations.bin_dir.join("bxdl").exists());
    assert!(!f.locations.root.join("current").exists());
    assert_eq!(info(&f.locations).unwrap().status, "NOT_INSTALLED");
    assert_eq!(
        install(&f.options(&a), false, permit).unwrap().status,
        "INSTALLED"
    );
}

#[test]
fn lock_busy_rejects_mutations_without_changing_current_or_metadata() {
    let f = Fixture::new();
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', None);
    let b = f.bundle("second.tar.gz", "0.1.0-dev.2", 'b', None);
    install(&f.options(&a), false, permit).unwrap();
    let root = Root::open(&f.locations).unwrap().unwrap();
    let lock = root.lock().unwrap();
    let before = snapshot(&f.base);
    assert_eq!(
        install(&f.options(&b), true, permit).unwrap_err().code,
        "CLI_BUSY"
    );
    assert_eq!(rollback(&f.locations, permit).unwrap_err().code, "CLI_BUSY");
    assert_eq!(snapshot(&f.base), before);
    drop(lock);
    assert_eq!(
        install(&f.options(&b), true, permit).unwrap().status,
        "UPDATED"
    );
}

#[test]
fn conflicting_release_identity_and_tampered_previous_release_never_activate() {
    let f = Fixture::new();
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', None);
    let b = f.bundle("second.tar.gz", "0.1.0-dev.2", 'b', None);
    let installed = install(&f.options(&a), false, permit).unwrap();
    // Same manifest identity, different completion content.
    stdfs::write(
        f.stage.join("share/zsh/site-functions/_bxdl"),
        b"#compdef bxdl\nchanged\n",
    )
    .unwrap();
    let conflict = f.base.join("conflict.tar.gz");
    bundle::build(&bundle::BuildOptions {
        root: f.stage.clone(),
        output: conflict.clone(),
        version: "0.1.0-dev.1".into(),
        revision: "a".repeat(40),
        signing_key_path: None,
        allow_unsigned_development: true,
    })
    .unwrap();
    let before = snapshot(&f.base);
    assert_eq!(
        install(&f.options(&conflict), true, permit)
            .unwrap_err()
            .code,
        "CLI_RELEASE_CONFLICT"
    );
    assert_eq!(snapshot(&f.base), before);
    install(&f.options(&b), true, permit).unwrap();
    let previous = f
        .locations
        .root
        .join("releases")
        .join(installed.current.unwrap().release_id)
        .join("payload/bin/bxdl");
    let mut bytes = stdfs::read(&previous).unwrap();
    bytes[100] ^= 1;
    stdfs::write(&previous, bytes).unwrap();
    let before = snapshot(&f.base);
    assert_eq!(
        rollback(&f.locations, permit).unwrap_err().code,
        "CLI_RELEASE_CHANGED"
    );
    assert_eq!(snapshot(&f.base), before);
    assert_eq!(
        info(&f.locations).unwrap().current.unwrap().version,
        "0.1.0-dev.2"
    );
}

#[test]
fn published_release_and_partial_work_directories_can_be_retried_after_interruption() {
    let f = Fixture::new();
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', None);
    let b = f.bundle("second.tar.gz", "0.1.0-dev.2", 'b', None);
    install(&f.options(&a), false, permit).unwrap();
    let selected = f.current_link();
    let candidate = bundle::verify(&b, &unsigned()).unwrap();
    let root = Root::open(&f.locations).unwrap().unwrap();
    let _admitted = root.stage(&candidate, permit).unwrap();
    assert_eq!(f.current_link(), selected);
    assert_eq!(
        info(&f.locations).unwrap().current.unwrap().version,
        "0.1.0-dev.1"
    );
    // Simulate interruption before a previous operation completed its work files.
    let abandoned_stage = fs::mkdir(&root.staging, "stage-interrupted").unwrap();
    fs::write_new(
        &abandoned_stage,
        "partial",
        b"preserve incomplete work",
        0o600,
    )
    .unwrap();
    let abandoned_activation = fs::mkdir(&root.activations, "0000000000000002").unwrap();
    fs::write_new(&abandoned_activation, "selection.json", b"{", 0o600).unwrap();
    let stage_before = snapshot(&f.locations.root.join("staging/stage-interrupted"));
    let activation_before = snapshot(&f.locations.root.join("activations/0000000000000002"));
    let released_before = snapshot(&f.locations.root.join("releases"));
    drop(root);
    // A published but unselected candidate must still pass its version probe.
    let before = snapshot(&f.base);
    assert_eq!(
        install(&f.options(&b), true, reject).unwrap_err().code,
        "CLI_TEST_PROBE_FAILED"
    );
    assert_eq!(f.current_link(), selected);
    assert_eq!(snapshot(&f.base), before);
    let retried = install(&f.options(&b), true, permit).unwrap();
    assert_eq!(retried.status, "UPDATED");
    assert_eq!(retried.current.unwrap().version, "0.1.0-dev.2");
    assert_eq!(retried.previous.unwrap().version, "0.1.0-dev.1");
    assert_eq!(
        snapshot(&f.locations.root.join("releases")),
        released_before
    );
    assert_eq!(
        snapshot(&f.locations.root.join("staging/stage-interrupted")),
        stage_before
    );
    assert_eq!(
        snapshot(&f.locations.root.join("activations/0000000000000002")),
        activation_before
    );
}

#[test]
fn an_unrelated_private_empty_directory_is_never_adopted_as_cli_root() {
    let f = Fixture::new();
    let a = f.bundle("first.tar.gz", "0.1.0-dev.1", 'a', None);
    stdfs::create_dir(&f.locations.root).unwrap();
    stdfs::set_permissions(&f.locations.root, stdfs::Permissions::from_mode(0o700)).unwrap();
    let before = snapshot(&f.base);
    assert_eq!(
        install(&f.options(&a), false, permit).unwrap_err().code,
        "CLI_STORAGE_UNSAFE"
    );
    assert_eq!(snapshot(&f.base), before);
}
