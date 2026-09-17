//! Deterministic development package assembly. Inputs are selected by the caller;
//! this module never downloads, compiles, generates keys, or starts an engine.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::MetadataExt as StdMetadataExt;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use cap_std::ambient_authority;
use cap_std::fs::{Dir, Metadata, MetadataExt as CapMetadataExt, OpenOptions, OpenOptionsExt};
use ed25519_dalek::Signer;
use flate2::{Compression, GzBuilder};
use sha2::{Digest, Sha256};

use super::{
    FileEntry, MAX_ARCHIVE_BYTES, MAX_FILE_BYTES, MAX_FILES, MAX_MANIFEST_BYTES, MAX_TOTAL_BYTES,
    Manifest, Report, VerifyOptions, decode_strict_json, load_private_key, validate_manifest,
    validate_path, verify,
};
use crate::error::{BxdlError, Result};

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub root: PathBuf,
    pub spec_path: PathBuf,
    pub output: PathBuf,
    pub signing_key_path: Option<PathBuf>,
    pub allow_unsigned_development: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    size: u64,
    mode: u32,
    modified_seconds: i64,
    modified_nanos: i64,
}

impl Identity {
    fn cap(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            mode: metadata.mode(),
            modified_seconds: metadata.mtime(),
            modified_nanos: metadata.mtime_nsec(),
        }
    }
    fn standard(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            mode: metadata.mode(),
            modified_seconds: metadata.mtime(),
            modified_nanos: metadata.mtime_nsec(),
        }
    }
    fn same_file(&self, other: &Self) -> bool {
        self.device == other.device && self.inode == other.inode
    }
}

#[derive(Debug, Clone)]
struct StagedFile {
    entry: FileEntry,
    identity: Identity,
}

