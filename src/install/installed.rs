//! Revalidate an installed tree against a separately authenticated archive report.
//! The receipt and installed manifest are compared inputs, never trust roots.
//! This is an observation, not isolation from a concurrent same-UID process.

use super::{
    Identity, InstallReport, READ_FLAGS, RECEIPT, parent_anchor, reject_inventory_aliases,
};
use crate::artifact::{self, Manifest, Report};
use crate::error::{BxdlError, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use cap_std::fs::{Dir, Metadata, MetadataExt, OpenOptions, OpenOptionsExt};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

// Receipts pretty-print the complete bounded manifest, plus fixed metadata.
// Keep this separate from the setup checkpoint's much smaller size limit.
const MAX_RECEIPT_BYTES: u64 = 8 * artifact::MAX_MANIFEST_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    identity: Identity,
    changed: (i64, i64),
    links: u64,
}
impl Stamp {
    fn of(metadata: &Metadata) -> Self {
        Self {
            identity: Identity::of(metadata),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
            links: metadata.nlink(),
        }
    }
}

enum Entry {
    Directory,
    File { size: u64, mode: u32, hash: String },
}

/// `expected` must come from the caller's successful `artifact::verify` with
/// external trust (or explicit unsigned-development policy). This function
/// cannot authenticate a caller-constructed Report. It does not run any binary.
/// The installed signature's presence/encoding is checked, but Report does not
/// carry its bytes or public key: trust comes from the verified manifest hash.
pub fn verify_installed(destination: &Path, expected: &Report) -> Result<()> {
    verify_observed(destination, expected, || {})
}

