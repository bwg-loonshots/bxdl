use super::*;
use serde_json::json;
use std::os::unix::fs::{PermissionsExt, symlink};

const CANARY: &str = "PRIVATE_SETUP_PLAN_CONTENT_CANARY";
const Q: &str = "nigo.protocol.consensus.qbft.node.";

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    product: Value,
    product_path: PathBuf,
    native: PathBuf,
    lock: PathBuf,
    package: artifact::Report,
}

fn write(path: &Path, raw: &[u8]) {
    fs::write(path, raw).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn write_json(path: &Path, value: &Value) {
    write(path, &serde_json::to_vec(value).unwrap());
}
fn edit(path: &Path, change: impl FnOnce(&mut Value)) {
    let mut value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    change(&mut value);
    write_json(path, &value);
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let identity: Identity = serde_json::from_slice(include_bytes!(
            "../../../contracts/nigo/development-clean-2026-09-18/evidence/engine-info.json"
        ))
        .unwrap();
        let jar_hash = files::digest(b"fake JAR bytes never executed");
        let java_hash = files::digest(b"fake Java bytes never executed");
        let mut files = Vec::new();
        for (path, size, sha256, mode) in [
            ("bin/bxdl", 4, files::digest(b"fake"), 0o755),
            ("engine/nigo-node.jar", 28, jar_hash.clone(), 0o644),
            ("runtime/bin/java", 29, java_hash.clone(), 0o755),
            (
                "licenses/THIRD_PARTY_NOTICES",
                4,
                files::digest(b"test"),
                0o644,
            ),
            ("licenses/SBOM.json", 2, files::digest(b"{}"), 0o644),
        ] {
            files.push(artifact::FileEntry {
                path: path.into(),
                size,
                sha256,
                mode,
            });
        }
        // Synthetic verified metadata tests the binding, not archive authenticity.
        let manifest: artifact::Manifest = serde_json::from_value(json!({
            "schemaVersion":1,"kind":"bxdl-package",
            "product":{"name":"BXDL","version":"0.1.0-dev","revision":"development"},
            "channel":"development",
            "platform":{"os":"darwin","arch":"arm64","libc":"none","minGlibc":"none","javaMajor":21,"backend":"rocksdb"},
            "engine":{"revision":identity.source.commit,"jarSha256":jar_hash,"contractStatus":"proposed","contractRevision":identity.contract.fingerprint},
            "runtime":{"vendor":"test-only","version":"21-test","javaSha256":java_hash},
            "files":files
        })).unwrap();
        let package = artifact::Report {
            archive_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            authenticity: "verified-external-ed25519".into(),
            files_verified: manifest.files.len(),
            bytes_verified: manifest.files.iter().map(|entry| entry.size).sum(),
            manifest,
        };
        let lock = root.join("trusted.lock.json");
        write_json(
            &lock,
            &json!({"schemaVersion":1,"jarSha256":jar_hash,"jarSizeBytes":28,
            "javaSha256":java_hash,"expected":identity}),
        );
        write_json(
            &root.join("chain.json"),
            &json!({
                "nigo.protocol.chain-id":"11578", "nigo.protocol.consensus.protocol":"QBFT",
                "nigo.protocol.consensus.profile-id":"TEST_QBFT"
            }),
        );
        let product_path = root.join("future-workspace/instance.json");
        let mut product: Value = serde_json::from_slice(include_bytes!(
            "../../../config/examples/instance.development.json"
        ))
        .unwrap();
        product["nodeId"] = json!(format!("0x{}", "a".repeat(64)));
        product["chainDescription"] = json!("../chain.json");
        product["storage"]["dataDirectory"] = json!("../data");
        let mut node = json!({"server.address":"127.0.0.1","server.port":"18080"});
        for (key, value) in [
            ("node-id", format!("0x{}", "a".repeat(64))),
            ("validator-id", format!("0x{}", "b".repeat(40))),
            ("role", "VALIDATOR".into()),
            ("transport-security-scheme", "MTLS".into()),
            ("listen-host", "192.0.2.10".into()),
            ("listen-port", "19090".into()),
        ] {
            node[format!("{Q}{key}")] = json!(value);
        }
        for (i, (product_key, property)) in [
            ("validatorKeystore", "keystore-path"),
            ("validatorPasswordFile", "keystore-password-file"),
            ("tlsKeyStore", "mtls-key-store-path"),
            ("tlsKeyPasswordFile", "mtls-key-store-password-file"),
            ("tlsTrustStore", "mtls-trust-store-path"),
            ("tlsTrustPasswordFile", "mtls-trust-store-password-file"),
        ]
        .iter()
        .enumerate()
        {
            let filename = format!("{i}.private");
            write(&root.join(&filename), CANARY.as_bytes());
            product["secrets"][product_key] = json!(format!("../{filename}"));
            node[format!("{Q}{property}")] = json!(filename);
        }
        let native = root.join("node.json");
        write_json(
            &native,
            &json!({"chainFile":"chain.json","dataDirectory":"data","backend":"rocksdb","node":node}),
        );
        Self {
            _temp: temp,
            root,
            product,
            product_path,
            native,
            lock,
            package,
        }
    }
    fn prepare(&self) -> Result<Prepared> {
        prepare(
            &serde_json::to_vec(&self.product).unwrap(),
            &self.product_path,
            &self.native,
            &self.lock,
            &self.package,
        )
    }
    fn snapshot(&self) -> Vec<(PathBuf, Vec<u8>, u32)> {
        let mut result = fs::read_dir(&self.root)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let mode = fs::symlink_metadata(&path).unwrap().permissions().mode();
                let raw = fs::read(&path).unwrap();
                (path, raw, mode)
            })
            .collect::<Vec<_>>();
        result.sort();
        result
    }
}

