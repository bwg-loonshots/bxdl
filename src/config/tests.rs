#![cfg(unix)]

use super::*;
use std::collections::BTreeMap;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::time::SystemTime;
use tempfile::TempDir;

const SAMPLE: &str = r#"{
 "schemaVersion":1,"instanceId":"node-one","role":"validator","nodeId":"public-node-one",
 "chainDescription":"chain.json","storage":{"backend":"rocksdb","dataDirectory":"data"},
 "secrets":{"validatorKeystore":"secrets/v.p12","validatorPasswordFile":"secrets/v.pass",
 "tlsKeyStore":"secrets/t.p12","tlsKeyPasswordFile":"secrets/t.pass",
 "tlsTrustStore":"secrets/trust.p12","tlsTrustPasswordFile":"secrets/trust.pass"},
 "http":{"address":"127.0.0.1","port":18080},"p2p":{"address":"192.0.2.10","port":19090}
}"#;

struct Fixture {
    _directory: TempDir,
    root: PathBuf,
    path: PathBuf,
}

fn write(path: &Path, contents: impl AsRef<[u8]>, mode: u32) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn fixture(references: bool) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    // /var and /tmp may be OS aliases on macOS. Fixtures explicitly choose the
    // actual path; the product preflight still rejects reference symlinks.
    let root = directory.path().canonicalize().unwrap();
    let path = root.join("instance.json");
    write(&path, SAMPLE, 0o600);
    if references {
        for name in ["secrets", "data"] {
            let target = root.join(name);
            fs::create_dir(&target).unwrap();
            fs::set_permissions(target, fs::Permissions::from_mode(0o700)).unwrap();
        }
        write(&root.join("chain.json"), "not even canonical JSON", 0o644);
        for name in [
            "v.p12",
            "v.pass",
            "t.p12",
            "t.pass",
            "trust.p12",
            "trust.pass",
        ] {
            write(
                &root.join("secrets").join(name),
                "SECRET_CONTENT_CANARY_948572",
                0o640,
            );
        }
        write(
            &root.join("data/DO_NOT_OPEN_DB_WAL"),
            "database content remains private",
            0o600,
        );
    }
    Fixture {
        _directory: directory,
        root,
        path,
    }
}

fn check_by_name<'a>(report: &'a Report, name: &str) -> &'a Check {
    report
        .checks
        .iter()
        .find(|check| check.name == name)
        .unwrap()
}

#[test]
fn validate_reads_only_product_config() {
    let fixture = fixture(false);
    let result = validate_file(&fixture.path).unwrap();
    assert_eq!(result.outcome, "VALIDATED_PRODUCT_CONFIG");
    assert_eq!(result.instance_id, "node-one");
    assert_eq!(result.config_sha256.len(), 64);
    let (config, _, _) = load(&fixture.path).unwrap();
    assert_eq!(
        Path::new(&config.chain_description),
        fixture.root.join("chain.json")
    );
    assert_eq!(
        Path::new(&config.secrets.validator_keystore),
        fixture.root.join("secrets/v.p12")
    );
    assert_eq!(
        Path::new(&config.storage.data_directory),
        fixture.root.join("data")
    );
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
}

