//! An explicitly resumed, private installation-session container. The open
//! object holds an exclusive lock until dropped. This prevents accidental
//! concurrent wizards; it is not isolation from another process of the same UID.
use super::store::Store;
use crate::{
    artifact::decode_strict_json,
    error::{BxdlError, Result},
};
use cap_std::{
    ambient_authority,
    fs::{
        Dir, DirBuilder, DirBuilderExt, File, Metadata, MetadataExt, OpenOptions, OpenOptionsExt,
    },
};
use rustix::fs::{FlockOperation, OFlags, flock};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const RECORD: &str = ".workflow.json";
const PENDING_RECORD: &str = ".workflow.pending";
const LOCK: &str = "workflow.lock";
const JOURNAL: &str = "workflow";
const DRAFT: &str = "draft";
const GENERATED: &str = "generated";
const MAX_RECORD: u64 = 4096;
const MAX_BYTES: usize = 262_144;
const MAX_ENTRIES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    device: u64,
    inode: u64,
}
impl Identity {
    fn of(m: &Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
        }
    }
    fn matches(self, m: &Metadata) -> bool {
        self == Self::of(m)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    root: Identity,
    lock: Identity,
    workflow: Identity,
    draft: Identity,
    generated: Identity,
    record: Identity,
}

pub(crate) struct WorkflowStore {
    path: PathBuf,
    draft_path: PathBuf,
    generated_path: PathBuf,
    root: Dir,
    lock: File,
    workflow_dir: Dir,
    draft_dir: Dir,
    generated_dir: Dir,
    record: Record,
    journal: Store,
}

