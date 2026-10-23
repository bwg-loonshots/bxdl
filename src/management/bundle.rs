//! A CLI-only supply contract. No runtime manifest, engine or node state lives
//! in this format. Verification returns bounded bytes; it never extracts them.
use crate::{
    artifact::{FileEntry, decode_strict_json, load_private_key, load_public_key},
    error::{BxdlError, Result},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use flate2::{Compression, GzBuilder, bufread::GzDecoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
};

#[path = "bundle/tar.rs"]
mod tar;
#[cfg(test)]
#[path = "bundle/tests.rs"]
mod tests;

pub const PAYLOADS: &[(&str, u32)] = &[
    ("bin/bxdl", 0o755),
    ("docs/completion.md", 0o644),
    ("docs/management-cli.md", 0o644),
    ("share/zsh/site-functions/_bxdl", 0o644),
];
const DIRECTORIES: &[&str] = &[
    "bin",
    "docs",
    "share",
    "share/zsh",
    "share/zsh/site-functions",
];
const MAX_BINARY: usize = 48 << 20;
const MAX_TEXT: usize = 1 << 20;
const MAX_METADATA: usize = 16 << 10;
const MAX_ARCHIVE: usize = 52 << 20;
const MAX_RAW: usize = 52 << 20;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub kind: String,
    pub version: String,
    pub revision: String,
    pub target: String,
    pub files: Vec<FileEntry>,
}
#[derive(Clone, Debug, Default)]
pub struct VerifyOptions {
    pub public_key_path: Option<PathBuf>,
    pub allow_unsigned_development: bool,
}
#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub root: PathBuf,
    pub output: PathBuf,
    pub version: String,
    pub revision: String,
    pub signing_key_path: Option<PathBuf>,
    pub allow_unsigned_development: bool,
}
#[derive(Clone, Debug)]
pub struct Bundle {
    pub manifest: Manifest,
    pub manifest_bytes: Vec<u8>,
    pub signature: Option<Vec<u8>>,
    pub files: BTreeMap<String, Vec<u8>>,
    pub authenticity: String,
    pub manifest_sha256: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub manifest: Manifest,
    pub manifest_sha256: String,
    pub authenticity: String,
    pub files_verified: usize,
    pub bytes_verified: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildReport {
    pub manifest: Manifest,
    pub manifest_sha256: String,
    pub archive_sha256: String,
    pub authenticity: String,
    pub archive_path: PathBuf,
}
impl Bundle {
    pub fn release_id(&self) -> String {
        format!("{}-{}", self.manifest.version, self.manifest.revision)
    }
    pub fn report(&self) -> VerifyReport {
        VerifyReport {
            manifest: self.manifest.clone(),
            manifest_sha256: self.manifest_sha256.clone(),
            authenticity: self.authenticity.clone(),
            files_verified: self.files.len(),
            bytes_verified: self.files.values().map(|v| v.len() as u64).sum(),
        }
    }
}

pub fn verify(path: &Path, options: &VerifyOptions) -> Result<Bundle> {
    if options.public_key_path.is_some() == options.allow_unsigned_development {
        return Err(trust_error());
    }
    let key = options
        .public_key_path
        .as_deref()
        .map(load_public_key)
        .transpose()?;
    verify_with_key(path, key.as_ref(), options.allow_unsigned_development)
}

pub(crate) fn verify_with_key(
    path: &Path,
    key: Option<&VerifyingKey>,
    allow_unsigned: bool,
) -> Result<Bundle> {
    if key.is_some() == allow_unsigned {
        return Err(trust_error());
    }
    authenticate(read_bundle(path)?, key, allow_unsigned)
}

/// Only for an already admitted release whose manifest hash is pinned by the
/// installer's private receipt. This deliberately cannot admit new supply.
pub(crate) fn verify_integrity(path: &Path) -> Result<Bundle> {
    if !fs::symlink_metadata(path).map_err(|_| invalid())?.is_dir() {
        return Err(invalid());
    }
    let mut bundle = read_bundle(path)?;
    bundle.authenticity = "previously-admitted-integrity-only".into();
    Ok(bundle)
}

fn authenticate(mut bundle: Bundle, key: Option<&VerifyingKey>, unsigned: bool) -> Result<Bundle> {
    match (&bundle.signature, key, unsigned) {
        (Some(raw), Some(key), false) => {
            if bundle.manifest.revision == "development" {
                return Err(manifest_error());
            }
            let signature = parse_signature(raw)?;
            key.verify_strict(&bundle.manifest_bytes, &signature)
                .map_err(|_| signature_error())?;
            bundle.authenticity = "verified-external-ed25519".into();
        }
        (None, None, true) => bundle.authenticity = "unsigned-development".into(),
        _ => return Err(trust_error()),
    }
    Ok(bundle)
}

fn read_bundle(path: &Path) -> Result<Bundle> {
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid())?;
    let entries = if metadata.is_dir() {
        directory_entries(path, true)?
    } else if metadata.is_file() {
        archive_entries(&read_file(path, MAX_ARCHIVE, None)?)?
    } else {
        return Err(invalid());
    };
    from_entries(entries)
}

