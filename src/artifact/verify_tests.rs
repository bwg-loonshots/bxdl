use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{
    Signer, SigningKey,
    pkcs8::{EncodePrivateKey, EncodePublicKey, spki::der::pem::LineEnding},
};
use flate2::{Compression, write::GzEncoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};
use tempfile::TempDir;

fn hash(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}
fn fixture() -> (Manifest, BTreeMap<String, Vec<u8>>) {
    let files: BTreeMap<_, _> = [
        ("bin/bxdl", b"fixture launcher".as_slice()),
        ("engine/nigo-node.jar", b"fake engine, not executable"),
        ("runtime/bin/java", b"fake Java, not executable"),
        ("licenses/THIRD_PARTY_NOTICES", b"fixture attribution"),
        ("licenses/SBOM.json", br#"{"fixture":true}"#),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_vec()))
    .collect();
    let m = Manifest {
        schema_version: 1,
        kind: "bxdl-package".into(),
        product: Product {
            name: "BXDL".into(),
            version: "0.0.0-dev".into(),
            revision: "development".into(),
        },
        channel: "development".into(),
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
            jar_sha256: hash(&files["engine/nigo-node.jar"]),
            contract_status: "proposed".into(),
            contract_revision: "fixture-v1".into(),
        },
        runtime: Runtime {
            vendor: "fixture".into(),
            version: "21-fixture".into(),
            java_sha256: hash(&files["runtime/bin/java"]),
        },
        files: files
            .iter()
            .map(|(p, b)| FileEntry {
                path: p.clone(),
                size: b.len() as u64,
                sha256: hash(b),
                mode: if p == "bin/bxdl" || p == "runtime/bin/java" {
                    0o755
                } else {
                    0o644
                },
            })
            .collect(),
    };
    (m, files)
}
fn keys(seed: u8) -> (SigningKey, TempDir, PathBuf) {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("public.pem");
    fs::write(
        &path,
        key.verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .unwrap(),
    )
    .unwrap();
    (key, dir, path)
}
#[derive(Clone)]
struct Entry {
    name: String,
    mode: u32,
    kind: u8,
    link: String,
    data: Vec<u8>,
}
fn entries(
    m: &Manifest,
    files: &BTreeMap<String, Vec<u8>>,
    key: Option<&SigningKey>,
) -> Vec<Entry> {
    let raw = serde_json::to_vec(m).unwrap();
    let mut out = vec![Entry {
        name: "manifest.json".into(),
        mode: 0o644,
        kind: b'0',
        link: String::new(),
        data: raw.clone(),
    }];
    if let Some(key) = key {
        out.push(Entry {
            name: "manifest.sig".into(),
            mode: 0o644,
            kind: b'0',
            link: String::new(),
            data: STANDARD.encode(key.sign(&raw).to_bytes()).into_bytes(),
        });
    }
    out.extend(m.files.iter().map(|f| Entry {
        name: f.path.clone(),
        mode: f.mode,
        kind: b'0',
        link: String::new(),
        data: files[&f.path].clone(),
    }));
    out
}
fn octal(field: &mut [u8], value: u64) {
    let v = format!("{:0width$o}", value, width = field.len() - 1);
    assert!(v.len() < field.len());
    field.fill(0);
    field[..v.len()].copy_from_slice(v.as_bytes());
}
fn checksum(header: &mut [u8]) {
    header[148..156].fill(b' ');
    let sum: u64 = header.iter().map(|v| u64::from(*v)).sum();
    let value = format!("{sum:06o}\0 ");
    header[148..156].copy_from_slice(value.as_bytes());
}
fn tar(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in entries {
        let mut h = [0_u8; 512];
        assert!(e.name.len() <= 100);
        h[..e.name.len()].copy_from_slice(e.name.as_bytes());
        octal(&mut h[100..108], u64::from(e.mode));
        octal(&mut h[108..116], 0);
        octal(&mut h[116..124], 0);
        octal(&mut h[124..136], e.data.len() as u64);
        octal(&mut h[136..148], 0);
        h[156] = e.kind;
        h[157..157 + e.link.len()].copy_from_slice(e.link.as_bytes());
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        checksum(&mut h);
        out.extend_from_slice(&h);
        out.extend_from_slice(&e.data);
        out.resize(out.len() + (512 - e.data.len() % 512) % 512, 0);
    }
    out.extend_from_slice(&[0; 1024]);
    out
}
fn gzip(raw: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(raw).unwrap();
    encoder.finish().unwrap()
}
fn package(bytes: &[u8]) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("package.tar.gz");
    fs::write(&path, bytes).unwrap();
    (dir, path)
}
fn unsigned() -> VerifyOptions {
    VerifyOptions {
        allow_unsigned_development: true,
        ..Default::default()
    }
}
fn check_unsigned(raw: &[u8]) -> crate::error::Result<Report> {
    let (_dir, path) = package(&gzip(raw));
    verify(&path, &unsigned())
}

