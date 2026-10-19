//! Offline maintainer staging of a complete runtime pinned by a trusted lock.
//! No download, archive extraction, JVM execution, or package support claim.
use crate::{
    artifact,
    error::{BxdlError, Result},
};
use cap_std::{
    ambient_authority,
    fs::{
        Dir, DirBuilder, DirBuilderExt, Metadata, MetadataExt, OpenOptions, OpenOptionsExt,
        Permissions, PermissionsExt,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    os::unix::fs::MetadataExt as _,
    path::{Component, Path, PathBuf},
};

#[derive(Debug)]
pub struct StageOptions {
    pub source: PathBuf,
    pub lock: PathBuf,
    pub output: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeLock {
    pub schema_version: u32,
    pub kind: String,
    pub vendor: String,
    pub version: String,
    pub platform: Platform,
    pub archive: Archive,
    pub source_archive: Archive,
    pub java_sha256: String,
    pub files: Vec<artifact::FileEntry>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Platform {
    pub os: String,
    pub arch: String,
    pub java_major: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Archive {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageReport {
    pub vendor: String,
    pub version: String,
    pub java_sha256: String,
    pub lock_sha256: String,
    pub files_verified: usize,
    pub bytes_verified: u64,
    pub symlinks_materialized: usize,
    pub archive_verification: &'static str,
    pub runtime_execution: &'static str,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    size: u64,
    mode: u32,
    links: u64,
    modified: (i64, i64),
}
impl Stamp {
    fn of(m: &Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
            size: m.len(),
            mode: m.mode(),
            links: m.nlink(),
            modified: (m.mtime(), m.mtime_nsec()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Observed {
    stamp: Stamp,
    link: Option<PathBuf>,
}
type Snapshot = BTreeMap<String, Observed>;
fn fail(code: &str) -> BxdlError {
    BxdlError::new(
        code,
        match code {
            "RUNTIME_LOCK_INVALID" => {
                "Runtime lock must contain a complete, valid pinned Java 21 inventory"
            }
            "RUNTIME_INVENTORY_MISMATCH" => "Runtime files do not match the trusted lock",
            "RUNTIME_SOURCE_UNSAFE" => "Runtime source paths, file types or modes are unsafe",
            "RUNTIME_INPUT_CHANGED" => "Runtime staging input changed during verification",
            "RUNTIME_OUTPUT_EXISTS" => {
                "Runtime staging output already exists; no files were replaced"
            }
            "RUNTIME_OUTPUT_INVALID" => {
                "Runtime staging requires a new directory outside the source"
            }
            _ => "Runtime staging failed; a partial output may remain without provenance",
        },
    )
}
fn hash(raw: &[u8]) -> String {
    hex::encode(Sha256::digest(raw))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
fn safe_path(path: &str) -> Result<()> {
    artifact::validate_path(&format!("runtime/{path}")).map_err(|_| fail("RUNTIME_LOCK_INVALID"))
}
fn validate(lock: &RuntimeLock) -> Result<BTreeSet<String>> {
    if lock.schema_version != 1
        || lock.kind != "bxdl-runtime-lock"
        || !label(&lock.vendor)
        || !label(&lock.version)
        || lock.platform.os != "darwin"
        || lock.platform.arch != "arm64"
        || lock.platform.java_major != 21
        || !valid_hash(&lock.java_sha256)
        || lock.files.is_empty()
        || lock.files.len() > artifact::MAX_FILES
    {
        return Err(fail("RUNTIME_LOCK_INVALID"));
    }
    for archive in [&lock.archive, &lock.source_archive] {
        if !label(&archive.name)
            || archive.name.contains(['/', '\\'])
            || archive.name == "."
            || archive.name == ".."
            || !archive.url.starts_with("https://")
            || archive.url.len() > 2048
            || archive.url.chars().any(char::is_whitespace)
            || archive.url.chars().any(char::is_control)
            || !valid_hash(&archive.sha256)
            || archive.size_bytes == 0
        {
            return Err(fail("RUNTIME_LOCK_INVALID"));
        }
    }
    let mut paths = BTreeMap::new();
    let mut dirs = BTreeSet::new();
    let mut total = 0_u64;
    for file in &lock.files {
        safe_path(&file.path)?;
        if !valid_hash(&file.sha256)
            || ![0o644, 0o755].contains(&file.mode)
            || file.size > artifact::MAX_FILE_BYTES
            || paths.insert(file.path.as_str(), file).is_some()
            || total > artifact::MAX_TOTAL_BYTES.saturating_sub(file.size)
        {
            return Err(fail("RUNTIME_LOCK_INVALID"));
        }
        total += file.size;
        let mut parent = Path::new(&file.path).parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            dirs.insert(
                path.to_str()
                    .ok_or_else(|| fail("RUNTIME_LOCK_INVALID"))?
                    .to_owned(),
            );
            parent = path.parent();
        }
    }
    if dirs.iter().any(|path| paths.contains_key(path.as_str())) {
        return Err(fail("RUNTIME_LOCK_INVALID"));
    }
    for required in ["bin/java", "release", "legal/java.base/LICENSE"] {
        if paths.get(required).is_none_or(|f| f.size == 0) {
            return Err(fail("RUNTIME_LOCK_INVALID"));
        }
    }
    if paths["bin/java"].sha256 != lock.java_sha256 || paths["bin/java"].mode != 0o755 {
        return Err(fail("RUNTIME_LOCK_INVALID"));
    }
    Ok(dirs)
}
fn open_root(path: &Path) -> Result<(PathBuf, Dir)> {
    let metadata = fs::symlink_metadata(path).map_err(|_| fail("RUNTIME_SOURCE_UNSAFE"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(fail("RUNTIME_SOURCE_UNSAFE"));
    }
    let canonical = fs::canonicalize(path).map_err(|_| fail("RUNTIME_SOURCE_UNSAFE"))?;
    let dir = Dir::open_ambient_dir(&canonical, ambient_authority())
        .map_err(|_| fail("RUNTIME_SOURCE_UNSAFE"))?;
    let opened = dir
        .dir_metadata()
        .map_err(|_| fail("RUNTIME_SOURCE_UNSAFE"))?;
    if metadata.dev() != opened.dev() || metadata.ino() != opened.ino() {
        return Err(fail("RUNTIME_INPUT_CHANGED"));
    }
    Ok((canonical, dir))
}
fn open_input(root: &Dir, path: &Path) -> Result<cap_std::fs::File> {
    root.open_with(
        path,
        OpenOptions::new().read(true).custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        ),
    )
    .map_err(|_| fail("RUNTIME_INPUT_CHANGED"))
}
fn read_lock(path: &Path) -> Result<Vec<u8>> {
    let parent = path
        .parent()
        .filter(|v| !v.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| fail("RUNTIME_LOCK_INVALID"))?;
    let (_, dir) = open_root(parent)?;
    let metadata = dir
        .symlink_metadata(name)
        .map_err(|_| fail("RUNTIME_LOCK_INVALID"))?;
    if !metadata.is_file() || metadata.len() > artifact::MAX_MANIFEST_BYTES {
        return Err(fail("RUNTIME_LOCK_INVALID"));
    }
    let mut file = open_input(&dir, Path::new(name))?;
    let stamp = Stamp::of(&metadata);
    if Stamp::of(&file.metadata().map_err(|_| fail("RUNTIME_LOCK_INVALID"))?) != stamp {
        return Err(fail("RUNTIME_INPUT_CHANGED"));
    }
    let mut raw = Vec::new();
    Read::by_ref(&mut file)
        .take(artifact::MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(|_| fail("RUNTIME_LOCK_INVALID"))?;
    if raw.len() as u64 != stamp.size
        || Stamp::of(&file.metadata().map_err(|_| fail("RUNTIME_INPUT_CHANGED"))?) != stamp
        || Stamp::of(
            &dir.symlink_metadata(name)
                .map_err(|_| fail("RUNTIME_INPUT_CHANGED"))?,
        ) != stamp
    {
        return Err(fail("RUNTIME_INPUT_CHANGED"));
    }
    Ok(raw)
}
fn scan(root: &Dir, expected_dirs: &BTreeSet<String>) -> Result<Snapshot> {
    let mut snapshot = BTreeMap::new();
    scan_dir(root, Path::new("."), expected_dirs, &mut snapshot)?;
    Ok(snapshot)
}
fn scan_dir(
    root: &Dir,
    path: &Path,
    expected_dirs: &BTreeSet<String>,
    snapshot: &mut Snapshot,
) -> Result<()> {
    for entry in root
        .read_dir(path)
        .map_err(|_| fail("RUNTIME_SOURCE_UNSAFE"))?
    {
        let entry = entry.map_err(|_| fail("RUNTIME_SOURCE_UNSAFE"))?;
        let relative = if path == Path::new(".") {
            PathBuf::from(entry.file_name())
        } else {
            path.join(entry.file_name())
        };
        let name = relative
            .to_str()
            .ok_or_else(|| fail("RUNTIME_SOURCE_UNSAFE"))?
            .to_owned();
        safe_path(&name)?;
        let m = root
            .symlink_metadata(&relative)
            .map_err(|_| fail("RUNTIME_SOURCE_UNSAFE"))?;
        let link = if m.file_type().is_symlink() {
            Some(
                root.read_link(&relative)
                    .map_err(|_| fail("RUNTIME_SOURCE_UNSAFE"))?,
            )
        } else {
            None
        };
        if snapshot.len() >= artifact::MAX_FILES * 8 {
            return Err(fail("RUNTIME_SOURCE_UNSAFE"));
        }
        snapshot.insert(
            name.clone(),
            Observed {
                stamp: Stamp::of(&m),
                link,
            },
        );
        if m.is_dir() {
            if !expected_dirs.contains(&name) {
                return Err(fail("RUNTIME_INVENTORY_MISMATCH"));
            }
            scan_dir(root, &relative, expected_dirs, snapshot)?;
        } else if !m.is_file() && !m.file_type().is_symlink() {
            return Err(fail("RUNTIME_SOURCE_UNSAFE"));
        }
    }
    Ok(())
}
fn normalize_link(parent: &Path, target: &Path) -> Result<PathBuf> {
    if target.is_absolute() {
        return Err(fail("RUNTIME_SOURCE_UNSAFE"));
    }
    let mut output = parent.to_path_buf();
    for component in target.components() {
        match component {
            Component::Normal(name) => output.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                if !output.pop() {
                    return Err(fail("RUNTIME_SOURCE_UNSAFE"));
                }
            }
            _ => return Err(fail("RUNTIME_SOURCE_UNSAFE")),
        }
    }
    safe_path(
        output
            .to_str()
            .ok_or_else(|| fail("RUNTIME_SOURCE_UNSAFE"))?,
    )
    .map_err(|_| fail("RUNTIME_SOURCE_UNSAFE"))?;
    Ok(output)
}
fn resolve<'a>(path: &str, snapshot: &'a Snapshot) -> Result<(PathBuf, &'a Observed)> {
    let mut current = PathBuf::from(path);
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(current.clone()) || visited.len() > 40 {
            return Err(fail("RUNTIME_SOURCE_UNSAFE"));
        }
        let mut ancestor = current.parent();
        while let Some(parent) = ancestor.filter(|p| !p.as_os_str().is_empty()) {
            let m = snapshot
                .get(
                    parent
                        .to_str()
                        .ok_or_else(|| fail("RUNTIME_SOURCE_UNSAFE"))?,
                )
                .ok_or_else(|| fail("RUNTIME_SOURCE_UNSAFE"))?;
            if m.link.is_some() || m.stamp.mode & 0o170000 != 0o040000 {
                return Err(fail("RUNTIME_SOURCE_UNSAFE"));
            }
            ancestor = parent.parent();
        }
        let item = snapshot
            .get(
                current
                    .to_str()
                    .ok_or_else(|| fail("RUNTIME_SOURCE_UNSAFE"))?,
            )
            .ok_or_else(|| fail("RUNTIME_SOURCE_UNSAFE"))?;
        if let Some(target) = &item.link {
            current = normalize_link(current.parent().unwrap_or(Path::new("")), target)?;
        } else if item.stamp.mode & 0o170000 == 0o100000 {
            return Ok((current, item));
        } else {
            return Err(fail("RUNTIME_SOURCE_UNSAFE"));
        }
    }
}
fn copy_file(
    root: &Dir,
    snapshot: &Snapshot,
    expected: &artifact::FileEntry,
    writer: &mut impl Write,
) -> Result<()> {
    let (path, observed) = resolve(&expected.path, snapshot)?;
    let mode = observed.stamp.mode & 0o7777;
    if mode & 0o7022 != 0
        || mode & 0o400 == 0
        || (if mode & 0o111 != 0 { 0o755 } else { 0o644 }) != expected.mode
        || observed.stamp.size != expected.size
    {
        return Err(fail("RUNTIME_SOURCE_UNSAFE"));
    }
    let before = root
        .symlink_metadata(&path)
        .map_err(|_| fail("RUNTIME_INPUT_CHANGED"))?;
    if !before.is_file() || Stamp::of(&before) != observed.stamp {
        return Err(fail("RUNTIME_INPUT_CHANGED"));
    }
    let mut file = open_input(root, &path)?;
    if Stamp::of(&file.metadata().map_err(|_| fail("RUNTIME_INPUT_CHANGED"))?) != observed.stamp {
        return Err(fail("RUNTIME_INPUT_CHANGED"));
    }
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0; 65536];
    loop {
        let limit = (expected.size + 1 - total).min(buffer.len() as u64) as usize;
        if limit == 0 {
            break;
        }
        let count = file
            .read(&mut buffer[..limit])
            .map_err(|_| fail("RUNTIME_INPUT_CHANGED"))?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > expected.size {
            return Err(fail("RUNTIME_INPUT_CHANGED"));
        }
        hasher.update(&buffer[..count]);
        writer
            .write_all(&buffer[..count])
            .map_err(|_| fail("RUNTIME_STAGE_IO"))?;
    }
    if total != expected.size || hex::encode(hasher.finalize()) != expected.sha256 {
        return Err(fail("RUNTIME_INVENTORY_MISMATCH"));
    }
    if Stamp::of(&file.metadata().map_err(|_| fail("RUNTIME_INPUT_CHANGED"))?) != observed.stamp
        || Stamp::of(
            &root
                .symlink_metadata(&path)
                .map_err(|_| fail("RUNTIME_INPUT_CHANGED"))?,
        ) != observed.stamp
    {
        return Err(fail("RUNTIME_INPUT_CHANGED"));
    }
    Ok(())
}
fn require_inventory(
    snapshot: &Snapshot,
    lock: &RuntimeLock,
    dirs: &BTreeSet<String>,
) -> Result<()> {
    let expected: BTreeSet<_> = lock
        .files
        .iter()
        .map(|f| f.path.as_str())
        .chain(dirs.iter().map(String::as_str))
        .collect();
    if snapshot.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected {
        return Err(fail("RUNTIME_INVENTORY_MISMATCH"));
    }
    for file in &lock.files {
        let _ = resolve(&file.path, snapshot)?;
    }
    Ok(())
}
fn create_directory(root: &Dir, path: &Path) -> Result<()> {
    let mut builder = DirBuilder::new();
    builder.mode(0o700);
    root.create_dir_with(path, &builder)
        .map_err(|_| fail("RUNTIME_STAGE_IO"))
}
fn new_file(root: &Dir, path: &Path) -> Result<cap_std::fs::File> {
    root.open_with(
        path,
        OpenOptions::new().write(true).create_new(true).mode(0o600),
    )
    .map_err(|_| fail("RUNTIME_STAGE_IO"))
}
fn visible_matches(path: &Path, root: &Dir) -> Result<()> {
    let visible = fs::symlink_metadata(path).map_err(|_| fail("RUNTIME_INPUT_CHANGED"))?;
    let anchored = root
        .dir_metadata()
        .map_err(|_| fail("RUNTIME_INPUT_CHANGED"))?;
    if !visible.is_dir() || visible.dev() != anchored.dev() || visible.ino() != anchored.ino() {
        return Err(fail("RUNTIME_INPUT_CHANGED"));
    }
    Ok(())
}
/// A supplied lock is the trust input. Its archive references are recorded, not downloaded or authenticated here.
/// Failure preserves any newly created private output, without a successful provenance file.
pub fn stage(options: &StageOptions) -> Result<StageReport> {
    let raw = read_lock(&options.lock)?;
    let lock: RuntimeLock =
        artifact::decode_strict_json(&raw).map_err(|_| fail("RUNTIME_LOCK_INVALID"))?;
    let dirs = validate(&lock)?;
    let (source_path, source) = open_root(&options.source)?;
    let absolute =
        std::path::absolute(&options.output).map_err(|_| fail("RUNTIME_OUTPUT_INVALID"))?;
    let name = absolute
        .file_name()
        .ok_or_else(|| fail("RUNTIME_OUTPUT_INVALID"))?;
    let (parent_path, parent) = open_root(
        absolute
            .parent()
            .ok_or_else(|| fail("RUNTIME_OUTPUT_INVALID"))?,
    )?;
    let output_path = parent_path.join(name);
    if output_path.starts_with(&source_path) || source_path.starts_with(&output_path) {
        return Err(fail("RUNTIME_OUTPUT_INVALID"));
    }
    match parent.symlink_metadata(name) {
        Ok(_) => return Err(fail("RUNTIME_OUTPUT_EXISTS")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(fail("RUNTIME_OUTPUT_INVALID")),
    }
    let snapshot = scan(&source, &dirs)?;
    require_inventory(&snapshot, &lock, &dirs)?;
    for file in &lock.files {
        copy_file(&source, &snapshot, file, &mut std::io::sink())?;
    }
    if scan(&source, &dirs)? != snapshot || read_lock(&options.lock)? != raw {
        return Err(fail("RUNTIME_INPUT_CHANGED"));
    }
    let mut builder = DirBuilder::new();
    builder.mode(0o700);
    parent.create_dir_with(name, &builder).map_err(|e| {
        fail(if e.kind() == std::io::ErrorKind::AlreadyExists {
            "RUNTIME_OUTPUT_EXISTS"
        } else {
            "RUNTIME_STAGE_IO"
        })
    })?;
    let created = parent
        .symlink_metadata(name)
        .map_err(|_| fail("RUNTIME_STAGE_IO"))?;
    if !created.is_dir() || created.mode() & 0o7777 != 0o700 {
        return Err(fail("RUNTIME_INPUT_CHANGED"));
    }
    let output = parent
        .open_dir(name)
        .map_err(|_| fail("RUNTIME_STAGE_IO"))?;
    let opened = output
        .dir_metadata()
        .map_err(|_| fail("RUNTIME_STAGE_IO"))?;
    if created.dev() != opened.dev() || created.ino() != opened.ino() {
        return Err(fail("RUNTIME_INPUT_CHANGED"));
    }
    create_directory(&output, Path::new("runtime"))?;
    create_directory(&output, Path::new("docs"))?;
    let runtime = output
        .open_dir("runtime")
        .map_err(|_| fail("RUNTIME_STAGE_IO"))?;
    for dir in &dirs {
        create_directory(&runtime, Path::new(dir))?;
    }
    for entry in &lock.files {
        let mut file = new_file(&runtime, Path::new(&entry.path))?;
        copy_file(&source, &snapshot, entry, &mut file)?;
        file.set_permissions(Permissions::from_mode(entry.mode))
            .map_err(|_| fail("RUNTIME_STAGE_IO"))?;
        file.sync_all().map_err(|_| fail("RUNTIME_STAGE_IO"))?;
    }
    let installed = scan(&runtime, &dirs)?;
    require_inventory(&installed, &lock, &dirs)?;
    if installed.values().any(|entry| {
        entry.link.is_some() || (entry.stamp.mode & 0o170000 == 0o100000 && entry.stamp.links != 1)
    }) {
        return Err(fail("RUNTIME_STAGE_IO"));
    }
    for file in &lock.files {
        copy_file(&runtime, &installed, file, &mut std::io::sink())?;
    }
    if scan(&source, &dirs)? != snapshot || read_lock(&options.lock)? != raw {
        return Err(fail("RUNTIME_INPUT_CHANGED"));
    }
    visible_matches(&source_path, &source)?;
    visible_matches(&output_path, &output)?;
    let docs = output
        .open_dir("docs")
        .map_err(|_| fail("RUNTIME_STAGE_IO"))?;
    let mut provenance = new_file(&docs, Path::new("runtime-provenance.pending"))?;
    provenance
        .write_all(&raw)
        .map_err(|_| fail("RUNTIME_STAGE_IO"))?;
    provenance
        .set_permissions(Permissions::from_mode(0o644))
        .map_err(|_| fail("RUNTIME_STAGE_IO"))?;
    provenance
        .sync_all()
        .map_err(|_| fail("RUNTIME_STAGE_IO"))?;
    // Publish only complete, flushed provenance; never replace an existing path.
    rustix::fs::renameat_with(
        &docs,
        "runtime-provenance.pending",
        &docs,
        "runtime-provenance.json",
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(|_| fail("RUNTIME_STAGE_IO"))?;
    Ok(StageReport {
        vendor: lock.vendor,
        version: lock.version,
        java_sha256: lock.java_sha256,
        lock_sha256: hash(&raw),
        files_verified: lock.files.len(),
        bytes_verified: lock.files.iter().map(|f| f.size).sum(),
        symlinks_materialized: snapshot.values().filter(|s| s.link.is_some()).count(),
        archive_verification: "NOT_PERFORMED",
        runtime_execution: "NOT_PERFORMED",
    })
}
#[cfg(test)]
mod tests;
