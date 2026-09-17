use super::keys::same_file;
use super::types::{
    MAX_ARCHIVE_BYTES, MAX_FILE_BYTES, MAX_FILES, MAX_MANIFEST_BYTES, MAX_TOTAL_BYTES, fail,
};
use super::{
    Manifest, Report, decode_strict_json, load_public_key, validate_manifest, validate_path,
};
use crate::error::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use flate2::bufread::GzDecoder;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, BufRead, BufReader, Read},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default)]
pub struct VerifyOptions {
    pub public_key_path: Option<PathBuf>,
    pub allow_unsigned_development: bool,
}

/// Checks package bytes without extracting files or establishing runtime support.
pub fn verify(path: &Path, options: &VerifyOptions) -> Result<Report> {
    let key = options
        .public_key_path
        .as_deref()
        .map(load_public_key)
        .transpose()?;
    verify_inner(path, key.as_ref(), options.allow_unsigned_development)
}
pub(super) fn verify_with_key(path: &Path, key: &VerifyingKey) -> Result<Report> {
    verify_inner(path, Some(key), false)
}

fn verify_inner(path: &Path, key: Option<&VerifyingKey>, allow_unsigned: bool) -> Result<Report> {
    let before = fs::symlink_metadata(path)
        .map_err(|_| fail("ARCHIVE_INVALID", "cannot inspect archive"))?;
    if !before.is_file() {
        return Err(fail("ARCHIVE_INVALID", "archive must be a regular file"));
    }
    if before.len() == 0 || before.len() > MAX_ARCHIVE_BYTES {
        return Err(fail(
            "LIMIT_EXCEEDED",
            "compressed archive size exceeds limit",
        ));
    }
    let file = File::open(path).map_err(|_| fail("ARCHIVE_INVALID", "cannot open archive"))?;
    let opened = file
        .metadata()
        .map_err(|_| fail("ARCHIVE_INVALID", "cannot inspect opened archive"))?;
    if !opened.is_file() || !same_file(&before, &opened) || before.len() != opened.len() {
        return Err(fail("ARCHIVE_INVALID", "archive changed while opening"));
    }
    let compressed = BufReader::new(HashReader {
        inner: (&file).take(MAX_ARCHIVE_BYTES + 1),
        digest: Sha256::new(),
        count: 0,
    });
    // bufread's single-member decoder preserves bytes after the gzip member.
    let gz = GzDecoder::new(compressed);
    let mut raw =
        gz.take(MAX_TOTAL_BYTES + MAX_MANIFEST_BYTES + (MAX_FILES as u64 + 2) * 1024 + (1 << 20));
    let first = read_header(&mut raw)?
        .ok_or_else(|| fail("MANIFEST_INVALID", "first entry must be manifest.json"))?;
    if first.name != "manifest.json"
        || first.mode != 0o644
        || first.size == 0
        || first.size > MAX_MANIFEST_BYTES
    {
        return Err(fail(
            "MANIFEST_INVALID",
            "first entry must be a bounded manifest.json",
        ));
    }
    let manifest_bytes = read_small(&mut raw, first.size)?;
    let manifest: Manifest = decode_strict_json(&manifest_bytes)?;
    validate_manifest(&manifest, true)?;
    let mut next = read_header(&mut raw)?;
    let authenticity;
    if next.as_ref().is_some_and(|h| h.name == "manifest.sig") {
        let signature_header = next.take().expect("signature header was checked");
        if signature_header.size == 0
            || signature_header.size > 128
            || signature_header.mode != 0o644
        {
            return Err(fail("SIGNATURE_INVALID", "signature entry is invalid"));
        }
        let signature_bytes = read_small(&mut raw, signature_header.size)?;
        let text = std::str::from_utf8(&signature_bytes)
            .map_err(|_| fail("SIGNATURE_INVALID", "invalid base64 signature"))?;
        let decoded = STANDARD
            .decode(text.trim())
            .map_err(|_| fail("SIGNATURE_INVALID", "signature is not base64 Ed25519"))?;
        let signature = Signature::from_slice(&decoded)
            .map_err(|_| fail("SIGNATURE_INVALID", "signature is not Ed25519 length"))?;
        let trusted = key.ok_or_else(|| {
            fail(
                "TRUST_KEY_REQUIRED",
                "signed packages require an external trusted public key",
            )
        })?;
        trusted
            .verify_strict(&manifest_bytes, &signature)
            .map_err(|_| {
                fail(
                    "SIGNATURE_INVALID",
                    "manifest signature does not match trusted key",
                )
            })?;
        authenticity = "verified-external-ed25519";
        next = read_header(&mut raw)?;
    } else {
        if !allow_unsigned || key.is_some() {
            return Err(fail(
                "SIGNATURE_REQUIRED",
                "unsigned development archives require explicit opt-in and no trusted-key expectation",
            ));
        }
        authenticity = "unsigned-development";
    }
    let mut inventory: HashMap<_, _> = manifest
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();
    let mut files_verified = 0;
    let mut bytes_verified = 0;
    while let Some(header) = next {
        validate_path(&header.name)?;
        let expected = inventory.remove(header.name.as_str()).ok_or_else(|| {
            fail(
                "INVENTORY_MISMATCH",
                "unexpected or duplicate payload entry",
            )
        })?;
        if header.size != expected.size || header.mode != expected.mode {
            return Err(fail(
                "INVENTORY_MISMATCH",
                "payload size or mode differs from inventory",
            ));
        }
        let mut digest = Sha256::new();
        let mut remaining = header.size;
        let mut buffer = [0_u8; 32768];
        while remaining > 0 {
            let amount = remaining.min(buffer.len() as u64) as usize;
            raw.read_exact(&mut buffer[..amount])
                .map_err(|_| fail("ARCHIVE_INVALID", "truncated or corrupt payload"))?;
            digest.update(&buffer[..amount]);
            remaining -= amount as u64;
        }
        if hex::encode(digest.finalize()) != expected.sha256 {
            return Err(fail("HASH_MISMATCH", "payload hash differs from inventory"));
        }
        read_padding(&mut raw, header.size)?;
        files_verified += 1;
        bytes_verified += header.size;
        next = read_header(&mut raw)?;
    }
    if !inventory.is_empty() {
        return Err(fail(
            "INVENTORY_MISMATCH",
            "archive is missing inventoried files",
        ));
    }
    drop(inventory);
    let mut padding = 0_u64;
    let mut buffer = [0_u8; 32768];
    loop {
        let n = raw.read(&mut buffer).map_err(|_| {
            fail(
                "ARCHIVE_INVALID",
                "gzip checksum or stream validation failed",
            )
        })?;
        padding += n as u64;
        if padding > 1 << 20 || raw.limit() == 0 {
            return Err(fail("LIMIT_EXCEEDED", "excessive archive padding"));
        }
        if !all_zero(&buffer[..n]) {
            return Err(fail(
                "ARCHIVE_TRAILING_DATA",
                "nonzero data follows tar EOF",
            ));
        }
        if n == 0 {
            break;
        }
    }
    let mut compressed = raw.into_inner().into_inner();
    if !compressed
        .fill_buf()
        .map_err(|_| fail("ARCHIVE_INVALID", "cannot finish compressed stream"))?
        .is_empty()
    {
        return Err(fail(
            "ARCHIVE_TRAILING_DATA",
            "extra gzip member or trailing compressed data",
        ));
    }
    let hashed = compressed.into_inner();
    if hashed.count > MAX_ARCHIVE_BYTES {
        return Err(fail(
            "LIMIT_EXCEEDED",
            "compressed archive size exceeds limit",
        ));
    }
    let after = file
        .metadata()
        .map_err(|_| fail("ARCHIVE_CHANGED", "cannot recheck archive"))?;
    if after.len() != before.len() || after.modified().ok() != before.modified().ok() {
        return Err(fail(
            "ARCHIVE_CHANGED",
            "archive changed during verification",
        ));
    }
    Ok(Report {
        manifest,
        archive_sha256: hex::encode(hashed.digest.finalize()),
        manifest_sha256: hex::encode(Sha256::digest(&manifest_bytes)),
        authenticity: authenticity.to_owned(),
        files_verified,
        bytes_verified,
    })
}

