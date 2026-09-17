use super::*;
use crate::artifact::{Engine, Platform, Product, Runtime};
use ed25519_dalek::{
    SigningKey,
    pkcs8::{EncodePrivateKey, EncodePublicKey},
};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::time::{Duration, UNIX_EPOCH};
use tempfile::TempDir;

struct Fixture {
    directory: TempDir,
    options: BuildOptions,
    public_key: PathBuf,
    manifest: Manifest,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("stage");
        let payload = [
            ("bin/bxdl", "fixture CLI bytes, not an executable"),
            (
                "engine/nigo-node.jar",
                "fixture engine bytes, not an official artifact",
            ),
            (
                "runtime/bin/java",
                "fixture runtime bytes, not an executable",
            ),
            ("runtime/lib/security/cacerts", "fixture trust store"),
            (
                "licenses/THIRD_PARTY_NOTICES",
                "Development fixture notices",
            ),
            ("licenses/SBOM.json", "{\"fixture\":true}\n"),
        ];
        for (path, bytes) in payload {
            let target = root.join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(&target, bytes).unwrap();
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode(if matches!(path, "bin/bxdl" | "runtime/bin/java") {
                    0o755
                } else {
                    0o644
                }),
            )
            .unwrap();
        }
        let key = SigningKey::from_bytes(&[0x42; 32]); // Fixed test-only seed.
        let private_key = directory.path().join("signing.pem");
        let public_key = directory.path().join("public.pem");
        let private = key.to_pkcs8_der().unwrap();
        let public = key.verifying_key().to_public_key_der().unwrap();
        fs::write(&private_key, pem("PRIVATE KEY", private.as_bytes())).unwrap();
        fs::set_permissions(&private_key, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&public_key, pem("PUBLIC KEY", public.as_bytes())).unwrap();
        let manifest = Manifest {
            schema_version: 1,
            kind: "bxdl-package".into(),
            channel: "development".into(),
            product: Product {
                name: "BXDL".into(),
                version: "0.1.0-dev".into(),
                revision: "development".into(),
            },
            platform: Platform {
                os: "linux".into(),
                arch: "amd64".into(),
                libc: "glibc".into(),
                min_glibc: "2.34".into(),
                java_major: 21,
                backend: "rocksdb".into(),
            },
            engine: Engine {
                revision: "a".repeat(40),
                jar_sha256: digest(payload[1].1),
                contract_status: "proposed".into(),
                contract_revision: "test-fixture-v1".into(),
            },
            runtime: Runtime {
                vendor: "fixture".into(),
                version: "21-fixture".into(),
                java_sha256: digest(payload[2].1),
            },
            files: Vec::new(),
        };
        let options = BuildOptions {
            root,
            spec_path: directory.path().join("spec.json"),
            output: directory.path().join("package.tar.gz"),
            signing_key_path: Some(private_key),
            allow_unsigned_development: false,
        };
        let fixture = Self {
            directory,
            options,
            public_key,
            manifest,
        };
        fixture.write_spec();
        fixture
    }
    fn write_spec(&self) {
        fs::write(
            &self.options.spec_path,
            serde_json::to_vec(&self.manifest).unwrap(),
        )
        .unwrap();
    }
    fn verify(&self, path: &Path) -> Report {
        verify(
            path,
            &VerifyOptions {
                public_key_path: Some(self.public_key.clone()),
                allow_unsigned_development: false,
            },
        )
        .unwrap()
    }
    fn open_root(&self) -> Dir {
        Dir::open_ambient_dir(&self.options.root, ambient_authority()).unwrap()
    }
}

fn pem(label: &str, der: &[u8]) -> String {
    format!(
        "-----BEGIN {label}-----\n{}\n-----END {label}-----\n",
        base64::engine::general_purpose::STANDARD.encode(der)
    )
}
fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
fn assert_code<T: std::fmt::Debug>(result: Result<T>, expected: &str) {
    assert_eq!(result.unwrap_err().code, expected);
}
fn assert_no_output(path: &Path) {
    assert_eq!(
        fs::symlink_metadata(path).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
}

#[test]
fn signed_round_trip_and_determinism_for_both_development_platforms() {
    for mac in [false, true] {
        let mut fixture = Fixture::new();
        if mac {
            fixture.manifest.platform.os = "darwin".into();
            fixture.manifest.platform.arch = "arm64".into();
            fixture.manifest.platform.libc = "none".into();
            fixture.manifest.platform.min_glibc = "none".into();
            fixture.write_spec();
        }
        let first = build(&fixture.options).unwrap();
        let independently_verified = fixture.verify(&fixture.options.output);
        assert_eq!(first, independently_verified);
        assert_eq!(first.authenticity, "verified-external-ed25519");
        assert_eq!(first.files_verified, 6);
        assert_eq!(
            first.manifest.engine.jar_sha256,
            fixture.manifest.engine.jar_sha256
        );
        assert_eq!(
            first.manifest.runtime.java_sha256,
            fixture.manifest.runtime.java_sha256
        );
        for entry in &first.manifest.files {
            fs::File::options()
                .write(true)
                .open(fixture.options.root.join(&entry.path))
                .unwrap()
                .set_times(
                    fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(1_000_000)),
                )
                .unwrap();
        }
        let mut second_options = fixture.options.clone();
        second_options.output = fixture.directory.path().join("second.tar.gz");
        let second = build(&second_options).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            fs::read(&fixture.options.output).unwrap(),
            fs::read(&second_options.output).unwrap()
        );
    }
}

