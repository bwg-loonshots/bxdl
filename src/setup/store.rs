//! Append-only checkpoints and exclusive output publication. This is a local
//! user store, not a security boundary against a process with the same UID.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use cap_std::ambient_authority;
use cap_std::fs::{
    Dir, DirBuilder, DirBuilderExt, Metadata, MetadataExt, OpenOptions, OpenOptionsExt,
};

use crate::error::{BxdlError, Result};

const MAX_BYTES: usize = 256 * 1024;
const MAX_REVISIONS: u32 = 4096;
const MAX_ENTRIES: usize = 8192;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
// Unix ABI flags: refuse final symlinks and never block on a substituted FIFO.
#[cfg(target_os = "macos")]
const READ_FLAGS: i32 = 0x100 | 0x4;
#[cfg(target_os = "linux")]
const READ_FLAGS: i32 = 0x20000 | 0x800;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    mode: u32,
    size: u64,
    seconds: i64,
    nanos: i64,
}
impl Identity {
    fn of(m: &Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
            mode: m.mode(),
            size: m.len(),
            seconds: m.mtime(),
            nanos: m.mtime_nsec(),
        }
    }
    fn same_inode(&self, m: &Metadata) -> bool {
        self.device == m.dev() && self.inode == m.ino()
    }
}

#[derive(Clone)]
struct Checkpoint {
    revision: u32,
    identity: Identity,
    raw: Vec<u8>,
}

pub struct Store {
    path: PathBuf,
    directory: Dir,
    identity: Identity,
    loaded: Option<Checkpoint>,
}
impl Store {
    pub fn create(path: &Path) -> Result<Self> {
        let (path, parent, name) = parent_anchor_with_creation(path, true)?;
        let mut options = DirBuilder::new();
        options.mode(0o700);
        parent.create_dir_with(&name, &options).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                err(
                    "SETUP_STATE_EXISTS",
                    "이미 초안 작업 폴더가 있습니다. --resume으로 이어가거나 새 --workspace를 지정하세요.",
                )
            } else {
                io_error()
            }
        })?;
        sync_directory(&parent)?;
        Self::open(&path)
    }

    pub fn open(path: &Path) -> Result<Self> {
        let (path, parent, name) = parent_anchor(path)?;
        let before = parent.symlink_metadata(&name).map_err(|_| io_error())?;
        require_directory(&before)?;
        let directory = parent.open_dir(&name).map_err(|_| unsafe_state())?;
        let after = directory.dir_metadata().map_err(|_| io_error())?;
        if !Identity::of(&before).same_inode(&after) {
            return Err(unsafe_state());
        }
        require_directory(&after)?;
        let mut store = Self {
            path,
            directory,
            identity: Identity::of(&after),
            loaded: None,
        };
        store.ensure_anchor()?;
        store.loaded = latest(&store.directory)?;
        Ok(store)
    }

    pub fn read(&self) -> Result<Option<Vec<u8>>> {
        self.ensure_anchor()?;
        let current = latest(&self.directory)?;
        compare_loaded(&self.loaded, &current)?;
        Ok(current.map(|c| c.raw))
    }

    pub fn save(&mut self, raw: &[u8]) -> Result<()> {
        check_size(raw)?;
        self.ensure_anchor()?;
        let current = self.current_for_save()?;
        compare_loaded(&self.loaded, &current)?;
        let revision = current.as_ref().map_or(1, |c| c.revision + 1);
        if revision > MAX_REVISIONS {
            return Err(limit_error());
        }
        let name = revision_name(revision);
        let mut temp = PendingFile::create(&self.directory, raw)?;
        // Recheck after writing: another writer may have completed while we wrote.
        self.ensure_anchor()?;
        compare_loaded(&self.loaded, &self.current_for_save()?)?;
        temp.publish(&name, "SETUP_CONFLICT")?;
        let current = latest(&self.directory)?;
        let checkpoint = current.as_ref().ok_or_else(corrupt_state)?;
        if checkpoint.revision != revision
            || checkpoint.raw != raw
            || !temp.identity.same_inode(
                &self
                    .directory
                    .symlink_metadata(&name)
                    .map_err(|_| io_error())?,
            )
        {
            return Err(corrupt_state());
        }
        self.loaded = current;
        self.ensure_anchor()?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn current_for_save(&self) -> Result<Option<Checkpoint>> {
        match latest(&self.directory) {
            Err(_)
                if self
                    .directory
                    .symlink_metadata(revision_name(
                        self.loaded.as_ref().map_or(1, |c| c.revision + 1),
                    ))
                    .is_ok() =>
            {
                Err(err(
                    "SETUP_CONFLICT",
                    "다른 작업이 초안을 갱신했습니다. --resume으로 최신 상태를 다시 여세요.",
                ))
            }
            result => result,
        }
    }

    fn ensure_anchor(&self) -> Result<()> {
        let (_, parent, name) = parent_anchor(&self.path)?;
        let visible = parent.symlink_metadata(&name).map_err(|_| unsafe_state())?;
        let anchored = self.directory.dir_metadata().map_err(|_| io_error())?;
        require_directory(&visible)?;
        require_directory(&anchored)?;
        if !self.identity.same_inode(&visible) || !self.identity.same_inode(&anchored) {
            return Err(unsafe_state());
        }
        Ok(())
    }
}