/// Build and then independently verify an archive. Staging must remain stable.
/// Existing output files are never replaced, including symlink destinations.
pub fn build(options: &BuildOptions) -> Result<Report> {
    if options.root.as_os_str().is_empty()
        || options.spec_path.as_os_str().is_empty()
        || options.output.as_os_str().is_empty()
    {
        return Err(error(
            "INVALID_BUILD_OPTIONS",
            "Root, spec and output are required",
        ));
    }
    if options.signing_key_path.is_some() == options.allow_unsigned_development {
        return Err(error(
            "INVALID_BUILD_OPTIONS",
            "Select a signing key or explicit unsigned development, exclusively",
        ));
    }
    let metadata = fs::symlink_metadata(&options.root)
        .map_err(|_| error("INVALID_STAGE", "Cannot inspect the staging directory"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(error(
            "INVALID_STAGE",
            "Staging root must be a real directory",
        ));
    }
    let root_path = fs::canonicalize(&options.root)
        .map_err(|_| error("INVALID_STAGE", "Cannot resolve the staging directory"))?;
    let root = Dir::open_ambient_dir(&root_path, ambient_authority())
        .map_err(|_| error("INVALID_STAGE", "Cannot open the staging directory"))?;
    let output_path = outside_stage_path(&root_path, &options.output, false)?;
    let mut signing_identity = None;
    let signing_key = if let Some(path) = &options.signing_key_path {
        let key_path = outside_stage_path(&root_path, path, true)?;
        let metadata = fs::symlink_metadata(&key_path)
            .map_err(|_| error("INVALID_BUILD_OPTIONS", "Cannot inspect the signing key"))?;
        if !metadata.is_file() {
            return Err(error(
                "INVALID_BUILD_OPTIONS",
                "Signing key must be an external regular file",
            ));
        }
        signing_identity = Some(Identity::standard(&metadata));
        Some(load_private_key(&key_path)?)
    } else {
        None
    };

    let spec_bytes = read_spec(&options.spec_path)?;
    let mut manifest: Manifest = decode_strict_json(&spec_bytes).map_err(|_| {
        error(
            "INVALID_SPEC",
            "Spec must be one strict manifest JSON object",
        )
    })?;
    if !manifest.files.is_empty() {
        return Err(error(
            "INVALID_SPEC",
            "Spec must omit its inventory or provide an empty inventory",
        ));
    }
    if !valid_hash(&manifest.engine.jar_sha256) || !valid_hash(&manifest.runtime.java_sha256) {
        return Err(error(
            "INVALID_SPEC",
            "Spec must supply expected engine and Java SHA-256 identities",
        ));
    }
    validate_manifest(&manifest, false)?;
    let files = collect_stage(&root)?;
    for file in &files {
        if signing_identity
            .as_ref()
            .is_some_and(|key| key.same_file(&file.identity))
        {
            return Err(error(
                "INVALID_STAGE",
                "Signing key must not have a staged hard-link alias",
            ));
        }
        if file.entry.path == "engine/nigo-node.jar"
            && file.entry.sha256 != manifest.engine.jar_sha256
        {
            return Err(error(
                "HASH_MISMATCH",
                "Staged engine does not match the expected identity",
            ));
        }
        if file.entry.path == "runtime/bin/java"
            && file.entry.sha256 != manifest.runtime.java_sha256
        {
            return Err(error(
                "HASH_MISMATCH",
                "Staged Java does not match the expected identity",
            ));
        }
    }
    manifest.files = files.iter().map(|file| file.entry.clone()).collect();
    validate_manifest(&manifest, true)?;
    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|_| error("INVALID_SPEC", "Cannot encode the manifest"))?;
    manifest_bytes.push(b'\n');
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(error(
            "INVALID_SPEC",
            "Generated manifest exceeds the supported size",
        ));
    }
    let signature = signing_key.as_ref().map(|key| {
        base64::engine::general_purpose::STANDARD
            .encode(key.sign(&manifest_bytes).to_bytes())
            .into_bytes()
    });

    let mut output = NewOutput::create(&output_path)?;
    write_archive(
        output.file.as_mut().expect("new output owns its file"),
        &root,
        &manifest_bytes,
        signature.as_deref(),
        &files,
    )?;
    output
        .file
        .as_ref()
        .expect("output remains open")
        .sync_all()
        .map_err(|_| error("BUILD_IO", "Cannot flush the output archive"))?;
    drop(output.file.take());
    check_stage_unchanged(&root, &files)?;
    if read_spec(&options.spec_path) != Ok(spec_bytes) {
        return Err(error(
            "INPUT_CHANGED",
            "Package spec changed during the build",
        ));
    }
    output.require_original_path(&output_path)?;
    let report = if let Some(key) = &signing_key {
        super::verify::verify_with_key(&output_path, &key.verifying_key())?
    } else {
        verify(
            &output_path,
            &VerifyOptions {
                public_key_path: None,
                allow_unsigned_development: true,
            },
        )?
    };
    output.require_original_path(&output_path)?;
    output.complete = true;
    Ok(report)
}

struct NewOutput {
    parent: Dir,
    name: OsString,
    file: Option<cap_std::fs::File>,
    identity: Identity,
    complete: bool,
}

impl NewOutput {
    fn create(path: &Path) -> Result<Self> {
        let parent = Dir::open_ambient_dir(
            path.parent()
                .ok_or_else(|| error("INVALID_BUILD_OPTIONS", "Output parent is required"))?,
            ambient_authority(),
        )
        .map_err(|_| error("BUILD_IO", "Cannot open the output directory"))?;
        let name = path
            .file_name()
            .ok_or_else(|| error("INVALID_BUILD_OPTIONS", "Output filename is required"))?
            .to_os_string();
        let file = parent
            .open_with(
                &name,
                OpenOptions::new().write(true).create_new(true).mode(0o644),
            )
            .map_err(|err| {
                if err.kind() == io::ErrorKind::AlreadyExists {
                    error(
                        "OUTPUT_EXISTS",
                        "Output already exists; no file was replaced",
                    )
                } else {
                    error("BUILD_IO", "Cannot create the output archive")
                }
            })?;
        let metadata = file
            .metadata()
            .map_err(|_| error("BUILD_IO", "Cannot inspect the newly created output"))?;
        Ok(Self {
            parent,
            name,
            file: Some(file),
            identity: Identity::cap(&metadata),
            complete: false,
        })
    }

