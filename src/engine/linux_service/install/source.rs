//! Operator-readable source boundary. Root never follows user-controlled links
//! or reads a root-private source merely because the helper can access it.
use super::*;
use cap_std::{
    ambient_authority,
    fs::{
        Dir, File as CapFile, Metadata, MetadataExt as _, OpenOptions as Options,
        OpenOptionsExt as _,
    },
};
use rustix::fs::OFlags;
use std::{ffi::OsString, path::Component};
fn changed() -> BxdlError {
    install_error("LINUX_INSTALL_INPUT_CHANGED")
}
fn authority(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.uid() == b.uid()
        && a.gid() == b.gid()
        && a.mode() == b.mode()
}
fn same(a: &Metadata, b: &Metadata) -> bool {
    authority(a, b)
        && a.nlink() == b.nlink()
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
}
fn directory_authority(owner: u32, mode: u32, uid: u32, root_process: bool) -> bool {
    let root = owner == 0;
    // Sticky shared ancestors are traversed only through no-follow descriptors.
    let shared = root && mode & 0o1000 != 0 && mode & 0o001 != 0;
    [0, uid].contains(&owner)
        && mode & 0o6000 == 0
        && (shared || mode & 0o022 == 0)
        && !(root && uid != 0 && mode & 0o001 == 0)
        && (root || mode & 0o100 != 0)
        && (uid != 0 || root_process)
}
fn directory(m: &Metadata, uid: u32) -> Result<()> {
    let root_process =
        rustix::process::getuid().as_raw() == 0 && rustix::process::geteuid().as_raw() == 0;
    if !m.is_dir()
        || m.file_type().is_symlink()
        || !directory_authority(m.uid(), m.mode(), uid, root_process)
    {
        return Err(changed());
    }
    Ok(())
}

