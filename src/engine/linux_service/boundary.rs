//! Root authority and Linux process witnesses. No shell or PID-only signal.
use super::{error, unknown};
use crate::error::Result;
use cap_std::{
    ambient_authority,
    fs::{
        Dir, File as CapFile, Metadata as CapMetadata, MetadataExt as _,
        OpenOptions as CapOpenOptions, OpenOptionsExt as _,
    },
};
use rustix::fs::{FlockOperation, OFlags};
use std::ffi::OsString;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path},
};

pub(super) fn root() -> Result<()> {
    if !cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        return Err(error("LINUX_SERVICE_PLATFORM_UNSUPPORTED"));
    }
    if rustix::process::getuid().as_raw() != 0 || rustix::process::geteuid().as_raw() != 0 {
        return Err(error("LINUX_SERVICE_ROOT_REQUIRED"));
    }
    Ok(())
}
pub(super) fn safe_absolute(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .to_str()
            .is_none_or(|s| s.len() > 4096 || s.chars().any(char::is_control))
        || path
            .components()
            .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
    {
        return Err(error("LINUX_SERVICE_PATH_UNSAFE"));
    }
    let rebuilt: std::path::PathBuf = path.components().collect();
    if rebuilt.as_os_str() != path.as_os_str() {
        return Err(error("LINUX_SERVICE_PATH_UNSAFE"));
    }
    Ok(())
}
/// Every ancestor is administrator controlled; sticky shared temporary roots
/// are deliberately excluded for a persistent root service authority.
pub(super) fn trusted(path: &Path) -> Result<()> {
    safe_absolute(path)?;
    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(|_| unknown())?;
        if metadata.uid() != 0
            || metadata.mode() & 0o022 != 0
            || metadata.file_type().is_symlink()
            || (!metadata.is_dir() && !metadata.is_file())
            || (current != path && !metadata.is_dir())
            || (metadata.is_file() && metadata.nlink() != 1)
        {
            return Err(error("LINUX_SERVICE_AUTHORITY_UNSAFE"));
        }
    }
    Ok(())
}
pub(super) fn mkdir(path: &Path, uid: u32, gid: u32, mode: u32) -> Result<()> {
    trusted(path.parent().ok_or_else(unknown)?)?;
    fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .map_err(|_| error("LINUX_SERVICE_PATH_EXISTS"))?;
    rustix::fs::chown(
        path,
        Some(rustix::process::Uid::from_raw(uid)),
        Some(rustix::process::Gid::from_raw(gid)),
    )
    .map_err(|_| unknown())?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|_| unknown())?;
    sync(path)?;
    sync(path.parent().ok_or_else(unknown)?)
}
pub(super) fn write_new(path: &Path, bytes: &[u8], gid: u32, mode: u32) -> Result<()> {
    trusted(path.parent().ok_or_else(unknown)?)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags((OFlags::NOFOLLOW | OFlags::CLOEXEC).bits() as i32)
        .open(path)
        .map_err(|_| error("LINUX_SERVICE_PATH_EXISTS"))?;
    file.write_all(bytes).map_err(|_| unknown())?;
    rustix::fs::fchown(
        &file,
        Some(rustix::process::Uid::from_raw(0)),
        Some(rustix::process::Gid::from_raw(gid)),
    )
    .map_err(|_| unknown())?;
    file.set_permissions(fs::Permissions::from_mode(mode))
        .map_err(|_| unknown())?;
    file.sync_all().map_err(|_| unknown())?;
    sync(path.parent().ok_or_else(unknown)?)
}
pub(super) fn sync(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| unknown())
}
pub(super) fn root_read(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    trusted(path)?;
    read_owned(path, 0, maximum)
}
/// Descriptor-relative traversal matters here: engine-owned data/output
/// directories may change while root observes them. Never follow their links.
struct DirectoryWitness {
    parent: Dir,
    name: OsString,
    metadata: CapMetadata,
}
struct PinnedFile {
    file: CapFile,
    parent: Dir,
    name: OsString,
    metadata: CapMetadata,
    ancestors: Vec<DirectoryWitness>,
}
fn same_inode(a: &CapMetadata, b: &CapMetadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino()
}
fn same_authority(a: &CapMetadata, b: &CapMetadata) -> bool {
    same_inode(a, b) && a.uid() == b.uid() && a.gid() == b.gid() && a.mode() == b.mode()
}
fn same_file(a: &CapMetadata, b: &CapMetadata) -> bool {
    same_authority(a, b)
        && a.nlink() == b.nlink()
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
}
fn permitted_directory(metadata: &CapMetadata, uid: u32) -> Result<()> {
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || ![0, uid].contains(&metadata.uid())
        || metadata.mode() & 0o7022 != 0
    {
        return Err(unknown());
    }
    Ok(())
}
impl PinnedFile {
    fn open(path: &Path, uid: u32, maximum: u64, writable: bool) -> Result<Self> {
        safe_absolute(path)?;
        let root = Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| unknown())?;
        let metadata = root.dir_metadata().map_err(|_| unknown())?;
        if metadata.uid() != 0 {
            return Err(unknown());
        }
        Self::at(
            root,
            path.strip_prefix("/").map_err(|_| unknown())?,
            uid,
            maximum,
            writable,
        )
    }
    fn at(
        mut directory: Dir,
        relative: &Path,
        uid: u32,
        maximum: u64,
        writable: bool,
    ) -> Result<Self> {
        permitted_directory(&directory.dir_metadata().map_err(|_| unknown())?, uid)?;
        let mut parts = relative
            .components()
            .map(|c| match c {
                Component::Normal(name) => Ok(name.to_owned()),
                _ => Err(unknown()),
            })
            .collect::<Result<Vec<_>>>()?;
        let name = parts.pop().ok_or_else(unknown)?;
        let mut ancestors = Vec::new();
        for name in parts {
            let before = directory.symlink_metadata(&name).map_err(|_| unknown())?;
            permitted_directory(&before, uid)?;
            let mut options = CapOpenOptions::new();
            options.read(true).custom_flags(
                (OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC).bits() as i32,
            );
            let file = directory
                .open_with(&name, &options)
                .map_err(|_| unknown())?;
            let opened = file.metadata().map_err(|_| unknown())?;
            let visible = directory.symlink_metadata(&name).map_err(|_| unknown())?;
            if !same_authority(&before, &opened) || !same_authority(&opened, &visible) {
                return Err(unknown());
            }
            permitted_directory(&opened, uid)?;
            let next = Dir::from_std_file(file.into_std());
            ancestors.push(DirectoryWitness {
                parent: directory,
                name,
                metadata: opened,
            });
            directory = next;
        }
        let before = directory.symlink_metadata(&name).map_err(|_| unknown())?;
        if !before.is_file()
            || before.uid() != uid
            || before.nlink() != 1
            || before.mode() & 0o7022 != 0
            || before.len() > maximum
        {
            return Err(unknown());
        }
        let mut options = CapOpenOptions::new();
        options
            .read(true)
            .write(writable)
            .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32);
        let file = directory
            .open_with(&name, &options)
            .map_err(|_| unknown())?;
        let result = Self {
            file,
            parent: directory,
            name,
            metadata: before,
            ancestors,
        };
        result.recheck()?;
        Ok(result)
    }
    fn recheck(&self) -> Result<()> {
        let opened = self.file.metadata().map_err(|_| unknown())?;
        let visible = self
            .parent
            .symlink_metadata(&self.name)
            .map_err(|_| unknown())?;
        if !same_file(&self.metadata, &opened) || !same_file(&opened, &visible) {
            return Err(unknown());
        }
        for ancestor in &self.ancestors {
            let visible = ancestor
                .parent
                .symlink_metadata(&ancestor.name)
                .map_err(|_| unknown())?;
            if !same_authority(&ancestor.metadata, &visible) {
                return Err(unknown());
            }
        }
        Ok(())
    }
}
pub(super) fn read_owned(path: &Path, uid: u32, maximum: u64) -> Result<Vec<u8>> {
    let mut pinned = PinnedFile::open(path, uid, maximum, false)?;
    let mut bytes = Vec::new();
    (&mut pinned.file)
        .take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| unknown())?;
    if bytes.len() as u64 != pinned.metadata.len() {
        return Err(unknown());
    }
    pinned.recheck()?;
    Ok(bytes)
}
pub(super) fn private_directory(path: &Path, uid: u32, gid: u32) -> Result<(u64, u64)> {
    trusted(path.parent().ok_or_else(unknown)?)?;
    let m = fs::symlink_metadata(path).map_err(|_| unknown())?;
    if !m.is_dir() || m.uid() != uid || m.gid() != gid || m.mode() & 0o7777 != 0o700 {
        return Err(unknown());
    }
    Ok((m.dev(), m.ino()))
}
pub(super) fn operation_lock(control: &Path) -> Result<File> {
    let path = control.join("operation.lock");
    trusted(&path)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((OFlags::NOFOLLOW | OFlags::CLOEXEC).bits() as i32)
        .open(path)
        .map_err(|_| unknown())?;
    if file.metadata().map_err(|_| unknown())?.len() != 0 {
        return Err(unknown());
    }
    rustix::fs::flock(&file, FlockOperation::NonBlockingLockExclusive)
        .map_err(|_| error("LINUX_SERVICE_BUSY"))?;
    Ok(file)
}
pub(super) fn busy(control: &Path) -> Result<bool> {
    match operation_lock(control) {
        Ok(_) => Ok(false),
        Err(e) if e.code == "LINUX_SERVICE_BUSY" => Ok(true),
        Err(e) => Err(e),
    }
}
pub(super) fn account(control: &Path, uid: u32, gid: u32) -> Result<()> {
    if uid == 0 || gid == 0 || uid == u32::MAX || gid == u32::MAX {
        return Err(error("LINUX_SERVICE_ACCOUNT_INVALID"));
    }
    let name = super::service_account(control)?;
    let passwd = String::from_utf8(root_read(Path::new("/etc/passwd"), 1024 * 1024)?)
        .map_err(|_| unknown())?;
    let users: Vec<Vec<&str>> = passwd
        .lines()
        .map(|l| l.split(':').collect())
        .filter(|p: &Vec<&str>| {
            p.len() == 7 && (p[0] == name || p[2].parse::<u32>().ok() == Some(uid))
        })
        .collect();
    if users.len() != 1
        || users[0][0] != name
        || users[0][2].parse::<u32>().ok() != Some(uid)
        || users[0][3].parse::<u32>().ok() != Some(gid)
        || !matches!(users[0][6], "/usr/sbin/nologin" | "/sbin/nologin")
    {
        return Err(error("LINUX_SERVICE_ACCOUNT_INVALID"));
    }
    if passwd
        .lines()
        .map(|l| l.split(':').collect::<Vec<_>>())
        .any(|p| p.len() == 7 && p[0] != name && p[3].parse::<u32>().ok() == Some(gid))
    {
        return Err(error("LINUX_SERVICE_ACCOUNT_INVALID"));
    }
    let groups = String::from_utf8(root_read(Path::new("/etc/group"), 1024 * 1024)?)
        .map_err(|_| unknown())?;
    let matches: Vec<Vec<&str>> = groups
        .lines()
        .map(|l| l.split(':').collect())
        .filter(|p: &Vec<&str>| {
            p.len() == 4 && (p[0] == name || p[2].parse::<u32>().ok() == Some(gid))
        })
        .collect();
    if matches.len() != 1
        || matches[0][0] != name
        || matches[0][2].parse::<u32>().ok() != Some(gid)
        || !matches[0][3].is_empty()
    {
        return Err(error("LINUX_SERVICE_ACCOUNT_INVALID"));
    }
    Ok(())
}