impl WorkflowStore {
    /// Parents must already exist. A failed create leaves its new container in
    /// place; absence of a complete identity record makes it non-resumable.
    pub(crate) fn create(path: &Path, initial: &[u8], initial_draft: &[u8]) -> Result<Self> {
        check_raw(initial)?;
        check_raw(initial_draft)?;
        let (path, parent, name) = parent_anchor(path)?;
        let mut options = DirBuilder::new();
        options.mode(0o700);
        parent.create_dir_with(&name, &options).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                exists()
            } else {
                io_error()
            }
        })?;
        sync_dir(&parent)?;
        let (root, identity) = open_private(&parent, &name)?;
        let mut options = file_options();
        options.read(true).write(true).create_new(true).mode(0o600);
        let lock = root.open_with(LOCK, &options).map_err(|_| io_error())?;
        require_lock(&lock.metadata().map_err(|_| io_error())?)?;
        acquire(&lock)?;
        lock.sync_all().map_err(|_| io_error())?;

        // Create stores only inside the root just acquired, then verify their
        // visible identity before and after every later operation.
        check_root(&path, &root, identity)?;
        let mut journal = Store::create(&path.join(JOURNAL))?;
        journal.save(initial)?;
        check_root(&path, &root, identity)?;
        let mut draft = Store::create(&path.join(DRAFT))?;
        draft.save(initial_draft)?;
        check_root(&path, &root, identity)?;
        root.create_dir_with(GENERATED, DirBuilder::new().mode(0o700))
            .map_err(|_| io_error())?;
        let (workflow_dir, workflow_identity) = open_private(&root, JOURNAL)?;
        let (draft_dir, draft_identity) = open_private(&root, DRAFT)?;
        let (generated_dir, generated_identity) = open_private(&root, GENERATED)?;
        sync_dir(&workflow_dir)?;
        sync_dir(&draft_dir)?;
        sync_dir(&generated_dir)?;
        sync_dir(&root)?;

        let mut options = file_options();
        options.write(true).create_new(true).mode(0o600);
        let mut pending = root
            .open_with(PENDING_RECORD, &options)
            .map_err(|_| io_error())?;
        let record = Record {
            schema_version: 1,
            root: identity,
            lock: Identity::of(&lock.metadata().map_err(|_| io_error())?),
            workflow: workflow_identity,
            draft: draft_identity,
            generated: generated_identity,
            record: Identity::of(&pending.metadata().map_err(|_| io_error())?),
        };
        let raw = serde_json::to_vec(&record).map_err(|_| io_error())?;
        pending.write_all(&raw).map_err(|_| io_error())?;
        pending.sync_all().map_err(|_| io_error())?;
        let before = root
            .symlink_metadata(PENDING_RECORD)
            .map_err(|_| unsafe_state())?;
        require_file(&before)?;
        if !record.record.matches(&before) {
            return Err(unsafe_state());
        }
        check_root(&path, &root, identity)?;
        root.hard_link(PENDING_RECORD, &root, RECORD)
            .map_err(|_| io_error())?;
        sync_dir(&root)?;
        root.remove_file(PENDING_RECORD).map_err(|_| io_error())?;
        sync_dir(&root)?;
        let store = Self {
            draft_path: path.join(DRAFT),
            generated_path: path.join(GENERATED),
            path,
            root,
            lock,
            workflow_dir,
            draft_dir,
            generated_dir,
            record,
            journal,
        };
        store.recheck()?;
        store.read()?;
        Ok(store)
    }

    /// Explicit resume only; missing locks, records or directories are never
    /// repaired, and a partial create is never silently adopted.
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let (path, parent, name) = parent_anchor(path)?;
        let (root, identity) = open_private(&parent, &name)?;
        let visible_lock = root.symlink_metadata(LOCK).map_err(|_| unsafe_state())?;
        require_lock(&visible_lock)?;
        let mut options = file_options();
        options.read(true).write(true);
        let lock = root.open_with(LOCK, &options).map_err(|_| unsafe_state())?;
        let opened_lock = lock.metadata().map_err(|_| unsafe_state())?;
        require_lock(&opened_lock)?;
        if !Identity::of(&visible_lock).matches(&opened_lock) {
            return Err(unsafe_state());
        }
        acquire(&lock)?;
        let record = read_record(&root)?;
        let (workflow_dir, workflow_identity) = open_private(&root, JOURNAL)?;
        let (draft_dir, draft_identity) = open_private(&root, DRAFT)?;
        let (generated_dir, generated_identity) = open_private(&root, GENERATED)?;
        if record.schema_version != 1
            || record.root != identity
            || record.lock != Identity::of(&opened_lock)
            || record.workflow != workflow_identity
            || record.draft != draft_identity
            || record.generated != generated_identity
        {
            return Err(unsafe_state());
        }
        let journal = Store::open(&path.join(JOURNAL))?;
        let store = Self {
            draft_path: path.join(DRAFT),
            generated_path: path.join(GENERATED),
            path,
            root,
            lock,
            workflow_dir,
            draft_dir,
            generated_dir,
            record,
            journal,
        };
        store.recheck()?;
        store.read()?;
        // A complete container always starts with a complete draft checkpoint.
        if Store::open(store.draft_path())?
            .read()?
            .is_none_or(|raw| raw.is_empty())
        {
            return Err(unsafe_state());
        }
        Ok(store)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn draft_path(&self) -> &Path {
        &self.draft_path
    }
    pub(crate) fn generated_path(&self) -> &Path {
        &self.generated_path
    }

    pub(crate) fn read(&self) -> Result<Vec<u8>> {
        self.recheck()?;
        let raw = self
            .journal
            .read()?
            .filter(|r| !r.is_empty())
            .ok_or_else(unsafe_state)?;
        self.recheck()?;
        Ok(raw)
    }
    pub(crate) fn save(&mut self, raw: &[u8]) -> Result<()> {
        check_raw(raw)?;
        self.recheck()?;
        self.journal.save(raw)?;
        self.recheck()
    }
    pub(crate) fn require_capacity(&self, revisions: u32) -> Result<()> {
        self.recheck()?;
        // Avoid addition overflow in the shared Store's bounded counter.
        if revisions > 4096 {
            return Err(limit());
        }
        self.journal.require_capacity(revisions)?;
        self.recheck()
    }

    pub(crate) fn recheck(&self) -> Result<()> {
        check_root(&self.path, &self.root, self.record.root)?;
        let names = self
            .root
            .entries()
            .map_err(|_| io_error())?
            .take(6)
            .map(|e| e.map(|v| v.file_name()).map_err(|_| unsafe_state()))
            .collect::<Result<BTreeSet<_>>>()?;
        let expected = [RECORD, LOCK, JOURNAL, DRAFT, GENERATED]
            .into_iter()
            .map(OsString::from)
            .collect();
        if names != expected || read_record(&self.root)? != self.record {
            return Err(unsafe_state());
        }
        let visible = self
            .root
            .symlink_metadata(LOCK)
            .map_err(|_| unsafe_state())?;
        let opened = self.lock.metadata().map_err(|_| unsafe_state())?;
        require_lock(&visible)?;
        require_lock(&opened)?;
        if !self.record.lock.matches(&visible) || !self.record.lock.matches(&opened) {
            return Err(unsafe_state());
        }
        for (name, dir, identity) in [
            (JOURNAL, &self.workflow_dir, self.record.workflow),
            (DRAFT, &self.draft_dir, self.record.draft),
            (GENERATED, &self.generated_dir, self.record.generated),
        ] {
            let visible = self
                .root
                .symlink_metadata(name)
                .map_err(|_| unsafe_state())?;
            let opened = dir.dir_metadata().map_err(|_| unsafe_state())?;
            require_private(&visible)?;
            require_private(&opened)?;
            if !identity.matches(&visible) || !identity.matches(&opened) {
                return Err(unsafe_state());
            }
            check_files(dir, name != GENERATED)?;
        }
        // In particular, validate hard-link accounting and append-only history
        // even when the caller is only checking before an external side effect.
        if self.journal.read()?.is_none_or(|raw| raw.is_empty())
            || Store::open(&self.draft_path)?
                .read()?
                .is_none_or(|raw| raw.is_empty())
        {
            return Err(unsafe_state());
        }
        Ok(())
    }
}