#[test]
fn requires_explicit_trust_and_expected_hashes() {
    for case in 0..6 {
        let mut fixture = Fixture::new();
        let expected = match case {
            0 => {
                fixture.options.signing_key_path = None;
                "INVALID_BUILD_OPTIONS"
            }
            1 => {
                fixture.options.allow_unsigned_development = true;
                "INVALID_BUILD_OPTIONS"
            }
            2 => {
                fixture.manifest.engine.jar_sha256.clear();
                "INVALID_SPEC"
            }
            3 => {
                fixture.manifest.engine.jar_sha256 = "0".repeat(64);
                "HASH_MISMATCH"
            }
            4 => {
                fixture.manifest.runtime.java_sha256 = "0".repeat(64);
                "HASH_MISMATCH"
            }
            5 => {
                fixture.manifest.files.push(FileEntry {
                    path: "docs/fixture".into(),
                    size: 0,
                    sha256: "0".repeat(64),
                    mode: 0o644,
                });
                "INVALID_SPEC"
            }
            _ => unreachable!(),
        };
        fixture.write_spec();
        assert_code(build(&fixture.options), expected);
        assert_no_output(&fixture.options.output);
    }
}

#[test]
fn unsigned_development_requires_explicit_opt_in() {
    let mut fixture = Fixture::new();
    fixture.options.signing_key_path = None;
    fixture.options.allow_unsigned_development = true;
    let report = build(&fixture.options).unwrap();
    assert_eq!(report.authenticity, "unsigned-development");
    assert!(
        verify(
            &fixture.options.output,
            &VerifyOptions {
                public_key_path: None,
                allow_unsigned_development: true
            }
        )
        .is_ok()
    );
    assert!(verify(&fixture.options.output, &VerifyOptions::default()).is_err());
}

#[test]
fn existing_output_is_never_replaced() {
    let fixture = Fixture::new();
    let original = b"existing output must survive";
    fs::write(&fixture.options.output, original).unwrap();
    assert_code(build(&fixture.options), "OUTPUT_EXISTS");
    assert_eq!(fs::read(&fixture.options.output).unwrap(), original);
    fs::remove_file(&fixture.options.output).unwrap();
    let target = fixture.directory.path().join("unrelated-file");
    fs::write(&target, original).unwrap();
    symlink(&target, &fixture.options.output).unwrap();
    assert_code(build(&fixture.options), "OUTPUT_EXISTS");
    assert_eq!(fs::read(target).unwrap(), original);
}

