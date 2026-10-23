//! Private console management roots. Opening is read-only; only explicit
//! catalog creation prepares missing parents. Same-UID processes are trusted.
use crate::{
    artifact::decode_strict_json,
    error::{BxdlError, Result},
    setup::store::Store,
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
    ffi::OsString,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const RECORD: &str = ".bxdl-home.json";
const LOCK: &str = ".catalog.lock";
const CATALOG: &str = "catalog";
const MAX_RECORD: u64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    device: u64,
    inode: u64,
}
impl Identity {
    fn of(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
    fn matches(self, metadata: &Metadata) -> bool {
        self == Self::of(metadata)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    kind: String,
    root: Identity,
    lock: Identity,
    catalog: Identity,
    record: Identity,
}

pub(crate) struct Home {
    path: PathBuf,
    root: Dir,
    catalog: Dir,
    record: Record,
}

pub(crate) fn default_home() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if home.is_absolute() {
            return crate::setup::absolute(&home.join("Library/Application Support/BXDL"));
        }
    }
    Err(error(
        "CONSOLE_HOME_REQUIRED",
        "기본 관리 위치를 확인할 수 없습니다. --home으로 관리 폴더를 지정하세요.",
    ))
}

impl Home {
    pub(crate) fn open(path: &Path) -> Result<Option<Self>> {
        let Some((path, parent, name)) = parent_anchor(path, false)? else {
            return Ok(None);
        };
        match parent.symlink_metadata(&name) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(io_error()),
            Ok(_) => (),
        }
        let (root, identity) = open_private(&parent, &name)?;
        let record = read_record(&root)?;
        let (catalog, catalog_identity) = open_private(&root, CATALOG)?;
        if record.schema_version != 1
            || record.kind != "BXDL_CONSOLE_HOME"
            || record.root != identity
            || record.catalog != catalog_identity
        {
            return Err(unsafe_home());
        }
        let home = Self {
            path,
            root,
            catalog,
            record,
        };
        home.recheck()?;
        Ok(Some(home))
    }

    /// No existing root is adopted. A failed create remains visibly incomplete.
    pub(crate) fn create(path: &Path, initial_catalog: &[u8]) -> Result<Self> {
        let (path, parent, name) = parent_anchor(path, true)?.ok_or_else(io_error)?;
        let mut options = DirBuilder::new();
        options.mode(0o700);
        parent.create_dir_with(&name, &options).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                error("CONSOLE_HOME_EXISTS", "관리 폴더가 이미 있습니다. 기존 자료는 변경하지 않았습니다. 새 위치를 지정하거나 기존 목록을 다시 여세요.")
            } else { io_error() }
        })?;
        sync(&parent)?;
        let (root, identity) = open_private(&parent, &name)?;
        let mut options = file_options();
        options.read(true).write(true).create_new(true).mode(0o600);
        let lock = root.open_with(LOCK, &options).map_err(|_| io_error())?;
        let lock_metadata = lock.metadata().map_err(|_| io_error())?;
        require_lock(&lock_metadata)?;
        lock.sync_all().map_err(|_| io_error())?;
        check_visible_root(&path, &root, identity)?;
        let mut store = Store::create(&path.join(CATALOG)).map_err(store_error)?;
        store.save(initial_catalog).map_err(store_error)?;
        check_visible_root(&path, &root, identity)?;
        let (catalog, catalog_identity) = open_private(&root, CATALOG)?;
        let mut options = file_options();
        options.write(true).create_new(true).mode(0o600);
        let mut marker = root.open_with(RECORD, &options).map_err(|_| io_error())?;
        let marker_metadata = marker.metadata().map_err(|_| io_error())?;
        require_file(&marker_metadata)?;
        let record = Record {
            schema_version: 1,
            kind: "BXDL_CONSOLE_HOME".into(),
            root: identity,
            lock: Identity::of(&lock_metadata),
            catalog: catalog_identity,
            record: Identity::of(&marker_metadata),
        };
        marker
            .write_all(&serde_json::to_vec(&record).map_err(|_| io_error())?)
            .map_err(|_| io_error())?;
        marker.sync_all().map_err(|_| io_error())?;
        sync(&root)?;
        let home = Self {
            path,
            root,
            catalog,
            record,
        };
        home.recheck()?;
        Ok(home)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    /// Explicitly approved preparation only. No package, control or data parent
    /// is created here, and existing namespaces must pass the home checks.
    pub(crate) fn ensure_sessions(&self) -> Result<PathBuf> {
        let _guard = self.lock()?;
        self.recheck()?;
        match self.root.symlink_metadata("sessions") {
            Ok(metadata) => require_private(&metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.root
                    .create_dir_with("sessions", DirBuilder::new().mode(0o700))
                    .map_err(|_| io_error())?;
                sync(&self.root)?;
            }
            Err(_) => return Err(io_error()),
        }
        self.recheck()?;
        Ok(self.path.join("sessions"))
    }

    /// Bounded inventory of the explicit managed namespace, never a disk scan.
    /// Opening a selected name separately verifies its preparation record.
    pub(crate) fn session_names(&self) -> Result<Vec<String>> {
        self.recheck()?;
        match self.root.symlink_metadata("sessions") {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(io_error()),
            Ok(metadata) => require_private(&metadata)?,
        }
        let (sessions, _) = open_private(&self.root, "sessions")?;
        let mut names = Vec::new();
        for (index, entry) in sessions.entries().map_err(|_| io_error())?.enumerate() {
            if index >= 256 {
                return Err(limit());
            }
            let entry = entry.map_err(|_| io_error())?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if super::install_flow::valid_name(name) {
                let metadata = sessions.symlink_metadata(name).map_err(|_| io_error())?;
                if metadata.is_dir() && !metadata.is_symlink() {
                    names.push(name.to_owned());
                }
            }
        }
        self.recheck()?;
        names.sort();
        Ok(names)
    }

    pub(crate) fn catalog_path(&self) -> PathBuf {
        self.path.join(CATALOG)
    }

    pub(crate) fn recheck(&self) -> Result<()> {
        check_visible_root(&self.path, &self.root, self.record.root)?;
        if read_record(&self.root)? != self.record {
            return Err(unsafe_home());
        }
        let lock = self
            .root
            .symlink_metadata(LOCK)
            .map_err(|_| unsafe_home())?;
        require_lock(&lock)?;
        if !self.record.lock.matches(&lock) {
            return Err(unsafe_home());
        }
        let visible_catalog = self
            .root
            .symlink_metadata(CATALOG)
            .map_err(|_| unsafe_home())?;
        let opened_catalog = self.catalog.dir_metadata().map_err(|_| unsafe_home())?;
        require_private(&visible_catalog)?;
        require_private(&opened_catalog)?;
        if !self.record.catalog.matches(&visible_catalog)
            || !self.record.catalog.matches(&opened_catalog)
        {
            return Err(unsafe_home());
        }
        for (index, item) in self.root.entries().map_err(|_| io_error())?.enumerate() {
            if index >= 10 {
                return Err(unsafe_home());
            }
            let item = item.map_err(|_| unsafe_home())?;
            let name = item.file_name();
            match name.to_str().ok_or_else(unsafe_home)? {
                RECORD | LOCK | CATALOG => (),
                "sessions" | "configs" | "packages" | "instances" | "data" | "reports" => {
                    require_private(
                        &self
                            .root
                            .symlink_metadata(&name)
                            .map_err(|_| unsafe_home())?,
                    )?;
                }
                _ => return Err(unsafe_home()),
            }
        }
        // Store validates ordering, hard links and publication snapshots; this
        // layer adds ownership and forbids its optional product-export name.
        for (index, item) in self.catalog.entries().map_err(|_| io_error())?.enumerate() {
            if index >= 8192 {
                return Err(limit());
            }
            let item = item.map_err(|_| unsafe_home())?;
            let name = item.file_name();
            if name == "instance.json" {
                return Err(unsafe_home());
            }
            let metadata = match self.catalog.symlink_metadata(&name) {
                Ok(metadata) => metadata,
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound
                        && name.to_str().is_some_and(|n| n.starts_with(".bxdl-tmp-")) =>
                {
                    continue;
                }
                Err(_) => return Err(unsafe_home()),
            };
            if !metadata.is_file()
                || metadata.mode() & 0o7777 != 0o600
                || metadata.uid() != rustix::process::geteuid().as_raw()
                || metadata.len() > 262_144
            {
                return Err(unsafe_home());
            }
        }
        Ok(())
    }

    /// The guard is local to a single write, never retained during a prompt.
    pub(crate) fn lock(&self) -> Result<WriterLock> {
        self.recheck()?;
        let mut options = file_options();
        options.read(true).write(true);
        let file = self
            .root
            .open_with(LOCK, &options)
            .map_err(|_| unsafe_home())?;
        let metadata = file.metadata().map_err(|_| unsafe_home())?;
        require_lock(&metadata)?;
        if !self.record.lock.matches(&metadata) {
            return Err(unsafe_home());
        }
        flock(&file, FlockOperation::NonBlockingLockExclusive).map_err(|e| {
            if e == rustix::io::Errno::WOULDBLOCK {
                error(
                    "CONSOLE_CATALOG_BUSY",
                    "다른 콘솔이 목록을 저장 중입니다. 잠시 후 목록을 다시 여세요.",
                )
            } else {
                io_error()
            }
        })?;
        let guard = WriterLock { file };
        self.recheck()?;
        Ok(guard)
    }
}

