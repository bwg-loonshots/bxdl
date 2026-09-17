//! End-to-end CLI boundaries with fake engine/runtime bytes and throwaway keys.
use bxdl::cli;
use ed25519_dalek::{
    SigningKey,
    pkcs8::{EncodePrivateKey, EncodePublicKey},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn write(path: &Path, content: &[u8], mode: u32) {
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn run(args: Vec<String>) -> (Value, i32) {
    let mut args = args;
    args.push("--json".into());
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = cli::run(&args, &mut out, &mut err);
    assert!(err.is_empty());
    let r = serde_json::from_slice(&out).expect("one complete JSON envelope");
    (r, code)
}
fn s(path: &Path) -> String {
    path.to_str().unwrap().into()
}

#[test]
fn signed_mac_package_build_verify_wrong_key_tampering_and_output_conflict() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let stage = root.join("stage");
    for dir in ["bin", "engine", "runtime/bin", "licenses"] {
        fs::create_dir_all(stage.join(dir)).unwrap();
    }
    write(&stage.join("bin/bxdl"), b"fake CLI", 0o755);
    write(
        &stage.join("engine/nigo-node.jar"),
        b"fake engine bytes",
        0o644,
    );
    write(
        &stage.join("runtime/bin/java"),
        b"fake runtime bytes",
        0o755,
    );
    write(
        &stage.join("licenses/THIRD_PARTY_NOTICES"),
        b"test fixture notices",
        0o644,
    );
    write(&stage.join("licenses/SBOM.json"), b"{}", 0o644);
    let spec = json!({"schemaVersion":1,"kind":"bxdl-package","product":{"name":"BXDL","version":"0.1.0-dev","revision":"development"},"channel":"development","platform":{"os":"darwin","arch":"arm64","libc":"none","minGlibc":"none","javaMajor":21,"backend":"rocksdb"},"engine":{"revision":"a".repeat(40),"jarSha256":hash(b"fake engine bytes"),"contractStatus":"proposed","contractRevision":"fixture-v1"},"runtime":{"vendor":"test-only","version":"21-test","javaSha256":hash(b"fake runtime bytes")}});
    let spec_path = root.join("spec.json");
    write(
        &spec_path,
        serde_json::to_string(&spec).unwrap().as_bytes(),
        0o644,
    );
    // Deterministic test material only: never a distribution signing key.
    let key = SigningKey::from_bytes(&[73; 32]);
    let private = root.join("test-private.pem");
    let public = root.join("test-public.pem");
    write(
        &private,
        key.to_pkcs8_pem(Default::default()).unwrap().as_bytes(),
        0o600,
    );
    write(
        &public,
        key.verifying_key()
            .to_public_key_pem(Default::default())
            .unwrap()
            .as_bytes(),
        0o644,
    );
    let output = root.join("development.tar.gz");
    let build = vec![
        "package".into(),
        "build".into(),
        "--root".into(),
        s(&stage),
        "--spec".into(),
        s(&spec_path),
        "--output".into(),
        s(&output),
        "--signing-key".into(),
        s(&private),
    ];
    let (built, exit) = run(build.clone());
    assert_eq!(exit, 0, "{built}");
    assert_eq!(built["data"]["authenticity"], "verified-external-ed25519");
    assert_eq!(built["data"]["manifest"]["platform"]["os"], "darwin");
    let original = fs::read(&output).unwrap();
    let (collision, exit) = run(build);
    assert_eq!(exit, 3);
    assert_eq!(collision["outcome"], "FAILED");
    assert_eq!(fs::read(&output).unwrap(), original);
    let verify = vec![
        "package".into(),
        "verify".into(),
        s(&output),
        "--public-key".into(),
        s(&public),
    ];
    let (verified, exit) = run(verify.clone());
    assert_eq!(exit, 0, "{verified}");
    assert_eq!(
        verified["data"]["archiveSha256"],
        built["data"]["archiveSha256"]
    );
    let other = SigningKey::from_bytes(&[74; 32]);
    write(
        &public,
        other
            .verifying_key()
            .to_public_key_pem(Default::default())
            .unwrap()
            .as_bytes(),
        0o644,
    );
    let (wrong, exit) = run(verify.clone());
    assert_eq!(exit, 3);
    assert_eq!(wrong["reasonCode"], "SIGNATURE_INVALID");
    write(
        &public,
        key.verifying_key()
            .to_public_key_pem(Default::default())
            .unwrap()
            .as_bytes(),
        0o644,
    );
    use std::io::{Read, Write};
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(original.as_slice())
        .read_to_end(&mut raw)
        .unwrap();
    let needle = b"fake engine bytes";
    let at = raw.windows(needle.len()).position(|w| w == needle).unwrap();
    raw[at] ^= 1;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&raw).unwrap();
    write(&output, &encoder.finish().unwrap(), 0o644);
    let (tampered, exit) = run(verify);
    assert_eq!(exit, 3);
    assert_eq!(tampered["reasonCode"], "HASH_MISMATCH");
    for value in [built, collision, verified, wrong, tampered] {
        let text = value.to_string();
        assert!(!text.contains(&s(&private)));
        assert!(!text.contains("BEGIN PRIVATE KEY"));
    }
}

#[test]
fn config_preflight_incomplete_then_permission_failure_are_secret_free() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(root.join("data")).unwrap();
    fs::set_permissions(root.join("data"), fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(root.join("secrets")).unwrap();
    fs::set_permissions(root.join("secrets"), fs::Permissions::from_mode(0o700)).unwrap();
    write(&root.join("chain.json"), b"{}", 0o644);
    let canary = "PRIVATE-SECRET-CONTENT-CANARY";
    for file in [
        "v.p12",
        "v-password",
        "tls.p12",
        "tls-password",
        "trust.p12",
        "trust-password",
    ] {
        write(&root.join("secrets").join(file), canary.as_bytes(), 0o600);
    }
    let cfg = json!({"schemaVersion":1,"instanceId":"node-a","role":"validator","nodeId":"node-a","chainDescription":"chain.json","storage":{"backend":"rocksdb","dataDirectory":"data"},"secrets":{"validatorKeystore":"secrets/v.p12","validatorPasswordFile":"secrets/v-password","tlsKeyStore":"secrets/tls.p12","tlsKeyPasswordFile":"secrets/tls-password","tlsTrustStore":"secrets/trust.p12","tlsTrustPasswordFile":"secrets/trust-password"},"http":{"address":"127.0.0.1","port":18080},"p2p":{"address":"127.0.0.1","port":19090}});
    let path = root.join("instance.json");
    write(
        &path,
        serde_json::to_string_pretty(&cfg).unwrap().as_bytes(),
        0o600,
    );
    let (valid, exit) = run(vec![
        "config".into(),
        "validate".into(),
        "--file".into(),
        s(&path),
    ]);
    assert_eq!(exit, 0, "{valid}");
    let command = vec!["preflight".into(), "--config".into(), s(&path)];
    let (partial, exit) = run(command.clone());
    assert_eq!(exit, 5, "{partial}");
    assert_eq!(partial["outcome"], "INCOMPLETE");
    fs::set_permissions(
        root.join("secrets/v-password"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let (failed, exit) = run(command);
    assert_eq!(exit, 4, "{failed}");
    assert_eq!(failed["reasonCode"], "LOCAL_CHECK_FAILED");
    for value in [valid, partial, failed] {
        let text = value.to_string();
        for sensitive in [canary, "secrets/v-password", &s(&root)] {
            assert!(!text.contains(sensitive));
        }
    }
}