#[test]
fn unsaved_draft_binds_and_pins_all_inputs_without_writes_or_engine_execution() {
    let f = Fixture::new();
    let before = f.snapshot();
    let prepared = f.prepare().unwrap();
    assert_eq!(prepared.data_directory, f.root.join("data"));
    assert_eq!(prepared.backend, "rocksdb");
    assert_eq!(prepared.pins.len(), 9);
    assert!(!prepared.automatic_gc);
    assert_eq!(prepared.issuer_count, Some(0));
    recheck(&prepared.pins).unwrap();
    assert_eq!(before, f.snapshot());
    assert!(!f.product_path.parent().unwrap().exists());
    assert!(!prepared.data_directory.exists());
    let serialized = serde_json::to_string(&prepared).unwrap();
    assert!(!serialized.contains(CANARY));
    assert_eq!(
        serde_json::from_str::<Prepared>(&serialized).unwrap(),
        prepared
    );
}

#[test]
fn manifest_and_lock_must_bind_source_contract_fingerprint_jar_size_and_java() {
    for case in 0..7 {
        let mut f = Fixture::new();
        match case {
            0 => f.package.manifest.engine.revision = "f".repeat(40),
            1 => {
                f.package.manifest.engine.contract_revision =
                    f.package.manifest.engine.revision.clone()
            }
            2 => edit(&f.lock, |value| value["jarSizeBytes"] = json!(27)),
            3 => edit(&f.lock, |value| value["jarSha256"] = json!("f".repeat(64))),
            4 => edit(&f.lock, |value| value["javaSha256"] = json!("f".repeat(64))),
            5 => f.package.manifest.platform.java_major = 17,
            _ => f.package.files_verified += 1,
        }
        let error = f.prepare().unwrap_err();
        assert_eq!(error.code, "ENGINE_SETUP_PACKAGE_MISMATCH", "case {case}");
        assert!(!error.to_string().contains(CANARY));
        assert!(!error.to_string().contains(f.root.to_str().unwrap()));
    }
    let f = Fixture::new();
    edit(&f.lock, |value| {
        value["expected"]["distribution"]["officialRelease"] = json!(true)
    });
    assert_eq!(f.prepare().unwrap_err().code, "ENGINE_LOCK_INVALID");
}

