use super::*;
use serde_json::{Value, json};
use std::os::unix::fs::{PermissionsExt, symlink};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    control: Control,
    binding: Binding,
    state: State,
    journal: Store,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let control = Control::create(&root.join("control")).unwrap();
        let binding = Binding {
            schema_version: 1,
            instance_id: "test-validator".into(),
            package: root.join("package"),
            archive: root.join("bundle.tar.gz"),
            public_key: None,
            allow_unsigned_development: true,
            product: root.join("product.json"),
            native: root.join("node.json"),
            lock: root.join("trusted.lock.json"),
            archive_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            identity: serde_json::from_slice(include_bytes!(
                "../../contracts/nigo/development-clean-2026-09-18/evidence/engine-info.json"
            ))
            .unwrap(),
            backend: "rocksdb".into(),
            chain_fingerprint: "c".repeat(64),
            node_identity: format!("0x{}:0x{}", "1".repeat(64), "2".repeat(40)),
            data_directory: root.join("data"),
            pins: vec![],
            supply: None,
        };
        let raw = serde_json::to_vec(&binding).unwrap();
        store::write_new(&control.path().join("binding.json"), &raw).unwrap();
        let mut journal = Store::create(&control.path().join("journal")).unwrap();
        let state = State {
            schema_version: 1,
            instance_id: binding.instance_id.clone(),
            binding_sha256: files::digest(&raw),
            phase: Phase::Registered,
            attempt: None,
            genesis_hash: None,
            reason: "REGISTERED_COLD_CHECKED".into(),
        };
        state.save(&mut journal).unwrap();
        Self {
            _temp: temp,
            root,
            control,
            binding,
            state,
            journal,
        }
    }
    fn pending(&mut self) {
        self.state.phase = Phase::InitIntent;
        self.state.attempt = Some(Attempt {
            id: "known-attempt".into(),
            command: "init".into(),
        });
        self.state.reason = "INITIALIZATION_ATTEMPT_UNRESOLVED".into();
        self.state.save(&mut self.journal).unwrap();
    }
    fn engine_journal(&self, status: &str) {
        fs::create_dir_all(self.binding.data_directory.join("ledger")).unwrap();
        let v = json!({"status":status,"backend":"rocksdb","chainFingerprint":self.binding.chain_fingerprint,"nodeIdentity":self.binding.node_identity,"dataDirectory":self.binding.data_directory,"genesisHash":if status=="INITIALIZING"{"".into()}else{format!("0x{}","3".repeat(64))}});
        put(
            &self.binding.data_directory.join("engine-instance.json"),
            &serde_json::to_vec(&v).unwrap(),
        );
        put(&self.binding.data_directory.join("engine.lock"), b"");
    }
}
fn put(path: &Path, raw: &[u8]) {
    fs::write(path, raw).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
#[test]
fn unresolved_intent_stays_unknown_and_inherited_lock_is_observed() {
    let mut f = Fixture::new();
    assert_eq!(
        show(f.control.path()).unwrap().initialization,
        "NOT_STARTED"
    );
    f.pending();
    let lock = f.control.lock().unwrap();
    let report = show(f.control.path()).unwrap();
    assert_eq!(report.initialization, "UNKNOWN");
    assert_eq!(report.operation_busy, Some(true));
    assert_eq!(report.attempt_id.as_deref(), Some("known-attempt"));
    let child = lock.child_stdin().unwrap();
    drop(lock);
    assert_eq!(show(f.control.path()).unwrap().operation_busy, Some(true));
    drop(child);
    assert_eq!(show(f.control.path()).unwrap().operation_busy, Some(false));
    assert_eq!(show(f.control.path()).unwrap().initialization, "UNKNOWN");
}
#[test]
fn journal_terminal_state_must_have_attempt_and_genesis() {
    let mut f = Fixture::new();
    f.state.phase = Phase::Initialized;
    f.state.genesis_hash = Some(format!("0x{}", "3".repeat(64)));
    f.state.save(&mut f.journal).unwrap();
    assert_eq!(
        show(f.control.path()).err().unwrap().code,
        "INSTANCE_STATE_INVALID"
    );
}
#[test]
fn changed_binding_is_not_adopted_and_summary_does_not_expose_paths_or_hashes() {
    let f = Fixture::new();
    let report = serde_json::to_string(&show(f.control.path()).unwrap()).unwrap();
    assert!(!report.contains(f.root.to_str().unwrap()));
    assert!(!report.contains(&f.state.binding_sha256));
    let path = f.control.path().join("binding.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["instanceId"] = json!("changed");
    put(&path, &serde_json::to_vec(&value).unwrap());
    assert_eq!(
        show(f.control.path()).err().unwrap().code,
        "INSTANCE_STATE_INVALID"
    );
}
#[test]
fn input_rotation_blocks_without_echoing_private_contents() {
    let mut f = Fixture::new();
    let path = f.root.join("secret");
    put(&path, b"CANARY-original-secret");
    f.binding.pins.push(Pin {
        path: path.clone(),
        sha256: files::digest(b"CANARY-original-secret"),
    });
    assert!(f.binding.check_pins().is_ok());
    put(&path, b"CANARY-rotated-secret");
    let error = f.binding.check_pins().err().unwrap();
    assert_eq!(error.code, "INSTANCE_INPUT_CHANGED");
    assert!(!error.message.contains("CANARY"));
    assert!(!error.message.contains(f.root.to_str().unwrap()));
}
#[test]
fn init_rejects_nonempty_symlink_and_missing_parent_without_changes() {
    let mut f = Fixture::new();
    assert!(data_precondition(&f.binding, false).is_ok());
    assert!(!f.binding.data_directory.exists());
    fs::create_dir(&f.binding.data_directory).unwrap();
    put(&f.binding.data_directory.join("sentinel"), b"keep");
    assert!(data_precondition(&f.binding, false).is_err());
    assert_eq!(
        fs::read(f.binding.data_directory.join("sentinel")).unwrap(),
        b"keep"
    );
    f.binding.data_directory = f.root.join("link");
    symlink(f.root.join("data"), &f.binding.data_directory).unwrap();
    assert!(data_precondition(&f.binding, false).is_err());
    f.binding.data_directory = f.root.join("missing-parent/data");
    assert!(data_precondition(&f.binding, false).is_err());
    assert!(!f.root.join("missing-parent").exists());
}
#[test]
fn resume_requires_initializing_same_identity_and_existing_ledger() {
    let f = Fixture::new();
    f.engine_journal("INITIALIZING");
    assert!(data_precondition(&f.binding, true).is_err());
    put(
        &f.binding.data_directory.join("ledger/CURRENT"),
        b"MANIFEST-00001\n",
    );
    assert!(data_precondition(&f.binding, true).is_ok());
    f.engine_journal("INITIALIZED");
    let before = fs::read(f.binding.data_directory.join("engine-instance.json")).unwrap();
    assert_eq!(
        data_precondition(&f.binding, true).unwrap_err().code,
        "INSTANCE_RESUME_NOT_ELIGIBLE"
    );
    assert_eq!(
        before,
        fs::read(f.binding.data_directory.join("engine-instance.json")).unwrap()
    );
    f.engine_journal("INITIALIZING");
    let path = f.binding.data_directory.join("engine-instance.json");
    let mut journal: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    journal["chainFingerprint"] = json!("f".repeat(64));
    put(&path, &serde_json::to_vec(&journal).unwrap());
    assert!(data_precondition(&f.binding, true).is_err());
}
#[test]
fn input_and_trust_paths_cannot_overlap_control_data_or_package() {
    let mut f = Fixture::new();
    assert!(disjoint(f.control.path(), &f.binding).is_ok());
    f.binding.lock = f.binding.package.join("docs/lock.json");
    assert_eq!(
        disjoint(f.control.path(), &f.binding).unwrap_err().code,
        "INSTANCE_TRUST_PATH_OVERLAP"
    );
    f.binding.lock = f.root.join("lock");
    f.binding.data_directory = f.control.path().join("data");
    assert!(disjoint(f.control.path(), &f.binding).is_err());
    f.binding.data_directory = f.root.join("data");
    f.binding.pins.push(Pin {
        path: f.binding.data_directory.join("key"),
        sha256: "a".repeat(64),
    });
    assert!(disjoint(f.control.path(), &f.binding).is_err());
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod owned {
    use super::*;
    use crate::artifact::{BuildOptions, Manifest, VerifyOptions};

    // Only static test bytes: no engine, JVM, native library, key parser or
    // network is executed. A fixture records the already-checked registration
    // boundary so the read-only reconstruction contract can be exercised.
    struct OwnedFixture {
        _temporary: tempfile::TempDir,
        root: PathBuf,
        options: RegisterOptions,
        owner: install::RootIdentity,
        state: State,
        journal: Store,
    }
    impl OwnedFixture {
        fn new() -> Self {
            Self::build(false, false)
        }
        fn managed(signed: bool) -> Self {
            Self::build(true, signed)
        }
        fn build(managed: bool, signed: bool) -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            let identity: Identity = serde_json::from_slice(include_bytes!(
                "../../contracts/nigo/development-clean-2026-09-18/evidence/engine-info.json"
            ))
            .unwrap();
            let stage = root.join("payload");
            let payload = [
                ("bin/bxdl", b"test-only CLI bytes".as_slice()),
                ("engine/nigo-node.jar", b"test-only JAR bytes"),
                ("runtime/bin/java", b"test-only Java bytes"),
                ("runtime/lib/modules", b"test-only JRE module bytes"),
                ("licenses/THIRD_PARTY_NOTICES", b"test-only notices"),
                ("licenses/SBOM.json", b"{}"),
            ];
            for (name, raw) in payload {
                let path = stage.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                put(&path, raw);
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
            manifest.engine.revision = identity.source.commit.clone();
            manifest.engine.contract_revision = identity.contract.fingerprint.clone();
            manifest.engine.jar_sha256 = files::digest(payload[1].1);
            manifest.runtime.java_sha256 = files::digest(payload[2].1);
            let spec = root.join("spec.json");
            put(&spec, &serde_json::to_vec(&manifest).unwrap());
            let archive = root.join("bundle.tar.gz");
            let (signing_key_path, public_key_path) = if signed {
                use ed25519_dalek::{
                    SigningKey,
                    pkcs8::{EncodePrivateKey, EncodePublicKey, spki::der::pem::LineEnding},
                };
                let key = SigningKey::from_bytes(&[0x52; 32]); // Fixed test-only seed.
                let private = root.join("test-signing.pem");
                let public = root.join("test-public.pem");
                put(
                    &private,
                    key.to_pkcs8_pem(LineEnding::LF).unwrap().as_bytes(),
                );
                put(
                    &public,
                    key.verifying_key()
                        .to_public_key_pem(LineEnding::LF)
                        .unwrap()
                        .as_bytes(),
                );
                (Some(private), Some(public))
            } else {
                (None, None)
            };
            artifact::build(&BuildOptions {
                root: stage,
                spec_path: spec,
                output: archive.clone(),
                signing_key_path,
                allow_unsigned_development: !signed,
            })
            .unwrap();
            let trust = VerifyOptions {
                public_key_path: public_key_path.clone(),
                allow_unsigned_development: !signed,
            };
            let verified = artifact::verify(&archive, &trust).unwrap();
            let package = root.join("package");
            install::install(&archive, &package, &trust).unwrap();
            let lock = root.join("engine.lock.json");
            put(&lock, &serde_json::to_vec(&json!({"schemaVersion":1,"jarSha256":files::digest(payload[1].1),"jarSizeBytes":payload[1].1.len(),"javaSha256":files::digest(payload[2].1),"expected":identity})).unwrap());
            put(&root.join("chain.json"), br#"{"nigo.protocol.chain-id":"11578","nigo.protocol.consensus.protocol":"QBFT","nigo.protocol.consensus.profile-id":"TEST_QBFT"}"#);
            let mut product: Value = serde_json::from_str(include_str!(
                "../../config/examples/instance.development.json"
            ))
            .unwrap();
            product["nodeId"] = json!(format!("0x{}", "a".repeat(64)));
            product["chainDescription"] = json!("chain.json");
            product["storage"]["dataDirectory"] = json!("data");
            let prefix = "nigo.protocol.consensus.qbft.node.";
            let mut node = json!({"server.address":"127.0.0.1","server.port":"18080"});
            for (name, value) in [
                ("node-id", format!("0x{}", "a".repeat(64))),
                ("validator-id", format!("0x{}", "b".repeat(40))),
                ("role", "VALIDATOR".into()),
                ("transport-security-scheme", "MTLS".into()),
                ("listen-host", "192.0.2.10".into()),
                ("listen-port", "19090".into()),
            ] {
                node[format!("{prefix}{name}")] = json!(value);
            }
            for (i, (product_name, native_name)) in [
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
                let name = format!("{i}.private");
                put(&root.join(&name), b"OWNERSHIP_TEST_SECRET_CANARY");
                product["secrets"][product_name] = json!(name);
                node[format!("{prefix}{native_name}")] = json!(name);
            }
            let product_path = root.join("product.json");
            put(&product_path, &serde_json::to_vec(&product).unwrap());
            let native = root.join("node.json");
            put(&native, &serde_json::to_vec(&json!({"chainFile":"chain.json","dataDirectory":"data","backend":"rocksdb","node":node})).unwrap());
            let options = RegisterOptions {
                instance: root.join("control"),
                package,
                archive,
                public_key: public_key_path,
                allow_unsigned_development: !signed,
                product: product_path,
                native,
                lock,
                timeout: Duration::from_secs(1),
            };
            let (mut binding, native, _) = registration_inputs(&options).unwrap();
            binding.instance_id = "validator-one".into();
            binding.archive_sha256 = verified.archive_sha256;
            binding.manifest_sha256 = verified.manifest_sha256;
            binding.chain_fingerprint = "c".repeat(64);
            binding.node_identity = format!("0x{}:0x{}", "a".repeat(64), "b".repeat(40));
            let mut owner = None;
            let control = Control::create_owned(&options.instance, &mut |identity| {
                owner = Some(identity);
                Ok(())
            })
            .unwrap();
            if managed {
                supply::retain(&control, &mut binding, &native).unwrap();
                verified_package(&binding, true).unwrap();
            }
            let raw = serde_json::to_vec(&binding).unwrap();
            store::write_new(&control.path().join("binding.json"), &raw).unwrap();
            let state = State {
                schema_version: 1,
                instance_id: binding.instance_id,
                binding_sha256: files::digest(&raw),
                phase: Phase::Registered,
                attempt: None,
                genesis_hash: None,
                reason: "REGISTERED_COLD_CHECKED".into(),
            };
            let mut journal = Store::create(&control.path().join("journal")).unwrap();
            state.save(&mut journal).unwrap();
            Self {
                _temporary: temporary,
                root,
                options,
                owner: owner.unwrap(),
                state,
                journal,
            }
        }
    }

    #[test]
    fn completed_registration_reconstruction_is_read_only_and_binds_request() {
        let mut f = OwnedFixture::new();
        let before = f.journal.read().unwrap();
        let summary = inspect_owned_registration(&f.options, &f.owner).unwrap();
        assert_eq!(summary.initialization, "NOT_STARTED");
        assert_eq!(summary.operation_busy, Some(false));
        assert_eq!(before, f.journal.read().unwrap());
        assert!(!f.root.join("data").exists());
        let output = serde_json::to_string(&summary).unwrap();
        assert!(!output.contains("CANARY"));
        assert!(!output.contains(f.root.to_str().unwrap()));
        let copy = f.root.join("same-product-copy.json");
        put(&copy, &fs::read(&f.options.product).unwrap());
        f.options.product = copy;
        assert!(inspect_owned_registration(&f.options, &f.owner).is_err());
    }

    #[test]
    fn full_binding_observation_preserves_unknown_and_works_while_lock_is_busy() {
        let mut f = OwnedFixture::new();
        f.state.phase = Phase::InitIntent;
        f.state.attempt = Some(Attempt {
            id: "existing-attempt".into(),
            command: "init".into(),
        });
        f.state.reason = "INITIALIZATION_ATTEMPT_UNRESOLVED".into();
        f.state.save(&mut f.journal).unwrap();
        let before = f.journal.read().unwrap();
        let control = Control::open(&f.options.instance).unwrap();
        let _guard = control.lock().unwrap();
        let summary = verify_owned_binding(&f.options, &f.owner).unwrap();
        assert_eq!(summary.initialization, "UNKNOWN");
        assert_eq!(summary.operation_busy, Some(true));
        assert_eq!(summary.attempt_id.as_deref(), Some("existing-attempt"));
        assert!(inspect_owned_registration(&f.options, &f.owner).is_err());
        assert_eq!(before, f.journal.read().unwrap());
    }

    #[test]
    fn owned_registration_rejects_partial_replaced_and_changed_inputs() {
        for case in 0..6 {
            let mut f = OwnedFixture::new();
            match case {
                0 => {
                    fs::remove_file(f.options.instance.join("binding.json")).unwrap();
                }
                1 => {
                    f.owner.inode += 1;
                }
                2 => {
                    put(&f.root.join("0.private"), b"CHANGED_SECRET_CANARY");
                }
                3 => {
                    put(&f.options.package.join("runtime/lib/modules"), b"modified");
                }
                4 => {
                    put(&f.options.archive, b"not the trusted archive");
                }
                _ => {
                    f.options.allow_unsigned_development = false;
                }
            }
            let error = inspect_owned_registration(&f.options, &f.owner)
                .err()
                .expect("must refuse");
            assert!(!error.message.contains("CANARY"));
            assert!(!error.message.contains(f.root.to_str().unwrap()));
        }
    }
    #[test]
    fn managed_signed_and_unsigned_copies_survive_original_supply_removal() {
        for signed in [false, true] {
            let f = OwnedFixture::managed(signed);
            let (original, _, _) = registration_inputs(&f.options).unwrap();
            let expected = supply::source_pins(&original).unwrap();
            let before = f.journal.read().unwrap();
            for path in [&f.options.archive, &f.options.lock]
                .into_iter()
                .chain(f.options.public_key.iter())
            {
                fs::remove_file(path).unwrap();
            }
            let observed = verify_owned_binding_with_package(&f.options, &f.owner, true).unwrap();
            assert_eq!(observed.summary.initialization, "NOT_STARTED");
            assert_eq!(observed.source_pins, expected);
            assert_eq!(
                observed.package.authenticity,
                if signed {
                    "verified-external-ed25519"
                } else {
                    "unsigned-development"
                }
            );
            assert_eq!(before, f.journal.read().unwrap());
            assert!(!f.root.join("data").exists());
            let trust = f.options.instance.join("trust");
            assert_eq!(
                fs::symlink_metadata(&trust).unwrap().permissions().mode() & 0o7777,
                0o700
            );
            for name in ["package.tar.gz", "engine-lock.json"]
                .into_iter()
                .chain(signed.then_some("public-key.pem"))
            {
                assert_eq!(
                    fs::symlink_metadata(trust.join(name))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o7777,
                    0o600
                );
            }
            let control = Control::open(&f.options.instance).unwrap();
            let (binding, _, _, _) = load(&control).unwrap();
            assert_eq!(binding.schema_version, 2);
            assert_eq!(
                binding.options(Duration::from_secs(1)).lock,
                trust.join("engine-lock.json")
            );
            assert!(!binding.pins.iter().any(|p| p.path == f.options.lock));
            let summary = serde_json::to_string(&observed.summary).unwrap();
            assert!(!summary.contains(f.root.to_str().unwrap()));
            assert!(!summary.contains("CANARY"));
        }
    }

    #[test]
    fn managed_supply_does_not_adopt_changed_request_or_operational_credentials() {
        for case in 0..6 {
            let mut f = OwnedFixture::managed(true);
            match case {
                0 => f.options.archive = f.options.instance.join("trust/package.tar.gz"),
                1 => f.options.lock = f.root.join("same-lock-copy.json"),
                2 => f.options.public_key = None,
                3 => f.options.allow_unsigned_development = true,
                4 => put(&f.root.join("0.private"), b"CHANGED_SECRET_CANARY"),
                _ => put(
                    &f.options.package.join("runtime/lib/modules"),
                    b"changed installed runtime",
                ),
            }
            let before = f.journal.read().unwrap();
            let error = verify_owned_binding(&f.options, &f.owner)
                .err()
                .expect("must refuse changed request/live input");
            assert!(!error.message.contains("CANARY"));
            assert!(!error.message.contains(f.root.to_str().unwrap()));
            assert_eq!(before, f.journal.read().unwrap());
            assert!(!f.root.join("data").exists());
        }
    }

    #[test]
    fn managed_copy_tampering_missing_files_links_and_permissions_fail_closed() {
        for case in 0..9 {
            let f = OwnedFixture::managed(true);
            let trust = f.options.instance.join("trust");
            match case {
                0 => put(&trust.join("package.tar.gz"), b"changed archive"),
                1 => put(&trust.join("engine-lock.json"), b"changed lock"),
                2 => put(&trust.join("public-key.pem"), b"changed key"),
                3 => {
                    fs::remove_file(trust.join("engine-lock.json")).unwrap();
                }
                4 => {
                    fs::remove_file(trust.join("public-key.pem")).unwrap();
                    symlink(
                        f.options.public_key.as_ref().unwrap(),
                        trust.join("public-key.pem"),
                    )
                    .unwrap();
                }
                5 => {
                    fs::hard_link(trust.join("package.tar.gz"), f.root.join("hardlink")).unwrap();
                }
                6 => {
                    fs::set_permissions(
                        trust.join("public-key.pem"),
                        fs::Permissions::from_mode(0o644),
                    )
                    .unwrap();
                }
                7 => put(&trust.join("unregistered-copy"), b"unexpected"),
                _ => {
                    fs::set_permissions(&trust, fs::Permissions::from_mode(0o755)).unwrap();
                }
            }
            let before = f.journal.read().unwrap();
            assert!(verify_owned_binding(&f.options, &f.owner).is_err());
            assert_eq!(before, f.journal.read().unwrap());
            assert!(!f.root.join("data").exists());
        }
    }

    #[test]
    fn managed_provenance_hash_and_fixed_layout_are_required() {
        for case in 0..5 {
            let mut f = OwnedFixture::managed(true);
            let control = Control::open(&f.options.instance).unwrap();
            let (mut binding, _, _, _) = load(&control).unwrap();
            match case {
                0 => binding.supply.as_mut().unwrap().lock.sha256 = "f".repeat(64),
                1 => binding.archive = f.options.archive.clone(),
                2 => binding.supply = None,
                3 => binding.pins.push(binding.pins[0].clone()),
                _ => binding.schema_version = 1,
            }
            let raw = serde_json::to_vec(&binding).unwrap();
            put(&control.path().join("binding.json"), &raw);
            f.state.binding_sha256 = files::digest(&raw);
            f.state.save(&mut f.journal).unwrap();
            assert!(verify_owned_binding(&f.options, &f.owner).is_err());
        }
    }

    #[test]
    fn legacy_v1_keeps_original_supply_dependency_without_implicit_upgrade() {
        let f = OwnedFixture::new();
        let before = fs::read(f.options.instance.join("binding.json")).unwrap();
        fs::remove_file(&f.options.archive).unwrap();
        assert!(verify_owned_binding(&f.options, &f.owner).is_err());
        assert_eq!(
            before,
            fs::read(f.options.instance.join("binding.json")).unwrap()
        );
        assert!(!f.options.instance.join("trust").exists());
        assert_eq!(
            show(&f.options.instance).unwrap().initialization,
            "NOT_STARTED"
        );
    }

    #[test]
    fn interrupted_supply_copy_is_preserved_and_cannot_be_adopted_or_overwritten() {
        let f = OwnedFixture::new();
        let instance = f.root.join("incomplete");
        let mut owner = None;
        let control = Control::create_owned(&instance, &mut |identity| {
            owner = Some(identity);
            Ok(())
        })
        .unwrap();
        let incorrect = "f".repeat(64);
        let lock_hash = files::digest(&fs::read(&f.options.lock).unwrap());
        let inputs = [
            (
                "package.tar.gz",
                f.options.archive.as_path(),
                incorrect.as_str(),
                artifact::MAX_ARCHIVE_BYTES,
            ),
            (
                "engine-lock.json",
                f.options.lock.as_path(),
                lock_hash.as_str(),
                65_536,
            ),
        ];
        assert!(control.retain_supply(&inputs).is_err());
        assert!(instance.join("trust/package.tar.gz").is_file());
        assert!(!instance.join("binding.json").exists());
        assert!(!instance.join("journal").exists());
        let before = fs::read(instance.join("trust/package.tar.gz")).unwrap();
        assert!(control.retain_supply(&inputs).is_err());
        assert_eq!(
            before,
            fs::read(instance.join("trust/package.tar.gz")).unwrap()
        );
        assert!(show(&instance).is_err());
        let mut options = f.options;
        options.instance = instance;
        assert!(inspect_owned_registration(&options, &owner.unwrap()).is_err());
    }
    #[test]
    fn managed_observation_preserves_unknown_attempt_and_busy_lock_without_sources() {
        let mut f = OwnedFixture::managed(true);
        for path in [&f.options.archive, &f.options.lock]
            .into_iter()
            .chain(f.options.public_key.iter())
        {
            fs::remove_file(path).unwrap();
        }
        f.state.phase = Phase::InitIntent;
        f.state.attempt = Some(Attempt {
            id: "managed-unresolved".into(),
            command: "init".into(),
        });
        f.state.reason = "INITIALIZATION_ATTEMPT_UNRESOLVED".into();
        f.state.save(&mut f.journal).unwrap();
        let before = f.journal.read().unwrap();
        let root = Control::open(&f.options.instance).unwrap();
        let _guard = root.lock().unwrap();
        let catalog = catalog_record(&f.options.instance).unwrap();
        assert_eq!(catalog.schema_version, 2);
        assert_eq!(catalog.initialization, "UNKNOWN");
        assert_eq!(catalog.cli_match, Some(false));
        let observed = verify_owned_binding_with_package(&f.options, &f.owner, false).unwrap();
        assert_eq!(observed.summary.initialization, "UNKNOWN");
        assert_eq!(observed.summary.operation_busy, Some(true));
        assert_eq!(
            observed.summary.attempt_id.as_deref(),
            Some("managed-unresolved")
        );
        assert!(inspect_owned_registration(&f.options, &f.owner).is_err());
        assert_eq!(before, f.journal.read().unwrap());
        assert!(!f.root.join("data").exists());
        check_catalog_location(&f.options.instance, &f.root.join("console/catalog")).unwrap();
        // Original supply locations are no longer operational references. A
        // later symlink there cannot block metadata linkage or redirect writes.
        for path in [&f.options.archive, &f.options.lock]
            .into_iter()
            .chain(f.options.public_key.iter())
        {
            symlink(&f.root, path).unwrap();
        }
        check_catalog_location(&f.options.instance, &f.root.join("console/catalog")).unwrap();
        assert_eq!(
            catalog_record(&f.options.instance).unwrap().schema_version,
            2
        );
        assert_eq!(before, f.journal.read().unwrap());
    }
}

#[test]
fn catalog_metadata_does_not_take_operation_lock_or_change_uncertain_record() {
    let mut f = Fixture::new();
    f.pending();
    let before = f.journal.read().unwrap();
    let _guard = f.control.lock().unwrap();
    let observed = catalog_record(f.control.path()).unwrap();
    assert_eq!(observed.schema_version, 1);
    assert_eq!(observed.instance_id, "test-validator");
    assert_eq!(observed.initialization, "UNKNOWN");
    assert_eq!(observed.cli_match, None);
    assert_eq!(before, f.journal.read().unwrap());
    assert!(!f.binding.data_directory.exists());
}

#[test]
fn catalog_cli_comparison_requires_exact_bytes_without_execution() {
    let f = Fixture::new();
    fs::create_dir_all(f.binding.package.join("bin")).unwrap();
    let cli = f.binding.package.join("bin/bxdl");
    put(&cli, b"must never execute these bytes");
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        catalog_record(f.control.path()).unwrap().cli_match,
        Some(false)
    );
    fs::copy(std::env::current_exe().unwrap(), &cli).unwrap();
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        catalog_record(f.control.path()).unwrap().cli_match,
        Some(true)
    );
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(catalog_record(f.control.path()).unwrap().cli_match, None);
    assert!(!f.binding.data_directory.exists());
}

#[test]
fn catalog_write_location_cannot_enter_instance_data_package_or_input_paths() {
    let f = Fixture::new();
    let before = f.journal.read().unwrap();
    for protected in [
        &f.binding.data_directory,
        &f.binding.package,
        &f.binding.archive,
        &f.binding.product,
        &f.binding.native,
        &f.binding.lock,
    ] {
        assert!(
            check_catalog_location(f.control.path(), &protected.join("console/catalog")).is_err()
        );
    }
    check_catalog_location(f.control.path(), &f.root.join("console/catalog")).unwrap();
    assert_eq!(before, f.journal.read().unwrap());
    assert!(!f.binding.data_directory.exists());
    assert!(!f.binding.package.exists());
}
