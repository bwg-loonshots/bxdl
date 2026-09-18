use super::*;

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
#[test]
fn unsupported_host_does_not_read_an_installation() {
    let manifest: Manifest =
        serde_json::from_str(include_str!("../../packaging/package-spec.example.json")).unwrap();
    let expected = Report {
        manifest,
        archive_sha256: String::new(),
        manifest_sha256: String::new(),
        authenticity: String::new(),
        files_verified: 0,
        bytes_verified: 0,
    };
    assert_eq!(
        verify_installed(Path::new("missing"), &expected)
            .unwrap_err()
            .code,
        "INSTALL_PLATFORM_UNSUPPORTED"
    );
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod mac {
    use super::*;
    use crate::artifact::{BuildOptions, VerifyOptions};
    use ed25519_dalek::{
        SigningKey,
        pkcs8::{EncodePrivateKey, EncodePublicKey},
    };
    use std::fs;
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    use std::path::PathBuf;

    struct Fixture {
        _temporary: tempfile::TempDir,
        root: PathBuf,
        destination: PathBuf,
        expected: Report,
    }
    impl Fixture {
        fn new(signed: bool) -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let stage = root.join("payload");
            let payload = [
                ("bin/bxdl", b"not executable; test CLI".as_slice()),
                ("engine/nigo-node.jar", b"not executable; test JAR"),
                ("runtime/bin/java", b"not executable; test Java"),
                ("runtime/lib/modules", b"test modules original"),
                ("licenses/THIRD_PARTY_NOTICES", b"test fixture notices"),
                ("licenses/SBOM.json", br#"{"testOnly":true}"#),
            ];
            for (name, raw) in payload {
                let path = stage.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, raw).unwrap();
                let mode = if matches!(name, "bin/bxdl" | "runtime/bin/java") {
                    0o755
                } else {
                    0o644
                };
                fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
            }
            let mut manifest: Manifest =
                serde_json::from_str(include_str!("../../packaging/package-spec.example.json"))
                    .unwrap();
            manifest.engine.revision = "a".repeat(40);
            manifest.engine.jar_sha256 = digest(payload[1].1);
            manifest.runtime.java_sha256 = digest(payload[2].1);
            let spec = root.join("spec.json");
            fs::write(&spec, serde_json::to_vec(&manifest).unwrap()).unwrap();
            let key = SigningKey::from_bytes(&[0x39; 32]); // Fixed test-only seed.
            let private = root.join("test-private.pem");
            let public = root.join("test-public.pem");
            for (path, label, raw) in [
                (
                    &private,
                    "PRIVATE KEY",
                    key.to_pkcs8_der().unwrap().as_bytes().to_vec(),
                ),
                (
                    &public,
                    "PUBLIC KEY",
                    key.verifying_key()
                        .to_public_key_der()
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                ),
            ] {
                fs::write(
                    path,
                    format!(
                        "-----BEGIN {label}-----\n{}\n-----END {label}-----\n",
                        STANDARD.encode(raw)
                    ),
                )
                .unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
            }
            let archive = root.join("test.tar.gz");
            artifact::build(&BuildOptions {
                root: stage,
                spec_path: spec,
                output: archive.clone(),
                signing_key_path: signed.then_some(private),
                allow_unsigned_development: !signed,
            })
            .unwrap();
            let options = VerifyOptions {
                public_key_path: signed.then_some(public),
                allow_unsigned_development: !signed,
            };
            let expected = artifact::verify(&archive, &options).unwrap();
            let destination = root.join("installed package");
            crate::install::install(&archive, &destination, &options).unwrap();
            Self {
                _temporary: temporary,
                root,
                destination,
                expected,
            }
        }
        fn verify(&self) -> Result<()> {
            verify_installed(&self.destination, &self.expected)
        }
        fn edit_receipt(&self, edit: impl FnOnce(&mut serde_json::Value)) {
            let path = self.destination.join(RECEIPT);
            let mut value: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            edit(&mut value);
            fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        }
    }

    #[test]
    fn full_signed_and_explicit_unsigned_installs_reverify_without_writing() {
        for signed in [true, false] {
            let fixture = Fixture::new(signed);
            let receipt_before = fs::read(fixture.destination.join(RECEIPT)).unwrap();
            fixture.verify().unwrap();
            assert_eq!(
                receipt_before,
                fs::read(fixture.destination.join(RECEIPT)).unwrap()
            );
        }
    }

    #[test]
    fn changed_jre_module_is_rejected_even_when_launcher_and_receipt_are_unchanged() {
        let fixture = Fixture::new(true);
        let launcher = fs::read(fixture.destination.join("runtime/bin/java")).unwrap();
        fs::write(
            fixture.destination.join("runtime/lib/modules"),
            b"test modules modified",
        )
        .unwrap();
        assert_eq!(
            fixture.verify().unwrap_err().code,
            "INSTALLED_PACKAGE_MISMATCH"
        );
        assert_eq!(
            launcher,
            fs::read(fixture.destination.join("runtime/bin/java")).unwrap()
        );
    }

    #[test]
    fn extra_missing_empty_directory_and_case_aliases_are_not_inventory() {
        for case in 0..5 {
            let fixture = Fixture::new(false);
            match case {
                0 => fs::write(fixture.destination.join("runtime/extra"), b"extra").unwrap(),
                1 => fs::remove_file(fixture.destination.join("runtime/lib/modules")).unwrap(),
                2 => {
                    fs::create_dir(fixture.destination.join("unexpected-empty-directory")).unwrap()
                }
                3 => fs::create_dir(fixture.destination.join("runtime/lib/extra-empty")).unwrap(),
                _ => fs::rename(
                    fixture.destination.join("runtime/lib/modules"),
                    fixture.destination.join("runtime/lib/MODULES"),
                )
                .unwrap(),
            }
            assert!(fixture.verify().is_err(), "case {case}");
        }
    }

    #[test]
    fn every_receipt_claim_is_checked_against_external_expectation() {
        for field in [
            "schemaVersion",
            "kind",
            "outcome",
            "destination",
            "archiveSha256",
            "manifestSha256",
            "authenticity",
            "filesInstalled",
            "bytesInstalled",
            "manifest",
            "engineValidation",
            "lifecycle",
        ] {
            let fixture = Fixture::new(true);
            fixture.edit_receipt(|v| {
                v[field] = match field {
                    "schemaVersion" | "filesInstalled" | "bytesInstalled" => serde_json::json!(0),
                    "manifest" => {
                        let mut m = v[field].clone();
                        m["product"]["version"] = serde_json::json!("tampered");
                        m
                    }
                    _ => serde_json::json!("PRIVATE_ERROR_CANARY"),
                };
            });
            let error = fixture.verify().unwrap_err();
            assert_eq!(error.code, "INSTALLED_PACKAGE_MISMATCH", "{field}");
            assert!(!error.message.contains("CANARY"));
            assert!(!error.message.contains(fixture.root.to_str().unwrap()));
        }
    }

    #[test]
    fn receipt_duplicate_missing_and_unknown_fields_fail_closed() {
        for case in 0..3 {
            let fixture = Fixture::new(false);
            let path = fixture.destination.join(RECEIPT);
            if case == 0 {
                let raw = fs::read_to_string(&path).unwrap();
                fs::write(path, raw.replacen('{', "{\"schemaVersion\":1,", 1)).unwrap();
            } else {
                fixture.edit_receipt(|v| {
                    if case == 1 {
                        v.as_object_mut().unwrap().remove("archiveSha256");
                    } else {
                        v["unexpected"] = serde_json::json!(true);
                    }
                });
            }
            assert_eq!(
                fixture.verify().unwrap_err().code,
                "INSTALLED_PACKAGE_MISMATCH"
            );
        }
    }

    #[test]
    fn installed_manifest_bytes_cannot_be_reformatted_or_replace_trusted_manifest() {
        let fixture = Fixture::new(true);
        let path = fixture.destination.join("manifest.json");
        let mut raw = fs::read(&path).unwrap();
        raw.push(b'\n');
        fs::write(&path, raw).unwrap();
        assert_eq!(
            fixture.verify().unwrap_err().code,
            "INSTALLED_PACKAGE_MISMATCH"
        );
        let fixture = Fixture::new(true);
        let path = fixture.destination.join("manifest.json");
        let mut manifest = fixture.expected.manifest.clone();
        manifest.runtime.vendor = "tampered".into();
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        fixture.edit_receipt(|v| {
            v["manifest"] = serde_json::to_value(manifest).unwrap();
        });
        assert_eq!(
            fixture.verify().unwrap_err().code,
            "INSTALLED_PACKAGE_MISMATCH"
        );
    }

    #[test]
    fn signature_presence_and_encoding_follow_external_authenticity() {
        let signed = Fixture::new(true);
        fs::remove_file(signed.destination.join("manifest.sig")).unwrap();
        assert!(signed.verify().is_err());
        let signed = Fixture::new(true);
        fs::write(
            signed.destination.join("manifest.sig"),
            b"invalid signature",
        )
        .unwrap();
        assert_eq!(
            signed.verify().unwrap_err().code,
            "INSTALLED_PACKAGE_MISMATCH"
        );
        let unsigned = Fixture::new(false);
        fs::write(
            unsigned.destination.join("manifest.sig"),
            STANDARD.encode([0_u8; 64]),
        )
        .unwrap();
        assert_eq!(
            unsigned.verify().unwrap_err().code,
            "INSTALLED_PACKAGE_MISMATCH"
        );
    }

    #[test]
    fn links_and_unsafe_file_directory_or_receipt_permissions_fail() {
        for case in 0..7 {
            let fixture = Fixture::new(true);
            let file = fixture.destination.join("runtime/lib/modules");
            match case {
                0 => {
                    fs::remove_file(&file).unwrap();
                    symlink(&fixture.expected.manifest.files[0].path, &file).unwrap();
                }
                1 => fs::hard_link(&file, fixture.root.join("external-hardlink")).unwrap(),
                2 => fs::set_permissions(&file, fs::Permissions::from_mode(0o664)).unwrap(),
                3 => fs::set_permissions(
                    fixture.destination.join("runtime/lib"),
                    fs::Permissions::from_mode(0o755),
                )
                .unwrap(),
                4 => fs::set_permissions(&fixture.destination, fs::Permissions::from_mode(0o755))
                    .unwrap(),
                5 => fs::set_permissions(
                    fixture.destination.join(RECEIPT),
                    fs::Permissions::from_mode(0o644),
                )
                .unwrap(),
                _ => {
                    let real = fixture.root.join("moved-runtime");
                    fs::rename(fixture.destination.join("runtime"), &real).unwrap();
                    symlink(real, fixture.destination.join("runtime")).unwrap();
                }
            }
            assert!(fixture.verify().is_err(), "case {case}");
        }
    }

    #[test]
    fn changed_file_or_visible_root_after_hashing_is_rejected() {
        let fixture = Fixture::new(true);
        assert!(
            verify_observed(&fixture.destination, &fixture.expected, || {
                fs::write(
                    fixture.destination.join("runtime/lib/modules"),
                    b"test modules modified",
                )
                .unwrap();
            })
            .is_err()
        );
        let fixture = Fixture::new(true);
        assert!(
            verify_observed(&fixture.destination, &fixture.expected, || {
                fs::rename(&fixture.destination, fixture.root.join("moved")).unwrap();
                fs::create_dir(&fixture.destination).unwrap();
            })
            .is_err()
        );
    }

    #[test]
    fn unsafe_receipt_size_and_inconsistent_expected_report_fail_without_trusting_receipt() {
        let fixture = Fixture::new(true);
        let file = fs::OpenOptions::new()
            .write(true)
            .open(fixture.destination.join(RECEIPT))
            .unwrap();
        file.set_len(MAX_RECEIPT_BYTES + 1).unwrap();
        assert_eq!(
            fixture.verify().unwrap_err().code,
            "INSTALLED_PACKAGE_UNSAFE"
        );
        let mut fixture = Fixture::new(true);
        fixture.expected.files_verified += 1;
        assert_eq!(
            fixture.verify().unwrap_err().code,
            "INSTALLED_PACKAGE_MISMATCH"
        );
    }
}