#[test]
fn static_binding_and_strict_json_errors_precede_any_generated_file() {
    let f = Fixture::new();
    edit(&f.native, |value| {
        value["node"]
            .as_object_mut()
            .unwrap()
            .remove(&format!("{Q}role"));
    });
    assert_eq!(f.prepare().unwrap_err().code, "ENGINE_PRODUCT_MISMATCH");
    assert!(!f.product_path.parent().unwrap().exists());
    let f = Fixture::new();
    write(&f.lock, br#"{"schemaVersion":1,"schemaVersion":1}"#);
    assert_eq!(f.prepare().unwrap_err().code, "ENGINE_LOCK_INVALID");
    let error = prepare(
        br#"{"PRIVATE_SETUP_PLAN_CONTENT_CANARY":1}"#,
        &f.product_path,
        &f.native,
        &f.lock,
        &f.package,
    )
    .unwrap_err();
    assert_eq!(error.code, "CONFIG_JSON_INVALID");
    assert!(!error.to_string().contains(CANARY));
}

#[test]
fn credential_changes_and_symlink_substitution_fail_without_exposing_contents() {
    let f = Fixture::new();
    let pins = f.prepare().unwrap().pins;
    let secret = f.root.join("0.private");
    write(&secret, b"CHANGED_PRIVATE_CONTENT_CANARY");
    let error = recheck(&pins).unwrap_err();
    assert_eq!(error.code, "ENGINE_INPUT_CHANGED");
    assert!(!error.to_string().contains("CANARY"));
    fs::remove_file(&secret).unwrap();
    symlink(f.root.join("1.private"), &secret).unwrap();
    assert!(recheck(&pins).is_err());
    assert!(f.prepare().is_err());
}

#[test]
fn pin_recheck_rejects_empty_relative_and_duplicate_records() {
    let f = Fixture::new();
    let pin = pin(&f.lock).unwrap();
    assert!(recheck(&[]).is_err());
    assert!(recheck(&[pin.clone(), pin.clone()]).is_err());
    assert!(
        recheck(&[Pin {
            path: "relative".into(),
            ..pin
        }])
        .is_err()
    );
}

#[test]
fn displayed_gc_and_issuer_metadata_does_not_guess_malformed_values() {
    let f = Fixture::new();
    edit(&f.native, |value| {
        value["node"]["nigo.storage.gc.automatic.enabled"] = json!("true")
    });
    edit(&f.root.join("chain.json"), |value| {
        value[format!("{ISSUER_PREFIX}0]")] = json!(format!("0x{}", "a".repeat(40)));
        value[format!("{ISSUER_PREFIX}1]")] = json!(format!("0x{}", "A".repeat(40)));
    });
    let prepared = f.prepare().unwrap();
    assert!(prepared.automatic_gc);
    assert_eq!(prepared.issuer_count, Some(1));
    edit(&f.root.join("chain.json"), |value| {
        value
            .as_object_mut()
            .unwrap()
            .remove(&format!("{ISSUER_PREFIX}0]"));
    });
    let incomplete = f.prepare().unwrap();
    assert_eq!(incomplete.issuer_count, None);
    let encoded = serde_json::to_vec(&incomplete).unwrap();
    let strict = artifact::decode_strict_json(&encoded).unwrap();
    assert_eq!(
        serde_json::from_value::<Prepared>(strict).unwrap(),
        incomplete
    );
    edit(&f.native, |value| {
        value["node"]["nigo.storage.gc.automatic.enabled"] = json!("unknown")
    });
    assert_eq!(f.prepare().unwrap_err().code, "ENGINE_CONFIG_INVALID");
}

#[test]
fn early_cli_check_requires_exact_current_executable_size_and_bytes() {
    let mut f = Fixture::new();
    assert_eq!(
        check_cli(&f.package).unwrap_err().code,
        "SETUP_CLI_MISMATCH"
    );
    let raw = fs::read(std::env::current_exe().unwrap()).unwrap();
    let entry = f
        .package
        .manifest
        .files
        .iter_mut()
        .find(|entry| entry.path == "bin/bxdl")
        .unwrap();
    entry.size = raw.len() as u64;
    entry.sha256 = files::digest(&raw);
    check_cli(&f.package).unwrap();
    f.package
        .manifest
        .files
        .iter_mut()
        .find(|entry| entry.path == "bin/bxdl")
        .unwrap()
        .size += 1;
    assert_eq!(
        check_cli(&f.package).unwrap_err().code,
        "SETUP_CLI_MISMATCH"
    );
}
