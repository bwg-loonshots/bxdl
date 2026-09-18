//! Private local control directories and inherited open-description locks.
//! This is an accidental-concurrency guard, not isolation from the same UID.
use crate::error::{BxdlError, Result};
use cap_std::{
    ambient_authority,
    fs::{
        Dir, DirBuilder, DirBuilderExt, File as CapFile, Metadata, MetadataExt, OpenOptions,
        OpenOptionsExt,
    },
};
use rustix::fs::{FlockOperation, OFlags, flock};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const LOCK: &str = ".operation.lock";
const OPERATIONS: &str = "operations";
const RECORD: &str = ".control.json";

/// Called only for explicit fresh init, after durable INIT_INTENT. NIGO accepts
/// an empty data directory; create it privately before NIGO's default umask can
/// produce a 0755 directory. Existing files/permissions are never repaired.
pub(super) fn prepare_data(path: &Path) -> Result<()> {
    let (path, parent, name) = parent_anchor(path)?;
    let parent_identity = Identity::of(&parent.dir_metadata().map_err(|_| unsafe_state())?);
    match parent.symlink_metadata(&name) {
        Ok(metadata) => require_private(&metadata)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut options = DirBuilder::new();
            options.mode(0o700);
            // A concurrent creator is a conflict, not an existing path to adopt.
            parent
                .create_dir_with(&name, &options)
                .map_err(|_| unsafe_state())?;
            sync_dir(&parent)?;
        }
        Err(_) => return Err(unsafe_state()),
    }
    let (data, identity) = open_private(&parent, &name)?;
    if data.entries().map_err(|_| io_error())?.next().is_some() {
        return Err(error(
            "INSTANCE_DATA_NOT_EMPTY",
            "새 초기화에는 비어 있는 데이터 폴더가 필요합니다. 기존 자료는 변경하지 않았습니다.",
        ));
    }
    sync_dir(&data)?;
    let (_, current_parent, current_name) = parent_anchor(&path)?;
    if !parent_identity.matches(&current_parent.dir_metadata().map_err(|_| unsafe_state())?) {
        return Err(unsafe_state());
    }
    let visible = current_parent
        .symlink_metadata(current_name)
        .map_err(|_| unsafe_state())?;
    let opened = data.dir_metadata().map_err(|_| unsafe_state())?;
    require_private(&visible)?;
    require_private(&opened)?;
    if !identity.matches(&visible) || !identity.matches(&opened) {
        return Err(unsafe_state());
    }
    if data.entries().map_err(|_| io_error())?.next().is_some() {
        return Err(error(
            "INSTANCE_DATA_NOT_EMPTY",
            "새 초기화에는 비어 있는 데이터 폴더가 필요합니다. 기존 자료는 변경하지 않았습니다.",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    device: u64,
    inode: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlRecord {
    version: u32,
    root: Identity,
    lock: Identity,
    operations: Identity,
    record: Identity,
}
impl Identity {
    fn of(m: &Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
        }
    }
    fn matches(self, m: &Metadata) -> bool {
        self.device == m.dev() && self.inode == m.ino()
    }
}

pub(super) struct Control {
    path: PathBuf,
    root: Dir,
    identity: Identity,
    lock_identity: Identity,
    operations: Dir,
    operations_identity: Identity,
    record: ControlRecord,
}

impl Control {
    /// The caller validates overlaps before this first mutation. Missing parent
    /// paths are not created; failed creation remains visibly incomplete.
    pub(super) fn create(path: &Path) -> Result<Self> {
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
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32);
        let lock = root.open_with(LOCK, &options).map_err(|_| io_error())?;
        let metadata = lock.metadata().map_err(|_| io_error())?;
        require_lock(&metadata)?;
        let lock_identity = Identity::of(&metadata);
        lock.sync_all().map_err(|_| io_error())?;
        let mut options = DirBuilder::new();
        options.mode(0o700);
        root.create_dir_with(OPERATIONS, &options)
            .map_err(|_| io_error())?;
        sync_dir(&root)?;
        let (operations, operations_identity) = open_private(&root, OPERATIONS)?;
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32);
        let mut file = root.open_with(RECORD, &options).map_err(|_| io_error())?;
        let metadata = file.metadata().map_err(|_| io_error())?;
        require_lock(&metadata)?;
        let record = ControlRecord {
            version: 1,
            root: identity,
            lock: lock_identity,
            operations: operations_identity,
            record: Identity::of(&metadata),
        };
        let raw = serde_json::to_vec(&record).map_err(|_| io_error())?;
        file.write_all(&raw).map_err(|_| io_error())?;
        file.sync_all().map_err(|_| io_error())?;
        sync_dir(&root)?;
        let control = Self {
            path,
            root,
            identity,
            lock_identity,
            operations,
            operations_identity,
            record,
        };
        control.recheck()?;
        Ok(control)
    }

    /// No missing lock or directory is repaired or synthesized on open.
    pub(super) fn open(path: &Path) -> Result<Self> {
        let (path, parent, name) = parent_anchor(path)?;
        let (root, identity) = open_private(&parent, &name)?;
        let metadata = root.symlink_metadata(LOCK).map_err(|_| unsafe_state())?;
        require_lock(&metadata)?;
        let lock_identity = Identity::of(&metadata);
        let (operations, operations_identity) = open_private(&root, OPERATIONS)?;
        let record = read_record(&root)?;
        if record.version != 1
            || record.root != identity
            || record.lock != lock_identity
            || record.operations != operations_identity
        {
            return Err(unsafe_state());
        }
        let control = Self {
            path,
            root,
            identity,
            lock_identity,
            operations,
            operations_identity,
            record,
        };
        control.recheck()?;
        Ok(control)
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn recheck(&self) -> Result<()> {
        check_anchor(&self.path, &self.root, self.identity)?;
        if read_record(&self.root)? != self.record {
            return Err(unsafe_state());
        }
        let lock = self
            .root
            .symlink_metadata(LOCK)
            .map_err(|_| unsafe_state())?;
        require_lock(&lock)?;
        if !self.lock_identity.matches(&lock) {
            return Err(unsafe_state());
        }
        let visible = self
            .root
            .symlink_metadata(OPERATIONS)
            .map_err(|_| unsafe_state())?;
        let opened = self.operations.dir_metadata().map_err(|_| unsafe_state())?;
        require_private(&visible)?;
        require_private(&opened)?;
        if !self.operations_identity.matches(&visible) || !self.operations_identity.matches(&opened)
        {
            return Err(unsafe_state());
        }
        Ok(())
    }

    pub(super) fn lock(&self) -> Result<OperationLock> {
        self.recheck()?;
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32);
        let file = self
            .root
            .open_with(LOCK, &options)
            .map_err(|_| unsafe_state())?;
        let metadata = file.metadata().map_err(|_| unsafe_state())?;
        require_lock(&metadata)?;
        if !self.lock_identity.matches(&metadata) {
            return Err(unsafe_state());
        }
        flock(&file, FlockOperation::NonBlockingLockExclusive).map_err(|e| {
            if e == rustix::io::Errno::WOULDBLOCK {
                error("INSTANCE_BUSY", "같은 인스턴스의 작업이 잠금을 유지하고 있습니다. 새 작업을 시작하지 않았습니다.")
            } else { io_error() }
        })?;
        self.recheck()?;
        let guard = OperationLock {
            file,
            root: self.root.try_clone().map_err(|_| io_error())?,
            path: self.path.clone(),
            identity: self.identity,
            lock_identity: self.lock_identity,
            record: self.record.clone(),
        };
        guard.recheck()?;
        Ok(guard)
    }

    /// The caller holds an OperationLock before allocating an execution attempt.
    /// The report file itself must remain absent for NIGO's create-new contract.
    pub(super) fn attempt(&self, id: &str) -> Result<PathBuf> {
        if id.is_empty()
            || id.len() > 80
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(error(
                "INSTANCE_ATTEMPT_INVALID",
                "실행 시도 식별자의 형식이 잘못되었습니다.",
            ));
        }
        self.recheck()?;
        let mut options = DirBuilder::new();
        options.mode(0o700);
        self.operations.create_dir_with(id, &options).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                exists()
            } else {
                io_error()
            }
        })?;
        sync_dir(&self.operations)?;
        let (attempt, identity) = open_private(&self.operations, id)?;
        sync_dir(&attempt)?;
        self.recheck()?;
        let metadata = self
            .operations
            .symlink_metadata(id)
            .map_err(|_| unsafe_state())?;
        if !identity.matches(&metadata) {
            return Err(unsafe_state());
        }
        Ok(self.path.join(OPERATIONS).join(id))
    }
}