struct Witness {
    parent: Dir,
    name: OsString,
    metadata: Metadata,
}
struct Pinned {
    file: CapFile,
    parent: Dir,
    name: OsString,
    metadata: Metadata,
    ancestors: Vec<Witness>,
}
impl Pinned {
    fn open(path: &Path, uid: u32, maximum: u64, private: bool) -> Result<Self> {
        boundary::safe_absolute(path)?;
        let mut parent = Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| changed())?;
        directory(&parent.dir_metadata().map_err(|_| changed())?, uid)?;
        let mut parts = path
            .strip_prefix("/")
            .map_err(|_| changed())?
            .components()
            .map(|p| match p {
                Component::Normal(n) => Ok(n.to_owned()),
                _ => Err(changed()),
            })
            .collect::<Result<Vec<_>>>()?;
        let name = parts.pop().ok_or_else(changed)?;
        let mut ancestors = Vec::new();
        for part in parts {
            let before = parent.symlink_metadata(&part).map_err(|_| changed())?;
            directory(&before, uid)?;
            let mut options = Options::new();
            options.read(true).custom_flags(
                (OFlags::NOFOLLOW | OFlags::DIRECTORY | OFlags::CLOEXEC).bits() as i32,
            );
            let opened = parent.open_with(&part, &options).map_err(|_| changed())?;
            let metadata = opened.metadata().map_err(|_| changed())?;
            directory(&metadata, uid)?;
            if !authority(&before, &metadata)
                || !authority(
                    &metadata,
                    &parent.symlink_metadata(&part).map_err(|_| changed())?,
                )
            {
                return Err(changed());
            }
            let next = Dir::from_std_file(opened.into_std());
            ancestors.push(Witness {
                parent,
                name: part,
                metadata,
            });
            parent = next;
        }
        let metadata = parent.symlink_metadata(&name).map_err(|_| changed())?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.mode() & 0o7022 != 0
            || metadata.len() > maximum
            || !((metadata.uid() == uid && metadata.mode() & 0o400 != 0)
                || (metadata.uid() == 0 && metadata.mode() & 0o004 != 0))
            || (private && (metadata.uid() != uid || metadata.mode() & 0o7777 != 0o600))
        {
            return Err(changed());
        }
        let mut options = Options::new();
        options
            .read(true)
            .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32);
        let file = parent.open_with(&name, &options).map_err(|_| changed())?;
        let pinned = Self {
            file,
            parent,
            name,
            metadata,
            ancestors,
        };
        pinned.recheck()?;
        Ok(pinned)
    }
    fn recheck(&self) -> Result<()> {
        if !same(
            &self.metadata,
            &self.file.metadata().map_err(|_| changed())?,
        ) || !same(
            &self.metadata,
            &self
                .parent
                .symlink_metadata(&self.name)
                .map_err(|_| changed())?,
        ) {
            return Err(changed());
        }
        for witness in &self.ancestors {
            if !authority(
                &witness.metadata,
                &witness
                    .parent
                    .symlink_metadata(&witness.name)
                    .map_err(|_| changed())?,
            ) {
                return Err(changed());
            }
        }
        Ok(())
    }
    fn transfer(&mut self, mut output: Option<&mut File>) -> Result<(String, u64)> {
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = self.file.read(&mut buffer).map_err(|_| changed())?;
            if n == 0 {
                break;
            }
            size = size.checked_add(n as u64).ok_or_else(changed)?;
            if size > self.metadata.len() {
                return Err(changed());
            }
            hash.update(&buffer[..n]);
            if let Some(out) = output.as_mut() {
                out.write_all(&buffer[..n]).map_err(|_| changed())?;
            }
        }
        self.recheck()?;
        if size != self.metadata.len() {
            return Err(changed());
        }
        Ok((format!("{:x}", hash.finalize()), size))
    }
}
pub(super) fn read(path: &Path, uid: u32, max: u64) -> Result<Vec<u8>> {
    read_inner(path, uid, max, false)
}
pub(super) fn read_request(path: &Path, uid: u32) -> Result<Vec<u8>> {
    read_inner(path, uid, REQUEST_LIMIT, true)
}
fn read_inner(path: &Path, uid: u32, max: u64, private: bool) -> Result<Vec<u8>> {
    let mut file = Pinned::open(path, uid, max, private)?;
    let mut raw = Vec::new();
    (&mut file.file)
        .take(max + 1)
        .read_to_end(&mut raw)
        .map_err(|_| changed())?;
    file.recheck()?;
    if raw.len() as u64 != file.metadata.len() {
        return Err(changed());
    }
    Ok(raw)
}
pub(super) fn pin(path: &Path, uid: u32, max: u64) -> Result<Pin> {
    let mut file = Pinned::open(path, uid, max, false)?;
    let (sha256, size) = file.transfer(None)?;
    Ok(Pin {
        path: path.to_owned(),
        sha256,
        size,
    })
}
pub(super) fn verify(pin: &Pin, uid: u32) -> Result<()> {
    let found = self::pin(&pin.path, uid, pin.size)?;
    if &found != pin {
        return Err(changed());
    }
    Ok(())
}
pub(super) fn copy(pin: &Pin, uid: u32, output: &mut File) -> Result<()> {
    let mut file = Pinned::open(&pin.path, uid, pin.size, false)?;
    let (sha, size) = file.transfer(Some(output))?;
    if sha != pin.sha256 || size != pin.size {
        return Err(changed());
    }
    Ok(())
}
fn inventory(root: &Path, uid: u32) -> Result<BTreeSet<PathBuf>> {
    boundary::safe_absolute(root)?;
    let mut dir = Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| changed())?;
    directory(&dir.dir_metadata().map_err(|_| changed())?, uid)?;
    let mut witnesses = Vec::new();
    for component in root.strip_prefix("/").map_err(|_| changed())?.components() {
        let Component::Normal(name) = component else {
            return Err(changed());
        };
        let before = dir.symlink_metadata(name).map_err(|_| changed())?;
        directory(&before, uid)?;
        let mut options = Options::new();
        options
            .read(true)
            .custom_flags((OFlags::NOFOLLOW | OFlags::DIRECTORY | OFlags::CLOEXEC).bits() as i32);
        let file = dir.open_with(name, &options).map_err(|_| changed())?;
        let metadata = file.metadata().map_err(|_| changed())?;
        if !authority(&before, &metadata) {
            return Err(changed());
        }
        directory(&metadata, uid)?;
        let next = Dir::from_std_file(file.into_std());
        witnesses.push(Witness {
            parent: dir,
            name: name.into(),
            metadata,
        });
        dir = next;
    }
    fn walk(dir: &Dir, relative: &Path, uid: u32, result: &mut BTreeSet<PathBuf>) -> Result<()> {
        directory(&dir.dir_metadata().map_err(|_| changed())?, uid)?;
        for entry in dir.entries().map_err(|_| changed())? {
            let name = entry.map_err(|_| changed())?.file_name();
            let before = dir.symlink_metadata(&name).map_err(|_| changed())?;
            let path = relative.join(&name);
            if !result.insert(path.clone()) || result.len() > 2048 {
                return Err(changed());
            }
            if before.is_dir() {
                directory(&before, uid)?;
                let mut options = Options::new();
                options.read(true).custom_flags(
                    (OFlags::NOFOLLOW | OFlags::DIRECTORY | OFlags::CLOEXEC).bits() as i32,
                );
                let file = dir.open_with(&name, &options).map_err(|_| changed())?;
                if !authority(&before, &file.metadata().map_err(|_| changed())?) {
                    return Err(changed());
                }
                walk(&Dir::from_std_file(file.into_std()), &path, uid, result)?;
                if !authority(
                    &before,
                    &dir.symlink_metadata(&name).map_err(|_| changed())?,
                ) {
                    return Err(changed());
                }
            } else if !before.is_file() || before.file_type().is_symlink() {
                return Err(changed());
            }
        }
        Ok(())
    }
    let mut result = BTreeSet::new();
    walk(&dir, Path::new(""), uid, &mut result)?;
    for witness in witnesses {
        if !authority(
            &witness.metadata,
            &witness
                .parent
                .symlink_metadata(&witness.name)
                .map_err(|_| changed())?,
        ) {
            return Err(changed());
        }
    }
    Ok(result)
}
pub(super) fn verify_runtime(root: &Path, uid: u32) -> Result<()> {
    let lock = runtime_lock()?;
    let mut expected = BTreeSet::new();
    for file in &lock.files {
        let relative = Path::new(&file.path);
        expected.insert(relative.to_owned());
        let mut parent = relative.parent();
        while let Some(p) = parent {
            if !p.as_os_str().is_empty() {
                expected.insert(p.to_owned());
            }
            parent = p.parent();
        }
        let mut pinned = Pinned::open(&root.join(relative), uid, file.size, false)?;
        if pinned.metadata.mode() & 0o7777 != file.mode {
            return Err(changed());
        }
        let (sha, size) = pinned.transfer(None)?;
        if sha != file.sha256 || size != file.size {
            return Err(changed());
        }
    }
    if inventory(root, uid)? != expected {
        return Err(changed());
    }
    Ok(())
}
pub(super) fn snapshot_runtime(root: &Path, uid: u32, target: &Path) -> Result<()> {
    verify_runtime(root, uid)?;
    boundary::mkdir(target, 0, 0, 0o700)?;
    for file in runtime_lock()?.files {
        let relative = Path::new(&file.path);
        let mut current = target.to_owned();
        if let Some(parent) = relative.parent() {
            for part in parent.components() {
                current.push(part);
                if fs::symlink_metadata(&current).is_err() {
                    boundary::mkdir(&current, 0, 0, 0o700)?;
                } else {
                    boundary::trusted(&current)?;
                }
            }
        }
        let output = target.join(relative);
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags((OFlags::NOFOLLOW | OFlags::CLOEXEC).bits() as i32)
            .open(&output)
            .map_err(|_| changed())?;
        copy(
            &Pin {
                path: root.join(relative),
                sha256: file.sha256,
                size: file.size,
            },
            uid,
            &mut out,
        )?;
        out.set_permissions(fs::Permissions::from_mode(file.mode))
            .map_err(|_| changed())?;
        out.sync_all().map_err(|_| changed())?;
    }
    verify_runtime(root, uid)?;
    File::open(target)
        .and_then(|f| f.sync_all())
        .map_err(|_| changed())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    fn fixture() -> (tempfile::TempDir, PathBuf, u32) {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.join("source");
        fs::write(&path, b"approved").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        (temp, path, rustix::process::getuid().as_raw())
    }
    #[test]
    fn pinned_content_change_link_and_permissions_fail_closed() {
        let (_temp, path, uid) = fixture();
        let approved = pin(&path, uid, 100).unwrap();
        verify(&approved, uid).unwrap();
        fs::write(&path, b"modified").unwrap();
        assert!(verify(&approved, uid).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(read(&path, uid, 100).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let hard = path.with_file_name("hard");
        fs::hard_link(&path, &hard).unwrap();
        assert!(read(&path, uid, 100).is_err());
        fs::remove_file(hard).unwrap();
        let link = path.with_file_name("link");
        symlink(&path, &link).unwrap();
        assert!(read(&link, uid, 100).is_err());
    }
    #[test]
    fn anchored_reader_rejects_replaced_ancestor_and_bounds_request() {
        let (temp, path, uid) = fixture();
        let parent = path.parent().unwrap();
        let nested = parent.join("nested");
        fs::create_dir(&nested).unwrap();
        fs::rename(&path, nested.join("source")).unwrap();
        let pinned = Pinned::open(&nested.join("source"), uid, 100, true).unwrap();
        fs::rename(&nested, parent.join("old")).unwrap();
        fs::create_dir(&nested).unwrap();
        assert!(pinned.recheck().is_err());
        drop(pinned);
        let file = parent.join("old/source");
        assert!(read(&file, uid, 2).is_err());
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_request(&file, uid).is_err());
        drop(temp);
    }
    #[test]
    fn inventory_never_follows_directory_links() {
        let (_temp, path, uid) = fixture();
        let root = path.parent().unwrap();
        symlink(root, root.join("loop")).unwrap();
        assert!(inventory(root, uid).is_err());
        fs::remove_file(root.join("loop")).unwrap();
        assert_eq!(
            inventory(root, uid).unwrap(),
            BTreeSet::from([PathBuf::from("source")])
        );
    }
    #[test]
    fn root_snapshot_authority_never_grants_operator_root_private_access() {
        assert!(directory_authority(0, 0o700, 0, true));
        assert!(!directory_authority(0, 0o700, 0, false));
        assert!(!directory_authority(0, 0o700, 1000, true));
        assert!(!directory_authority(0, 0o700, 1000, false));
        assert!(directory_authority(0, 0o755, 1000, true));
        assert!(directory_authority(1000, 0o700, 1000, true));
        assert!(!directory_authority(1001, 0o755, 1000, true));
        assert!(!directory_authority(0, 0o777, 1000, true));
        assert!(directory_authority(0, 0o1777, 1000, true));
    }
}
