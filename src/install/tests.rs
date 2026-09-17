use super::*;

#[test]
fn inventory_aliases_include_unicode_and_file_directory_conflicts() {
    let mut manifest: Manifest =
        serde_json::from_str(include_str!("../../packaging/package-spec.example.json")).unwrap();
    for names in [
        ["docs/A", "docs/a"],
        ["docs/é", "docs/e\u{301}"],
        ["docs/UPPER/one", "docs/upper/two"],
        ["docs/Case", "docs/case/child"],
    ] {
        manifest.files = names
            .iter()
            .map(|name| FileEntry {
                path: (*name).into(),
                size: 1,
                sha256: "a".repeat(64),
                mode: 0o644,
            })
            .collect();
        assert_eq!(
            reject_inventory_aliases(&manifest).unwrap_err().code,
            "INSTALL_PATH_COLLISION"
        );
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
#[test]
fn unsupported_host_fails_before_creating_any_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let destination = temporary.path().join("new");
    assert_eq!(
        install(
            Path::new("missing"),
            &destination,
            &VerifyOptions::default()
        )
        .unwrap_err()
        .code,
        "INSTALL_PLATFORM_UNSUPPORTED"
    );
    assert!(!destination.exists());
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod mac {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use ed25519_dalek::{
        Signer, SigningKey,
        pkcs8::{EncodePublicKey, spki::der::pem::LineEnding},
    };
    use flate2::{Compression, write::GzEncoder};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt as _, symlink};

    struct Fixture {
        _temporary: tempfile::TempDir,
        root: PathBuf,
        archive: PathBuf,
        destination: PathBuf,
        key: SigningKey,
        options: VerifyOptions,
        manifest: Manifest,
        payload: BTreeMap<String, Vec<u8>>,
    }
    impl Fixture {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let payload: BTreeMap<String, Vec<u8>> = [
                ("bin/bxdl", b"fixture CLI, not executable".as_slice()),
                ("engine/nigo-node.jar", b"fixture engine, not official"),
                ("runtime/bin/java", b"fixture Java, not executable"),
                ("licenses/THIRD_PARTY_NOTICES", b"fixture notices"),
                ("licenses/SBOM.json", br#"{"fixture":true}"#),
            ]
            .into_iter()
            .map(|(p, b)| (p.into(), b.to_vec()))
            .collect();
            let mut manifest: Manifest =
                serde_json::from_str(include_str!("../../packaging/package-spec.example.json"))
                    .unwrap();
            manifest.engine.revision = "a".repeat(40);
            manifest.engine.contract_revision = "test-fixture-v1".into();
            manifest.runtime.vendor = "test-fixture".into();
            manifest.runtime.version = "21-fixture".into();
            manifest.engine.jar_sha256 = hash(&payload["engine/nigo-node.jar"]);
            manifest.runtime.java_sha256 = hash(&payload["runtime/bin/java"]);
            manifest.files = payload
                .iter()
                .map(|(path, raw)| FileEntry {
                    path: path.clone(),
                    size: raw.len() as u64,
                    sha256: hash(raw),
                    mode: if matches!(path.as_str(), "bin/bxdl" | "runtime/bin/java") {
                        0o755
                    } else {
                        0o644
                    },
                })
                .collect();
            let key = SigningKey::from_bytes(&[0x72; 32]); // Explicit fixed test-only seed.
            let public = root.join("test-public.pem");
            fs::write(
                &public,
                key.verifying_key()
                    .to_public_key_pem(LineEnding::LF)
                    .unwrap(),
            )
            .unwrap();
            let fixture = Self {
                archive: root.join("archive.tar.gz"),
                destination: root.join("installed"),
                options: VerifyOptions {
                    public_key_path: Some(public),
                    allow_unsigned_development: false,
                },
                _temporary: temporary,
                root,
                key,
                manifest,
                payload,
            };
            fixture.write_archive();
            fixture
        }
        fn write_archive(&self) {
            let raw_manifest = serde_json::to_vec(&self.manifest).unwrap();
            let signature = STANDARD.encode(self.key.sign(&raw_manifest).to_bytes());
            let mut tar = Vec::new();
            append(&mut tar, "manifest.json", 0o644, &raw_manifest);
            append(&mut tar, "manifest.sig", 0o644, signature.as_bytes());
            for entry in &self.manifest.files {
                append(
                    &mut tar,
                    &entry.path,
                    entry.mode,
                    &self.payload[&entry.path],
                );
            }
            tar.extend_from_slice(&[0; 1024]);
            let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
            gzip.write_all(&tar).unwrap();
            fs::write(&self.archive, gzip.finish().unwrap()).unwrap();
        }
        fn install(&self) -> Result<InstallReport> {
            install(&self.archive, &self.destination, &self.options)
        }
        fn assert_incomplete(&self) {
            assert!(self.destination.is_dir());
            assert!(!self.destination.join(RECEIPT).exists());
            assert_eq!(
                fs::metadata(&self.destination).unwrap().mode() & 0o7777,
                0o700
            );
        }
    }
    fn hash(raw: &[u8]) -> String {
        hex::encode(Sha256::digest(raw))
    }
    fn append(tar: &mut Vec<u8>, name: &str, mode: u32, raw: &[u8]) {
        let mut header = [0_u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        octal(&mut header[100..108], mode as u64);
        octal(&mut header[108..116], 0);
        octal(&mut header[116..124], 0);
        octal(&mut header[124..136], raw.len() as u64);
        octal(&mut header[136..148], 0);
        header[148..156].fill(b' ');
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum: u64 = header.iter().map(|b| *b as u64).sum();
        let value = format!("{checksum:06o}\0 ");
        header[148..156].copy_from_slice(value.as_bytes());
        tar.extend_from_slice(&header);
        tar.extend_from_slice(raw);
        tar.resize(tar.len() + (512 - raw.len() % 512) % 512, 0);
    }
    fn octal(destination: &mut [u8], value: u64) {
        let text = format!("{value:0width$o}\0", width = destination.len() - 1);
        destination.copy_from_slice(text.as_bytes());
    }

    #[test]
    fn signed_install_commits_exact_files_and_private_receipt_after_verification() {
        let fixture = Fixture::new();
        let expected = artifact::verify(&fixture.archive, &fixture.options).unwrap();
        let report = fixture.install().unwrap();
        assert_eq!(report.outcome, "INSTALLED");
        assert_eq!(report.archive_sha256, expected.archive_sha256);
        assert_eq!(report.manifest_sha256, expected.manifest_sha256);
        assert_eq!(report.authenticity, "verified-external-ed25519");
        assert_eq!(report.engine_validation, "NOT_CHECKED");
        assert_eq!(report.lifecycle, "NOT_PERFORMED");
        assert_eq!(report.files_installed, fixture.payload.len());
        assert_eq!(
            fs::metadata(&fixture.destination).unwrap().mode() & 0o7777,
            0o700
        );
        for file in &fixture.manifest.files {
            let path = fixture.destination.join(&file.path);
            assert_eq!(fs::read(&path).unwrap(), fixture.payload[&file.path]);
            let m = fs::symlink_metadata(path).unwrap();
            assert!(m.is_file());
            assert_eq!(m.mode() & 0o7777, file.mode);
            assert_eq!(m.nlink(), 1);
        }
        let receipt: InstallReport =
            serde_json::from_slice(&fs::read(fixture.destination.join(RECEIPT)).unwrap()).unwrap();
        assert_eq!(receipt.archive_sha256, report.archive_sha256);
        assert_eq!(
            fs::metadata(fixture.destination.join(RECEIPT))
                .unwrap()
                .mode()
                & 0o7777,
            0o600
        );
        assert!(!fixture.destination.join(RECEIPT_TEMP).exists());
        assert_eq!(
            hash(&fs::read(fixture.destination.join("manifest.json")).unwrap()),
            report.manifest_sha256
        );
        assert!(fixture.destination.join("manifest.sig").is_file());
    }

    #[test]
    fn late_payload_corruption_leaves_no_receipt_or_executable_mode_and_cannot_resume() {
        let mut fixture = Fixture::new();
        *fixture
            .payload
            .get_mut("runtime/bin/java")
            .unwrap()
            .last_mut()
            .unwrap() ^= 1;
        fixture.write_archive();
        assert_eq!(fixture.install().unwrap_err().code, "HASH_MISMATCH");
        fixture.assert_incomplete();
        assert_eq!(
            fs::metadata(fixture.destination.join("bin/bxdl"))
                .unwrap()
                .mode()
                & 0o7777,
            0o600
        );
        let bytes = fs::read(fixture.destination.join("bin/bxdl")).unwrap();
        assert_eq!(
            fixture.install().unwrap_err().code,
            "INSTALL_DESTINATION_EXISTS"
        );
        assert_eq!(
            fs::read(fixture.destination.join("bin/bxdl")).unwrap(),
            bytes
        );
    }

    #[test]
    fn gzip_crc_and_concatenated_member_fail_after_streaming_without_receipt() {
        for corruption in 0..3 {
            let fixture = Fixture::new();
            let mut raw = fs::read(&fixture.archive).unwrap();
            match corruption {
                0 => {
                    let index = raw.len() - 8;
                    raw[index] ^= 1;
                }
                1 => raw.extend_from_slice(
                    &GzEncoder::new(Vec::new(), Compression::default())
                        .finish()
                        .unwrap(),
                ),
                _ => raw.extend_from_slice(b"extra compressed bytes"),
            }
            fs::write(&fixture.archive, raw).unwrap();
            assert!(fixture.install().is_err());
            fixture.assert_incomplete();
            assert!(!fixture.destination.join("manifest.json").exists());
        }
    }

    #[test]
    fn bad_signature_and_other_platform_create_no_destination() {
        let mut fixture = Fixture::new();
        let wrong_key = SigningKey::from_bytes(&[0x73; 32]);
        fs::write(
            fixture.options.public_key_path.as_ref().unwrap(),
            wrong_key
                .verifying_key()
                .to_public_key_pem(LineEnding::LF)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(fixture.install().unwrap_err().code, "SIGNATURE_INVALID");
        assert!(!fixture.destination.exists());
        fs::write(
            fixture.options.public_key_path.as_ref().unwrap(),
            fixture
                .key
                .verifying_key()
                .to_public_key_pem(LineEnding::LF)
                .unwrap(),
        )
        .unwrap();
        fixture.manifest.platform.os = "linux".into();
        fixture.manifest.platform.arch = "amd64".into();
        fixture.manifest.platform.libc = "glibc".into();
        fixture.manifest.platform.min_glibc = "2.34".into();
        fixture.write_archive();
        assert_eq!(
            fixture.install().unwrap_err().code,
            "INSTALL_PLATFORM_UNSUPPORTED"
        );
        assert!(!fixture.destination.exists());
    }

    #[test]
    fn signed_mac_path_aliases_are_rejected_before_reserving_destination() {
        let mut fixture = Fixture::new();
        for name in ["docs/Case", "docs/case"] {
            fixture
                .payload
                .insert(name.into(), b"alias fixture".to_vec());
            fixture.manifest.files.push(FileEntry {
                path: name.into(),
                size: 13,
                sha256: hash(b"alias fixture"),
                mode: 0o644,
            });
        }
        fixture.write_archive();
        artifact::verify(&fixture.archive, &fixture.options).unwrap();
        assert_eq!(
            fixture.install().unwrap_err().code,
            "INSTALL_PATH_COLLISION"
        );
        assert!(!fixture.destination.exists());
    }

    #[test]
    fn existing_destination_alias_symlinks_and_missing_parent_are_never_adopted() {
        let fixture = Fixture::new();
        fs::create_dir(&fixture.destination).unwrap();
        let private = fixture.destination.join("customer-key.pem");
        fs::write(&private, b"PRIVATE_TEST_CANARY").unwrap();
        for destination in [&fixture.destination, &fixture.root.join("INSTALLED")] {
            assert_eq!(
                install(&fixture.archive, destination, &fixture.options)
                    .unwrap_err()
                    .code,
                "INSTALL_DESTINATION_EXISTS"
            );
        }
        assert_eq!(fs::read(&private).unwrap(), b"PRIVATE_TEST_CANARY");
        assert!(!fixture.destination.join(RECEIPT).exists());
        let link = fixture.root.join("alias");
        symlink(&fixture.destination, &link).unwrap();
        assert!(install(&fixture.archive, &link, &fixture.options).is_err());
        assert!(install(&fixture.archive, &link.join("child"), &fixture.options).is_err());
        assert!(!fixture.destination.join("child").exists());
        let missing = fixture.root.join("missing/new");
        assert!(install(&fixture.archive, &missing, &fixture.options).is_err());
        assert!(!fixture.root.join("missing").exists());
    }

    struct ReplacePathSink {
        installer: Installer,
        archive: PathBuf,
        saved: PathBuf,
    }
    impl PayloadSink for ReplacePathSink {
        fn begin(
            &mut self,
            manifest: &Manifest,
            raw: &[u8],
            signature: Option<&[u8]>,
        ) -> Result<()> {
            self.installer.begin(manifest, raw, signature)?;
            fs::rename(&self.archive, &self.saved).unwrap();
            fs::write(
                &self.archive,
                b"unverified replacement must never be extracted",
            )
            .unwrap();
            Ok(())
        }
        fn start_file(&mut self, entry: &FileEntry) -> Result<()> {
            self.installer.start_file(entry)
        }
        fn write_chunk(&mut self, bytes: &[u8]) -> Result<()> {
            self.installer.write_chunk(bytes)
        }
        fn finish_file(&mut self) -> Result<()> {
            self.installer.finish_file()
        }
    }
    #[test]
    fn extraction_uses_open_verified_stream_even_if_archive_path_is_replaced() {
        let fixture = Fixture::new();
        let mut original = Vec::new();
        fs::File::open(&fixture.archive)
            .unwrap()
            .read_to_end(&mut original)
            .unwrap();
        let mut sink = ReplacePathSink {
            installer: Installer::new(&fixture.destination).unwrap(),
            archive: fixture.archive.clone(),
            saved: fixture.root.join("opened-original.tar.gz"),
        };
        let verified =
            artifact::verify_to_sink(&fixture.archive, &fixture.options, &mut sink).unwrap();
        assert_eq!(verified.archive_sha256, hash(&original));
        let report = sink.installer.commit(verified).unwrap();
        assert_eq!(report.archive_sha256, hash(&original));
        for (path, raw) in &fixture.payload {
            assert_eq!(fs::read(fixture.destination.join(path)).unwrap(), *raw);
        }
        assert!(
            String::from_utf8(fs::read(&fixture.archive).unwrap())
                .unwrap()
                .contains("unverified replacement")
        );
    }
    #[test]
    fn concurrent_first_install_has_one_winner_without_overwrite() {
        let fixture = Fixture::new();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let barrier = std::sync::Arc::clone(&barrier);
                let archive = fixture.archive.clone();
                let destination = fixture.destination.clone();
                let options = fixture.options.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    install(&archive, &destination, &options)
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results.iter().find_map(|r| r.as_ref().err()).unwrap().code,
            "INSTALL_DESTINATION_EXISTS"
        );
        assert!(fixture.destination.join(RECEIPT).is_file());
        for (path, raw) in &fixture.payload {
            assert_eq!(fs::read(fixture.destination.join(path)).unwrap(), *raw);
        }
    }

    #[test]
    fn uncertain_publication_guard_removes_only_its_own_receipt_inode() {
        let fixture = Fixture::new();
        let root = Dir::open_ambient_dir(&fixture.root, ambient_authority()).unwrap();
        for replace in [false, true] {
            let temp = write_regular(&root, RECEIPT_TEMP, b"test receipt", 0o600).unwrap();
            let identity = Identity::of(&temp.metadata().unwrap());
            root.hard_link(RECEIPT_TEMP, &root, RECEIPT).unwrap();
            let guard = ReceiptPublication {
                root: &root,
                identity,
                published: true,
                complete: false,
            };
            if replace {
                root.remove_file(RECEIPT).unwrap();
                write_regular(&root, RECEIPT, b"unrelated replacement", 0o600).unwrap();
            }
            drop(guard);
            assert!(!fixture.root.join(RECEIPT_TEMP).exists());
            if replace {
                assert_eq!(
                    fs::read(fixture.root.join(RECEIPT)).unwrap(),
                    b"unrelated replacement"
                );
            } else {
                assert!(!fixture.root.join(RECEIPT).exists());
            }
        }
    }
}