struct HashReader<R> {
    inner: R,
    digest: Sha256,
    count: u64,
}
impl<R: Read> Read for HashReader<R> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(b)?;
        self.digest.update(&b[..n]);
        self.count += n as u64;
        Ok(n)
    }
}
#[derive(Debug)]
struct Header {
    name: String,
    size: u64,
    mode: u32,
}

// Process each physical block directly: no library may silently consume an
// extension/global/long-name/sparse header outside the signed inventory.
fn read_header(r: &mut impl Read) -> Result<Option<Header>> {
    let mut block = [0_u8; 512];
    r.read_exact(&mut block).map_err(|_| {
        fail(
            "ARCHIVE_INVALID",
            "missing complete tar header or two-block EOF",
        )
    })?;
    if all_zero(&block) {
        r.read_exact(&mut block)
            .map_err(|_| fail("ARCHIVE_INVALID", "tar EOF needs two zero blocks"))?;
        if !all_zero(&block) {
            return Err(fail("ARCHIVE_INVALID", "tar EOF needs two zero blocks"));
        }
        return Ok(None);
    }
    if ![0, b'0'].contains(&block[156]) {
        return Err(fail(
            "ENTRY_TYPE_FORBIDDEN",
            "only physical regular-file tar entries are accepted",
        ));
    }
    if &block[257..263] != b"ustar\0"
        || &block[263..265] != b"00"
        || !all_zero(&block[157..257])
        || !all_zero(&block[500..512])
    {
        return Err(fail(
            "ARCHIVE_INVALID",
            "invalid or unsupported USTAR header",
        ));
    }
    let checksum = octal(&block[148..156])?;
    let actual: u64 = block
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if (148..156).contains(&i) {
                u64::from(b' ')
            } else {
                u64::from(*b)
            }
        })
        .sum();
    if checksum != actual {
        return Err(fail("ARCHIVE_INVALID", "invalid tar checksum"));
    }
    let mode = octal(&block[100..108])?;
    let size = octal(&block[124..136])?;
    for field in [
        &block[108..116],
        &block[116..124],
        &block[136..148],
        &block[329..337],
        &block[337..345],
    ] {
        octal(field)?;
    }
    if size > MAX_FILE_BYTES || ![0o644, 0o755].contains(&mode) {
        return Err(fail(
            "ENTRY_INVALID",
            "entry size or mode outside allowed values",
        ));
    }
    let name = c_string(&block[..100])?;
    let prefix = c_string(&block[345..500])?;
    let name = if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}/{name}")
    };
    Ok(Some(Header {
        name,
        size,
        mode: mode as u32,
    }))
}
fn octal(bytes: &[u8]) -> Result<u64> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| fail("ARCHIVE_INVALID", "invalid tar numeric field"))?
        .trim_matches(['\0', ' ']);
    if text.is_empty() {
        return Ok(0);
    }
    if !text.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return Err(fail("ARCHIVE_INVALID", "tar numeric field must be octal"));
    }
    u64::from_str_radix(text, 8)
        .map_err(|_| fail("ARCHIVE_INVALID", "tar numeric field exceeds limit"))
}
fn c_string(bytes: &[u8]) -> Result<&str> {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    if !all_zero(&bytes[end..]) {
        return Err(fail(
            "ARCHIVE_INVALID",
            "nonzero data after tar string terminator",
        ));
    }
    std::str::from_utf8(&bytes[..end])
        .map_err(|_| fail("ARCHIVE_INVALID", "tar path must be UTF-8"))
}
fn read_small(r: &mut impl Read, size: u64) -> Result<Vec<u8>> {
    let length = usize::try_from(size)
        .map_err(|_| fail("LIMIT_EXCEEDED", "metadata size exceeds addressable limit"))?;
    let mut bytes = vec![0; length];
    r.read_exact(&mut bytes)
        .map_err(|_| fail("ARCHIVE_INVALID", "truncated metadata entry"))?;
    read_padding(r, size)?;
    Ok(bytes)
}
fn read_padding(r: &mut impl Read, size: u64) -> Result<()> {
    let n = ((512 - size % 512) % 512) as usize;
    let mut buffer = [0_u8; 512];
    r.read_exact(&mut buffer[..n])
        .map_err(|_| fail("ARCHIVE_INVALID", "truncated tar padding"))?;
    if !all_zero(&buffer[..n]) {
        return Err(fail("ARCHIVE_INVALID", "nonzero tar padding"));
    }
    Ok(())
}
fn all_zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0)
}
