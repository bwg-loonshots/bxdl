//! Bounded private diagnostic inputs and exclusive, atomic JSON publication.
//! Overlap policy and JSON redaction belong to the caller. These checks do not
//! isolate a user from another process running with the same effective UID.
use crate::error::{BxdlError, Result};
use cap_std::{
    ambient_authority,
    fs::{Dir, File, Metadata, MetadataExt, OpenOptions, OpenOptionsExt},
};
use rustix::fs::OFlags;
use std::{
    ffi::OsString,
    io::{ErrorKind, Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(super) struct ReadSnapshot {
    pub raw: Vec<u8>,
    /// Full file size observed before and after the bounded read.
    pub bytes: u64,
    pub truncated: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    mode: u32,
    uid: u32,
    links: u64,
    bytes: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl Stamp {
    fn of(m: &Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
            mode: m.mode(),
            uid: m.uid(),
            links: m.nlink(),
            bytes: m.len(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        }
    }
    fn same_inode(&self, m: &Metadata) -> bool {
        self.device == m.dev() && self.inode == m.ino()
    }
}

struct Anchor {
    path: PathBuf,
    dir: Dir,
    name: OsString,
    device: u64,
    inode: u64,
    private: bool,
}
impl Anchor {
    fn open(path: &Path, private: bool) -> Result<Self> {
        validate_path(path)?;
        let mut components = path.components().skip(1).collect::<Vec<_>>();
        let Some(Component::Normal(name)) = components.pop() else {
            return Err(unsafe_input());
        };
        let mut dir = Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| unavailable())?;
        safe_directory(&dir.dir_metadata().map_err(|_| unavailable())?, false)?;
        for component in components {
            let Component::Normal(name) = component else {
                return Err(unsafe_input());
            };
            let before = dir.symlink_metadata(name).map_err(|_| unavailable())?;
            safe_directory(&before, false)?;
            let mut options = OpenOptions::new();
            options.read(true).custom_flags(
                (OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits()
                    as i32,
            );
            let file = dir.open_with(name, &options).map_err(|_| unavailable())?;
            let opened = file.metadata().map_err(|_| unavailable())?;
            let visible = dir.symlink_metadata(name).map_err(|_| unavailable())?;
            safe_directory(&opened, false)?;
            safe_directory(&visible, false)?;
            if !same_directory(&before, &opened) || !same_directory(&opened, &visible) {
                return Err(changed());
            }
            dir = Dir::from_std_file(file.into_std());
        }
        let metadata = dir.dir_metadata().map_err(|_| unavailable())?;
        safe_directory(&metadata, private)?;
        Ok(Self {
            path: path.into(),
            dir,
            name: name.to_os_string(),
            device: metadata.dev(),
            inode: metadata.ino(),
            private,
        })
    }

    fn recheck(&self) -> Result<()> {
        let visible = Self::open(&self.path, self.private)?;
        let opened = self.dir.dir_metadata().map_err(|_| changed())?;
        safe_directory(&opened, self.private)?;
        if visible.device != self.device
            || visible.inode != self.inode
            || opened.dev() != self.device
            || opened.ino() != self.inode
        {
            return Err(changed());
        }
        Ok(())
    }
}

struct PrivateInput {
    anchor: Anchor,
    file: File,
    stamp: Stamp,
}
impl PrivateInput {
    fn open(path: &Path) -> Result<Self> {
        let anchor = Anchor::open(path, true)?;
        let before = anchor
            .dir
            .symlink_metadata(&anchor.name)
            .map_err(|_| unavailable())?;
        safe_file(&before)?;
        let mut options = OpenOptions::new();
        options
            .read(true)
            .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32);
        let file = anchor
            .dir
            .open_with(&anchor.name, &options)
            .map_err(|_| unavailable())?;
        let input = Self {
            anchor,
            file,
            stamp: Stamp::of(&before),
        };
        input.recheck()?;
        Ok(input)
    }
    fn recheck(&self) -> Result<()> {
        self.anchor.recheck()?;
        let visible = self
            .anchor
            .dir
            .symlink_metadata(&self.anchor.name)
            .map_err(|_| changed())?;
        let opened = self.file.metadata().map_err(|_| changed())?;
        safe_file(&visible)?;
        safe_file(&opened)?;
        if Stamp::of(&visible) != self.stamp || Stamp::of(&opened) != self.stamp {
            return Err(changed());
        }
        Ok(())
    }
}

/// A live append/change makes this individual source unavailable; it is not
/// silently combined with a different observation. Empty files are valid.
pub(super) fn read_private(path: &Path, max: u64) -> Result<ReadSnapshot> {
    if max > MAX_BYTES {
        return Err(input_limit());
    }
    let mut input = PrivateInput::open(path)?;
    let count = input.stamp.bytes.min(max);
    let mut raw = vec![0; count as usize];
    input.file.read_exact(&mut raw).map_err(|_| changed())?;
    input.recheck()?;
    Ok(ReadSnapshot {
        raw,
        bytes: input.stamp.bytes,
        truncated: count < input.stamp.bytes,
    })
}

/// Metadata only: never reads raw stdout/stderr or any other file contents.
pub(super) fn private_size(path: &Path) -> Result<u64> {
    let input = PrivateInput::open(path)?;
    input.recheck()?;
    Ok(input.stamp.bytes)
}

/// Caller must validate overlap with control/data/package and every input path
/// before invoking this function. No parent directory is created or repaired.
pub(super) fn write_export(path: &Path, raw: &[u8]) -> Result<()> {
    if raw.len() as u64 > MAX_BYTES {
        return Err(output_limit());
    }
    let anchor = Anchor::open(path, false).map_err(|_| output_unsafe())?;
    match anchor.dir.symlink_metadata(&anchor.name) {
        Err(e) if e.kind() == ErrorKind::NotFound => (),
        Ok(_) => return Err(output_exists()),
        Err(_) => return Err(output_unsafe()),
    }
    let mut pending = Pending::create(&anchor.dir, raw)?;
    anchor.recheck().map_err(|_| output_unsafe())?;
    pending.publish(&anchor.name)?;
    // Publication has occurred. A failed directory sync or final identity check
    // is uncertain, never proof that no output file was written.
    sync_dir(&anchor.dir).map_err(|_| output_uncertain())?;
    pending.remove_private().map_err(|_| output_uncertain())?;
    sync_dir(&anchor.dir).map_err(|_| output_uncertain())?;
    anchor.recheck().map_err(|_| output_uncertain())?;
    let final_metadata = anchor
        .dir
        .symlink_metadata(&anchor.name)
        .map_err(|_| output_uncertain())?;
    safe_file(&final_metadata).map_err(|_| output_uncertain())?;
    if !pending.identity.same_inode(&final_metadata) || final_metadata.len() != raw.len() as u64 {
        return Err(output_uncertain());
    }
    Ok(())
}

struct Pending<'a> {
    dir: &'a Dir,
    name: String,
    identity: Stamp,
}
impl<'a> Pending<'a> {
    fn create(dir: &'a Dir, raw: &[u8]) -> Result<Self> {
        let ticks = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| output_io())?
            .as_nanos();
        for _ in 0..64 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let name = format!(
                ".bxdl-diagnostic-{}-{ticks:x}-{sequence:x}.tmp",
                std::process::id()
            );
            let mut options = OpenOptions::new();
            options
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(
                    (OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32,
                );
            let mut file = match dir.open_with(&name, &options) {
                Ok(file) => file,
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(output_io()),
            };
            let metadata = file.metadata().map_err(|_| output_io())?;
            let mut pending = Self {
                dir,
                name,
                identity: Stamp::of(&metadata),
            };
            safe_file(&metadata).map_err(|_| output_unsafe())?;
            file.write_all(raw).map_err(|_| output_io())?;
            file.sync_all().map_err(|_| output_io())?;
            let metadata = file.metadata().map_err(|_| output_io())?;
            safe_file(&metadata).map_err(|_| output_unsafe())?;
            if !pending.identity.same_inode(&metadata) || metadata.len() != raw.len() as u64 {
                return Err(output_unsafe());
            }
            pending.identity = Stamp::of(&metadata);
            return Ok(pending);
        }
        Err(output_io())
    }
    fn publish(&mut self, name: &OsString) -> Result<()> {
        let metadata = self
            .dir
            .symlink_metadata(&self.name)
            .map_err(|_| output_unsafe())?;
        safe_file(&metadata).map_err(|_| output_unsafe())?;
        if Stamp::of(&metadata) != self.identity {
            return Err(output_unsafe());
        }
        self.dir.hard_link(&self.name, self.dir, name).map_err(|e| {
            if e.kind() == ErrorKind::AlreadyExists {
                output_exists()
            } else {
                output_io()
            }
        })
    }
    fn remove_private(&self) -> Result<()> {
        match self.dir.symlink_metadata(&self.name) {
            Ok(m) if self.identity.same_inode(&m) => {
                self.dir.remove_file(&self.name).map_err(|_| output_io())
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            _ => Err(output_unsafe()),
        }
    }
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        let _ = self.remove_private();
    }
}

