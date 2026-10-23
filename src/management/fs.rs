//! Anchored, no-follow filesystem access for the independent CLI installation.
use super::{Result, error};
use cap_std::{
    ambient_authority,
    fs::{
        Dir, DirBuilder, DirBuilderExt, File, Metadata, MetadataExt, OpenOptions, OpenOptionsExt,
        Permissions, PermissionsExt,
    },
};
use rustix::fs::OFlags;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub device: u64,
    pub inode: u64,
}
impl Identity {
    pub fn of(m: &Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
        }
    }
}
pub(super) struct Directory {
    pub path: PathBuf,
    pub dir: Dir,
    pub id: Identity,
    private: bool,
}
impl Directory {
    pub fn recheck(&self) -> Result<()> {
        let current = open(&self.path, false, self.private)?.ok_or_else(unsafe_path)?;
        if current.id != self.id
            || Identity::of(&self.dir.dir_metadata().map_err(|_| io())?) != self.id
        {
            return Err(unsafe_path());
        }
        Ok(())
    }
}
fn uid() -> u32 {
    rustix::process::geteuid().as_raw()
}
pub(super) fn options() -> OpenOptions {
    let mut o = OpenOptions::new();
    o.custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32);
    o
}
fn directory(m: &Metadata, private: bool) -> Result<()> {
    if !m.is_dir()
        || m.is_symlink()
        || (private && (m.uid() != uid() || m.mode() & 0o7777 != 0o700))
        || (!private
            && (![0, uid()].contains(&m.uid())
                || (m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0)))
    {
        return Err(unsafe_path());
    }
    Ok(())
}
pub(super) fn private_file(m: &Metadata) -> Result<()> {
    if !m.is_file()
        || m.is_symlink()
        || m.uid() != uid()
        || m.nlink() != 1
        || m.mode() & 0o7777 != 0o600
    {
        return Err(unsafe_path());
    }
    Ok(())
}
pub(super) fn sync(dir: &Dir) -> Result<()> {
    dir.try_clone()
        .map_err(|_| io())?
        .into_std_file()
        .sync_all()
        .map_err(|_| io())
}