    fn require_original_path(&self, path: &Path) -> Result<()> {
        let anchored = self.parent.symlink_metadata(&self.name).map_err(|_| {
            error(
                "OUTPUT_CHANGED",
                "Output identity changed during verification",
            )
        })?;
        let visible = fs::symlink_metadata(path).map_err(|_| {
            error(
                "OUTPUT_CHANGED",
                "Output identity changed during verification",
            )
        })?;
        if !anchored.is_file()
            || !visible.is_file()
            || !self.identity.same_file(&Identity::cap(&anchored))
            || !self.identity.same_file(&Identity::standard(&visible))
        {
            return Err(error(
                "OUTPUT_CHANGED",
                "Output identity changed during verification",
            ));
        }
        Ok(())
    }
}

impl Drop for NewOutput {
    fn drop(&mut self) {
        drop(self.file.take());
        if !self.complete {
            if let Ok(metadata) = self.parent.symlink_metadata(&self.name) {
                if metadata.is_file() && self.identity.same_file(&Identity::cap(&metadata)) {
                    let _ = self.parent.remove_file(&self.name);
                }
            }
        }
    }
}

fn read_spec(path: &Path) -> Result<Vec<u8>> {
    let before = fs::symlink_metadata(path)
        .map_err(|_| error("INVALID_SPEC", "Cannot inspect the package spec"))?;
    if !before.is_file() || before.len() > MAX_MANIFEST_BYTES {
        return Err(error("INVALID_SPEC", "Spec must be a bounded regular file"));
    }
    let mut file =
        fs::File::open(path).map_err(|_| error("INVALID_SPEC", "Cannot read the package spec"))?;
    let opened = file
        .metadata()
        .map_err(|_| error("INVALID_SPEC", "Cannot inspect the package spec"))?;
    if !opened.is_file() || Identity::standard(&before) != Identity::standard(&opened) {
        return Err(error("INPUT_CHANGED", "Spec changed while being opened"));
    }
    let mut data = Vec::new();
    (&mut file)
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|_| error("INVALID_SPEC", "Cannot read the bounded package spec"))?;
    let after = file
        .metadata()
        .map_err(|_| error("INPUT_CHANGED", "Spec changed while being read"))?;
    if data.len() as u64 > MAX_MANIFEST_BYTES
        || Identity::standard(&before) != Identity::standard(&after)
    {
        return Err(error("INPUT_CHANGED", "Spec changed while being read"));
    }
    Ok(data)
}

fn outside_stage_path(root: &Path, path: &Path, existing: bool) -> Result<PathBuf> {
    let absolute = std::path::absolute(path)
        .map_err(|_| error("INVALID_BUILD_OPTIONS", "Cannot resolve the selected path"))?;
    let resolved = if existing {
        fs::canonicalize(&absolute)
    } else {
        absolute
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "parent required"))
            .and_then(fs::canonicalize)
            .map(|parent| parent.join(absolute.file_name().unwrap_or_default()))
    }
    .map_err(|_| {
        error(
            "INVALID_BUILD_OPTIONS",
            "Selected external path or its parent does not exist",
        )
    })?;
    if resolved.starts_with(root) {
        return Err(error(
            "INVALID_BUILD_OPTIONS",
            "Output and signing key must be outside the staging directory",
        ));
    }
    Ok(resolved)
}

fn collect_stage(root: &Dir) -> Result<Vec<StagedFile>> {
    let mut files = Vec::new();
    let mut total = 0;
    collect_directory(root, Path::new("."), &mut files, &mut total)?;
    files.sort_by(|left, right| left.entry.path.cmp(&right.entry.path));
    Ok(files)
}