/// Publish a complete 0600 output; an existing destination is never replaced.
pub fn write_new(path: &Path, raw: &[u8]) -> Result<()> {
    check_size(raw)?;
    let (absolute, parent, name) = parent_anchor(path)?;
    let parent_identity = Identity::of(&parent.dir_metadata().map_err(|_| io_error())?);
    if parent.symlink_metadata(&name).is_ok() {
        return Err(err(
            "OUTPUT_EXISTS",
            "출력 파일이 이미 있습니다. 기존 파일은 보존되며 다른 출력 경로가 필요합니다.",
        ));
    }
    let mut temp = PendingFile::create(&parent, raw)?;
    let (_, visible_parent, _) = parent_anchor(&absolute)?;
    if !parent_identity.same_inode(&visible_parent.dir_metadata().map_err(|_| io_error())?) {
        return Err(unsafe_state());
    }
    temp.publish(&name, "OUTPUT_EXISTS")?;
    let (_, visible_parent, _) = parent_anchor(&absolute)?;
    let visible = visible_parent
        .symlink_metadata(&name)
        .map_err(|_| unsafe_state())?;
    if !parent_identity.same_inode(&visible_parent.dir_metadata().map_err(|_| io_error())?)
        || !temp.identity.same_inode(&visible)
    {
        return Err(unsafe_state());
    }
    Ok(())
}

fn compare_loaded(loaded: &Option<Checkpoint>, current: &Option<Checkpoint>) -> Result<()> {
    if loaded.as_ref().map(|c| c.revision) != current.as_ref().map(|c| c.revision) {
        return Err(err(
            "SETUP_CONFLICT",
            "초안이 다른 작업에서 변경되었습니다. --resume으로 다시 여세요.",
        ));
    }
    if let (Some(a), Some(b)) = (loaded, current) {
        if a.identity != b.identity || a.raw != b.raw {
            return Err(corrupt_state());
        }
    }
    Ok(())
}