#[test]
fn verifier_signed_external_trust_and_identity() {
    let (m, files) = fixture();
    let (key, _kd, path) = keys(1);
    let b = gzip(&tar(&entries(&m, &files, Some(&key))));
    let (_dir, archive) = package(&b);
    let report = verify(
        &archive,
        &VerifyOptions {
            public_key_path: Some(path),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(report.authenticity, "verified-external-ed25519");
    assert_eq!(report.files_verified, files.len());
    assert_eq!(report.archive_sha256, hash(&b));
    assert!(report.bytes_verified > 0);
    let (_wrong, _wd, wrong) = keys(2);
    for options in [
        VerifyOptions::default(),
        unsigned(),
        VerifyOptions {
            public_key_path: Some(wrong),
            allow_unsigned_development: true,
        },
    ] {
        assert!(verify(&archive, &options).is_err());
    }
}
#[test]
fn verifier_signature_covers_exact_manifest_bytes() {
    let (m, files) = fixture();
    let (key, _kd, path) = keys(3);
    let mut e = entries(&m, &files, Some(&key));
    e[0].data.push(b'\n');
    let (_dir, archive) = package(&gzip(&tar(&e)));
    assert_eq!(
        verify(
            &archive,
            &VerifyOptions {
                public_key_path: Some(path),
                allow_unsigned_development: true
            }
        )
        .unwrap_err()
        .code,
        "SIGNATURE_INVALID"
    );
}
#[test]
fn verifier_unsigned_is_explicit_development_only() {
    let (mut m, files) = fixture();
    let (_dir, path) = package(&gzip(&tar(&entries(&m, &files, None))));
    assert!(verify(&path, &VerifyOptions::default()).is_err());
    assert_eq!(
        verify(&path, &unsigned()).unwrap().authenticity,
        "unsigned-development"
    );
    let (_key, _kd, pubkey) = keys(4);
    assert!(
        verify(
            &path,
            &VerifyOptions {
                public_key_path: Some(pubkey),
                allow_unsigned_development: true
            }
        )
        .is_err()
    );
    m.channel = "release".into();
    assert!(check_unsigned(&tar(&entries(&m, &files, None))).is_err());
}
#[test]
fn verifier_malformed_payload_and_inventory() {
    let (m, files) = fixture();
    let original = entries(&m, &files, None);
    for case in [
        "traversal",
        "absolute",
        "backslash",
        "missing",
        "unexpected",
        "duplicate",
        "mode",
        "setuid",
        "hash",
        "symlink",
        "hardlink",
        "directory",
        "extraJSON",
    ] {
        let mut e = original.clone();
        let i = e.iter().position(|v| v.name == "bin/bxdl").unwrap();
        match case {
            "traversal" => e[i].name = "../evil".into(),
            "absolute" => e[i].name = "/bin/bxdl".into(),
            "backslash" => e[i].name = "bin\\bxdl".into(),
            "missing" => {
                e.remove(i);
            }
            "unexpected" => e[i].name = "docs/unlisted".into(),
            "duplicate" => e.push(e[i].clone()),
            "mode" => e[i].mode = 0o644,
            "setuid" => e[i].mode = 0o4755,
            "hash" => e[i].data.fill(b'!'),
            "symlink" | "hardlink" | "directory" => {
                e[i].kind = if case == "symlink" {
                    b'2'
                } else if case == "hardlink" {
                    b'1'
                } else {
                    b'5'
                };
                e[i].link = "/etc/passwd".into();
                e[i].data.clear();
            }
            "extraJSON" => e[0].data.extend_from_slice(b" {}"),
            _ => unreachable!(),
        }
        assert!(check_unsigned(&tar(&e)).is_err(), "accepted {case}");
    }
}
#[test]
fn verifier_gzip_and_tar_trailers() {
    let (m, files) = fixture();
    let raw = tar(&entries(&m, &files, None));
    let good = gzip(&raw);
    let mut corrupt = good.clone();
    let pos = corrupt.len() - 8;
    corrupt[pos] ^= 1;
    let mut member = good.clone();
    member.extend(gzip(b""));
    let mut byte = good.clone();
    byte.push(0);
    let mut trailer = raw.clone();
    trailer.push(1);
    let mut padding = raw.clone();
    padding.resize(padding.len() + (1 << 20) + 1, 0);
    for (name, b) in [
        ("crc", corrupt),
        ("truncated", good[..good.len() - 1].to_vec()),
        ("member", member),
        ("byte", byte),
        ("trailer", gzip(&trailer)),
        ("noeof", gzip(&raw[..raw.len() - 1024])),
        ("singleeof", gzip(&raw[..raw.len() - 512])),
        ("padding", gzip(&padding)),
    ] {
        let (_dir, path) = package(&b);
        assert!(verify(&path, &unsigned()).is_err(), "accepted {name}");
    }
    let mut allowed = raw;
    allowed.extend_from_slice(&[0; 1024]);
    assert!(check_unsigned(&allowed).is_ok());
}
#[test]
fn verifier_physical_extension_headers_rejected() {
    let (m, files) = fixture();
    let raw = tar(&entries(&m, &files, None));
    for kind in [b'x', b'g', b'L', b'K', b'S', b'3', b'6'] {
        let mut b = raw.clone();
        b[156] = kind;
        checksum(&mut b[..512]);
        assert!(check_unsigned(&b).is_err(), "accepted type {kind}");
    }
}
#[test]
fn verifier_strict_json_and_manifest() {
    let (m, _) = fixture();
    let raw = serde_json::to_string(&m).unwrap();
    for b in [
        raw.replacen(
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"schemaVersion\":1",
            1,
        ),
        raw.replacen("schemaVersion", "SchemaVersion", 1),
        raw.replacen(
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"future\":true",
            1,
        ),
        raw.replacen("\"channel\":\"development\"", "\"channel\":null", 1),
        format!("{raw} {{}}"),
        format!("{}1{}", "[".repeat(40), "]".repeat(40)),
    ] {
        assert!(decode_strict_json::<Manifest>(b.as_bytes()).is_err());
    }
    for name in [
        "identity",
        "size",
        "duplicate",
        "platform",
        "official",
        "missing",
        "empty",
        "mode",
    ] {
        let (mut m, _) = fixture();
        match name {
            "identity" => m.engine.jar_sha256 = "0".repeat(64),
            "size" => m.files[0].size = MAX_FILE_BYTES + 1,
            "duplicate" => m.files.push(m.files[0].clone()),
            "platform" => m.platform.os = "windows".into(),
            "official" => m.engine.contract_status = "official".into(),
            "missing" => {
                m.files.remove(0);
            }
            "empty" => m.files[0].size = 0,
            "mode" => m.files[0].mode = 0o600,
            _ => unreachable!(),
        };
        assert!(validate_manifest(&m, true).is_err(), "accepted {name}");
    }
}
#[test]
fn verifier_path_policy() {
    for path in [
        "/runtime/bin/java",
        "runtime/../secrets/a",
        "runtime//x",
        "runtime/x/",
        "runtime\\bin\\java",
        "runtime/keys/key",
        "runtime/foo.p12",
        "licenses/private.pem",
        "docs/.git/config",
        "runtime/node_modules/x",
        "runtime/a.db",
        "runtime/a\0b",
        "bin/other",
        "engine/other.jar",
        "unknown/a",
        "docs/Keys/a",
        "docs/a.Key",
    ] {
        assert!(validate_path(path).is_err(), "accepted {path}");
    }
    assert!(validate_path(&format!("runtime/{}", "x".repeat(101))).is_err());
    for path in [
        "runtime/lib/security/cacerts",
        "runtime/bin/keytool",
        "licenses/THIRD_PARTY_NOTICES",
        "docs/install.md",
    ] {
        assert!(validate_path(path).is_ok(), "rejected {path}");
    }
}
#[test]
fn verifier_empty_optional_file_requires_explicit_size() {
    let (mut m, mut files) = fixture();
    files.insert("docs/empty".into(), Vec::new());
    m.files.push(FileEntry {
        path: "docs/empty".into(),
        size: 0,
        sha256: hash(b""),
        mode: 0o644,
    });
    let mut e = entries(&m, &files, None);
    assert!(check_unsigned(&tar(&e)).is_ok());
    let mut raw: Value = serde_json::from_slice(&e[0].data).unwrap();
    for f in raw["files"].as_array_mut().unwrap() {
        if f["path"] == "docs/empty" {
            f.as_object_mut().unwrap().remove("size");
        }
    }
    e[0].data = serde_json::to_vec(&raw).unwrap();
    assert_eq!(check_unsigned(&tar(&e)).unwrap_err().code, "JSON_INVALID");
}
#[test]
fn verifier_strict_required_presence_and_build_spec_inventory() {
    let (m, _) = fixture();
    let mut raw = serde_json::to_value(m).unwrap();
    raw.as_object_mut().unwrap().remove("files");
    let decoded: Manifest = decode_strict_json(&serde_json::to_vec(&raw).unwrap()).unwrap();
    assert!(validate_manifest(&decoded, false).is_ok());
    assert!(validate_manifest(&decoded, true).is_err());
    raw["platform"].as_object_mut().unwrap().remove("javaMajor");
    assert!(decode_strict_json::<Manifest>(&serde_json::to_vec(&raw).unwrap()).is_err());
}
#[test]
fn verifier_key_formats_and_nonregular_inputs() {
    let (key, dir, public) = keys(5);
    assert_eq!(load_public_key(&public).unwrap(), key.verifying_key());
    let private = dir.path().join("private.pem");
    let pem = key.to_pkcs8_pem(LineEnding::LF).unwrap();
    fs::write(&private, pem.as_bytes()).unwrap();
    assert_eq!(
        load_private_key(&private).unwrap().to_bytes(),
        key.to_bytes()
    );
    assert!(load_public_key(&private).is_err());
    fs::write(&private, format!("{}{}", pem.as_str(), pem.as_str())).unwrap();
    assert!(load_private_key(&private).is_err());
    assert!(verify(dir.path(), &unsigned()).is_err());
    #[cfg(unix)]
    {
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&public, &link).unwrap();
        assert!(load_public_key(&link).is_err());
        let (m, files) = fixture();
        let (_ad, archive) = package(&gzip(&tar(&entries(&m, &files, None))));
        let link = dir.path().join("archive-link");
        std::os::unix::fs::symlink(archive, &link).unwrap();
        assert!(verify(&link, &unsigned()).is_err());
    }
}
#[test]
fn verifier_darwin_arm64_target_and_capacity_limits() {
    let (mut m, files) = fixture();
    m.platform.os = "darwin".into();
    m.platform.arch = "arm64".into();
    m.platform.libc = "none".into();
    m.platform.min_glibc = "none".into();
    assert!(check_unsigned(&tar(&entries(&m, &files, None))).is_ok());
    m.platform.libc = "glibc".into();
    assert!(validate_manifest(&m, true).is_err());
    let (m, _) = fixture();
    let mut raw = serde_json::to_value(&m).unwrap();
    raw["files"][0]["size"] = json!(-1);
    assert!(decode_strict_json::<Manifest>(&serde_json::to_vec(&raw).unwrap()).is_err());
    assert!(decode_strict_json::<Manifest>(&vec![b' '; MAX_MANIFEST_BYTES as usize + 1]).is_err());
    let mut many = m.clone();
    many.files = vec![m.files[0].clone(); MAX_FILES + 1];
    assert!(validate_manifest(&many, true).is_err());
    let mut total = m.clone();
    for i in 0..9 {
        total.files.push(FileEntry {
            path: format!("docs/large-{i}"),
            size: MAX_FILE_BYTES,
            sha256: "a".repeat(64),
            mode: 0o644,
        });
    }
    assert!(validate_manifest(&total, true).is_err());
    let (m, files) = fixture();
    let mut raw = tar(&entries(&m, &files, None));
    octal(&mut raw[124..136], MAX_FILE_BYTES + 1);
    checksum(&mut raw[..512]);
    assert!(check_unsigned(&raw).is_err());
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("oversized");
    fs::File::create(&path)
        .unwrap()
        .set_len(MAX_ARCHIVE_BYTES + 1)
        .unwrap();
    assert_eq!(
        verify(&path, &unsigned()).unwrap_err().code,
        "LIMIT_EXCEEDED"
    );
}
