use crate::error::{BxdlError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MAX_ARCHIVE_BYTES: u64 = 1 << 30;
pub const MAX_TOTAL_BYTES: u64 = 4 << 30;
pub const MAX_FILE_BYTES: u64 = 512 << 20;
pub const MAX_FILES: usize = 10_000;
pub const MAX_MANIFEST_BYTES: u64 = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub kind: String,
    pub product: Product,
    pub channel: String,
    pub platform: Platform,
    pub engine: Engine,
    pub runtime: Runtime,
    #[serde(default)]
    pub files: Vec<FileEntry>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Product {
    pub name: String,
    pub version: String,
    pub revision: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Platform {
    pub os: String,
    pub arch: String,
    pub libc: String,
    pub min_glibc: String,
    pub java_major: u32,
    pub backend: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Engine {
    pub revision: String,
    pub jar_sha256: String,
    pub contract_status: String,
    pub contract_revision: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Runtime {
    pub vendor: String,
    pub version: String,
    pub java_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub mode: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub manifest: Manifest,
    pub archive_sha256: String,
    pub manifest_sha256: String,
    pub authenticity: String,
    pub files_verified: usize,
    pub bytes_verified: u64,
}
pub(super) fn fail(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn hash_string(s: &str, length: usize) -> bool {
    s.len() == length
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn label(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.trim() == s && !s.chars().any(char::is_control)
}

/// Conservative filename policy, not a general content secret detector.
pub fn validate_path(p: &str) -> Result<()> {
    if p.is_empty()
        || p.len() > 255
        || p.contains(['\\', ':', '\0'])
        || p.starts_with('/')
        || p.ends_with('/')
        || p.chars().any(char::is_control)
    {
        return Err(fail(
            "PATH_INVALID",
            "payload path must be normalized, relative and USTAR-compatible",
        ));
    }
    let parts: Vec<_> = p.split('/').collect();
    if parts.len() < 2
        || parts.last().is_some_and(|s| s.len() > 100)
        || parts
            .iter()
            .any(|s| s.is_empty() || *s == "." || *s == "..")
    {
        return Err(fail(
            "PATH_INVALID",
            "payload path must have an allowed root and normalized components",
        ));
    }
    if ![
        "bin", "engine", "runtime", "deploy", "schemas", "docs", "licenses",
    ]
    .contains(&parts[0])
    {
        return Err(fail("PATH_INVALID", "payload root is not allowed"));
    }
    if (parts[0] == "bin" && p != "bin/bxdl")
        || (parts[0] == "engine" && p != "engine/nigo-node.jar")
    {
        return Err(fail("PATH_INVALID", "unexpected binary or engine payload"));
    }
    let forbidden = [
        ".git",
        "node_modules",
        "secret",
        "secrets",
        "key",
        "keys",
        "db",
        "data",
        "wal",
        "credentials",
        "passwords",
    ];
    if parts
        .iter()
        .any(|part| forbidden.contains(&part.to_lowercase().as_str()))
    {
        return Err(fail(
            "PATH_FORBIDDEN",
            "secret, database or development directory in payload",
        ));
    }
    let lower = p.to_lowercase();
    if [
        ".key", ".pem", ".p12", ".pfx", ".jks", ".db", ".wal", ".env",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
    {
        return Err(fail(
            "PATH_FORBIDDEN",
            "secret or database filename in payload",
        ));
    }
    Ok(())
}

/// With complete=false the computed file inventory may be omitted by a build spec.
pub fn validate_manifest(m: &Manifest, complete: bool) -> Result<()> {
    if m.schema_version != 1
        || m.kind != "bxdl-package"
        || m.product.name != "BXDL"
        || m.channel != "development"
    {
        return Err(fail(
            "MANIFEST_INVALID",
            "only schema 1 BXDL development packages are supported",
        ));
    }
    if !label(&m.product.version)
        || !(hash_string(&m.product.revision, 40) || m.product.revision == "development")
    {
        return Err(fail("MANIFEST_INVALID", "invalid product identity"));
    }
    let p = &m.platform;
    let linux = p.os == "linux" && p.arch == "amd64" && p.libc == "glibc" && p.min_glibc == "2.34";
    let mac = p.os == "darwin" && p.arch == "arm64" && p.libc == "none" && p.min_glibc == "none";
    if !(linux || mac) || p.java_major != 21 || p.backend != "rocksdb" {
        return Err(fail(
            "PLATFORM_UNSUPPORTED",
            "only development darwin arm64 or linux amd64 target schemas with Java 21 and RocksDB are accepted",
        ));
    }
    if !hash_string(&m.engine.revision, 40)
        || !hash_string(&m.engine.jar_sha256, 64)
        || m.engine.contract_status != "proposed"
        || !label(&m.engine.contract_revision)
    {
        return Err(fail("MANIFEST_INVALID", "invalid proposed engine identity"));
    }
    if !label(&m.runtime.vendor)
        || !label(&m.runtime.version)
        || !hash_string(&m.runtime.java_sha256, 64)
    {
        return Err(fail("MANIFEST_INVALID", "invalid runtime identity"));
    }
    if !complete && m.files.is_empty() {
        return Ok(());
    }
    if m.files.is_empty() || m.files.len() > MAX_FILES {
        return Err(fail("LIMIT_EXCEEDED", "invalid inventory count"));
    }
    let mut seen = HashMap::with_capacity(m.files.len());
    let mut total = 0_u64;
    for f in &m.files {
        validate_path(&f.path)?;
        if seen.insert(f.path.as_str(), f).is_some() {
            return Err(fail("INVENTORY_DUPLICATE", "duplicate inventory path"));
        }
        if f.size > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES.saturating_sub(f.size) {
            return Err(fail("LIMIT_EXCEEDED", "payload size limit exceeded"));
        }
        total += f.size;
        if !hash_string(&f.sha256, 64) || ![0o644, 0o755].contains(&f.mode) {
            return Err(fail("MANIFEST_INVALID", "invalid file hash or mode"));
        }
    }
    for name in seen.keys() {
        let mut parent = *name;
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            if seen.contains_key(prefix) {
                return Err(fail(
                    "INVENTORY_CONFLICT",
                    "file path is also a parent directory",
                ));
            }
            parent = prefix;
        }
    }
    for required in [
        "bin/bxdl",
        "engine/nigo-node.jar",
        "runtime/bin/java",
        "licenses/THIRD_PARTY_NOTICES",
        "licenses/SBOM.json",
    ] {
        if seen.get(required).is_none_or(|f| f.size == 0) {
            return Err(fail(
                "REQUIRED_FILE_MISSING",
                "required nonempty package file missing",
            ));
        }
    }
    if seen["bin/bxdl"].mode != 0o755 || seen["runtime/bin/java"].mode != 0o755 {
        return Err(fail(
            "MANIFEST_INVALID",
            "required launcher files must be executable",
        ));
    }
    if seen["engine/nigo-node.jar"].sha256 != m.engine.jar_sha256
        || seen["runtime/bin/java"].sha256 != m.runtime.java_sha256
    {
        return Err(fail(
            "IDENTITY_MISMATCH",
            "engine or Java identity does not match inventory",
        ));
    }
    Ok(())
}