fn latest(dir: &Dir) -> Result<Option<Checkpoint>> {
    let mut revisions = BTreeMap::new();
    let mut exported_config = None;
    let mut temporary_links = BTreeMap::<(u64, u64), (u64, u64)>::new();
    for (count, entry) in dir.entries().map_err(|_| io_error())?.enumerate() {
        if count >= MAX_ENTRIES {
            return Err(limit_error());
        }
        let entry = entry.map_err(|_| io_error())?;
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(corrupt_state)?;
        let metadata = match dir.symlink_metadata(name) {
            Ok(metadata) => metadata,
            Err(e) if is_temp_name(name) && e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(corrupt_state()),
        };
        if name == "instance.json" {
            // Optional final config export belongs to the caller, not history.
            require_regular(&metadata)?;
            exported_config = Some(metadata);
        } else if is_temp_name(name) {
            // Unpublished bytes are never read. A crash may leave the private
            // name as the only additional hard link of a published checkpoint.
            require_regular(&metadata)?;
            let entry = temporary_links
                .entry((metadata.dev(), metadata.ino()))
                .or_insert((0, metadata.nlink()));
            entry.0 += 1;
        } else {
            let revision = parse_revision(name).ok_or_else(corrupt_state)?;
            require_regular(&metadata)?;
            revisions.insert(revision, metadata);
        }
    }
    for (&identity, &(links, actual)) in &temporary_links {
        let published = revisions
            .values()
            .chain(exported_config.iter())
            .filter(|m| (m.dev(), m.ino()) == identity)
            .count() as u64;
        if actual != links + published {
            return Err(unsafe_state());
        }
    }
    for (index, &revision) in revisions.keys().enumerate() {
        if revision != index as u32 + 1 {
            return Err(corrupt_state());
        }
    }
    for metadata in revisions.values().chain(exported_config.iter()) {
        let internal_links = temporary_links
            .get(&(metadata.dev(), metadata.ino()))
            .map_or(0, |v| v.0);
        if metadata.nlink() != 1 + internal_links {
            return Err(unsafe_state());
        }
    }
    let Some((&revision, metadata)) = revisions.last_key_value() else {
        return Ok(None);
    };
    let name = revision_name(revision);
    let identity = Identity::of(metadata);
    let mut options = OpenOptions::new();
    options.read(true).custom_flags(READ_FLAGS);
    let mut file = dir.open_with(&name, &options).map_err(|_| unsafe_state())?;
    let opened = file.metadata().map_err(|_| io_error())?;
    if Identity::of(&opened) != identity {
        return Err(corrupt_state());
    }
    let mut raw = Vec::with_capacity(metadata.len() as usize);
    (&mut file)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|_| io_error())?;
    if raw.len() > MAX_BYTES
        || raw.len() as u64 != metadata.len()
        || Identity::of(&file.metadata().map_err(|_| io_error())?) != identity
        || Identity::of(&dir.symlink_metadata(&name).map_err(|_| corrupt_state())?) != identity
    {
        return Err(corrupt_state());
    }
    Ok(Some(Checkpoint {
        revision,
        identity,
        raw,
    }))
}

fn revision_name(revision: u32) -> String {
    format!("revision{revision:08}.json")
}
fn parse_revision(name: &str) -> Option<u32> {
    let digits = name.strip_prefix("revision")?.strip_suffix(".json")?;
    if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    (1..=MAX_REVISIONS).contains(&n).then_some(n)
}
fn is_temp_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(".bxdl-tmp-") else {
        return false;
    };
    let parts: Vec<_> = rest.split('-').collect();
    parts.len() == 3
        && parts[0].parse::<u32>().is_ok()
        && parts[1].parse::<u64>().is_ok()
        && parts[2].len() == 32
        && parts[2].bytes().all(|b| b.is_ascii_hexdigit())
}
fn require_directory(m: &Metadata) -> Result<()> {
    if !m.is_dir() || m.mode() & 0o7777 != 0o700 {
        return Err(unsafe_state());
    }
    Ok(())
}
fn require_regular(m: &Metadata) -> Result<()> {
    if !m.is_file() || m.mode() & 0o7777 != 0o600 || m.len() > MAX_BYTES as u64 {
        return Err(unsafe_state());
    }
    Ok(())
}

/// Anchor every existing path component without accepting symlink aliases.
fn parent_anchor(path: &Path) -> Result<(PathBuf, Dir, OsString)> {
    parent_anchor_with_creation(path, false)
}
fn parent_anchor_with_creation(
    path: &Path,
    create_missing: bool,
) -> Result<(PathBuf, Dir, OsString)> {
    if path.as_os_str().is_empty() {
        return Err(unsafe_state());
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(|_| io_error())?.join(path)
    };
    let mut components = Vec::new();
    for component in absolute.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => components.push(name.to_os_string()),
            _ => return Err(unsafe_state()),
        }
    }
    let name = components.pop().ok_or_else(unsafe_state)?;
    let mut dir = Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| io_error())?;
    let mut normalized = PathBuf::from("/");
    for component in components {
        let before = match dir.symlink_metadata(&component) {
            Ok(metadata) => metadata,
            Err(e) if create_missing && e.kind() == std::io::ErrorKind::NotFound => {
                let mut options = DirBuilder::new();
                options.mode(0o700);
                match dir.create_dir_with(&component, &options) {
                    Ok(()) => sync_directory(&dir)?,
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(_) => return Err(io_error()),
                }
                dir.symlink_metadata(&component).map_err(|_| io_error())?
            }
            Err(_) => return Err(io_error()),
        };
        if !before.is_dir() {
            return Err(unsafe_state());
        }
        let child = dir.open_dir(&component).map_err(|_| unsafe_state())?;
        if !Identity::of(&before).same_inode(&child.dir_metadata().map_err(|_| io_error())?) {
            return Err(unsafe_state());
        }
        dir = child;
        normalized.push(component);
    }
    normalized.push(&name);
    Ok((normalized, dir, name))
}