pub(super) fn open(path: &Path, create: bool, private: bool) -> Result<Option<Directory>> {
    let text = path.to_str().ok_or_else(unsafe_path)?;
    if !path.is_absolute() || text.len() > 4096 || text.chars().any(char::is_control) {
        return Err(unsafe_path());
    }
    let mut names = Vec::new();
    for c in path.components() {
        match c {
            Component::RootDir => (),
            Component::Normal(n) => names.push(n),
            _ => return Err(unsafe_path()),
        }
    }
    if names.is_empty() {
        return Err(unsafe_path());
    }
    let mut dir = Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| io())?;
    directory(&dir.dir_metadata().map_err(|_| io())?, false)?;
    for (index, name) in names.iter().enumerate() {
        if create {
            reject_product_root(&dir)?;
        }
        let last = index + 1 == names.len();
        let before = match dir.symlink_metadata(name) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !create => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                dir.create_dir_with(name, DirBuilder::new().mode(0o700))
                    .map_err(|_| io())?;
                sync(&dir)?;
                dir.symlink_metadata(name).map_err(|_| io())?
            }
            Err(_) => return Err(unsafe_path()),
        };
        directory(&before, last && private)?;
        let mut o = options();
        o.read(true).custom_flags(
            (OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits()
                as i32,
        );
        let next = Dir::from_std_file(
            dir.open_with(name, &o)
                .map_err(|_| unsafe_path())?
                .into_std(),
        );
        let opened = next.dir_metadata().map_err(|_| io())?;
        let visible = dir.symlink_metadata(name).map_err(|_| io())?;
        directory(&opened, last && private)?;
        directory(&visible, last && private)?;
        if Identity::of(&before) != Identity::of(&opened)
            || Identity::of(&before) != Identity::of(&visible)
        {
            return Err(unsafe_path());
        }
        dir = next;
    }
    let m = dir.dir_metadata().map_err(|_| io())?;
    Ok(Some(Directory {
        path: path.into(),
        id: Identity::of(&m),
        dir,
        private,
    }))
}
/// Refuse embedding CLI files inside known node, preparation or runtime roots.
pub(super) fn reject_product_root(dir: &Dir) -> Result<()> {
    for name in [
        ".bxdl-cli.json",
        ".bxdl-home.json",
        ".bxdl-install.json",
        ".workflow.json",
        ".workflow.pending",
        "binding.json",
        "engine-instance.json",
    ] {
        match dir.symlink_metadata(name) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            _ => {
                return Err(error(
                    "CLI_LOCATION_CONFLICT",
                    "관리 CLI 설치 경로가 기존 관리 자료·노드·패키지 안에 있습니다. 별도 위치를 지정하세요.",
                ));
            }
        }
    }
    Ok(())
}
pub(super) fn check_destination(path: &Path) -> Result<()> {
    let mut ancestor = Some(path);
    while let Some(p) = ancestor {
        if p.parent().is_none() {
            break;
        }
        if let Some(dir) = open(p, false, false)? {
            reject_product_root(&dir.dir)?;
        }
        ancestor = p.parent();
    }
    Ok(())
}
pub(super) fn mkdir(parent: &Dir, name: &str) -> Result<Dir> {
    parent
        .create_dir_with(name, DirBuilder::new().mode(0o700))
        .map_err(|_| io())?;
    sync(parent)?;
    child(parent, name)
}
pub(super) fn child(parent: &Dir, name: &str) -> Result<Dir> {
    let before = parent.symlink_metadata(name).map_err(|_| unsafe_path())?;
    directory(&before, true)?;
    let mut o = options();
    o.read(true).custom_flags(
        (OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32,
    );
    let dir = Dir::from_std_file(
        parent
            .open_with(name, &o)
            .map_err(|_| unsafe_path())?
            .into_std(),
    );
    if Identity::of(&before) != Identity::of(&dir.dir_metadata().map_err(|_| io())?) {
        return Err(unsafe_path());
    }
    Ok(dir)
}
pub(super) fn write_new(parent: &Dir, name: &str, bytes: &[u8], mode: u32) -> Result<()> {
    let mut o = options();
    o.write(true).create_new(true).mode(mode);
    let mut f = parent.open_with(name, &o).map_err(|_| io())?;
    f.write_all(bytes).map_err(|_| io())?;
    f.set_permissions(Permissions::from_mode(mode))
        .map_err(|_| io())?;
    f.sync_all().map_err(|_| io())?;
    sync(parent)
}
pub(super) fn read(parent: &Dir, name: &str) -> Result<Vec<u8>> {
    let before = parent.symlink_metadata(name).map_err(|_| unsafe_path())?;
    private_file(&before)?;
    if before.len() == 0 || before.len() > 32768 {
        return Err(unsafe_path());
    }
    let mut o = options();
    o.read(true);
    let mut f = parent.open_with(name, &o).map_err(|_| unsafe_path())?;
    let opened = f.metadata().map_err(|_| io())?;
    if !same(&before, &opened) {
        return Err(unsafe_path());
    }
    let mut b = Vec::new();
    (&mut f).take(32769).read_to_end(&mut b).map_err(|_| io())?;
    if b.len() as u64 != before.len()
        || !same(&before, &f.metadata().map_err(|_| io())?)
        || !same(&before, &parent.symlink_metadata(name).map_err(|_| io())?)
    {
        return Err(unsafe_path());
    }
    Ok(b)
}
fn same(a: &Metadata, b: &Metadata) -> bool {
    Identity::of(a) == Identity::of(b)
        && a.len() == b.len()
        && a.mode() == b.mode()
        && a.uid() == b.uid()
        && a.nlink() == b.nlink()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
}
pub(super) fn lock_file(parent: &Dir, name: &str) -> Result<File> {
    let mut o = options();
    o.read(true).write(true);
    let f = parent.open_with(name, &o).map_err(|_| unsafe_path())?;
    let m = f.metadata().map_err(|_| io())?;
    private_file(&m)?;
    if m.len() != 0
        || Identity::of(&m) != Identity::of(&parent.symlink_metadata(name).map_err(|_| io())?)
    {
        return Err(unsafe_path());
    }
    Ok(f)
}
pub(super) fn io() -> crate::error::BxdlError {
    error(
        "CLI_STORAGE_IO",
        "관리 CLI 파일을 읽거나 저장하지 못했습니다. cli info로 선택 상태를 확인하세요.",
    )
}
pub(super) fn unsafe_path() -> crate::error::BxdlError {
    error(
        "CLI_STORAGE_UNSAFE",
        "관리 CLI 경로·소유권·권한·기록을 확인하지 못했습니다. 기존 자료를 보존하고 경로를 확인하세요.",
    )
}

/// Publication must never replace an unrelated file or empty directory.
pub(super) fn publish(source: &Dir, name: &str, destination: &Dir, target: &str) -> Result<()> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        rustix::fs::renameat_with(
            source,
            name,
            destination,
            target,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|_| io())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (source, name, destination, target);
        Err(error(
            "CLI_PLATFORM_UNSUPPORTED",
            "이 운영체제에서는 CLI 게시를 지원하지 않습니다.",
        ))
    }
}