pub(crate) struct WriterLock {
    file: File,
}
impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = flock(&self.file, FlockOperation::Unlock);
    }
}

fn file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32);
    options
}
fn open_private(parent: &Dir, name: impl AsRef<Path>) -> Result<(Dir, Identity)> {
    let before = parent.symlink_metadata(&name).map_err(|_| unsafe_home())?;
    require_private(&before)?;
    let mut options = file_options();
    options.read(true).custom_flags(
        (OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32,
    );
    let dir = Dir::from_std_file(
        parent
            .open_with(&name, &options)
            .map_err(|_| unsafe_home())?
            .into_std(),
    );
    let opened = dir.dir_metadata().map_err(|_| unsafe_home())?;
    let visible = parent.symlink_metadata(name).map_err(|_| unsafe_home())?;
    require_private(&opened)?;
    require_private(&visible)?;
    let identity = Identity::of(&before);
    if !identity.matches(&opened) || !identity.matches(&visible) {
        return Err(unsafe_home());
    }
    Ok((dir, identity))
}
fn read_record(root: &Dir) -> Result<Record> {
    let before = root.symlink_metadata(RECORD).map_err(|_| unsafe_home())?;
    require_file(&before)?;
    if before.len() == 0 || before.len() > MAX_RECORD {
        return Err(unsafe_home());
    }
    let mut options = file_options();
    options.read(true);
    let mut file = root
        .open_with(RECORD, &options)
        .map_err(|_| unsafe_home())?;
    let opened = file.metadata().map_err(|_| unsafe_home())?;
    require_file(&opened)?;
    if !same_snapshot(&before, &opened) {
        return Err(unsafe_home());
    }
    let mut raw = Vec::new();
    (&mut file)
        .take(MAX_RECORD + 1)
        .read_to_end(&mut raw)
        .map_err(|_| io_error())?;
    let after = file.metadata().map_err(|_| unsafe_home())?;
    let visible = root.symlink_metadata(RECORD).map_err(|_| unsafe_home())?;
    if raw.len() as u64 != before.len()
        || !same_snapshot(&before, &after)
        || !same_snapshot(&before, &visible)
    {
        return Err(unsafe_home());
    }
    let record: Record =
        serde_json::from_value(decode_strict_json(&raw).map_err(|_| unsafe_home())?)
            .map_err(|_| unsafe_home())?;
    if !record.record.matches(&opened) {
        return Err(unsafe_home());
    }
    Ok(record)
}
fn same_snapshot(a: &Metadata, b: &Metadata) -> bool {
    Identity::of(a).matches(b)
        && a.len() == b.len()
        && a.mode() == b.mode()
        && a.uid() == b.uid()
        && a.nlink() == b.nlink()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
}
fn require_private(metadata: &Metadata) -> Result<()> {
    if !metadata.is_dir()
        || metadata.mode() & 0o7777 != 0o700
        || metadata.uid() != rustix::process::geteuid().as_raw()
    {
        return Err(unsafe_home());
    }
    Ok(())
}
fn require_file(metadata: &Metadata) -> Result<()> {
    if !metadata.is_file()
        || metadata.mode() & 0o7777 != 0o600
        || metadata.nlink() != 1
        || metadata.uid() != rustix::process::geteuid().as_raw()
    {
        return Err(unsafe_home());
    }
    Ok(())
}
fn require_lock(metadata: &Metadata) -> Result<()> {
    require_file(metadata)?;
    if metadata.len() != 0 {
        return Err(unsafe_home());
    }
    Ok(())
}
fn require_ancestor(metadata: &Metadata) -> Result<()> {
    if !metadata.is_dir()
        || (metadata.mode() & 0o022 != 0 && metadata.mode() & 0o1000 == 0)
        || ![0, rustix::process::geteuid().as_raw()].contains(&metadata.uid())
    {
        return Err(unsafe_home());
    }
    Ok(())
}

type ParentAnchor = (PathBuf, Dir, OsString);
fn parent_anchor(path: &Path, create: bool) -> Result<Option<ParentAnchor>> {
    let text = path.to_str().ok_or_else(unsafe_home)?;
    if !path.is_absolute()
        || text.is_empty()
        || text.len() > 4096
        || text.chars().any(char::is_control)
    {
        return Err(unsafe_home());
    }
    let mut names = Vec::new();
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir => (),
            Component::Normal(name) => {
                names.push(name.to_os_string());
                normalized.push(name);
            }
            _ => return Err(unsafe_home()),
        }
    }
    if normalized.as_os_str() != path.as_os_str() {
        return Err(unsafe_home());
    }
    let name = names.pop().ok_or_else(unsafe_home)?;
    let mut parent = Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| io_error())?;
    require_ancestor(&parent.dir_metadata().map_err(|_| unsafe_home())?)?;
    for component in names {
        let before = match parent.symlink_metadata(&component) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !create => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut options = DirBuilder::new();
                options.mode(0o700);
                // A concurrent creator is not silently adopted.
                parent
                    .create_dir_with(&component, &options)
                    .map_err(|_| unsafe_home())?;
                sync(&parent)?;
                let metadata = parent
                    .symlink_metadata(&component)
                    .map_err(|_| unsafe_home())?;
                require_private(&metadata)?;
                metadata
            }
            Err(_) => return Err(io_error()),
        };
        require_ancestor(&before)?;
        let mut options = file_options();
        options.read(true).custom_flags(
            (OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits()
                as i32,
        );
        let child = Dir::from_std_file(
            parent
                .open_with(&component, &options)
                .map_err(|_| unsafe_home())?
                .into_std(),
        );
        let opened = child.dir_metadata().map_err(|_| unsafe_home())?;
        let visible = parent
            .symlink_metadata(&component)
            .map_err(|_| unsafe_home())?;
        require_ancestor(&opened)?;
        require_ancestor(&visible)?;
        if !Identity::of(&before).matches(&opened) || !Identity::of(&before).matches(&visible) {
            return Err(unsafe_home());
        }
        parent = child;
    }
    Ok(Some((normalized, parent, name)))
}
fn check_visible_root(path: &Path, root: &Dir, identity: Identity) -> Result<()> {
    let (_, parent, name) = parent_anchor(path, false)?.ok_or_else(unsafe_home)?;
    let visible = parent.symlink_metadata(name).map_err(|_| unsafe_home())?;
    let opened = root.dir_metadata().map_err(|_| unsafe_home())?;
    require_private(&visible)?;
    require_private(&opened)?;
    if !identity.matches(&visible) || !identity.matches(&opened) {
        return Err(unsafe_home());
    }
    Ok(())
}
fn sync(dir: &Dir) -> Result<()> {
    dir.try_clone()
        .map_err(|_| io_error())?
        .into_std_file()
        .sync_all()
        .map_err(|_| io_error())
}
fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn unsafe_home() -> BxdlError {
    error(
        "CONSOLE_HOME_UNSAFE",
        "관리 위치의 소유권·권한·기록을 확인할 수 없습니다. 기존 자료는 변경하거나 자동 복구하지 않았습니다.",
    )
}
fn io_error() -> BxdlError {
    error(
        "CONSOLE_CATALOG_IO",
        "목록 자료를 읽거나 저장할 수 없습니다. 실제 노드나 설치 결과는 변경하지 않았습니다.",
    )
}
pub(crate) fn limit() -> BxdlError {
    error(
        "CONSOLE_CATALOG_LIMIT",
        "목록 저장 한도에 도달했습니다. 기존 기록은 보존됩니다.",
    )
}
pub(crate) fn store_error(error_value: BxdlError) -> BxdlError {
    match error_value.code.as_str() {
        "SETUP_CONFLICT" => error(
            "CONSOLE_CATALOG_CONFLICT",
            "다른 콘솔이 목록을 갱신했습니다. 최신 목록을 다시 열어 변경하세요.",
        ),
        "SETUP_LIMIT" => limit(),
        _ => io_error(),
    }
}