fn collect_directory(
    root: &Dir,
    directory: &Path,
    files: &mut Vec<StagedFile>,
    total: &mut u64,
) -> Result<()> {
    let entries = root
        .read_dir(directory)
        .map_err(|_| error("INVALID_STAGE", "Cannot enumerate staging"))?;
    for entry in entries {
        let entry = entry.map_err(|_| error("INVALID_STAGE", "Cannot enumerate staging"))?;
        let relative = if directory == Path::new(".") {
            PathBuf::from(entry.file_name())
        } else {
            directory.join(entry.file_name())
        };
        let path = relative
            .to_str()
            .ok_or_else(|| error("INVALID_STAGE", "Payload paths must be valid UTF-8"))?;
        let metadata = root
            .symlink_metadata(&relative)
            .map_err(|_| error("INVALID_STAGE", "Cannot inspect staging"))?;
        if metadata.is_dir() {
            let first = path.split('/').next().unwrap_or_default();
            if !matches!(
                first,
                "bin" | "engine" | "runtime" | "deploy" | "schemas" | "docs" | "licenses"
            ) {
                return Err(error(
                    "INVALID_STAGE",
                    "Staging contains an unsupported directory",
                ));
            }
            if path != first {
                validate_path(path)?;
            }
            collect_directory(root, &relative, files, total)?;
            continue;
        }
        validate_path(path)?;
        if !metadata.is_file() {
            return Err(error(
                "INVALID_STAGE",
                "Staging payload must contain only regular files",
            ));
        }
        let mode = metadata.mode() & 0o7777;
        if mode != 0o644 && mode != 0o755 {
            return Err(error(
                "INVALID_STAGE",
                "Staged file mode must be 0644 or 0755",
            ));
        }
        if metadata.len() > MAX_FILE_BYTES
            || files.len() >= MAX_FILES
            || metadata.len() > MAX_TOTAL_BYTES.saturating_sub(*total)
        {
            return Err(error("PACKAGE_LIMIT", "Staging exceeds package limits"));
        }
        let mut staged = StagedFile {
            entry: FileEntry {
                path: path.to_owned(),
                size: metadata.len(),
                sha256: String::new(),
                mode,
            },
            identity: Identity::cap(&metadata),
        };
        staged.entry.sha256 = stream_stage_file(&mut io::sink(), root, &staged)?;
        *total += staged.entry.size;
        files.push(staged);
    }
    Ok(())
}

fn stream_stage_file(writer: &mut impl Write, root: &Dir, staged: &StagedFile) -> Result<String> {
    let path_info = root
        .symlink_metadata(&staged.entry.path)
        .map_err(|_| error("INPUT_CHANGED", "Staged input changed during the build"))?;
    if !path_info.is_file() || Identity::cap(&path_info) != staged.identity {
        return Err(error(
            "INPUT_CHANGED",
            "Staged input changed during the build",
        ));
    }
    let mut file = root
        .open(&staged.entry.path)
        .map_err(|_| error("INPUT_CHANGED", "Cannot open the original staged input"))?;
    let opened = file
        .metadata()
        .map_err(|_| error("INPUT_CHANGED", "Cannot inspect the original staged input"))?;
    if !opened.is_file() || Identity::cap(&opened) != staged.identity {
        return Err(error(
            "INPUT_CHANGED",
            "Staged input identity changed during the build",
        ));
    }
    let mut hash = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = [0_u8; 65536];
    loop {
        let allowed = (staged.entry.size + 1 - count).min(buffer.len() as u64) as usize;
        if allowed == 0 {
            break;
        }
        let read = file
            .read(&mut buffer[..allowed])
            .map_err(|_| error("BUILD_IO", "Cannot read staged payload"))?;
        if read == 0 {
            break;
        }
        count += read as u64;
        if count > staged.entry.size {
            return Err(error("INPUT_CHANGED", "Staged input grew during the build"));
        }
        writer
            .write_all(&buffer[..read])
            .map_err(|_| error("BUILD_IO", "Cannot write the staged payload"))?;
        hash.update(&buffer[..read]);
    }
    let after = file
        .metadata()
        .map_err(|_| error("INPUT_CHANGED", "Staged input changed during the build"))?;
    let current = root
        .symlink_metadata(&staged.entry.path)
        .map_err(|_| error("INPUT_CHANGED", "Staged input changed during the build"))?;
    if count != staged.entry.size
        || !after.is_file()
        || !current.is_file()
        || Identity::cap(&after) != staged.identity
        || Identity::cap(&current) != staged.identity
    {
        return Err(error(
            "INPUT_CHANGED",
            "Staged input changed during the build",
        ));
    }
    let digest = hex::encode(hash.finalize());
    if !staged.entry.sha256.is_empty() && staged.entry.sha256 != digest {
        return Err(error(
            "INPUT_CHANGED",
            "Staged input content changed during the build",
        ));
    }
    Ok(digest)
}