fn from_entries(mut entries: BTreeMap<String, (u32, Vec<u8>)>) -> Result<Bundle> {
    let (mode, manifest_bytes) = entries.remove("manifest.json").ok_or_else(manifest_error)?;
    if mode != 0o644 || manifest_bytes.is_empty() || manifest_bytes.len() > MAX_METADATA {
        return Err(manifest_error());
    }
    let manifest: Manifest = decode_strict_json(&manifest_bytes).map_err(|_| manifest_error())?;
    validate_manifest(&manifest)?;
    let signature = entries
        .remove("manifest.sig")
        .map(|(mode, raw)| {
            if mode != 0o644 {
                return Err(signature_error());
            }
            parse_signature(&raw)?;
            Ok(raw)
        })
        .transpose()?;
    if entries.len() != PAYLOADS.len() {
        return Err(invalid());
    }
    let mut files = BTreeMap::new();
    for expected in &manifest.files {
        let (mode, bytes) = entries.remove(&expected.path).ok_or_else(invalid)?;
        if mode != expected.mode
            || bytes.len() as u64 != expected.size
            || digest(&bytes) != expected.sha256
        {
            return Err(hash_error());
        }
        files.insert(expected.path.clone(), bytes);
    }
    validate_binary(files.get("bin/bxdl").ok_or_else(invalid)?)?;
    Ok(Bundle {
        manifest,
        manifest_sha256: digest(&manifest_bytes),
        manifest_bytes,
        signature,
        files,
        authenticity: String::new(),
    })
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.schema_version != 1
        || manifest.kind != "bxdl-management-cli"
        || manifest.target != "darwin-arm64"
        || !version(&manifest.version)
        || !(hex_hash(&manifest.revision, 40) || manifest.revision == "development")
        || manifest.files.len() != PAYLOADS.len()
    {
        return Err(manifest_error());
    }
    let mut paths = BTreeSet::new();
    for file in &manifest.files {
        let (_, mode) = PAYLOADS
            .iter()
            .find(|(path, _)| *path == file.path)
            .ok_or_else(manifest_error)?;
        if !paths.insert(&file.path)
            || file.mode != *mode
            || file.size == 0
            || file.size > limit(&file.path) as u64
            || !hex_hash(&file.sha256, 64)
        {
            return Err(manifest_error());
        }
    }
    Ok(())
}
fn version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_digit()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".+-".contains(&b))
        && !value.contains("..")
        && !value.ends_with('.')
}
fn hex_hash(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn validate_binary(bytes: &[u8]) -> Result<()> {
    // Thin little-endian Mach-O 64, CPU_TYPE_ARM64. Other formats are never
    // selected for execution by this first Mac arm64 supply contract.
    if bytes.len() < 32 || bytes[..8] != [0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1] {
        return Err(manifest_error());
    }
    Ok(())
}
fn parse_signature(raw: &[u8]) -> Result<Signature> {
    if raw.is_empty() || raw.len() > 128 {
        return Err(signature_error());
    }
    let text = std::str::from_utf8(raw).map_err(|_| signature_error())?;
    let bytes = STANDARD
        .decode(text.trim())
        .map_err(|_| signature_error())?;
    Signature::from_slice(&bytes).map_err(|_| signature_error())
}
fn limit(path: &str) -> usize {
    match path {
        "bin/bxdl" => MAX_BINARY,
        "manifest.json" => MAX_METADATA,
        "manifest.sig" => 128,
        _ => MAX_TEXT,
    }
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

// No-follow on the final file, bounded reads, and metadata comparisons prevent
// accidental FIFO blocking or adoption of files swapped during verification.
fn read_file(path: &Path, maximum: usize, expected_mode: Option<u32>) -> Result<Vec<u8>> {
    let before = fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !before.is_file()
        || before.nlink() != 1
        || before.len() == 0
        || before.len() > maximum as u64
        || expected_mode.is_some_and(|mode| before.mode() & 0o7777 != mode)
    {
        return Err(invalid());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(
            (rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC)
                .bits() as i32,
        )
        .open(path)
        .map_err(|_| invalid())?;
    if identity(&before) != identity(&file.metadata().map_err(|_| invalid())?) {
        return Err(changed());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() != before.len() as usize
        || bytes.len() > maximum
        || identity(&before) != identity(&file.metadata().map_err(|_| invalid())?)
        || identity(&before) != identity(&fs::symlink_metadata(path).map_err(|_| changed())?)
    {
        return Err(changed());
    }
    Ok(bytes)
}
fn identity(m: &fs::Metadata) -> (u64, u64, u64, u32, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mode(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}

fn directory_entries(root: &Path, metadata: bool) -> Result<BTreeMap<String, (u32, Vec<u8>)>> {
    let before = fs::symlink_metadata(root).map_err(|_| invalid())?;
    if !before.is_dir() || before.file_type().is_symlink() {
        return Err(invalid());
    }
    let mut entries = BTreeMap::new();
    walk(root, root, metadata, &mut entries)?;
    let after = fs::symlink_metadata(root).map_err(|_| changed())?;
    if identity(&before) != identity(&after) {
        return Err(changed());
    }
    Ok(entries)
}
fn walk(
    root: &Path,
    directory: &Path,
    metadata: bool,
    entries: &mut BTreeMap<String, (u32, Vec<u8>)>,
) -> Result<()> {
    let before = fs::symlink_metadata(directory).map_err(|_| invalid())?;
    if !before.is_dir() || before.file_type().is_symlink() {
        return Err(invalid());
    }
    for (index, entry) in fs::read_dir(directory).map_err(|_| invalid())?.enumerate() {
        if index > 8 {
            return Err(invalid());
        }
        let entry = entry.map_err(|_| invalid())?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| invalid())?
            .to_str()
            .ok_or_else(invalid)?
            .to_owned();
        let kind = fs::symlink_metadata(&path).map_err(|_| invalid())?;
        if kind.is_dir() && DIRECTORIES.contains(&relative.as_str()) {
            walk(root, &path, metadata, entries)?;
        } else {
            let mode = PAYLOADS
                .iter()
                .find(|(p, _)| *p == relative)
                .map(|(_, m)| *m)
                .or_else(|| {
                    (metadata && matches!(relative.as_str(), "manifest.json" | "manifest.sig"))
                        .then_some(0o644)
                })
                .ok_or_else(invalid)?;
            let bytes = read_file(&path, limit(&relative), Some(mode))?;
            if entries.insert(relative, (mode, bytes)).is_some() {
                return Err(invalid());
            }
        }
    }
    if identity(&before) != identity(&fs::symlink_metadata(directory).map_err(|_| changed())?) {
        return Err(changed());
    }
    Ok(())
}

fn archive_entries(bytes: &[u8]) -> Result<BTreeMap<String, (u32, Vec<u8>)>> {
    let mut decoder = GzDecoder::new(Cursor::new(bytes));
    let mut raw = Vec::new();
    (&mut decoder)
        .take(MAX_RAW as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|_| invalid())?;
    if raw.len() > MAX_RAW || decoder.into_inner().position() != bytes.len() as u64 {
        return Err(invalid());
    }
    tar::read(&raw)
}

pub fn build(options: &BuildOptions) -> Result<BuildReport> {
    if options.signing_key_path.is_some() == options.allow_unsigned_development {
        return Err(trust_error());
    }
    if !version(&options.version)
        || !(hex_hash(&options.revision, 40)
            || (options.allow_unsigned_development && options.revision == "development"))
    {
        return Err(manifest_error());
    }
    let entries = directory_entries(&options.root, false)?;
    if entries.len() != PAYLOADS.len() {
        return Err(invalid());
    }
    let mut files = Vec::new();
    for (path, (mode, bytes)) in &entries {
        files.push(FileEntry {
            path: path.clone(),
            size: bytes.len() as u64,
            sha256: digest(bytes),
            mode: *mode,
        });
    }
    let manifest = Manifest {
        schema_version: 1,
        kind: "bxdl-management-cli".into(),
        version: options.version.clone(),
        revision: options.revision.clone(),
        target: "darwin-arm64".into(),
        files,
    };
    validate_manifest(&manifest)?;
    validate_binary(&entries.get("bin/bxdl").ok_or_else(invalid)?.1)?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|_| invalid())?;
    let key = options
        .signing_key_path
        .as_deref()
        .map(load_private_key)
        .transpose()?;
    if let Some(key_path) = &options.signing_key_path {
        let key_metadata = fs::metadata(key_path).map_err(|_| invalid())?;
        for path in entries.keys() {
            let payload_metadata = fs::metadata(options.root.join(path)).map_err(|_| invalid())?;
            if key_metadata.dev() == payload_metadata.dev()
                && key_metadata.ino() == payload_metadata.ino()
            {
                return Err(trust_error());
            }
        }
    }
    let signature = key.as_ref().map(|key| {
        STANDARD
            .encode(key.sign(&manifest_bytes).to_bytes())
            .into_bytes()
    });
    let mut payload = BTreeMap::new();
    payload.insert("manifest.json".into(), (0o644, manifest_bytes.clone()));
    if let Some(signature) = &signature {
        payload.insert("manifest.sig".into(), (0o644, signature.clone()));
    }
    payload.extend(entries);
    let raw = tar::write(&payload)?;
    let mut gzip = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(Vec::new(), Compression::best());
    gzip.write_all(&raw).map_err(|_| invalid())?;
    let archive = gzip.finish().map_err(|_| invalid())?;
    if archive.len() > MAX_ARCHIVE {
        return Err(invalid());
    }
    // Independently parse the exact bytes about to be published.
    let verified = authenticate(
        from_entries(archive_entries(&archive)?)?,
        key.as_ref().map(|k| k.verifying_key()).as_ref(),
        options.allow_unsigned_development,
    )?;
    let output = absolute_output(&options.output)?;
    let root = options.root.canonicalize().map_err(|_| invalid())?;
    if output.starts_with(&root) {
        return Err(invalid());
    }
    if directory_entries(&options.root, false)?
        != payload
            .iter()
            .filter(|(p, _)| !p.starts_with("manifest."))
            .map(|(p, v)| (p.clone(), v.clone()))
            .collect()
    {
        return Err(changed());
    }
    // create_new never replaces an existing destination, including links.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&output)
        .map_err(|_| {
            error(
                "CLI_BUNDLE_OUTPUT_EXISTS",
                "출력 위치에 파일이 있거나 새 배포본을 만들 수 없습니다.",
            )
        })?;
    if file
        .write_all(&archive)
        .and_then(|_| file.sync_all())
        .is_err()
    {
        // Keep any partial file visible rather than risk removing a replaced path.
        return Err(error(
            "CLI_BUNDLE_OUTPUT_INCOMPLETE",
            "배포본 저장을 완료하지 못했습니다. 새 출력 위치로 다시 제작하세요.",
        ));
    }
    File::open(output.parent().ok_or_else(invalid)?)
        .and_then(|f| f.sync_all())
        .map_err(|_| invalid())?;
    Ok(BuildReport {
        manifest: verified.manifest,
        manifest_sha256: verified.manifest_sha256,
        archive_sha256: digest(&archive),
        authenticity: verified.authenticity,
        archive_path: output,
    })
}
fn absolute_output(path: &Path) -> Result<PathBuf> {
    let full = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().map_err(|_| invalid())?.join(path)
    };
    if full
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(invalid());
    }
    let name = full.file_name().ok_or_else(invalid)?;
    let parent = full
        .parent()
        .ok_or_else(invalid)?
        .canonicalize()
        .map_err(|_| invalid())?;
    Ok(parent.join(name))
}
fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn invalid() -> BxdlError {
    error(
        "CLI_BUNDLE_INVALID",
        "관리 CLI 배포본의 경로·파일 형식 또는 크기가 올바르지 않습니다.",
    )
}
fn manifest_error() -> BxdlError {
    error(
        "CLI_BUNDLE_MANIFEST_INVALID",
        "지원하는 Mac arm64 관리 CLI의 배포 식별 정보가 아닙니다.",
    )
}
fn hash_error() -> BxdlError {
    error(
        "CLI_BUNDLE_HASH_MISMATCH",
        "관리 CLI 배포본의 파일 내용이 manifest와 일치하지 않습니다.",
    )
}
fn changed() -> BxdlError {
    error(
        "CLI_BUNDLE_CHANGED",
        "검증 중 관리 CLI 배포 자료가 변경되었습니다. 다시 확인하세요.",
    )
}
fn trust_error() -> BxdlError {
    error(
        "CLI_BUNDLE_TRUST_REQUIRED",
        "외부 신뢰 공개키 또는 명시적인 unsigned 개발 배포 허용 중 하나가 필요합니다.",
    )
}
fn signature_error() -> BxdlError {
    error(
        "CLI_BUNDLE_SIGNATURE_INVALID",
        "관리 CLI 배포본 서명을 검증하지 못했습니다.",
    )
}