pub(super) fn proc_start_ticks(raw: &str, pid: u32) -> Result<u64> {
    let (prefix, rest) = raw.rsplit_once(") ").ok_or_else(unknown)?;
    if prefix
        .split_once(" (")
        .and_then(|(p, _)| p.parse::<u32>().ok())
        != Some(pid)
    {
        return Err(unknown());
    }
    let fields: Vec<_> = rest.split_whitespace().collect();
    fields
        .get(19)
        .ok_or_else(unknown)?
        .parse()
        .map_err(|_| unknown())
}
fn proc_read(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let mut file = File::open(path).map_err(|_| unknown())?;
    let mut raw = Vec::new();
    (&mut file)
        .take(maximum + 1)
        .read_to_end(&mut raw)
        .map_err(|_| unknown())?;
    if raw.len() as u64 > maximum {
        return Err(unknown());
    }
    Ok(raw)
}
pub(super) fn ticks(pid: u32) -> Result<u64> {
    proc_start_ticks(
        &String::from_utf8(proc_read(
            &Path::new("/proc").join(pid.to_string()).join("stat"),
            8192,
        )?)
        .map_err(|_| unknown())?,
        pid,
    )
}
pub(super) fn cgroup(pid: u32) -> Result<String> {
    let raw = String::from_utf8(proc_read(
        &Path::new("/proc").join(pid.to_string()).join("cgroup"),
        8192,
    )?)
    .map_err(|_| unknown())?;
    let mut lines = raw.lines();
    let path = lines
        .next()
        .and_then(|l| l.strip_prefix("0::"))
        .ok_or_else(unknown)?;
    if lines.next().is_some()
        || !path.starts_with("/system.slice/bxdl-")
        || !path.ends_with(".service")
        || path.contains("..")
        || path.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(unknown());
    }
    Ok(path.into())
}
/// Confirm the privilege reduction on the actual process before using its
/// identity to declare readiness or authorize a stop. Duplicate or absent
/// fields do not establish credentials; checking only effective UID is weaker.
fn credentials(status: &str, uid: u32, gid: u32) -> Result<()> {
    if uid == 0 || gid == 0 || uid == u32::MAX || gid == u32::MAX {
        return Err(unknown());
    }
    let field = |name: &str| -> Result<&str> {
        let mut values = status.lines().filter_map(|line| line.strip_prefix(name));
        let value = values.next().ok_or_else(unknown)?;
        if values.next().is_some() {
            return Err(unknown());
        }
        Ok(value.trim())
    };
    for (name, value) in [("Uid:", uid), ("Gid:", gid)] {
        let values = field(name)?
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Vec<_>>();
        if values.len() != 4 || values.into_iter().any(|n| n.ok() != Some(value)) {
            return Err(unknown());
        }
    }
    if !field("Groups:")?.is_empty() || field("NoNewPrivs:")? != "1" {
        return Err(unknown());
    }
    for name in ["CapInh:", "CapPrm:", "CapEff:", "CapBnd:", "CapAmb:"] {
        let value = field(name)?;
        if value.len() != 16 || !value.bytes().all(|b| b == b'0') {
            return Err(unknown());
        }
    }
    Ok(())
}
pub(super) fn witness(
    pid: u32,
    start: u64,
    group: &str,
    uid: u32,
    gid: u32,
    java: &Path,
    args: &[String],
) -> Result<()> {
    if ticks(pid)? != start || cgroup(pid)? != group {
        return Err(unknown());
    }
    let base = Path::new("/proc").join(pid.to_string());
    let status =
        String::from_utf8(proc_read(&base.join("status"), 32768)?).map_err(|_| unknown())?;
    credentials(&status, uid, gid)?;
    if fs::read_link(base.join("exe")).map_err(|_| unknown())? != java {
        return Err(unknown());
    }
    let cmd = proc_read(&base.join("cmdline"), 32768)?;
    if !cmd.ends_with(&[0]) {
        return Err(unknown());
    }
    let actual: Vec<_> = cmd[..cmd.len() - 1].split(|b| *b == 0).collect();
    if actual.len() != args.len() || actual.iter().zip(args).any(|(a, b)| *a != b.as_bytes()) {
        return Err(unknown());
    }
    if ticks(pid)? != start || cgroup(pid)? != group {
        return Err(unknown());
    }
    Ok(())
}
pub(super) fn absent(pid: u32, start: u64, group: &str) -> Result<()> {
    match fs::symlink_metadata(Path::new("/proc").join(pid.to_string())) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Ok(_) => {
            if ticks(pid)? == start {
                return Err(unknown());
            }
        }
        Err(_) => return Err(unknown()),
    }
    if !group.starts_with("/system.slice/bxdl-") || group.contains("..") {
        return Err(unknown());
    }
    let path = Path::new("/sys/fs/cgroup").join(group.trim_start_matches('/'));
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(m) if m.is_dir() => {
            // cgroup.events populated includes descendants; empty cgroup.procs
            // alone is insufficient when a process has created child cgroups.
            let raw = String::from_utf8(proc_read(&path.join("cgroup.events"), 4096)?)
                .map_err(|_| unknown())?;
            if raw
                .lines()
                .filter(|l| l.starts_with("populated "))
                .collect::<Vec<_>>()
                != vec!["populated 0"]
            {
                return Err(unknown());
            }
            Ok(())
        }
        _ => Err(unknown()),
    }
}
pub(super) fn database_unlocked(data: &Path, uid: u32) -> Result<Vec<File>> {
    let mut pinned = Vec::new();
    for path in [data.join("engine.lock"), data.join("ledger/LOCK")] {
        let candidate = PinnedFile::open(&path, uid, 1024 * 1024, true)?;
        rustix::fs::fcntl_lock(&candidate.file, FlockOperation::NonBlockingLockExclusive)
            .map_err(|_| unknown())?;
        pinned.push(candidate);
    }
    for candidate in &pinned {
        candidate.recheck()?;
    }
    Ok(pinned
        .into_iter()
        .map(|candidate| candidate.file.into_std())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proc_stat_handles_parentheses_and_rejects_pid_reuse() {
        let raw =
            "42 (java worker ) with spaces) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 987 20";
        assert_eq!(proc_start_ticks(raw, 42).unwrap(), 987);
        assert!(proc_start_ticks(raw, 43).is_err());
        assert!(proc_start_ticks("42 (broken)", 42).is_err());
    }
    #[test]
    fn service_control_rejects_relative_and_parent_paths() {
        assert!(safe_absolute(Path::new("relative")).is_err());
        assert!(safe_absolute(Path::new("/var/lib/../tmp/node")).is_err());
        assert!(safe_absolute(Path::new("/var/lib/bxdl/node")).is_ok());
        for value in [
            "/var/lib/bxdl//node",
            "/var/lib/bxdl/./node",
            "/var/lib/bxdl/node/",
        ] {
            assert!(safe_absolute(Path::new(value)).is_err());
        }
    }
    #[test]
    fn ordinary_user_or_mac_cannot_enter_privileged_backend() {
        if !cfg!(target_os = "linux") || rustix::process::getuid().as_raw() != 0 {
            assert!(root().is_err());
        }
    }
    fn directory(path: &Path) -> Dir {
        Dir::open_ambient_dir(path, ambient_authority()).unwrap()
    }
    fn test_file(root: &Path, relative: &str) -> PinnedFile {
        PinnedFile::at(
            directory(root),
            Path::new(relative),
            rustix::process::getuid().as_raw(),
            4096,
            false,
        )
        .unwrap()
    }
    #[test]
    fn descriptor_reader_rejects_symlink_hardlink_and_foreign_uid() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("report.jsonl"), b"original").unwrap();
        let mut file = test_file(root, "report.jsonl");
        let mut raw = String::new();
        file.file.read_to_string(&mut raw).unwrap();
        assert_eq!(raw, "original");
        file.recheck().unwrap();
        symlink("report.jsonl", root.join("link")).unwrap();
        let uid = rustix::process::getuid().as_raw();
        assert!(PinnedFile::at(directory(root), Path::new("link"), uid, 4096, false).is_err());
        fs::hard_link(root.join("report.jsonl"), root.join("alias")).unwrap();
        assert!(file.recheck().is_err());
        assert!(
            PinnedFile::at(directory(root), Path::new("report.jsonl"), uid, 4096, false).is_err()
        );
        fs::remove_file(root.join("alias")).unwrap();
        assert!(
            PinnedFile::at(
                directory(root),
                Path::new("report.jsonl"),
                uid.wrapping_add(1),
                4096,
                false
            )
            .is_err()
        );
    }
    #[test]
    fn descriptor_reader_rejects_swapped_mutable_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join("ledger"))
            .unwrap();
        fs::write(root.join("ledger/LOCK"), b"").unwrap();
        let file = test_file(root, "ledger/LOCK");
        fs::rename(root.join("ledger"), root.join("old-ledger")).unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join("ledger"))
            .unwrap();
        fs::write(root.join("ledger/LOCK"), b"").unwrap();
        assert!(file.recheck().is_err());
        fs::remove_file(root.join("ledger/LOCK")).unwrap();
        fs::remove_dir(root.join("ledger")).unwrap();
        std::os::unix::fs::symlink("old-ledger", root.join("ledger")).unwrap();
        assert!(
            PinnedFile::at(
                directory(root),
                Path::new("ledger/LOCK"),
                rustix::process::getuid().as_raw(),
                4096,
                false
            )
            .is_err()
        );
    }
    #[test]
    fn descriptor_reader_detects_permission_change_and_output_bound() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("report.jsonl"), b"unchanged").unwrap();
        let file = test_file(root, "report.jsonl");
        fs::set_permissions(root.join("report.jsonl"), fs::Permissions::from_mode(0o400)).unwrap();
        assert!(file.recheck().is_err());
        assert!(
            PinnedFile::at(
                directory(root),
                Path::new("report.jsonl"),
                rustix::process::getuid().as_raw(),
                1,
                false
            )
            .is_err()
        );
    }
    fn status_fixture() -> String {
        "Name:\tjava\nUid:\t991 991 991 991\nGid:\t992 992 992 992\nGroups:\t\nNoNewPrivs:\t1\nCapInh:\t0000000000000000\nCapPrm:\t0000000000000000\nCapEff:\t0000000000000000\nCapBnd:\t0000000000000000\nCapAmb:\t0000000000000000\n".into()
    }
    #[test]
    fn process_witness_requires_all_ids_empty_groups_and_no_privileges() {
        let raw = status_fixture();
        assert!(credentials(&raw, 991, 992).is_ok());
        for (before, after) in [
            ("991 991 991 991", "991 0 991 991"),
            ("992 992 992 992", "992 992 0 992"),
            ("Groups:\t\n", "Groups:\t0 10\n"),
            ("NoNewPrivs:\t1", "NoNewPrivs:\t0"),
        ] {
            assert!(credentials(&raw.replace(before, after), 991, 992).is_err());
        }
        assert!(credentials(&raw, 0, 992).is_err());
        assert!(credentials(&raw, 991, 0).is_err());
        for name in ["CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb"] {
            assert!(
                credentials(
                    &raw.replace(
                        &format!("{name}:\t0000000000000000"),
                        &format!("{name}:\t0000000000000001")
                    ),
                    991,
                    992
                )
                .is_err()
            );
        }
    }
    #[test]
    fn process_credentials_reject_duplicate_missing_and_malformed_fields() {
        let raw = status_fixture();
        for line in [
            "Uid:\t991 991 991 991\n",
            "Gid:\t992 992 992 992\n",
            "Groups:\t\n",
            "NoNewPrivs:\t1\n",
            "CapEff:\t0000000000000000\n",
        ] {
            assert!(credentials(&format!("{raw}{line}"), 991, 992).is_err());
            assert!(credentials(&raw.replace(line, ""), 991, 992).is_err());
        }
        assert!(
            credentials(
                &raw.replace("CapEff:\t0000000000000000", "CapEff:\t0garbage000000000"),
                991,
                992
            )
            .is_err()
        );
    }
}