fn verify_observed(
    destination: &Path,
    expected: &Report,
    after_hashing: impl FnOnce(),
) -> Result<()> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err(BxdlError::new(
            "INSTALL_PLATFORM_UNSUPPORTED",
            "Installed package verification currently requires a macOS arm64 host",
        ));
    }
    artifact::validate_manifest(&expected.manifest, true).map_err(|_| mismatch())?;
    reject_inventory_aliases(&expected.manifest).map_err(|_| mismatch())?;
    if expected.manifest.platform.os != "darwin"
        || expected.manifest.platform.arch != "arm64"
        || expected.files_verified != expected.manifest.files.len()
        || expected.bytes_verified != expected.manifest.files.iter().map(|f| f.size).sum::<u64>()
        || !valid_hash(&expected.archive_sha256)
        || !valid_hash(&expected.manifest_sha256)
    {
        return Err(mismatch());
    }
    let signed = match expected.authenticity.as_str() {
        "verified-external-ed25519" => true,
        "unsigned-development" => false,
        _ => return Err(mismatch()),
    };
    let (path, parent, name) = parent_anchor(destination).map_err(|_| unsafe_path())?;
    let parent_id = Identity::of(&parent.dir_metadata().map_err(|_| unsafe_path())?);
    let before = parent.symlink_metadata(&name).map_err(|_| unsafe_path())?;
    require_directory(&before)?;
    let root_stamp = Stamp::of(&before);
    let root = parent.open_dir(&name).map_err(|_| unsafe_path())?;
    if Stamp::of(&root.dir_metadata().map_err(|_| unsafe_path())?) != root_stamp {
        return Err(changed());
    }

    let manifest_raw = read_small(&root, "manifest.json", 0o644, artifact::MAX_MANIFEST_BYTES)?;
    if digest(&manifest_raw) != expected.manifest_sha256 {
        return Err(mismatch());
    }
    let manifest: Manifest = artifact::decode_strict_json(&manifest_raw).map_err(|_| mismatch())?;
    if manifest != expected.manifest {
        return Err(mismatch());
    }
    let receipt_raw = read_small(&root, RECEIPT, 0o600, MAX_RECEIPT_BYTES)?;
    // All receipt/nested fields are typed and deny unknown fields. Typed serde
    // also rejects duplicate fields, missing required fields, nulls and trailing
    // input; the inventory must equal the independently verified full manifest.
    let receipt: InstallReport = serde_json::from_slice(&receipt_raw).map_err(|_| mismatch())?;
    if receipt.schema_version != 1
        || receipt.kind != "BXDL_INSTALL_RECEIPT"
        || receipt.outcome != "INSTALLED"
        || receipt.destination != path.to_str().ok_or_else(unsafe_path)?
        || receipt.archive_sha256 != expected.archive_sha256
        || receipt.manifest_sha256 != expected.manifest_sha256
        || receipt.authenticity != expected.authenticity
        || receipt.files_installed != expected.files_verified
        || receipt.bytes_installed != expected.bytes_verified
        || receipt.manifest != expected.manifest
        || receipt.engine_validation != "NOT_CHECKED"
        || receipt.lifecycle != "NOT_PERFORMED"
    {
        return Err(mismatch());
    }
    let mut inventory = BTreeMap::new();
    for entry in &expected.manifest.files {
        inventory.insert(
            entry.path.clone(),
            Entry::File {
                size: entry.size,
                mode: entry.mode,
                hash: entry.sha256.clone(),
            },
        );
        let mut directory = Path::new(&entry.path).parent();
        while let Some(path) = directory.filter(|p| !p.as_os_str().is_empty()) {
            inventory.insert(path.to_str().ok_or_else(mismatch)?.into(), Entry::Directory);
            directory = path.parent();
        }
    }
    for (name, raw, mode) in [
        ("manifest.json", &manifest_raw, 0o644),
        (RECEIPT, &receipt_raw, 0o600),
    ] {
        inventory.insert(
            name.into(),
            Entry::File {
                size: raw.len() as u64,
                mode,
                hash: digest(raw),
            },
        );
    }
    if signed {
        let signature = read_small(&root, "manifest.sig", 0o644, 128)?;
        let encoded = std::str::from_utf8(&signature).map_err(|_| mismatch())?;
        let decoded = STANDARD.decode(encoded.trim()).map_err(|_| mismatch())?;
        if decoded.len() != 64 {
            return Err(mismatch());
        }
        inventory.insert(
            "manifest.sig".into(),
            Entry::File {
                size: signature.len() as u64,
                mode: 0o644,
                hash: digest(&signature),
            },
        );
    }
    let first = scan(&root, &inventory, true)?;
    after_hashing();
    let last = scan(&root, &inventory, false)?;
    if first != last || Stamp::of(&root.dir_metadata().map_err(|_| changed())?) != root_stamp {
        return Err(changed());
    }
    // Rewalk the ambient path as well: an anchored orphaned directory is not
    // proof that the caller's visible installation still denotes these files.
    let (_, visible_parent, visible_name) = parent_anchor(&path).map_err(|_| changed())?;
    if !parent_id.same_inode(&visible_parent.dir_metadata().map_err(|_| changed())?)
        || Stamp::of(
            &visible_parent
                .symlink_metadata(visible_name)
                .map_err(|_| changed())?,
        ) != root_stamp
    {
        return Err(changed());
    }
    Ok(())
}

fn scan(
    root: &Dir,
    expected: &BTreeMap<String, Entry>,
    hash: bool,
) -> Result<BTreeMap<String, Stamp>> {
    let mut observed = BTreeMap::new();
    walk(root, Path::new(""), expected, &mut observed, hash)?;
    if observed.len() != expected.len() {
        return Err(mismatch());
    }
    Ok(observed)
}

