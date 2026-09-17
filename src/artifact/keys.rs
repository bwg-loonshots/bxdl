use super::types::fail;
use crate::error::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{
    SigningKey, VerifyingKey,
    pkcs8::{DecodePrivateKey, DecodePublicKey},
};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{
    fs::{self, File, Metadata},
    io::Read,
    path::Path,
};

pub(super) fn same_file(a: &Metadata, b: &Metadata) -> bool {
    #[cfg(unix)]
    {
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        false
    }
}
fn read_key(path: &Path, label: &str) -> Result<Vec<u8>> {
    let before =
        fs::symlink_metadata(path).map_err(|_| fail("KEY_INVALID", "cannot inspect key file"))?;
    if !before.is_file() || before.len() > 16384 {
        return Err(fail(
            "KEY_INVALID",
            "key must be a bounded regular PEM file",
        ));
    }
    let file = File::open(path).map_err(|_| fail("KEY_INVALID", "cannot open key file"))?;
    let actual = file
        .metadata()
        .map_err(|_| fail("KEY_INVALID", "cannot inspect opened key file"))?;
    if !same_file(&before, &actual) || !actual.is_file() {
        return Err(fail("KEY_INVALID", "key file changed while opening"));
    }
    let mut bytes = Vec::new();
    file.take(16385)
        .read_to_end(&mut bytes)
        .map_err(|_| fail("KEY_INVALID", "cannot read bounded key file"))?;
    if bytes.len() > 16384 {
        return Err(fail("KEY_INVALID", "key exceeds allowed size"));
    }
    let pem = String::from_utf8(bytes).map_err(|_| fail("KEY_INVALID", "invalid PEM encoding"))?;
    let pem = pem.trim();
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    if !pem.starts_with(&begin)
        || !pem.ends_with(&end)
        || pem.matches("-----BEGIN ").count() != 1
        || pem.matches("-----END ").count() != 1
    {
        return Err(fail(
            "KEY_INVALID",
            "expected exactly one unencrypted Ed25519 PEM block",
        ));
    }
    // Accept ordinary wrapped and unwrapped PEM bodies, as the previous Go
    // implementation did, without allowing headers, multiple blocks or junk.
    let body = &pem[begin.len()..pem.len() - end.len()];
    let encoded: Vec<u8> = body.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    STANDARD
        .decode(encoded)
        .map_err(|_| fail("KEY_INVALID", "invalid PEM base64 body"))
}
pub fn load_public_key(path: &Path) -> Result<VerifyingKey> {
    VerifyingKey::from_public_key_der(&read_key(path, "PUBLIC KEY")?)
        .map_err(|_| fail("KEY_INVALID", "expected SPKI Ed25519 public key"))
}
pub fn load_private_key(path: &Path) -> Result<SigningKey> {
    SigningKey::from_pkcs8_der(&read_key(path, "PRIVATE KEY")?).map_err(|_| {
        fail(
            "KEY_INVALID",
            "expected unencrypted PKCS8 Ed25519 private key",
        )
    })
}