fn acquire(file: &File) -> Result<()> {
    flock(file, FlockOperation::NonBlockingLockExclusive).map_err(|e| {
        if e == rustix::io::Errno::WOULDBLOCK {
            error("SETUP_WORKFLOW_BUSY", "다른 설치 도우미가 이 세션을 사용하고 있습니다. 기존 작업을 종료한 뒤 명시적으로 재개하세요.")
        } else { io_error() }
    })
}
fn file_options() -> OpenOptions {
    let mut o = OpenOptions::new();
    o.custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32);
    o
}
fn open_private(parent: &Dir, name: impl AsRef<Path>) -> Result<(Dir, Identity)> {
    let before = parent.symlink_metadata(&name).map_err(|_| unsafe_state())?;
    require_private(&before)?;
    let mut options = file_options();
    options.read(true).custom_flags(
        (OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32,
    );
    let dir = Dir::from_std_file(
        parent
            .open_with(&name, &options)
            .map_err(|_| unsafe_state())?
            .into_std(),
    );
    let opened = dir.dir_metadata().map_err(|_| unsafe_state())?;
    let visible = parent.symlink_metadata(name).map_err(|_| unsafe_state())?;
    require_private(&opened)?;
    require_private(&visible)?;
    let identity = Identity::of(&before);
    if !identity.matches(&opened) || !identity.matches(&visible) {
        return Err(unsafe_state());
    }
    Ok((dir, identity))
}
fn read_record(root: &Dir) -> Result<Record> {
    let before = root.symlink_metadata(RECORD).map_err(|_| unsafe_state())?;
    require_file(&before)?;
    if before.len() == 0 || before.len() > MAX_RECORD {
        return Err(unsafe_state());
    }
    let mut options = file_options();
    options.read(true);
    let mut file = root
        .open_with(RECORD, &options)
        .map_err(|_| unsafe_state())?;
    let opened = file.metadata().map_err(|_| unsafe_state())?;
    if !same_snapshot(&before, &opened) {
        return Err(unsafe_state());
    }
    let mut raw = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take(MAX_RECORD + 1)
        .read_to_end(&mut raw)
        .map_err(|_| unsafe_state())?;
    let after = file.metadata().map_err(|_| unsafe_state())?;
    let visible = root.symlink_metadata(RECORD).map_err(|_| unsafe_state())?;
    if raw.len() as u64 != before.len()
        || !same_snapshot(&before, &after)
        || !same_snapshot(&before, &visible)
    {
        return Err(unsafe_state());
    }
    let record: Record = decode_strict_json(&raw).map_err(|_| unsafe_state())?;
    if !record.record.matches(&before) {
        return Err(unsafe_state());
    }
    Ok(record)
}
fn same_snapshot(a: &Metadata, b: &Metadata) -> bool {
    Identity::of(a).matches(b)
        && a.mode() == b.mode()
        && a.uid() == b.uid()
        && a.nlink() == b.nlink()
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
}
fn check_files(dir: &Dir, journal: bool) -> Result<()> {
    // Store independently validates revision continuity, CAS and crash-temporary
    // links. Supplement that contract with effective-owner checks here.
    for (count, entry) in dir.entries().map_err(|_| io_error())?.enumerate() {
        if count >= MAX_ENTRIES {
            return Err(limit());
        }
        let entry = entry.map_err(|_| unsafe_state())?;
        let metadata = dir
            .symlink_metadata(entry.file_name())
            .map_err(|_| unsafe_state())?;
        if !metadata.is_file()
            || metadata.mode() & 0o7777 != 0o600
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.len() > MAX_BYTES as u64
            || (!journal && metadata.nlink() != 1)
        {
            return Err(unsafe_state());
        }
    }
    Ok(())
}
fn require_private(m: &Metadata) -> Result<()> {
    if !m.is_dir() || m.mode() & 0o7777 != 0o700 || m.uid() != rustix::process::geteuid().as_raw() {
        Err(unsafe_state())
    } else {
        Ok(())
    }
}
fn require_file(m: &Metadata) -> Result<()> {
    if !m.is_file()
        || m.mode() & 0o7777 != 0o600
        || m.nlink() != 1
        || m.uid() != rustix::process::geteuid().as_raw()
    {
        Err(unsafe_state())
    } else {
        Ok(())
    }
}
fn require_lock(m: &Metadata) -> Result<()> {
    require_file(m)?;
    if m.len() != 0 {
        Err(unsafe_state())
    } else {
        Ok(())
    }
}
fn require_ancestor(m: &Metadata) -> Result<()> {
    if !m.is_dir() || (m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0) {
        Err(unsafe_state())
    } else {
        Ok(())
    }
}
fn parent_anchor(path: &Path) -> Result<(PathBuf, Dir, OsString)> {
    let text = path.to_str().ok_or_else(unsafe_state)?;
    if !path.is_absolute() || text.len() > 4096 || text.chars().any(char::is_control) {
        return Err(unsafe_state());
    }
    let mut names = Vec::new();
    let mut normalized = PathBuf::from("/");
    for c in path.components() {
        match c {
            Component::RootDir => (),
            Component::Normal(n) => {
                names.push(n.to_os_string());
                normalized.push(n);
            }
            _ => return Err(unsafe_state()),
        }
    }
    if normalized.as_os_str() != path.as_os_str() {
        return Err(unsafe_state());
    }
    let name = names.pop().ok_or_else(unsafe_state)?;
    let mut dir = Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| io_error())?;
    require_ancestor(&dir.dir_metadata().map_err(|_| unsafe_state())?)?;
    for part in names {
        let before = dir.symlink_metadata(&part).map_err(|_| unsafe_state())?;
        require_ancestor(&before)?;
        let mut o = file_options();
        o.read(true).custom_flags(
            (OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits()
                as i32,
        );
        let child = Dir::from_std_file(
            dir.open_with(&part, &o)
                .map_err(|_| unsafe_state())?
                .into_std(),
        );
        let opened = child.dir_metadata().map_err(|_| unsafe_state())?;
        let visible = dir.symlink_metadata(&part).map_err(|_| unsafe_state())?;
        require_ancestor(&opened)?;
        require_ancestor(&visible)?;
        if !Identity::of(&before).matches(&opened) || !Identity::of(&before).matches(&visible) {
            return Err(unsafe_state());
        }
        dir = child;
    }
    Ok((normalized, dir, name))
}
fn check_root(path: &Path, root: &Dir, identity: Identity) -> Result<()> {
    let (_, parent, name) = parent_anchor(path)?;
    let visible = parent.symlink_metadata(name).map_err(|_| unsafe_state())?;
    let opened = root.dir_metadata().map_err(|_| unsafe_state())?;
    require_private(&visible)?;
    require_private(&opened)?;
    if !identity.matches(&visible) || !identity.matches(&opened) {
        Err(unsafe_state())
    } else {
        Ok(())
    }
}
fn sync_dir(dir: &Dir) -> Result<()> {
    dir.open(".")
        .and_then(|f| f.sync_all())
        .map_err(|_| io_error())
}
fn check_raw(raw: &[u8]) -> Result<()> {
    if raw.is_empty() || raw.len() > MAX_BYTES {
        Err(limit())
    } else {
        Ok(())
    }
}
fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn io_error() -> BxdlError {
    error(
        "SETUP_WORKFLOW_IO",
        "설치 세션을 안전하게 저장하지 못했습니다. 기존 자료를 보존하고 상태를 확인하세요.",
    )
}
fn unsafe_state() -> BxdlError {
    error(
        "SETUP_WORKFLOW_UNSAFE",
        "설치 세션이 불완전하거나 변경되어 사용할 수 없습니다. 기존 자료는 자동 복구하거나 덮어쓰지 않습니다.",
    )
}
fn exists() -> BxdlError {
    error(
        "SETUP_WORKFLOW_EXISTS",
        "설치 세션 경로가 이미 있습니다. 완성된 세션은 --resume으로 명시적으로 재개하세요.",
    )
}
fn limit() -> BxdlError {
    error(
        "SETUP_WORKFLOW_LIMIT",
        "설치 세션의 저장 크기 또는 기록 한도를 넘었습니다.",
    )
}

#[cfg(test)]
#[path = "workflow_store_tests.rs"]
mod tests;