fn walk(
    dir: &Dir,
    relative: &Path,
    expected: &BTreeMap<String, Entry>,
    observed: &mut BTreeMap<String, Stamp>,
    hash: bool,
) -> Result<()> {
    let directory_stamp = Stamp::of(&dir.dir_metadata().map_err(|_| unsafe_path())?);
    for entry in dir.entries().map_err(|_| unsafe_path())? {
        if observed.len() >= expected.len() {
            return Err(mismatch());
        }
        let entry = entry.map_err(|_| unsafe_path())?;
        let name = entry.file_name();
        let path = relative.join(&name);
        let name_text = path.to_str().ok_or_else(unsafe_path)?;
        let wanted = expected.get(name_text).ok_or_else(mismatch)?;
        let metadata = dir.symlink_metadata(&name).map_err(|_| unsafe_path())?;
        let stamp = Stamp::of(&metadata);
        if observed.insert(name_text.into(), stamp.clone()).is_some() {
            return Err(mismatch());
        }
        match wanted {
            Entry::Directory => {
                require_directory(&metadata)?;
                let child = dir.open_dir(&name).map_err(|_| unsafe_path())?;
                if Stamp::of(&child.dir_metadata().map_err(|_| unsafe_path())?) != stamp {
                    return Err(changed());
                }
                walk(&child, &path, expected, observed, hash)?;
            }
            Entry::File {
                size,
                mode,
                hash: expected_hash,
            } => {
                require_file(&metadata, *mode, *size)?;
                if metadata.len() != *size {
                    return Err(mismatch());
                }
                if hash {
                    let mut file = open_file(dir, &name, &stamp)?;
                    let mut reader = Read::by_ref(&mut file).take(size + 1);
                    let mut digest = Sha256::new();
                    let mut read = 0_u64;
                    let mut buffer = [0_u8; 64 * 1024];
                    loop {
                        let count = reader.read(&mut buffer).map_err(|_| unsafe_path())?;
                        if count == 0 {
                            break;
                        }
                        read += count as u64;
                        digest.update(&buffer[..count]);
                    }
                    if read != *size || hex::encode(digest.finalize()) != *expected_hash {
                        return Err(mismatch());
                    }
                    if Stamp::of(&file.metadata().map_err(|_| changed())?) != stamp {
                        return Err(changed());
                    }
                }
            }
        }
        if Stamp::of(&dir.symlink_metadata(&name).map_err(|_| changed())?) != stamp {
            return Err(changed());
        }
    }
    if Stamp::of(&dir.dir_metadata().map_err(|_| changed())?) != directory_stamp {
        return Err(changed());
    }
    Ok(())
}

fn read_small(dir: &Dir, name: &str, mode: u32, maximum: u64) -> Result<Vec<u8>> {
    let metadata = dir.symlink_metadata(name).map_err(|_| unsafe_path())?;
    require_file(&metadata, mode, maximum)?;
    let stamp = Stamp::of(&metadata);
    let mut file = open_file(dir, Path::new(name), &stamp)?;
    let mut raw = Vec::new();
    Read::by_ref(&mut file)
        .take(maximum + 1)
        .read_to_end(&mut raw)
        .map_err(|_| unsafe_path())?;
    if raw.len() as u64 != metadata.len()
        || Stamp::of(&file.metadata().map_err(|_| changed())?) != stamp
        || Stamp::of(&dir.symlink_metadata(name).map_err(|_| changed())?) != stamp
    {
        return Err(changed());
    }
    Ok(raw)
}

fn open_file(dir: &Dir, name: impl AsRef<Path>, stamp: &Stamp) -> Result<cap_std::fs::File> {
    let mut options = OpenOptions::new();
    options.read(true).custom_flags(READ_FLAGS);
    let file = dir.open_with(name, &options).map_err(|_| unsafe_path())?;
    if Stamp::of(&file.metadata().map_err(|_| unsafe_path())?) != *stamp {
        return Err(changed());
    }
    Ok(file)
}
fn require_directory(metadata: &Metadata) -> Result<()> {
    if !metadata.is_dir() || metadata.mode() & 0o7777 != 0o700 {
        return Err(unsafe_path());
    }
    Ok(())
}
fn require_file(metadata: &Metadata, mode: u32, maximum: u64) -> Result<()> {
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.mode() & 0o7777 != mode
        || metadata.len() > maximum
    {
        return Err(unsafe_path());
    }
    Ok(())
}
fn valid_hash(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn digest(raw: &[u8]) -> String {
    hex::encode(Sha256::digest(raw))
}
fn mismatch() -> BxdlError {
    BxdlError::new(
        "INSTALLED_PACKAGE_MISMATCH",
        "설치된 패키지와 외부에서 검증한 archive의 내용 또는 설치 기록이 일치하지 않습니다.",
    )
}
fn unsafe_path() -> BxdlError {
    BxdlError::new(
        "INSTALLED_PACKAGE_UNSAFE",
        "설치된 패키지의 파일 종류·권한·링크·크기 또는 접근 경로를 확인하세요.",
    )
}
fn changed() -> BxdlError {
    BxdlError::new(
        "INSTALLED_PACKAGE_CHANGED",
        "검사 중 설치된 패키지 또는 경로가 변경되었습니다.",
    )
}

#[cfg(test)]
#[path = "installed_tests.rs"]
mod tests;