struct PendingFile<'a> {
    dir: &'a Dir,
    name: String,
    identity: Identity,
}
impl<'a> PendingFile<'a> {
    fn create(dir: &'a Dir, raw: &[u8]) -> Result<Self> {
        let ticks = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| io_error())?
            .as_nanos();
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!(".bxdl-tmp-{}-{sequence}-{ticks:032x}", std::process::id());
        let mut options = OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        let mut file = dir.open_with(&name, &options).map_err(|_| io_error())?;
        let identity = Identity::of(&file.metadata().map_err(|_| io_error())?);
        let pending = Self {
            dir,
            name,
            identity,
        };
        file.write_all(raw).map_err(|_| io_error())?;
        file.sync_all().map_err(|_| io_error())?;
        let metadata = file.metadata().map_err(|_| io_error())?;
        require_regular(&metadata)?;
        if metadata.nlink() != 1 || !pending.identity.same_inode(&metadata) {
            return Err(unsafe_state());
        }
        Ok(pending)
    }
    fn publish(&mut self, name: impl AsRef<Path>, conflict_code: &str) -> Result<()> {
        let metadata = self
            .dir
            .symlink_metadata(&self.name)
            .map_err(|_| unsafe_state())?;
        require_regular(&metadata)?;
        if metadata.nlink() != 1 || !self.identity.same_inode(&metadata) {
            return Err(unsafe_state());
        }
        self.dir
            .hard_link(&self.name, self.dir, name)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    err(
                        conflict_code,
                        "다른 작업이 같은 대상에 저장했습니다. 기존 파일은 덮어쓰지 않았습니다.",
                    )
                } else {
                    io_error()
                }
            })?;
        // Persist the publication before unlinking the recoverable private name.
        sync_directory(self.dir)?;
        self.remove_private()?;
        sync_directory(self.dir)
    }
    fn remove_private(&self) -> Result<()> {
        match self.dir.symlink_metadata(&self.name) {
            Ok(m) if self.identity.same_inode(&m) => {
                self.dir.remove_file(&self.name).map_err(|_| io_error())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(unsafe_state()),
        }
    }
}
impl Drop for PendingFile<'_> {
    fn drop(&mut self) {
        let _ = self.remove_private();
    }
}
fn sync_directory(dir: &Dir) -> Result<()> {
    dir.try_clone()
        .map_err(|_| io_error())?
        .into_std_file()
        .sync_all()
        .map_err(|_| io_error())
}
fn check_size(raw: &[u8]) -> Result<()> {
    if raw.len() > MAX_BYTES {
        Err(limit_error())
    } else {
        Ok(())
    }
}
fn err(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn io_error() -> BxdlError {
    err(
        "SETUP_IO",
        "초안 저장 작업에 실패했습니다. 작업 폴더 상태를 확인한 뒤 --resume으로 다시 여세요.",
    )
}
fn unsafe_state() -> BxdlError {
    err(
        "SETUP_STATE_UNSAFE",
        "초안 경로의 파일 종류·식별·링크·권한을 확인하세요. 폴더는 0700, 파일은 0600이어야 합니다.",
    )
}
fn corrupt_state() -> BxdlError {
    err(
        "SETUP_STATE_CORRUPT",
        "저장된 초안 기록이 일치하지 않습니다. 자동으로 이전 기록으로 되돌리지 않았습니다.",
    )
}
fn limit_error() -> BxdlError {
    err(
        "SETUP_LIMIT",
        "초안 저장 크기 또는 기록 개수 한도를 초과했습니다.",
    )
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