pub(super) struct OperationLock {
    file: CapFile,
    root: Dir,
    path: PathBuf,
    identity: Identity,
    lock_identity: Identity,
    record: ControlRecord,
}
impl OperationLock {
    fn recheck(&self) -> Result<()> {
        check_anchor(&self.path, &self.root, self.identity)?;
        if read_record(&self.root)? != self.record {
            return Err(unsafe_state());
        }
        let visible = self
            .root
            .symlink_metadata(LOCK)
            .map_err(|_| unsafe_state())?;
        let opened = self.file.metadata().map_err(|_| unsafe_state())?;
        require_lock(&visible)?;
        require_lock(&opened)?;
        if !self.lock_identity.matches(&visible) || !self.lock_identity.matches(&opened) {
            return Err(unsafe_state());
        }
        Ok(())
    }

    /// Pass this clone to Command::stdin(Stdio::from(file)). Keep CLOEXEC on
    /// other descriptors. The child must not close/reopen stdin while active.
    pub(super) fn child_stdin(&self) -> Result<File> {
        self.recheck()?;
        self.file
            .try_clone()
            .map(|file| file.into_std())
            .map_err(|_| io_error())
    }
}
// Intentionally no explicit LOCK_UN or lock-file deletion in Drop. Cloned
// descriptors refer to the same locked open description: a Java child keeps
// the lock after its controller dies, until the final inherited fd is closed.