fn check_stage_unchanged(root: &Dir, files: &[StagedFile]) -> Result<()> {
    let again = collect_stage(root)
        .map_err(|_| error("INPUT_CHANGED", "Staging changed during the build"))?;
    if files.len() != again.len()
        || files
            .iter()
            .zip(&again)
            .any(|(before, after)| before.identity != after.identity || before.entry != after.entry)
    {
        return Err(error("INPUT_CHANGED", "Staging changed during the build"));
    }
    Ok(())
}

fn write_archive(
    output: &mut impl Write,
    root: &Dir,
    manifest: &[u8],
    signature: Option<&[u8]>,
    files: &[StagedFile],
) -> Result<()> {
    let limited = ArchiveWriter {
        writer: output,
        remaining: MAX_ARCHIVE_BYTES,
    };
    let mut gzip = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(limited, Compression::best());
    write_small_entry(&mut gzip, "manifest.json", manifest)?;
    if let Some(signature) = signature {
        write_small_entry(&mut gzip, "manifest.sig", signature)?;
    }
    for file in files {
        gzip.write_all(&tar_header(
            &file.entry.path,
            file.entry.size,
            file.entry.mode,
        )?)
        .map_err(|_| error("BUILD_IO", "Cannot write a payload header"))?;
        stream_stage_file(&mut gzip, root, file)?;
        write_padding(&mut gzip, file.entry.size)?;
    }
    gzip.write_all(&[0; 1024])
        .map_err(|_| error("BUILD_IO", "Cannot finish the tar archive"))?;
    gzip.finish()
        .map_err(|_| error("BUILD_IO", "Cannot finish the compressed archive"))?;
    Ok(())
}

fn write_small_entry(writer: &mut impl Write, name: &str, bytes: &[u8]) -> Result<()> {
    writer
        .write_all(&tar_header(name, bytes.len() as u64, 0o644)?)
        .and_then(|_| writer.write_all(bytes))
        .map_err(|_| error("BUILD_IO", "Cannot write package metadata"))?;
    write_padding(writer, bytes.len() as u64)
}

fn write_padding(writer: &mut impl Write, size: u64) -> Result<()> {
    let length = ((512 - size % 512) % 512) as usize;
    writer
        .write_all(&[0; 512][..length])
        .map_err(|_| error("BUILD_IO", "Cannot write tar padding"))
}

fn tar_header(path: &str, size: u64, mode: u32) -> Result<[u8; 512]> {
    let mut header = [0_u8; 512];
    let (prefix, name) = if path.len() <= 100 {
        ("", path)
    } else {
        path.match_indices('/')
            .rev()
            .find_map(|(offset, _)| {
                let prefix = &path[..offset];
                let name = &path[offset + 1..];
                (prefix.len() <= 155 && name.len() <= 100).then_some((prefix, name))
            })
            .ok_or_else(|| error("BUILD_IO", "Cannot encode payload path in USTAR format"))?
    };
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
    write_octal(&mut header[100..108], mode as u64)?;
    write_octal(&mut header[108..116], 0)?;
    write_octal(&mut header[116..124], 0)?;
    write_octal(&mut header[124..136], size)?;
    write_octal(&mut header[136..148], 0)?;
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u64 = header.iter().map(|byte| *byte as u64).sum();
    let checksum_text = format!("{checksum:06o}");
    if checksum_text.len() != 6 {
        return Err(error("BUILD_IO", "Cannot encode tar checksum"));
    }
    header[148..154].copy_from_slice(checksum_text.as_bytes());
    header[154] = 0;
    header[155] = b' ';
    Ok(header)
}

fn write_octal(field: &mut [u8], value: u64) -> Result<()> {
    let value = format!("{value:o}");
    if value.len() >= field.len() {
        return Err(error("BUILD_IO", "Tar numeric field overflow"));
    }
    field.fill(b'0');
    let offset = field.len() - 1 - value.len();
    field[offset..offset + value.len()].copy_from_slice(value.as_bytes());
    let end = field.len() - 1;
    field[end] = 0;
    Ok(())
}

struct ArchiveWriter<'a, W> {
    writer: &'a mut W,
    remaining: u64,
}
impl<W: Write> Write for ArchiveWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::other("compressed archive size limit"));
        }
        let written = self.writer.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}

#[cfg(test)]
mod tests;