#[test]
fn rejects_symlinks_external_path_aliases_signing_hardlinks_and_unsafe_modes() {
    for case in 0..8 {
        let mut fixture = Fixture::new();
        match case {
            0 => symlink(
                fixture.options.signing_key_path.as_ref().unwrap(),
                fixture.options.root.join("runtime/private-link"),
            )
            .unwrap(),
            1 => {
                let alias = fixture.directory.path().join("stage-alias");
                symlink(&fixture.options.root, &alias).unwrap();
                fixture.options.root = alias;
            }
            2 => {
                fixture.options.output = fixture.options.root.join("docs/output.tar.gz");
                fs::create_dir_all(fixture.options.output.parent().unwrap()).unwrap();
            }
            3 | 4 => {
                let inside = fixture.options.root.join("runtime/signing-material");
                fs::copy(fixture.options.signing_key_path.as_ref().unwrap(), &inside).unwrap();
                if case == 3 {
                    fixture.options.signing_key_path = Some(inside);
                } else {
                    let alias = fixture.directory.path().join("key-alias");
                    symlink(&inside, &alias).unwrap();
                    fixture.options.signing_key_path = Some(alias);
                }
            }
            5 => {
                let key = fixture.options.signing_key_path.as_ref().unwrap();
                fs::set_permissions(key, fs::Permissions::from_mode(0o644)).unwrap();
                fs::hard_link(key, fixture.options.root.join("runtime/innocuous-material"))
                    .unwrap();
            }
            6 => fs::create_dir_all(fixture.options.root.join("runtime/secrets")).unwrap(),
            7 => fs::set_permissions(
                fixture.options.root.join("engine/nigo-node.jar"),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap(),
            _ => unreachable!(),
        }
        let result = build(&fixture.options);
        if case == 5 {
            assert_code(result, "INVALID_STAGE");
        } else {
            assert!(result.is_err(), "unsafe case {case} was accepted");
        }
        assert_no_output(&fixture.options.output);
    }
}

#[test]
fn rejects_unknown_duplicate_null_missing_and_extra_spec_json() {
    for suffix in [
        ",\"unknown\":true}",
        ",\"channel\":\"development\"}",
        "} {}",
    ] {
        let fixture = Fixture::new();
        let original = fs::read_to_string(&fixture.options.spec_path).unwrap();
        fs::write(
            &fixture.options.spec_path,
            format!("{}{suffix}", original.strip_suffix('}').unwrap()),
        )
        .unwrap();
        assert_code(build(&fixture.options), "INVALID_SPEC");
        assert_no_output(&fixture.options.output);
    }
    for field in ["product", "runtime", "schemaVersion"] {
        let fixture = Fixture::new();
        let mut value = serde_json::to_value(&fixture.manifest).unwrap();
        value.as_object_mut().unwrap().remove(field);
        fs::write(
            &fixture.options.spec_path,
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        assert_code(build(&fixture.options), "INVALID_SPEC");
        assert_no_output(&fixture.options.output);
    }
    let fixture = Fixture::new();
    let mut value = serde_json::to_value(&fixture.manifest).unwrap();
    value["files"] = serde_json::Value::Null;
    fs::write(
        &fixture.options.spec_path,
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    assert_code(build(&fixture.options), "INVALID_SPEC");
    assert_no_output(&fixture.options.output);
    // Omitted inventory is explicitly supported in a spec, unlike null.
    value.as_object_mut().unwrap().remove("files");
    fs::write(
        &fixture.options.spec_path,
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    assert!(build(&fixture.options).is_ok());
}

#[test]
fn detects_content_mutation_with_preserved_inode_size_mode_and_timestamp() {
    let fixture = Fixture::new();
    let root = fixture.open_root();
    let files = collect_stage(&root).unwrap();
    let path = fixture.options.root.join("engine/nigo-node.jar");
    let before = fs::metadata(&path).unwrap();
    let mut bytes = fs::read(&path).unwrap();
    bytes[0] ^= 1;
    fs::write(&path, bytes).unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(before.modified().unwrap()))
        .unwrap();
    assert_eq!(
        Identity::standard(&before),
        Identity::standard(&fs::metadata(&path).unwrap())
    );
    let mut archive = Vec::new();
    assert_code(
        write_archive(&mut archive, &root, b"fixture manifest", None, &files),
        "INPUT_CHANGED",
    );
    assert_code(check_stage_unchanged(&root, &files), "INPUT_CHANGED");
}

#[test]
fn detects_additions_and_symlink_replacement_after_snapshot() {
    let fixture = Fixture::new();
    let root = fixture.open_root();
    let files = collect_stage(&root).unwrap();
    let late = fixture.options.root.join("licenses/late.txt");
    fs::write(&late, b"added after inventory").unwrap();
    fs::set_permissions(&late, fs::Permissions::from_mode(0o644)).unwrap();
    assert_code(check_stage_unchanged(&root, &files), "INPUT_CHANGED");
    fs::remove_file(late).unwrap();
    let path = fixture.options.root.join("engine/nigo-node.jar");
    fs::remove_file(&path).unwrap();
    symlink(fixture.options.signing_key_path.as_ref().unwrap(), path).unwrap();
    let mut archive = Vec::new();
    assert_code(
        write_archive(&mut archive, &root, b"fixture manifest", None, &files),
        "INPUT_CHANGED",
    );
}

#[test]
fn failure_removes_only_the_new_output_and_preserves_replacements() {
    let fixture = Fixture::new();
    let long_dir = format!("{}/{}/{}", "a".repeat(80), "b".repeat(80), "c".repeat(80));
    let path = fixture
        .options
        .root
        .join("docs")
        .join(long_dir)
        .join("fixture");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"payload").unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
    assert_code(build(&fixture.options), "BUILD_IO"); // USTAR name/prefix cannot represent it.
    assert_no_output(&fixture.options.output);

    let temporary_output = fixture.directory.path().join("owned.tar.gz");
    let guard = NewOutput::create(&temporary_output).unwrap();
    fs::rename(
        &temporary_output,
        fixture.directory.path().join("moved-owned-file"),
    )
    .unwrap();
    fs::write(&temporary_output, b"replacement belonging to someone else").unwrap();
    drop(guard);
    assert_eq!(
        fs::read(temporary_output).unwrap(),
        b"replacement belonging to someone else"
    );
}