fn require_private(m: &Metadata) -> Result<()> {
    if !m.is_dir() || m.mode() & 0o7777 != 0o700 {
        return Err(unsafe_state());
    }
    Ok(())
}
fn read_record(root: &Dir) -> Result<ControlRecord> {
    let visible = root.symlink_metadata(RECORD).map_err(|_| unsafe_state())?;
    if !visible.is_file()
        || visible.nlink() != 1
        || visible.mode() & 0o7777 != 0o600
        || visible.len() == 0
        || visible.len() > 4096
    {
        return Err(unsafe_state());
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32);
    let mut file = root
        .open_with(RECORD, &options)
        .map_err(|_| unsafe_state())?;
    let before = file.metadata().map_err(|_| unsafe_state())?;
    let identity = Identity::of(&visible);
    if !identity.matches(&before) {
        return Err(unsafe_state());
    }
    let mut raw = Vec::new();
    (&mut file)
        .take(4097)
        .read_to_end(&mut raw)
        .map_err(|_| unsafe_state())?;
    let after = file.metadata().map_err(|_| unsafe_state())?;
    let visible_after = root.symlink_metadata(RECORD).map_err(|_| unsafe_state())?;
    if raw.len() > 4096
        || raw.len() as u64 != visible.len()
        || !identity.matches(&after)
        || !identity.matches(&visible_after)
        || visible_after.nlink() != 1
        || visible_after.mode() & 0o7777 != 0o600
        || before.modified().ok() != after.modified().ok()
        || before.len() != after.len()
    {
        return Err(unsafe_state());
    }
    let record: ControlRecord = serde_json::from_slice(&raw).map_err(|_| unsafe_state())?;
    if record.record != identity {
        return Err(unsafe_state());
    }
    Ok(record)
}
fn require_lock(m: &Metadata) -> Result<()> {
    if !m.is_file() || m.nlink() != 1 || m.len() != 0 || m.mode() & 0o7777 != 0o600 {
        return Err(unsafe_state());
    }
    Ok(())
}
fn open_private(parent: &Dir, name: impl AsRef<Path>) -> Result<(Dir, Identity)> {
    let before = parent.symlink_metadata(&name).map_err(|_| unsafe_state())?;
    require_private(&before)?;
    let child = parent.open_dir(&name).map_err(|_| unsafe_state())?;
    let after = child.dir_metadata().map_err(|_| unsafe_state())?;
    require_private(&after)?;
    let identity = Identity::of(&before);
    if !identity.matches(&after) {
        return Err(unsafe_state());
    }
    Ok((child, identity))
}
fn check_anchor(path: &Path, root: &Dir, identity: Identity) -> Result<()> {
    let (_, parent, name) = parent_anchor(path)?;
    let visible = parent.symlink_metadata(name).map_err(|_| unsafe_state())?;
    let opened = root.dir_metadata().map_err(|_| unsafe_state())?;
    require_private(&visible)?;
    require_private(&opened)?;
    if !identity.matches(&visible) || !identity.matches(&opened) {
        return Err(unsafe_state());
    }
    Ok(())
}
fn parent_anchor(path: &Path) -> Result<(PathBuf, Dir, OsString)> {
    let text = path.to_str().ok_or_else(unsafe_state)?;
    if !path.is_absolute()
        || text.is_empty()
        || text.len() > 4096
        || text.chars().any(char::is_control)
    {
        return Err(unsafe_state());
    }
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => components.push(name.to_os_string()),
            _ => return Err(unsafe_state()),
        }
    }
    let name = components.pop().ok_or_else(unsafe_state)?;
    let mut parent = Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| io_error())?;
    let mut normalized = PathBuf::from("/");
    for component in components {
        let before = parent
            .symlink_metadata(&component)
            .map_err(|_| unsafe_state())?;
        if !before.is_dir() {
            return Err(unsafe_state());
        }
        let child = parent.open_dir(&component).map_err(|_| unsafe_state())?;
        if !Identity::of(&before).matches(&child.dir_metadata().map_err(|_| unsafe_state())?) {
            return Err(unsafe_state());
        }
        parent = child;
        normalized.push(component);
    }
    normalized.push(&name);
    Ok((normalized, parent, name))
}
fn sync_dir(dir: &Dir) -> Result<()> {
    dir.open(".")
        .map_err(|_| io_error())?
        .sync_all()
        .map_err(|_| io_error())
}
fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn unsafe_state() -> BxdlError {
    error(
        "INSTANCE_CONTROL_UNSAFE",
        "인스턴스 제어 경로의 종류·식별·링크·권한을 확인하세요. 자동 복구하지 않았습니다.",
    )
}
fn io_error() -> BxdlError {
    error(
        "INSTANCE_CONTROL_IO",
        "인스턴스 제어 기록 작업에 실패했습니다. 기존 기록을 보존하고 상태를 확인하세요.",
    )
}
fn exists() -> BxdlError {
    error(
        "INSTANCE_CONTROL_EXISTS",
        "이미 제어 폴더 또는 실행 시도가 있습니다. 기존 경로를 덮어쓰지 않았습니다.",
    )
}

#[cfg(test)]
mod tests;