fn validate_path(path: &Path) -> Result<()> {
    let text = path.to_str().ok_or_else(unsafe_input)?;
    if !path.is_absolute() || text.len() > 4096 || text.chars().any(char::is_control) {
        return Err(unsafe_input());
    }
    let mut normalized = PathBuf::from("/");
    for component in path.components().skip(1) {
        let Component::Normal(name) = component else {
            return Err(unsafe_input());
        };
        normalized.push(name);
    }
    if normalized.as_os_str() != path.as_os_str() || path == Path::new("/") {
        return Err(unsafe_input());
    }
    Ok(())
}
fn safe_directory(m: &Metadata, private: bool) -> Result<()> {
    if !m.is_dir()
        || m.file_type().is_symlink()
        || (m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0)
        || (private
            && (m.mode() & 0o7777 != 0o700 || m.uid() != rustix::process::geteuid().as_raw()))
    {
        return Err(unsafe_input());
    }
    Ok(())
}
fn safe_file(m: &Metadata) -> Result<()> {
    if !m.is_file()
        || m.file_type().is_symlink()
        || m.mode() & 0o7777 != 0o600
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.nlink() != 1
    {
        return Err(unsafe_input());
    }
    Ok(())
}
fn same_directory(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino() && a.mode() == b.mode() && a.uid() == b.uid()
}
fn sync_dir(dir: &Dir) -> Result<()> {
    dir.open(".")
        .and_then(|file| file.sync_all())
        .map_err(|_| output_io())
}
fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn unavailable() -> BxdlError {
    error(
        "DIAGNOSTIC_INPUT_UNAVAILABLE",
        "진단 자료를 읽을 수 없습니다.",
    )
}
fn unsafe_input() -> BxdlError {
    error(
        "DIAGNOSTIC_INPUT_UNSAFE",
        "진단 자료의 경로·파일 형식·권한이 안전하지 않습니다.",
    )
}
fn changed() -> BxdlError {
    error(
        "DIAGNOSTIC_INPUT_CHANGED",
        "읽는 동안 진단 자료가 변경되어 해당 관측을 사용하지 않았습니다.",
    )
}
fn input_limit() -> BxdlError {
    error(
        "DIAGNOSTIC_INPUT_LIMIT",
        "진단 읽기 크기 한도를 초과했습니다.",
    )
}
fn output_limit() -> BxdlError {
    error(
        "DIAGNOSTIC_OUTPUT_LIMIT",
        "진단 내보내기는 1 MiB 이하만 허용합니다.",
    )
}
fn output_exists() -> BxdlError {
    error(
        "DIAGNOSTIC_OUTPUT_EXISTS",
        "진단 출력 대상이 이미 있습니다. 기존 파일은 변경하지 않았습니다.",
    )
}
fn output_unsafe() -> BxdlError {
    error(
        "DIAGNOSTIC_OUTPUT_UNSAFE",
        "진단 출력 경로 또는 파일의 동일성을 확인할 수 없습니다.",
    )
}
fn output_io() -> BxdlError {
    error(
        "DIAGNOSTIC_OUTPUT_IO",
        "진단 출력 파일을 기록할 수 없습니다.",
    )
}
fn output_uncertain() -> BxdlError {
    error(
        "DIAGNOSTIC_EXPORT_UNCERTAIN",
        "진단 출력이 생성되었을 수 있으나 최종 저장을 확인하지 못했습니다. 기존 출력은 덮어쓰지 않습니다.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink},
        sync::{Arc, Barrier},
    };

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        (temp, root)
    }
    fn private(path: &Path, raw: &[u8]) {
        fs::write(path, raw).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn bounded_prefix_reports_full_size_and_accepts_empty_and_zero_budget() {
        let (_temp, root) = fixture();
        let file = root.join("report.jsonl");
        private(&file, b"first\nsecond\n");
        let snapshot = read_private(&file, 6).unwrap();
        assert_eq!(snapshot.raw, b"first\n");
        assert_eq!(snapshot.bytes, 13);
        assert!(snapshot.truncated);
        assert_eq!(private_size(&file).unwrap(), 13);
        let empty_prefix = read_private(&file, 0).unwrap();
        assert!(empty_prefix.raw.is_empty());
        assert!(empty_prefix.truncated);
        private(&file, b"");
        let empty = read_private(&file, 10).unwrap();
        assert_eq!(empty.bytes, 0);
        assert!(empty.raw.is_empty());
        assert!(!empty.truncated);
        assert_eq!(private_size(&file).unwrap(), 0);
    }

    #[test]
    fn size_only_and_small_prefix_do_not_allocate_or_read_whole_large_file() {
        let (_temp, root) = fixture();
        let path = root.join("stderr.private");
        private(&path, b"RAW_SECRET_CANARY");
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(64 * 1024 * 1024).unwrap();
        assert_eq!(private_size(&path).unwrap(), 64 * 1024 * 1024);
        let prefix = read_private(&path, 3).unwrap();
        assert_eq!(prefix.raw, b"RAW");
        assert!(prefix.truncated);
        assert_eq!(prefix.bytes, 64 * 1024 * 1024);
        assert_eq!(
            read_private(&path, MAX_BYTES + 1).unwrap_err().code,
            "DIAGNOSTIC_INPUT_LIMIT"
        );
    }

    #[test]
    fn symlinks_hardlinks_nonprivate_modes_and_parents_are_rejected() {
        let (_temp, root) = fixture();
        let file = root.join("private.json");
        private(&file, b"private");
        let alias = root.join("alias");
        symlink(&file, &alias).unwrap();
        assert!(read_private(&alias, 20).is_err());
        assert!(private_size(&alias).is_err());
        fs::remove_file(&alias).unwrap();
        fs::hard_link(&file, &alias).unwrap();
        assert!(read_private(&file, 20).is_err());
        assert!(private_size(&file).is_err());
        fs::remove_file(&alias).unwrap();
        for mode in [0o644, 0o666, 0o400, 0o1600] {
            fs::set_permissions(&file, fs::Permissions::from_mode(mode)).unwrap();
            assert!(read_private(&file, 20).is_err());
        }
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(read_private(&file, 20).is_err());
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let dir_alias = root.join("linked-parent");
        symlink(&root, &dir_alias).unwrap();
        assert!(read_private(&dir_alias.join("private.json"), 20).is_err());
    }

    #[test]
    fn fifo_is_rejected_without_opening_or_waiting_for_a_writer() {
        let (_temp, root) = fixture();
        let path = root.join("owned.fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(
            read_private(&path, 32).unwrap_err().code,
            "DIAGNOSTIC_INPUT_UNSAFE"
        );
        assert!(private_size(&path).is_err());
        assert_eq!(
            write_export(&path, b"{}").unwrap_err().code,
            "DIAGNOSTIC_OUTPUT_EXISTS"
        );
    }

    #[test]
    fn changed_or_replaced_input_and_parent_invalidates_snapshot() {
        let (_temp, root) = fixture();
        let directory = root.join("attempt");
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let path = directory.join("report.jsonl");
        private(&path, b"one");
        let input = PrivateInput::open(&path).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"two")
            .unwrap();
        assert_eq!(
            input.recheck().unwrap_err().code,
            "DIAGNOSTIC_INPUT_CHANGED"
        );
        let input = PrivateInput::open(&path).unwrap();
        fs::rename(&path, directory.join("old-report")).unwrap();
        private(&path, b"onetwo");
        assert_eq!(
            input.recheck().unwrap_err().code,
            "DIAGNOSTIC_INPUT_CHANGED"
        );
        let input = PrivateInput::open(&path).unwrap();
        fs::rename(&directory, root.join("old-attempt")).unwrap();
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        private(&path, b"onetwo");
        assert_eq!(
            input.recheck().unwrap_err().code,
            "DIAGNOSTIC_INPUT_CHANGED"
        );
    }

    #[test]
    fn export_is_private_complete_exclusive_and_accepts_more_than_store_limit() {
        let (_temp, root) = fixture();
        let path = root.join("diagnose.json");
        let raw =
            serde_json::to_vec(&serde_json::json!({"message":"x".repeat(300 * 1024)})).unwrap();
        write_export(&path, &raw).unwrap();
        assert_eq!(fs::read(&path).unwrap(), raw);
        let metadata = fs::metadata(&path).unwrap();
        assert_eq!(metadata.mode() & 0o7777, 0o600);
        assert_eq!(metadata.nlink(), 1);
        assert_eq!(
            write_export(&path, b"replacement").unwrap_err().code,
            "DIAGNOSTIC_OUTPUT_EXISTS"
        );
        assert_eq!(fs::read(&path).unwrap(), raw);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    }

    #[test]
    fn export_never_creates_parents_or_replaces_links_or_directories() {
        let (_temp, root) = fixture();
        assert!(write_export(&root.join("missing/output.json"), b"{}").is_err());
        assert!(!root.join("missing").exists());
        let original = root.join("original");
        private(&original, b"unchanged");
        let link = root.join("link");
        symlink(&original, &link).unwrap();
        assert_eq!(
            write_export(&link, b"{}").unwrap_err().code,
            "DIAGNOSTIC_OUTPUT_EXISTS"
        );
        fs::remove_file(&link).unwrap();
        fs::hard_link(&original, &link).unwrap();
        assert!(write_export(&link, b"{}").is_err());
        assert!(write_export(&root, b"{}").is_err());
        let directory_link = root.join("parent-alias");
        symlink(&root, &directory_link).unwrap();
        assert!(write_export(&directory_link.join("output.json"), b"{}").is_err());
        assert_eq!(fs::read(original).unwrap(), b"unchanged");
        assert!(!root.join("output.json").exists());
    }

    #[test]
    fn concurrent_exports_have_one_complete_winner() {
        let (_temp, root) = fixture();
        let path = root.join("diagnose.json");
        let barrier = Arc::new(Barrier::new(2));
        let workers = [b"first".to_vec(), b"second".to_vec()]
            .into_iter()
            .map(|raw| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    write_export(&path, &raw)
                })
            })
            .collect::<Vec<_>>();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .next()
                .unwrap()
                .code,
            "DIAGNOSTIC_OUTPUT_EXISTS"
        );
        assert!(
            [b"first".as_slice(), b"second".as_slice()]
                .contains(&fs::read(&path).unwrap().as_slice())
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    }

    #[test]
    fn bounds_and_fixed_errors_do_not_echo_paths_or_contents() {
        let (_temp, root) = fixture();
        let path = root.join("SECRET_PATH_CANARY");
        let err = read_private(&path, 10).unwrap_err();
        assert!(!err.to_string().contains("CANARY"));
        assert!(!err.to_string().contains(root.to_str().unwrap()));
        assert_eq!(
            write_export(&path, &vec![0; MAX_BYTES as usize + 1])
                .unwrap_err()
                .code,
            "DIAGNOSTIC_OUTPUT_LIMIT"
        );
        assert!(!path.exists());
        for path in [
            "relative",
            "/private/../tmp/f",
            "/private//tmp/f",
            "/private/line\nbreak",
            "/",
        ] {
            assert!(validate_path(Path::new(path)).is_err());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn existing_mac_case_alias_is_not_overwritten() {
        let (_temp, root) = fixture();
        let original = root.join("Report.json");
        private(&original, b"preserved");
        let alias = root.join("REPORT.JSON");
        // On case-sensitive volumes these are distinct names, not an alias.
        if alias.exists() {
            assert_eq!(
                write_export(&alias, b"{}").unwrap_err().code,
                "DIAGNOSTIC_OUTPUT_EXISTS"
            );
            assert_eq!(fs::read(original).unwrap(), b"preserved");
        }
    }
}
