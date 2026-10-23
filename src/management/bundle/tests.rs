use super::*;
use ed25519_dalek::{
    SigningKey,
    pkcs8::{EncodePrivateKey, EncodePublicKey},
};
use std::os::unix::fs::{PermissionsExt, symlink};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    stage: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let stage = root.join("CLI stage with spaces");
        fs::create_dir(&stage).unwrap();
        for dir in DIRECTORIES {
            fs::create_dir_all(stage.join(dir)).unwrap();
        }
        for (path, mode) in PAYLOADS {
            let bytes = if *path == "bin/bxdl" {
                let mut bytes = vec![0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1];
                bytes.resize(128, 0);
                bytes
            } else {
                format!("fixture for {path}\n").into_bytes()
            };
            fs::write(stage.join(path), bytes).unwrap();
            fs::set_permissions(stage.join(path), fs::Permissions::from_mode(*mode)).unwrap();
        }
        Self {
            _temp: temp,
            root,
            stage,
        }
    }
    fn options(&self, name: &str) -> BuildOptions {
        BuildOptions {
            root: self.stage.clone(),
            output: self.root.join(name),
            version: "0.1.0-dev.1".into(),
            revision: "a".repeat(40),
            signing_key_path: None,
            allow_unsigned_development: true,
        }
    }
    fn build(&self, name: &str) -> PathBuf {
        let o = self.options(name);
        build(&o).unwrap();
        o.output
    }
    fn keys(&self, seed: u8) -> (PathBuf, PathBuf) {
        let key = SigningKey::from_bytes(&[seed; 32]);
        let private = self.root.join(format!("private-{seed}.pem"));
        let public = self.root.join(format!("public-{seed}.pem"));
        fs::write(
            &private,
            pem("PRIVATE KEY", key.to_pkcs8_der().unwrap().as_bytes()),
        )
        .unwrap();
        fs::write(
            &public,
            pem(
                "PUBLIC KEY",
                key.verifying_key().to_public_key_der().unwrap().as_bytes(),
            ),
        )
        .unwrap();
        (private, public)
    }
}
fn pem(label: &str, raw: &[u8]) -> String {
    format!(
        "-----BEGIN {label}-----\n{}\n-----END {label}-----\n",
        STANDARD.encode(raw)
    )
}
fn unsigned() -> VerifyOptions {
    VerifyOptions {
        public_key_path: None,
        allow_unsigned_development: true,
    }
}
fn expanded(f: &Fixture, path: &Path) -> PathBuf {
    let bundle = verify(path, &unsigned()).unwrap();
    let target = f.root.join("expanded");
    fs::create_dir(&target).unwrap();
    for dir in DIRECTORIES {
        fs::create_dir_all(target.join(dir)).unwrap();
    }
    fs::write(target.join("manifest.json"), &bundle.manifest_bytes).unwrap();
    fs::set_permissions(
        target.join("manifest.json"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    for e in &bundle.manifest.files {
        fs::write(target.join(&e.path), &bundle.files[&e.path]).unwrap();
        fs::set_permissions(target.join(&e.path), fs::Permissions::from_mode(e.mode)).unwrap();
    }
    target
}
fn raw(path: &Path) -> Vec<u8> {
    let mut output = Vec::new();
    GzDecoder::new(Cursor::new(fs::read(path).unwrap()))
        .read_to_end(&mut output)
        .unwrap();
    output
}
fn compressed(raw: &[u8]) -> Vec<u8> {
    let mut encoder = GzBuilder::new().write(Vec::new(), Compression::fast());
    encoder.write_all(raw).unwrap();
    encoder.finish().unwrap()
}
fn checksum(header: &mut [u8]) {
    header[148..156].fill(b' ');
    let sum: u64 = header.iter().map(|b| u64::from(*b)).sum();
    header[148..154].copy_from_slice(format!("{sum:06o}").as_bytes());
    header[154] = 0;
    header[155] = b' ';
}

#[test]
fn deterministic_archive_and_expanded_directory_have_the_same_identity() {
    let f = Fixture::new();
    let first = f.build("first.tar.gz");
    let second = f.build("second.tar.gz");
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    let archive = verify(&first, &unsigned()).unwrap();
    let directory = expanded(&f, &first);
    let expanded = verify(&directory, &unsigned()).unwrap();
    assert_eq!(archive.manifest_sha256, expanded.manifest_sha256);
    assert_eq!(archive.files, expanded.files);
    assert_eq!(
        archive.release_id(),
        format!("0.1.0-dev.1-{}", "a".repeat(40))
    );
    assert_eq!(archive.report().files_verified, 4);
    assert_eq!(archive.authenticity, "unsigned-development");
    assert_eq!(
        verify_integrity(&directory).unwrap().manifest_sha256,
        archive.manifest_sha256
    );
    assert!(verify_integrity(&first).is_err());
}

#[test]
fn signature_and_explicit_unsigned_policies_never_fall_back() {
    let f = Fixture::new();
    let unsigned_archive = f.build("unsigned.tar.gz");
    let (private, public) = f.keys(42);
    let (_, wrong) = f.keys(43);
    let trusted = VerifyOptions {
        public_key_path: Some(public.clone()),
        allow_unsigned_development: false,
    };
    assert!(verify(&unsigned_archive, &VerifyOptions::default()).is_err());
    assert!(verify(&unsigned_archive, &trusted).is_err());
    let mut o = f.options("signed.tar.gz");
    o.signing_key_path = Some(private);
    o.allow_unsigned_development = false;
    let report = build(&o).unwrap();
    assert_eq!(report.authenticity, "verified-external-ed25519");
    assert_eq!(
        verify(&o.output, &trusted).unwrap().manifest_sha256,
        report.manifest_sha256
    );
    assert!(verify(&o.output, &unsigned()).is_err());
    assert!(
        verify(
            &o.output,
            &VerifyOptions {
                public_key_path: Some(wrong),
                allow_unsigned_development: false
            }
        )
        .is_err()
    );
    assert!(
        verify(
            &o.output,
            &VerifyOptions {
                public_key_path: Some(public),
                allow_unsigned_development: true
            }
        )
        .is_err()
    );
    let key = SigningKey::from_bytes(&[42; 32]).verifying_key();
    assert!(verify_with_key(&o.output, Some(&key), false).is_ok());
}

#[test]
fn stage_and_directory_reject_unknown_links_hardlinks_modes_and_changed_payload() {
    let f = Fixture::new();
    let archive = f.build("valid.tar.gz");
    let directory = expanded(&f, &archive);
    fs::write(directory.join("unknown"), b"private-canary").unwrap();
    assert!(verify(&directory, &unsigned()).is_err());
    fs::remove_file(directory.join("unknown")).unwrap();
    let binary = directory.join("bin/bxdl");
    let backup = f.root.join("binary-backup");
    fs::rename(&binary, &backup).unwrap();
    symlink(&backup, &binary).unwrap();
    assert!(verify(&directory, &unsigned()).is_err());
    fs::remove_file(&binary).unwrap();
    fs::hard_link(&backup, &binary).unwrap();
    assert!(verify(&directory, &unsigned()).is_err());
    fs::remove_file(&binary).unwrap();
    fs::rename(&backup, &binary).unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(verify(&directory, &unsigned()).is_err());
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut bytes = fs::read(&binary).unwrap();
    bytes[33] = 1;
    fs::write(&binary, bytes).unwrap();
    assert_eq!(
        verify(&directory, &unsigned()).unwrap_err().code,
        "CLI_BUNDLE_HASH_MISMATCH"
    );
    fs::write(f.stage.join("docs/extra.md"), b"not allowed").unwrap();
    assert!(build(&f.options("extra.tar.gz")).is_err());
}

#[test]
fn malformed_tar_never_introduces_links_duplicates_or_other_paths() {
    let f = Fixture::new();
    let archive = f.build("valid.tar.gz");
    let original = raw(&archive);
    for kind in [b'1', b'2', b'3', b'5', b'x', b'g', b'L', b'S'] {
        let mut raw = original.clone();
        raw[156] = kind;
        checksum(&mut raw[..512]);
        assert!(archive_entries(&compressed(&raw)).is_err());
    }
    for name in [
        "../secret",
        "/absolute",
        "bin/../bxdl",
        "manifest.json/",
        "MANIFEST.JSON",
        "./manifest.json",
    ] {
        let mut raw = original.clone();
        raw[..100].fill(0);
        raw[..name.len()].copy_from_slice(name.as_bytes());
        checksum(&mut raw[..512]);
        assert!(archive_entries(&compressed(&raw)).is_err());
    }
    let manifest_size = super::tar::read(&original).unwrap()["manifest.json"]
        .1
        .len();
    let boundary = 512 + manifest_size + (512 - manifest_size % 512) % 512;
    let mut duplicate = original[..boundary].to_vec();
    duplicate.extend(&original);
    assert!(archive_entries(&compressed(&duplicate)).is_err());
    for cut in [0, 1, 511, 512, original.len() - 512] {
        assert!(archive_entries(&compressed(&original[..cut])).is_err());
    }
    let mut trailing = original.clone();
    trailing.push(1);
    assert!(archive_entries(&compressed(&trailing)).is_err());
    let mut wrong_checksum = original.clone();
    wrong_checksum[100] ^= 1;
    assert!(archive_entries(&compressed(&wrong_checksum)).is_err());
    let bytes = fs::read(&archive).unwrap();
    let mut joined = bytes.clone();
    joined.extend(&bytes);
    assert!(archive_entries(&joined).is_err());
    let mut bytes = bytes;
    bytes.push(0);
    assert!(archive_entries(&bytes).is_err());
}

#[test]
fn manifests_are_exact_bounded_and_cannot_claim_another_platform_or_binary() {
    let f = Fixture::new();
    let archive = f.build("valid.tar.gz");
    let entries = archive_entries(&fs::read(&archive).unwrap()).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&entries["manifest.json"].1).unwrap();
    for (field, value) in [
        ("kind", serde_json::json!("bxdl-package")),
        ("target", serde_json::json!("linux-amd64")),
        ("version", serde_json::json!("../escape")),
        ("revision", serde_json::json!("latest")),
        ("unknown", serde_json::json!(true)),
    ] {
        let mut changed = manifest.clone();
        changed[field] = value;
        let mut entries = entries.clone();
        entries.insert(
            "manifest.json".into(),
            (0o644, serde_json::to_vec(&changed).unwrap()),
        );
        assert!(from_entries(entries).is_err());
    }
    let mut changed = manifest.clone();
    changed["files"][0] = changed["files"][1].clone();
    let mut duplicated = entries.clone();
    duplicated.insert(
        "manifest.json".into(),
        (0o644, serde_json::to_vec(&changed).unwrap()),
    );
    assert!(from_entries(duplicated).is_err());
    let mut bad = entries.clone();
    bad.insert(
        "manifest.json".into(),
        (0o644, vec![b'x'; MAX_METADATA + 1]),
    );
    assert!(from_entries(bad).is_err());
    let mut non_macho = f.options("not-macho.tar.gz");
    fs::write(f.stage.join("bin/bxdl"), b"#!/bin/sh\nexit 0\n").unwrap();
    assert!(build(&non_macho).is_err());
    non_macho.revision = "development".into();
    non_macho.allow_unsigned_development = false;
    assert!(build(&non_macho).is_err());
}

#[test]
fn output_collision_and_invalid_build_identity_preserve_existing_files() {
    let f = Fixture::new();
    let first = f.build("existing.tar.gz");
    let before = fs::read(&first).unwrap();
    assert!(build(&f.options("existing.tar.gz")).is_err());
    assert_eq!(fs::read(&first).unwrap(), before);
    let outside = f.root.join("outside");
    fs::write(&outside, b"keep").unwrap();
    let link = f.root.join("linked-output.tar.gz");
    symlink(&outside, &link).unwrap();
    assert!(build(&f.options("linked-output.tar.gz")).is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"keep");
    let mut inside = f.options("unused.tar.gz");
    inside.output = f.stage.join("new.tar.gz");
    assert!(build(&inside).is_err());
    assert!(!inside.output.exists());
    let mut dev = f.options("dev.tar.gz");
    dev.revision = "development".into();
    assert!(build(&dev).is_ok());
    for bad in ["", "../bad", "V1", "1..2", "1/"] {
        let mut o = f.options("invalid.tar.gz");
        o.version = bad.into();
        assert!(build(&o).is_err());
        assert!(!o.output.exists());
    }
}