#[test]
fn paths_with_spaces_and_absolute_references() {
    let fixture = fixture(false);
    let directory = fixture.root.join("directory with spaces");
    fs::create_dir(&directory).unwrap();
    let absolute = fixture.root.join("external data");
    let encoded = serde_json::to_string(&absolute.to_str().unwrap()).unwrap();
    let raw = SAMPLE.replace(
        r#""dataDirectory":"data""#,
        &format!(r#""dataDirectory":{encoded}"#),
    );
    let path = directory.join("instance.json");
    write(&path, raw, 0o600);
    let (config, _, _) = load(&path).unwrap();
    assert_eq!(Path::new(&config.storage.data_directory), absolute);
    assert_eq!(
        Path::new(&config.chain_description),
        directory.join("chain.json")
    );
    // Lexical normalization does not require the intermediate directory to exist.
    write(
        &path,
        SAMPLE.replace("chain.json", "not-created/../chain.json"),
        0o600,
    );
    let (config, _, _) = load(&path).unwrap();
    assert_eq!(
        Path::new(&config.chain_description),
        directory.join("chain.json")
    );
    assert!(!directory.join("not-created").exists());
}

#[test]
fn reject_invalid_product_input_without_echoing_values() {
    let changes = [
        (
            "unknown field",
            r#""schemaVersion":1"#,
            r#""SECRET_UNKNOWN_CANARY":"SECRET_VALUE_CANARY","schemaVersion":1"#,
        ),
        (
            "unknown nested",
            r#""backend":"rocksdb""#,
            r#""backend":"rocksdb","SECRET_UNKNOWN_CANARY":true"#,
        ),
        (
            "duplicate",
            r#""schemaVersion":1"#,
            r#""schemaVersion":1,"schemaVersion":1"#,
        ),
        (
            "duplicate escaped",
            r#""role":"validator""#,
            r#""role":"validator","\u0072ole":"validator""#,
        ),
        (
            "nested duplicate",
            r#""port":18080"#,
            r#""port":18080,"port":18080"#,
        ),
        ("case alias", r#""instanceId""#, r#""InstanceId""#),
        ("missing", r#""nodeId":"public-node-one","#, ""),
        (
            "null secret",
            r#""validatorKeystore":"secrets/v.p12""#,
            r#""validatorKeystore":null"#,
        ),
        (
            "null numeric",
            r#""schemaVersion":1"#,
            r#""schemaVersion":null"#,
        ),
        ("missing numeric", r#""schemaVersion":1,"#, ""),
        (
            "instance traversal",
            r#""node-one""#,
            r#""../SECRET_VALUE_CANARY""#,
        ),
        ("port conflict", "19090", "18080"),
        ("port fractional", "18080", "18080.1"),
        ("port range", "18080", "65536"),
        ("port negative", "18080", "-1"),
        ("http wildcard", "127.0.0.1", "0.0.0.0"),
        ("p2p wildcard", "192.0.2.10", "::"),
        ("p2p mapped wildcard", "192.0.2.10", "::ffff:0.0.0.0"),
        ("p2p multicast", "192.0.2.10", "224.0.0.1"),
        ("p2p mapped multicast", "192.0.2.10", "::ffff:224.0.0.1"),
        ("dns p2p", "192.0.2.10", "example.invalid"),
        ("wrong backend", "rocksdb", "memory"),
        (
            "root data",
            r#""dataDirectory":"data""#,
            r#""dataDirectory":"/""#,
        ),
        (
            "ref self",
            r#""chain.json""#,
            r#""SECRET_FILENAME_CANARY.json""#,
        ),
        ("ref conflict", r#""chain.json""#, r#""data""#),
        ("home expansion", r#""chain.json""#, r#""~/chain.json""#),
        ("control path", r#""chain.json""#, r#""chain\n.json""#),
    ];
    let mut cases: Vec<(String, Vec<u8>)> = changes
        .into_iter()
        .map(|(name, old, new)| {
            let raw = SAMPLE.replacen(old, new, 1).into_bytes();
            assert!(
                serde_json::from_slice::<serde_json::Value>(&raw).is_ok(),
                "invalid test JSON: {name}"
            );
            (name.to_owned(), raw)
        })
        .collect();
    cases.extend([
        (
            "extra document".into(),
            format!("{SAMPLE} {{}}").into_bytes(),
        ),
        ("array".into(), b"[]".to_vec()),
        ("null".into(), b"null".to_vec()),
        ("invalid UTF8".into(), [SAMPLE.as_bytes(), &[0xff]].concat()),
    ]);
    for (name, contents) in cases {
        let fixture = fixture(false);
        let path = fixture.root.join("SECRET_FILENAME_CANARY.json");
        write(&path, contents, 0o600);
        let failure = validate_file(&path).expect_err(&name);
        assert!(!failure.message.contains("CANARY"), "{name}");
        assert!(!failure.code.contains("CANARY"), "{name}");
        assert!(
            !failure.message.contains(fixture.root.to_str().unwrap()),
            "{name}"
        );
    }
}

#[test]
fn reject_oversized_and_nonregular_config() {
    let fixture = fixture(false);
    write(&fixture.path, vec![b' '; MAX_CONFIG_BYTES + 1], 0o600);
    assert_eq!(
        validate_file(&fixture.path).unwrap_err().code,
        "CONFIG_TOO_LARGE"
    );
    let alias = fixture.root.join("alias.json");
    symlink(&fixture.path, &alias).unwrap();
    assert_eq!(
        validate_file(&alias).unwrap_err().code,
        "CONFIG_NOT_REGULAR"
    );
    assert_eq!(
        validate_file(&fixture.root).unwrap_err().code,
        "CONFIG_NOT_REGULAR"
    );
}

#[test]
fn v1_json_shape_and_schema_reason_codes_remain_distinct() {
    let cases = [
        (
            SAMPLE.replace(
                r#""schemaVersion":1"#,
                r#""schemaVersion":1,"schemaVersion":1"#,
            ),
            "CONFIG_JSON_INVALID",
        ),
        (
            SAMPLE.replace(
                r#""role":"validator""#,
                r#""role":"validator","\u0072ole":"validator""#,
            ),
            "CONFIG_JSON_INVALID",
        ),
        (
            SAMPLE.replace(r#""instanceId""#, r#""InstanceId""#),
            "CONFIG_JSON_INVALID",
        ),
        (
            SAMPLE.replace(r#""port":18080"#, r#""SECRET_UNKNOWN_CANARY":18080"#),
            "CONFIG_JSON_INVALID",
        ),
        (
            SAMPLE.replace(
                r#""schemaVersion":1"#,
                r#""schemaVersion":"SECRET_TYPE_CANARY""#,
            ),
            "CONFIG_SCHEMA_INVALID",
        ),
        (
            SAMPLE.replace(r#""schemaVersion":1"#, r#""schemaVersion":null"#),
            "CONFIG_SCHEMA_INVALID",
        ),
        (
            SAMPLE.replace(r#""schemaVersion":1,"#, ""),
            "CONFIG_SCHEMA_INVALID",
        ),
        // This name exists in the global v1 key vocabulary but not at root.
        (
            SAMPLE.replace(r#""schemaVersion":1"#, r#""port":1,"schemaVersion":1"#),
            "CONFIG_SCHEMA_INVALID",
        ),
        (SAMPLE.replace("18080", "1e400"), "CONFIG_SCHEMA_INVALID"),
        (
            SAMPLE.replace("18080", "99999999999999999999999999999999"),
            "CONFIG_SCHEMA_INVALID",
        ),
        (SAMPLE.replace("18080", "01"), "CONFIG_JSON_INVALID"),
        (SAMPLE.replace("18080", "1."), "CONFIG_JSON_INVALID"),
        (SAMPLE.replace("18080", "1e+"), "CONFIG_JSON_INVALID"),
        (format!("{SAMPLE} null"), "CONFIG_JSON_INVALID"),
        ("[]".into(), "CONFIG_JSON_INVALID"),
        ("null".into(), "CONFIG_SCHEMA_INVALID"),
        (
            format!("{}null{}", r#"{"storage":"#.repeat(9), "}".repeat(9)),
            "CONFIG_JSON_INVALID",
        ),
    ];
    for (raw, expected_code) in cases {
        let fixture = fixture(false);
        write(&fixture.path, raw, 0o600);
        let failure = validate_file(&fixture.path).unwrap_err();
        assert_eq!(failure.code, expected_code);
        assert!(!failure.message.contains("CANARY"));
    }
}

#[test]
fn p2p_rejects_limited_broadcast_without_guessing_subnet_policy() {
    for address in [
        "255.255.255.255",
        "::ffff:255.255.255.255",
        "::ffff:ffff:ffff",
    ] {
        let fixture = fixture(false);
        write(&fixture.path, SAMPLE.replace("192.0.2.10", address), 0o600);
        for run in [validate_file, preflight] {
            assert_eq!(run(&fixture.path).unwrap_err().code, "P2P_ADDRESS_INVALID");
        }
    }
    for address in ["192.0.2.255", "127.0.0.1", "::1", "::ffff:127.0.0.1"] {
        let fixture = fixture(false);
        write(&fixture.path, SAMPLE.replace("192.0.2.10", address), 0o600);
        assert!(
            validate_file(&fixture.path).is_ok(),
            "existing address policy changed"
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    digest: Option<Vec<u8>>,
    mode: u32,
    size: u64,
    modified: SystemTime,
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Snapshot> {
    fn visit(path: &Path, result: &mut BTreeMap<PathBuf, Snapshot>) {
        let info = fs::symlink_metadata(path).unwrap();
        let digest = if info.is_file() {
            Some(Sha256::digest(fs::read(path).unwrap()).to_vec())
        } else {
            None
        };
        result.insert(
            path.to_path_buf(),
            Snapshot {
                digest,
                mode: info.permissions().mode(),
                size: info.len(),
                modified: info.modified().unwrap(),
            },
        );
        if info.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(&entry.unwrap().path(), result);
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, &mut result);
    result
}

#[test]
fn preflight_is_incomplete_secret_free_and_does_not_mutate() {
    let fixture = fixture(true);
    let before = snapshot(&fixture.root);
    let result = preflight(&fixture.path).unwrap();
    assert_eq!(result.outcome, "INCOMPLETE");
    assert_eq!(
        result
            .checks
            .iter()
            .filter(|check| check.name.ends_with("Metadata") && check.status == "PASS")
            .count(),
        9
    );
    for name in [
        "keyAndCertificateIdentity",
        "databaseIntegrity",
        "runtimeAndNativeSupport",
        "hostAndServiceManager",
        "instancePersistence",
        "networkAvailability",
        "effectiveServicePermissions",
        "nigoCanonicalConfiguration",
    ] {
        assert_eq!(check_by_name(&result, name).status, "NOT_CHECKED");
    }
    let encoded = serde_json::to_string(&result).unwrap();
    for forbidden in [
        "SECRET_CONTENT_CANARY",
        "secrets/v.p12",
        "secrets/v.pass",
        fixture.root.to_str().unwrap(),
        "database content",
    ] {
        assert!(!encoded.contains(forbidden), "private value exported");
    }
    assert_eq!(
        before,
        snapshot(&fixture.root),
        "preflight mutated an input or referenced path"
    );
}

#[test]
fn missing_references_are_explicit_and_not_created() {
    let fixture = fixture(false);
    let result = preflight(&fixture.path).unwrap();
    assert_eq!(result.outcome, "FAIL");
    assert_eq!(
        check_by_name(&result, "chainDescriptionMetadata").reason_code,
        "REFERENCE_MISSING"
    );
    let check = check_by_name(&result, "dataDirectoryMetadata");
    assert_eq!(check.status, "NOT_CHECKED");
    assert_eq!(check.reason_code, "DATA_NOT_INITIALIZED");
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
}

#[test]
fn secret_reference_canary_never_appears_in_reports() {
    let fixture = fixture(false);
    write(
        &fixture.path,
        SAMPLE.replace("secrets/", "PRIVATE_REFERENCE_CANARY/"),
        0o600,
    );
    for run in [validate_file, preflight] {
        let result = run(&fixture.path).unwrap();
        assert!(
            !serde_json::to_string(&result)
                .unwrap()
                .contains("PRIVATE_REFERENCE_CANARY")
        );
    }
}

#[test]
fn preflight_rejects_unsafe_permissions() {
    for (target, name, mode) in [
        ("instance.json", "configMetadata", 0o660),
        ("chain.json", "chainDescriptionMetadata", 0o666),
        ("secrets/v.p12", "validatorKeystoreMetadata", 0o644),
        ("secrets/v.p12", "validatorKeystoreMetadata", 0o700),
        ("secrets", "validatorKeystoreMetadata", 0o755),
        ("data", "dataDirectoryMetadata", 0o755),
    ] {
        let fixture = fixture(true);
        fs::set_permissions(fixture.root.join(target), fs::Permissions::from_mode(mode)).unwrap();
        let result = preflight(&fixture.path).unwrap();
        assert_eq!(result.outcome, "FAIL");
        assert_eq!(check_by_name(&result, name).status, "FAIL");
    }
}

#[test]
fn preflight_rejects_symlink_files_and_parent_directories() {
    for target in ["secrets/v.p12", "secrets", "data"] {
        let fixture = fixture(true);
        let original = fixture.root.join(target);
        let moved = fixture.root.join(format!("{target}-original"));
        fs::rename(&original, &moved).unwrap();
        symlink(&moved, &original).unwrap();
        let result = preflight(&fixture.path).unwrap();
        assert_eq!(result.outcome, "FAIL");
        let name = if target == "data" {
            "dataDirectoryMetadata"
        } else {
            "validatorKeystoreMetadata"
        };
        assert_eq!(
            check_by_name(&result, name).reason_code,
            "REFERENCE_SYMLINK"
        );
    }
}

#[test]
fn normalize_unsaved_config_resolves_references_without_creating_or_reading_them() {
    let fixture = fixture(false);
    let source = fixture
        .root
        .join("future location/not-created/../instance.json");
    let before = snapshot(&fixture.root);
    let raw = SAMPLE.replace("chain.json", "missing/../chain.json");
    let normalized = normalize_bytes(raw.as_bytes(), &source).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&normalized).unwrap();
    let base = fixture.root.join("future location");
    for (actual, relative) in [
        (&value["chainDescription"], "chain.json"),
        (&value["storage"]["dataDirectory"], "data"),
        (&value["secrets"]["validatorKeystore"], "secrets/v.p12"),
        (&value["secrets"]["validatorPasswordFile"], "secrets/v.pass"),
        (&value["secrets"]["tlsKeyStore"], "secrets/t.p12"),
        (&value["secrets"]["tlsKeyPasswordFile"], "secrets/t.pass"),
        (&value["secrets"]["tlsTrustStore"], "secrets/trust.p12"),
        (
            &value["secrets"]["tlsTrustPasswordFile"],
            "secrets/trust.pass",
        ),
    ] {
        assert_eq!(Path::new(actual.as_str().unwrap()), base.join(relative));
    }
    assert!(normalized.ends_with(b"\n"));
    assert_eq!(normalize_bytes(&normalized, &source).unwrap(), normalized);
    assert_eq!(snapshot(&fixture.root), before);
    assert!(!source.exists());
}

#[test]
fn unsaved_relative_source_uses_cwd_lexically_without_rebasing_absolute_references() {
    let source = Path::new("future-config/../candidate folder/instance.json");
    let expected_base = std::env::current_dir().unwrap().join("candidate folder");
    let normalized = normalize_bytes(SAMPLE.as_bytes(), source).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&normalized).unwrap();
    assert_eq!(
        Path::new(value["chainDescription"].as_str().unwrap()),
        expected_base.join("chain.json")
    );
    let fixture = fixture(false);
    // Absolute references stay attached to their original source when the
    // future destination changes. This is the import-to-draft boundary.
    assert_eq!(
        normalize_bytes(&normalized, &fixture.root.join("different.json")).unwrap(),
        normalized
    );
}

#[test]
fn normalized_file_import_is_a_snapshot_not_a_later_reread() {
    let fixture = fixture(true);
    let imported = normalized_file(&fixture.path).unwrap();
    let disk = validate_file(&fixture.path).unwrap();
    assert_eq!(
        disk.config_sha256,
        hex::encode(Sha256::digest(SAMPLE.as_bytes()))
    );
    write(&fixture.path, "INVALID_SECRET_CONFIG_CONTENT_CANARY", 0o600);
    let before = snapshot(&fixture.root);
    let future = fixture.root.join("elsewhere/not-created/instance.json");
    let candidate = normalize_bytes(&imported, &future).unwrap();
    assert_eq!(candidate, imported);
    let result = preflight_bytes(&candidate, &future).unwrap();
    assert_eq!(result.outcome, "INCOMPLETE");
    assert_eq!(result.instance_id, "node-one");
    assert_eq!(
        result.config_sha256,
        hex::encode(Sha256::digest(&candidate))
    );
    assert_eq!(
        check_by_name(&result, "chainDescriptionMetadata").status,
        "PASS"
    );
    assert_eq!(
        check_by_name(&result, "validatorKeystoreMetadata").status,
        "PASS"
    );
    assert!(!serde_json::to_string(&result).unwrap().contains("CANARY"));
    assert_eq!(snapshot(&fixture.root), before);
    assert!(!future.exists());
}

#[test]
fn draft_preflight_matches_reference_checks_but_never_checks_the_config_path() {
    let fixture = fixture(true);
    let saved = preflight(&fixture.path).unwrap();
    // A dangling symlink at the future config location must not be followed,
    // read, or interpreted as this candidate's file. The setup model handles
    // collisions before persistence.
    fs::remove_file(&fixture.path).unwrap();
    symlink("PRIVATE_CONFIG_TARGET_CANARY", &fixture.path).unwrap();
    let before = snapshot(&fixture.root);
    let draft = preflight_bytes(SAMPLE.as_bytes(), &fixture.path).unwrap();
    let config_check = check_by_name(&draft, "configMetadata");
    assert_eq!(config_check.status, "NOT_CHECKED");
    assert_eq!(config_check.reason_code, "DRAFT_NOT_WRITTEN");
    assert_eq!(draft.outcome, "INCOMPLETE");
    assert_eq!(draft.config_sha256, saved.config_sha256);
    assert_eq!(draft.checks.len(), saved.checks.len());
    for check in &saved.checks {
        if check.name != "configMetadata" {
            assert_eq!(
                serde_json::to_value(check_by_name(&draft, &check.name)).unwrap(),
                serde_json::to_value(check).unwrap()
            );
        }
    }
    let report = serde_json::to_string(&draft).unwrap();
    for sensitive in ["CANARY", "secrets/v.pass", fixture.root.to_str().unwrap()] {
        assert!(!report.contains(sensitive));
    }
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn candidate_apis_preserve_strict_reason_codes_and_hide_invalid_input() {
    let fixture = fixture(false);
    let source = fixture.root.join("PRIVATE_CONFIG_CANARY.json");
    let cases = [
        (
            SAMPLE
                .replace(
                    r#""schemaVersion":1"#,
                    r#""PRIVATE_FIELD_CANARY":"SECRET_CANARY","schemaVersion":1"#,
                )
                .into_bytes(),
            "CONFIG_JSON_INVALID",
        ),
        (
            SAMPLE
                .replace(
                    r#""schemaVersion":1"#,
                    r#""schemaVersion":1,"schemaVersion":1"#,
                )
                .into_bytes(),
            "CONFIG_JSON_INVALID",
        ),
        (
            SAMPLE
                .replace(r#""schemaVersion":1"#, r#""schemaVersion":null"#)
                .into_bytes(),
            "CONFIG_SCHEMA_INVALID",
        ),
        (
            SAMPLE.replace(r#""schemaVersion":1,"#, "").into_bytes(),
            "CONFIG_SCHEMA_INVALID",
        ),
        (
            SAMPLE
                .replace("chain.json", "PRIVATE_CONFIG_CANARY.json")
                .into_bytes(),
            "REFERENCE_PATH_INVALID",
        ),
        (vec![b' '; MAX_CONFIG_BYTES + 1], "CONFIG_TOO_LARGE"),
        ([SAMPLE.as_bytes(), &[0xff]].concat(), "CONFIG_JSON_INVALID"),
    ];
    for (raw, reason) in cases {
        write(&source, &raw, 0o600);
        for error in [
            normalize_bytes(&raw, &source).unwrap_err(),
            preflight_bytes(&raw, &source).unwrap_err(),
            normalized_file(&source).unwrap_err(),
            validate_file(&source).unwrap_err(),
        ] {
            assert_eq!(error.code, reason);
            assert!(!error.message.contains("CANARY"));
            assert!(!error.message.contains(fixture.root.to_str().unwrap()));
        }
    }
    let invalid_source = Path::new("PRIVATE_PATH_CANARY\n.json");
    for error in [
        normalize_bytes(SAMPLE.as_bytes(), invalid_source).unwrap_err(),
        preflight_bytes(SAMPLE.as_bytes(), invalid_source).unwrap_err(),
    ] {
        assert_eq!(error.code, "CONFIG_PATH_INVALID");
        assert!(!error.message.contains("CANARY"));
    }
}

#[test]
fn imported_config_requires_a_bounded_regular_file_and_normalized_bytes_stay_valid() {
    let fixture = fixture(false);
    let alias = fixture.root.join("alias.json");
    symlink(&fixture.path, &alias).unwrap();
    assert_eq!(
        normalized_file(&alias).unwrap_err().code,
        "CONFIG_NOT_REGULAR"
    );
    assert_eq!(
        normalized_file(&fixture.root).unwrap_err().code,
        "CONFIG_NOT_REGULAR"
    );
    write(&fixture.path, vec![b' '; MAX_CONFIG_BYTES + 1], 0o600);
    assert_eq!(
        normalized_file(&fixture.path).unwrap_err().code,
        "CONFIG_TOO_LARGE"
    );

    // A relative input can meet its path limit before absolutization while its
    // absolute equivalent exceeds it. Reject instead of returning an unusable
    // normalized document. No such reference is read or created.
    let raw = SAMPLE.replace("chain.json", &"a".repeat(4096));
    assert_eq!(
        normalize_bytes(raw.as_bytes(), &fixture.path)
            .unwrap_err()
            .code,
        "REFERENCE_PATH_INVALID"
    );
}
