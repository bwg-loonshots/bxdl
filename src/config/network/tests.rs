use super::*;
use std::os::unix::fs::{PermissionsExt as _, symlink};
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    product: PathBuf,
    network: PathBuf,
    chain: PathBuf,
}
fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn fixture(count: usize) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let secrets = root.join("secrets");
    fs::create_dir(&secrets).unwrap();
    fs::set_permissions(&secrets, fs::Permissions::from_mode(0o700)).unwrap();
    for name in [
        "validator",
        "validator-password",
        "tls",
        "tls-password",
        "trust",
        "trust-password",
    ] {
        let path = secrets.join(name);
        fs::write(&path, b"deliberately not a credential").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let product = root.join("instance.json");
    let network = root.join("network.json");
    let chain = root.join("chain.json");
    write(
        &product,
        &serde_json::json!({"schemaVersion":1,"instanceId":"one","role":"validator","nodeId":format!("0x{:064x}",1),
        "chainDescription":"chain.json","storage":{"backend":"rocksdb","dataDirectory":"data"},
        "secrets":{"validatorKeystore":"secrets/validator","validatorPasswordFile":"secrets/validator-password",
        "tlsKeyStore":"secrets/tls","tlsKeyPasswordFile":"secrets/tls-password","tlsTrustStore":"secrets/trust","tlsTrustPasswordFile":"secrets/trust-password"},
        "http":{"address":"127.0.0.1","port":18080},"p2p":{"address":"127.0.0.1","port":30303}}),
    );
    let mut chain_values = serde_json::json!({"nigo.protocol.chain-id":"11578","nigo.protocol.consensus.profile-id":"TEST_QBFT",
        "nigo.protocol.consensus.protocol":"QBFT","nigo.protocol.native-assets.registration-mode":"ALLOWLISTED"});
    let mut nodes = Vec::new();
    for n in 0..count {
        let validator = format!("0x{:040x}", n + 1);
        chain_values[format!("{MEMBERS}[{n}]")] = validator.clone().into();
        nodes.push(serde_json::json!({"nodeId":format!("0x{:064x}",n+1),"validatorId":validator,"host":"127.0.0.1","port":30303+n,
            "transactionPermission":"RELAY","credentialFingerprints":[format!("0x{:064x}",100+n)]}));
    }
    write(&chain, &chain_values);
    write(
        &network,
        &serde_json::json!({"schemaVersion":1,"kind":"BXDL_QBFT_NETWORK","nodes":nodes}),
    );
    Fixture {
        _temp: temp,
        root,
        product,
        network,
        chain,
    }
}
fn mutate(path: &Path, edit: impl FnOnce(&mut Value)) {
    let mut v = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    edit(&mut v);
    write(path, &v);
}
fn prepared(f: &Fixture) -> Prepared {
    load(&f.product, &f.network, RuntimeOptions::default()).unwrap()
}
fn rejected(f: &Fixture) {
    assert!(load(&f.product, &f.network, RuntimeOptions::default()).is_err());
}
#[test]
fn four_validators_render_exact_flat_keys_without_reading_credentials() {
    let f = fixture(4);
    let p = prepared(&f);
    let native: Value = serde_json::from_slice(&p.native_bytes).unwrap();
    assert_eq!(native["chainFile"], f.chain.to_str().unwrap());
    assert_eq!(native["backend"], "rocksdb");
    let n = &native["node"];
    assert_eq!(n[format!("{PREFIX}role")], "VALIDATOR");
    assert_eq!(
        n[format!("{PREFIX}peers[0].validator-id")],
        format!("0x{:040x}", 2)
    );
    assert_eq!(n["nigo.transaction-ingress.permission"], "RELAY");
    assert_eq!(n["nigo.protocol.consensus.auto-start"], true);
    assert_eq!(n["nigo.protocol.consensus.qbft.sync.enabled"], false);
    assert_eq!(n["nigo.storage.gc.automatic.enabled"], false);
    assert_eq!(p.summary.peer_count, 3);
    assert_eq!(p.summary.engine_configuration, "NOT_CHECKED");
    assert!(
        n.as_object()
            .unwrap()
            .values()
            .all(|v| !v.is_array() && !v.is_object())
    );
    assert_eq!(p.input_pins.len(), 3);
    assert_eq!(p.referenced_paths.len(), 10);
    assert!(!f.root.join("data").exists());
    let report = serde_json::to_string(&p.summary).unwrap();
    assert!(!report.contains(f.root.to_str().unwrap()));
    assert!(!report.contains("deliberately"));
}
#[test]
fn join_options_are_explicit_and_single_validator_has_no_peer_properties() {
    let f = fixture(1);
    let options = RuntimeOptions {
        mode: NetworkMode::JoinNetwork,
        key_store_type: StoreType::Jks,
        trust_store_type: StoreType::Jks,
        consensus_auto_start: false,
        archive_enabled: true,
    };
    let p = load(&f.product, &f.network, options).unwrap();
    let native: Value = serde_json::from_slice(&p.native_bytes).unwrap();
    let n = &native["node"];
    assert_eq!(n["nigo.protocol.consensus.qbft.sync.enabled"], true);
    assert_eq!(n["nigo.protocol.consensus.qbft.sync.archive.enabled"], true);
    assert_eq!(
        n["nigo.protocol.consensus.qbft.sync.trust-mode"],
        "GENESIS_VERIFIED"
    );
    assert_eq!(n["nigo.protocol.consensus.auto-start"], false);
    assert_eq!(n[format!("{PREFIX}mtls-key-store-type")], "JKS");
    assert!(!n.as_object().unwrap().keys().any(|k| k.contains("peers[")));
}
#[test]
fn public_order_does_not_change_rendered_bytes() {
    let f = fixture(3);
    let expected = prepared(&f).native_bytes;
    mutate(&f.network, |v| v["nodes"].as_array_mut().unwrap().reverse());
    assert_eq!(prepared(&f).native_bytes, expected);
}
#[test]
fn membership_missing_extra_duplicate_and_noncontiguous_are_rejected() {
    for variant in 0..4 {
        let f = fixture(2);
        match variant {
            0 => mutate(&f.network, |v| {
                v["nodes"].as_array_mut().unwrap().pop();
            }),
            1 => mutate(&f.chain, |v| {
                v.as_object_mut().unwrap().remove(&format!("{MEMBERS}[1]"));
            }),
            2 => mutate(&f.chain, |v| {
                v[format!("{MEMBERS}[1]")] = v[format!("{MEMBERS}[0]")].clone();
            }),
            _ => mutate(&f.chain, |v| {
                let x = v
                    .as_object_mut()
                    .unwrap()
                    .remove(&format!("{MEMBERS}[1]"))
                    .unwrap();
                v[format!("{MEMBERS}[2]")] = x;
            }),
        }
        rejected(&f);
    }
}
#[test]
fn duplicate_id_endpoint_pin_and_revocation_overlap_are_rejected() {
    for field in ["nodeId", "validatorId", "port", "credentialFingerprints"] {
        let f = fixture(2);
        mutate(&f.network, |v| {
            v["nodes"][1][field] = v["nodes"][0][field].clone()
        });
        rejected(&f);
    }
    let f = fixture(2);
    mutate(&f.network, |v| {
        v["nodes"][1]["revokedCredentialFingerprints"] =
            v["nodes"][1]["credentialFingerprints"].clone()
    });
    rejected(&f);
}
#[test]
fn endpoint_identity_canonicalizes_ip_and_dns_without_lookups() {
    let f = fixture(2);
    mutate(&f.network, |v| {
        v["nodes"][1]["host"] = "::ffff:127.0.0.1".into();
        v["nodes"][1]["port"] = 30303.into();
    });
    rejected(&f);
    let f = fixture(3);
    mutate(&f.network, |v| {
        v["nodes"][1]["host"] = "Node.Example.".into();
        v["nodes"][2]["host"] = "node.example".into();
        v["nodes"][2]["port"] = 30304.into();
    });
    rejected(&f);
}
#[test]
fn malformed_public_inputs_and_local_binding_are_rejected() {
    for field in ["nodeId", "host", "transactionPermission"] {
        let f = fixture(1);
        mutate(&f.network, |v| {
            v["nodes"][0][field] = "invalid value".into()
        });
        rejected(&f);
    }
    let f = fixture(1);
    mutate(&f.network, |v| v["nodes"][0]["port"] = 30304.into());
    rejected(&f);
    let f = fixture(1);
    mutate(&f.network, |v| {
        v["nodes"][0]["credentialFingerprints"] = serde_json::json!([])
    });
    rejected(&f);
    let f = fixture(1);
    mutate(&f.network, |v| v["nodes"][0]["role"] = "OBSERVER".into());
    rejected(&f);
    let f = fixture(1);
    let raw = fs::read_to_string(&f.network).unwrap().replacen(
        "\"schemaVersion\":1",
        "\"schemaVersion\":1,\"schemaVersion\":1",
        1,
    );
    fs::write(&f.network, raw).unwrap();
    rejected(&f);
}
#[test]
fn document_pin_rejects_same_byte_replacement_and_serializes_for_resume() {
    let f = fixture(1);
    let p = prepared(&f);
    let pins: Vec<InputPin> =
        serde_json::from_slice(&serde_json::to_vec(&p.input_pins).unwrap()).unwrap();
    verify_pins(&pins).unwrap();
    let raw = fs::read(&f.chain).unwrap();
    fs::rename(&f.chain, f.root.join("old-chain.json")).unwrap();
    fs::write(&f.chain, raw).unwrap();
    assert_eq!(
        verify_pins(&pins).unwrap_err().code,
        "NETWORK_INPUT_CHANGED"
    );
    assert!(p.recheck().is_err());
}
#[test]
fn symlinks_secret_permissions_and_data_overlap_are_rejected() {
    let f = fixture(1);
    let original = f.root.join("original.json");
    fs::rename(&f.network, &original).unwrap();
    symlink(&original, &f.network).unwrap();
    rejected(&f);
    let f = fixture(1);
    fs::set_permissions(
        f.root.join("secrets/tls"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    rejected(&f);
    let f = fixture(1);
    mutate(&f.product, |v| {
        v["storage"]["dataDirectory"] = "secrets".into()
    });
    rejected(&f);
}
#[test]
fn prepared_recheck_detects_secret_metadata_change_without_reading_content() {
    let f = fixture(1);
    let p = prepared(&f);
    fs::write(f.root.join("secrets/tls"), b"changed opaque contents").unwrap();
    assert!(p.recheck().is_err());
}
fn der_fixture() -> Vec<u8> {
    // Structurally certificate-shaped but deliberately not semantically valid X.509.
    // The helper must report NOT_CHECKED, never certificate validation success.
    vec![
        0x30, 43, 0x30, 32, 0x02, 1, 1, 0x30, 3, 0x06, 1, 42, 0x30, 0, 0x30, 6, 0x17, 1, b'0',
        0x17, 1, b'1', 0x30, 0, 0x30, 10, 0x30, 3, 0x06, 1, 42, 0x03, 3, 0, 1, 2, 0x30, 3, 0x06, 1,
        42, 0x03, 2, 0, 1,
    ]
}
#[test]
fn der_and_pem_compute_same_domain_separated_pin_with_no_validation_claim() {
    let f = fixture(1);
    let der = f.root.join("public.der");
    let pem = f.root.join("public.pem");
    let bytes = der_fixture();
    fs::write(&der, &bytes).unwrap();
    fs::write(
        &pem,
        format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
            STANDARD.encode(&bytes)
        ),
    )
    .unwrap();
    let a = fingerprint(&der).unwrap();
    let b = fingerprint(&pem).unwrap();
    assert_eq!(a.credential_fingerprint, b.credential_fingerprint);
    let mut hash = Sha256::new();
    hash.update(b"NIGO_TRANSPORT_CREDENTIAL/v1\0MTLS\0");
    hash.update(bytes);
    assert_eq!(
        a.credential_fingerprint,
        format!("0x{}", hex::encode(hash.finalize()))
    );
    assert_eq!(a.certificate_validation, "NOT_CHECKED");
    assert_ne!(
        a.credential_fingerprint,
        format!("0x{}", hex::encode(Sha256::digest(fs::read(der).unwrap())))
    );
}
#[test]
fn fingerprint_rejects_private_key_labels_multiple_certs_and_noncert_der() {
    let f = fixture(1);
    let path = f.root.join("public.pem");
    let encoded = STANDARD.encode(der_fixture());
    for text in [
        format!("-----BEGIN PRIVATE KEY-----\n{encoded}\n-----END PRIVATE KEY-----"),
        format!(
            "-----BEGIN CERTIFICATE-----\n{encoded}\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\n{encoded}\n-----END CERTIFICATE-----"
        ),
        format!("-----BEGIN CERTIFICATE-----\n{encoded}\n-----END CERTIFICATE----- extra"),
    ] {
        fs::write(&path, text).unwrap();
        assert!(fingerprint(&path).is_err());
    }
    for bytes in [
        vec![0x30, 3, 2, 1, 1],
        {
            let mut b = der_fixture();
            b.push(0);
            b
        },
        {
            let mut b = der_fixture();
            let n = b.len();
            b[n - 2] = 8;
            b
        },
    ] {
        fs::write(&path, bytes).unwrap();
        assert!(fingerprint(&path).is_err());
    }
}

thread_local! {
    static CONTENT_READS: std::cell::RefCell<Option<Vec<PathBuf>>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn record_content_read(path: &Path) {
    CONTENT_READS.with(|reads| {
        if let Some(paths) = reads.borrow_mut().as_mut() {
            paths.push(path.to_path_buf());
        }
    });
}
fn observe_reads(action: impl FnOnce()) -> Vec<PathBuf> {
    CONTENT_READS.with(|reads| *reads.borrow_mut() = Some(Vec::new()));
    action();
    CONTENT_READS.with(|reads| reads.borrow_mut().take().unwrap())
}
#[test]
fn public_network_aliases_of_secret_or_data_are_rejected_before_content_read() {
    for variant in 0..3 {
        let f = fixture(1);
        let candidate = match variant {
            0 => f.root.join("secrets/validator-password"),
            1 => {
                fs::remove_file(&f.network).unwrap();
                fs::hard_link(f.root.join("secrets/tls"), &f.network).unwrap();
                f.network.clone()
            }
            _ => {
                let data = f.root.join("data");
                fs::create_dir(&data).unwrap();
                fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
                let file = data.join("ledger.bin");
                fs::write(&file, b"opaque database bytes").unwrap();
                file
            }
        };
        let reads = observe_reads(|| {
            let error = load(&f.product, &candidate, RuntimeOptions::default())
                .err()
                .unwrap();
            assert_eq!(error.message, input_conflict().message);
        });
        assert_eq!(reads, vec![f.product]);
    }
}
#[test]
fn chain_alias_of_secret_is_rejected_before_network_or_chain_read() {
    let f = fixture(1);
    mutate(&f.product, |value| {
        value["chainDescription"] = "secrets/tls".into();
    });
    let reads = observe_reads(|| {
        let error = load(&f.product, &f.network, RuntimeOptions::default())
            .err()
            .unwrap();
        assert_eq!(error.message, input_conflict().message);
    });
    assert_eq!(reads, vec![f.product]);
}
#[test]
fn rechecking_replaced_public_pin_never_reads_new_credential_contents() {
    let f = fixture(1);
    let p = prepared(&f);
    fs::remove_file(&f.network).unwrap();
    fs::hard_link(f.root.join("secrets/tls"), &f.network).unwrap();
    let reads = observe_reads(|| {
        assert_eq!(
            verify_pins(&p.input_pins).unwrap_err().code,
            "NETWORK_INPUT_CHANGED"
        );
    });
    assert!(reads.is_empty());
}
