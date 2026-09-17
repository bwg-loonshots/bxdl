//! First-install extraction for a development macOS arm64 package. The caller
//! chooses a new directory; this module never initializes an engine, opens data,
//! starts a service, or downloads anything. A failed reserved directory remains
//! incomplete and requires explicit inspection/removal; it is never resumed.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use cap_std::ambient_authority;
use cap_std::fs::{
    Dir, DirBuilder, DirBuilderExt, File, Metadata, MetadataExt, OpenOptions, OpenOptionsExt,
    Permissions, PermissionsExt,
};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization as _;

use crate::artifact::{self, FileEntry, Manifest, PayloadSink, Report, VerifyOptions};
use crate::error::{BxdlError, Result};

const RECEIPT: &str = ".bxdl-install.json";
const RECEIPT_TEMP: &str = ".bxdl-install-receipt.tmp";
const MAX_PARENT_ENTRIES: usize = 100_000;
#[cfg(target_os = "macos")]
const READ_FLAGS: i32 = 0x100 | 0x4;
#[cfg(target_os = "linux")]
const READ_FLAGS: i32 = 0x20000 | 0x800;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallReport {
    pub schema_version: u32,
    pub kind: String,
    pub outcome: String,
    pub destination: String,
    pub archive_sha256: String,
    pub manifest_sha256: String,
    pub authenticity: String,
    pub files_installed: usize,
    pub bytes_installed: u64,
    pub manifest: Manifest,
    pub engine_validation: String,
    pub lifecycle: String,
}

/// Consume and extract one verified stream into an exclusively reserved new
/// destination. Receipt absence means INCOMPLETE, including after a late gzip
/// failure. No caller-supplied existing directory is adopted or cleaned up.
pub fn install(
    archive: &Path,
    destination: &Path,
    options: &VerifyOptions,
) -> Result<InstallReport> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err(error(
            "INSTALL_PLATFORM_UNSUPPORTED",
            "Installation currently requires a macOS arm64 host",
        ));
    }
    let mut sink = Installer::new(destination)?;
    let verified = artifact::verify_to_sink(archive, options, &mut sink)?;
    sink.commit(verified)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    dev: u64,
    ino: u64,
    size: u64,
    mode: u32,
    seconds: i64,
    nanos: i64,
}
impl Identity {
    fn of(m: &Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            size: m.len(),
            mode: m.mode(),
            seconds: m.mtime(),
            nanos: m.mtime_nsec(),
        }
    }
    fn same_inode(&self, m: &Metadata) -> bool {
        self.dev == m.dev() && self.ino == m.ino()
    }
}

struct InstalledFile {
    entry: FileEntry,
    identity: Identity,
}
struct ActiveFile {
    file: File,
    entry: FileEntry,
    identity: Identity,
    written: u64,
}
struct Installer {
    path: PathBuf,
    parent: Dir,
    parent_identity: Identity,
    name: OsString,
    root: Option<Dir>,
    root_identity: Option<Identity>,
    directories: BTreeMap<PathBuf, Identity>,
    files: Vec<InstalledFile>,
    active: Option<ActiveFile>,
    manifest_bytes: Vec<u8>,
    signature: Option<Vec<u8>>,
}
impl Installer {
    fn new(destination: &Path) -> Result<Self> {
        let (path, parent, name) = parent_anchor(destination)?;
        reject_destination_alias(&parent, &name)?;
        let parent_identity = Identity::of(&parent.dir_metadata().map_err(|_| io_error())?);
        Ok(Self {
            path,
            parent,
            parent_identity,
            name,
            root: None,
            root_identity: None,
            directories: BTreeMap::new(),
            files: Vec::new(),
            active: None,
            manifest_bytes: Vec::new(),
            signature: None,
        })
    }
    fn root(&self) -> Result<&Dir> {
        self.root.as_ref().ok_or_else(unsafe_error)
    }
    fn check_anchor(&self) -> Result<()> {
        let (_, visible_parent, name) = parent_anchor(&self.path)?;
        if !self
            .parent_identity
            .same_inode(&visible_parent.dir_metadata().map_err(|_| io_error())?)
        {
            return Err(unsafe_error());
        }
        if let Some(identity) = &self.root_identity {
            let visible = visible_parent
                .symlink_metadata(name)
                .map_err(|_| unsafe_error())?;
            let anchored = self.root()?.dir_metadata().map_err(|_| io_error())?;
            if !identity.same_inode(&visible)
                || !identity.same_inode(&anchored)
                || !visible.is_dir()
                || !anchored.is_dir()
                || visible.mode() & 0o7777 != 0o700
                || anchored.mode() & 0o7777 != 0o700
            {
                return Err(unsafe_error());
            }
        }
        Ok(())
    }
    fn reserve(&mut self) -> Result<()> {
        self.check_anchor()?;
        reject_destination_alias(&self.parent, &self.name)?;
        let mut builder = DirBuilder::new();
        builder.mode(0o700);
        self.parent
            .create_dir_with(&self.name, &builder)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    exists_error()
                } else {
                    io_error()
                }
            })?;
        let before = self
            .parent
            .symlink_metadata(&self.name)
            .map_err(|_| unsafe_error())?;
        if !before.is_dir() || before.mode() & 0o7777 != 0o700 {
            return Err(unsafe_error());
        }
        let root = self
            .parent
            .open_dir(&self.name)
            .map_err(|_| unsafe_error())?;
        if !Identity::of(&before).same_inode(&root.dir_metadata().map_err(|_| io_error())?) {
            return Err(unsafe_error());
        }
        self.root_identity = Some(Identity::of(&before));
        self.root = Some(root);
        sync_dir(&self.parent)?;
        self.check_anchor()
    }
    fn file_parent(&mut self, path: &str) -> Result<(Dir, OsString)> {
        self.check_anchor()?;
        let path = Path::new(path);
        let mut directory = self.root()?.try_clone().map_err(|_| io_error())?;
        let mut relative = PathBuf::new();
        let parent = path.parent().ok_or_else(unsafe_error)?;
        for component in parent.components() {
            let Component::Normal(name) = component else {
                return Err(unsafe_error());
            };
            relative.push(name);
            if !self.directories.contains_key(&relative) {
                let mut builder = DirBuilder::new();
                builder.mode(0o700);
                directory
                    .create_dir_with(name, &builder)
                    .map_err(|_| unsafe_error())?;
                let metadata = directory
                    .symlink_metadata(name)
                    .map_err(|_| unsafe_error())?;
                if !metadata.is_dir() || metadata.mode() & 0o7777 != 0o700 {
                    return Err(unsafe_error());
                }
                self.directories
                    .insert(relative.clone(), Identity::of(&metadata));
                sync_dir(&directory)?;
            }
            let expected = self.directories.get(&relative).ok_or_else(unsafe_error)?;
            let metadata = directory
                .symlink_metadata(name)
                .map_err(|_| unsafe_error())?;
            if !metadata.is_dir()
                || metadata.mode() & 0o7777 != 0o700
                || !expected.same_inode(&metadata)
            {
                return Err(unsafe_error());
            }
            let opened = directory.open_dir(name).map_err(|_| unsafe_error())?;
            if !expected.same_inode(&opened.dir_metadata().map_err(|_| unsafe_error())?) {
                return Err(unsafe_error());
            }
            directory = opened;
        }
        Ok((
            directory,
            path.file_name().ok_or_else(unsafe_error)?.to_os_string(),
        ))
    }
    fn commit(mut self, verified: Report) -> Result<InstallReport> {
        if self.active.is_some() || self.files.len() != verified.files_verified {
            return Err(unsafe_error());
        }
        self.check_anchor()?;
        // All archive bytes, including gzip CRC/EOF, have now passed the same
        // parser that supplied the written bytes. Recheck identities before
        // enabling executable modes; the private root remains 0700.
        for index in 0..self.files.len() {
            let entry = self.files[index].entry.clone();
            let expected = self.files[index].identity.clone();
            let (parent, name) = self.file_parent(&entry.path)?;
            let metadata = parent.symlink_metadata(&name).map_err(|_| unsafe_error())?;
            if Identity::of(&metadata) != expected || metadata.nlink() != 1 {
                return Err(unsafe_error());
            }
            let mut options = OpenOptions::new();
            options.read(true).custom_flags(READ_FLAGS);
            let file = parent
                .open_with(&name, &options)
                .map_err(|_| unsafe_error())?;
            if Identity::of(&file.metadata().map_err(|_| io_error())?) != expected {
                return Err(unsafe_error());
            }
            file.set_permissions(Permissions::from_mode(entry.mode))
                .map_err(|_| io_error())?;
            file.sync_all().map_err(|_| io_error())?;
            sync_dir(&parent)?;
        }
        write_regular(self.root()?, "manifest.json", &self.manifest_bytes, 0o644)?;
        if let Some(signature) = &self.signature {
            write_regular(self.root()?, "manifest.sig", signature, 0o644)?;
        }
        let report = InstallReport {
            schema_version: 1,
            kind: "BXDL_INSTALL_RECEIPT".into(),
            outcome: "INSTALLED".into(),
            destination: self.path.to_str().ok_or_else(unsafe_error)?.into(),
            archive_sha256: verified.archive_sha256,
            manifest_sha256: verified.manifest_sha256,
            authenticity: verified.authenticity,
            files_installed: verified.files_verified,
            bytes_installed: verified.bytes_verified,
            manifest: verified.manifest,
            engine_validation: "NOT_CHECKED".into(),
            lifecycle: "NOT_PERFORMED".into(),
        };
        let mut raw = serde_json::to_vec_pretty(&report).map_err(|_| io_error())?;
        raw.push(b'\n');
        self.check_anchor()?;
        sync_dir(self.root()?)?;
        let temp = write_regular(self.root()?, RECEIPT_TEMP, &raw, 0o600)?;
        let mut publication = ReceiptPublication {
            root: self.root()?,
            identity: Identity::of(&temp.metadata().map_err(|_| io_error())?),
            published: false,
            complete: false,
        };
        publication
            .root
            .hard_link(RECEIPT_TEMP, publication.root, RECEIPT)
            .map_err(|_| unsafe_error())?;
        publication.published = true;
        let committed = publication
            .root
            .remove_file(RECEIPT_TEMP)
            .map_err(|_| io_error())
            .and_then(|_| sync_dir(publication.root))
            .and_then(|_| self.check_anchor());
        if committed.is_err() {
            // Drop attempts to remove only this receipt inode. If the filesystem
            // also refuses cleanup, presence/durability is uncertain rather than
            // an unverified package being reported as successfully installed.
            return Err(error(
                "INSTALL_COMMIT_UNCERTAIN",
                "Receipt publication could not be confirmed; inspect the reserved installation before any further operation",
            ));
        }
        publication.complete = true;
        Ok(report)
    }
}
impl PayloadSink for Installer {
    fn begin(
        &mut self,
        manifest: &Manifest,
        manifest_bytes: &[u8],
        signature: Option<&[u8]>,
    ) -> Result<()> {
        if manifest.platform.os != "darwin" || manifest.platform.arch != "arm64" {
            return Err(error(
                "INSTALL_PLATFORM_UNSUPPORTED",
                "Package target must be darwin arm64 for this installer",
            ));
        }
        reject_inventory_aliases(manifest)?;
        self.manifest_bytes = manifest_bytes.to_vec();
        self.signature = signature.map(<[u8]>::to_vec);
        self.reserve()
    }
    fn start_file(&mut self, entry: &FileEntry) -> Result<()> {
        if self.active.is_some() {
            return Err(unsafe_error());
        }
        let (parent, name) = self.file_parent(&entry.path)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        let file = parent
            .open_with(name, &options)
            .map_err(|_| unsafe_error())?;
        let metadata = file.metadata().map_err(|_| io_error())?;
        if !metadata.is_file() || metadata.mode() & 0o7777 != 0o600 || metadata.nlink() != 1 {
            return Err(unsafe_error());
        }
        self.active = Some(ActiveFile {
            file,
            entry: entry.clone(),
            identity: Identity::of(&metadata),
            written: 0,
        });
        Ok(())
    }
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<()> {
        let active = self.active.as_mut().ok_or_else(unsafe_error)?;
        if bytes.len() as u64 > active.entry.size.saturating_sub(active.written) {
            return Err(unsafe_error());
        }
        active.file.write_all(bytes).map_err(|_| io_error())?;
        active.written += bytes.len() as u64;
        Ok(())
    }
    fn finish_file(&mut self) -> Result<()> {
        let active = self.active.take().ok_or_else(unsafe_error)?;
        active.file.sync_all().map_err(|_| io_error())?;
        let metadata = active.file.metadata().map_err(|_| io_error())?;
        if !metadata.is_file()
            || metadata.mode() & 0o7777 != 0o600
            || metadata.nlink() != 1
            || !active.identity.same_inode(&metadata)
            || metadata.len() != active.entry.size
            || active.written != active.entry.size
        {
            return Err(unsafe_error());
        }
        self.files.push(InstalledFile {
            entry: active.entry,
            identity: Identity::of(&metadata),
        });
        Ok(())
    }
}

struct ReceiptPublication<'a> {
    root: &'a Dir,
    identity: Identity,
    published: bool,
    complete: bool,
}
impl Drop for ReceiptPublication<'_> {
    fn drop(&mut self) {
        if !self.complete {
            if self.published {
                remove_owned(self.root, RECEIPT, &self.identity);
            }
            remove_owned(self.root, RECEIPT_TEMP, &self.identity);
            let _ = sync_dir(self.root);
        }
    }
}
fn remove_owned(root: &Dir, name: &str, identity: &Identity) {
    if root
        .symlink_metadata(name)
        .is_ok_and(|m| identity.same_inode(&m))
    {
        let _ = root.remove_file(name);
    }
}
fn write_regular(root: &Dir, name: &str, raw: &[u8], mode: u32) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = root.open_with(name, &options).map_err(|_| unsafe_error())?;
    file.write_all(raw).map_err(|_| io_error())?;
    let metadata = file.metadata().map_err(|_| io_error())?;
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() != raw.len() as u64 {
        return Err(unsafe_error());
    }
    file.set_permissions(Permissions::from_mode(mode))
        .map_err(|_| io_error())?;
    file.sync_all().map_err(|_| io_error())?;
    Ok(file)
}
fn folded(name: &str) -> String {
    name.nfd()
        .flat_map(char::to_uppercase)
        .flat_map(char::to_lowercase)
        .nfd()
        .collect()
}
fn reject_inventory_aliases(manifest: &Manifest) -> Result<()> {
    let mut entries = BTreeMap::<String, (String, bool)>::new();
    for entry in &manifest.files {
        let parts: Vec<_> = entry.path.split('/').collect();
        for end in 1..=parts.len() {
            let path = parts[..end].join("/");
            let directory = end < parts.len();
            let key = folded(&path);
            if let Some((old_path, old_directory)) = entries.get(&key) {
                if old_path != &path || !directory || !old_directory {
                    return Err(error(
                        "INSTALL_PATH_COLLISION",
                        "Package paths collide under macOS filename equivalence",
                    ));
                }
            } else {
                entries.insert(key, (path, directory));
            }
        }
    }
    Ok(())
}
fn reject_destination_alias(parent: &Dir, name: &std::ffi::OsStr) -> Result<()> {
    let target = folded(name.to_str().ok_or_else(unsafe_error)?);
    for (count, entry) in parent.entries().map_err(|_| io_error())?.enumerate() {
        if count >= MAX_PARENT_ENTRIES {
            return Err(error(
                "INSTALL_LIMIT",
                "Destination parent enumeration exceeds the supported limit",
            ));
        }
        let entry = entry.map_err(|_| io_error())?;
        let name = entry.file_name();
        if folded(name.to_str().ok_or_else(unsafe_error)?) == target {
            return Err(exists_error());
        }
    }
    Ok(())
}
fn parent_anchor(path: &Path) -> Result<(PathBuf, Dir, OsString)> {
    let text = path.to_str().ok_or_else(unsafe_error)?;
    if text.is_empty() || text.len() > 4096 || text.chars().any(char::is_control) {
        return Err(unsafe_error());
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(|_| io_error())?.join(path)
    };
    let mut components = Vec::new();
    for component in absolute.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => components.push(name.to_os_string()),
            _ => return Err(unsafe_error()),
        }
    }
    let name = components.pop().ok_or_else(unsafe_error)?;
    let mut parent = Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| io_error())?;
    let mut normalized = PathBuf::from("/");
    for component in components {
        let before = parent
            .symlink_metadata(&component)
            .map_err(|_| io_error())?;
        if !before.is_dir() {
            return Err(unsafe_error());
        }
        let next = parent.open_dir(&component).map_err(|_| unsafe_error())?;
        if !Identity::of(&before).same_inode(&next.dir_metadata().map_err(|_| io_error())?) {
            return Err(unsafe_error());
        }
        parent = next;
        normalized.push(component);
    }
    normalized.push(&name);
    Ok((normalized, parent, name))
}
fn sync_dir(dir: &Dir) -> Result<()> {
    dir.try_clone()
        .map_err(|_| io_error())?
        .into_std_file()
        .sync_all()
        .map_err(|_| io_error())
}
fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn io_error() -> BxdlError {
    error(
        "INSTALL_IO",
        "Installation I/O failed; a reserved directory is incomplete and must be inspected explicitly",
    )
}
fn unsafe_error() -> BxdlError {
    error(
        "INSTALL_UNSAFE_PATH",
        "Installation path identity, type, link or permissions are unsafe",
    )
}
fn exists_error() -> BxdlError {
    error(
        "INSTALL_DESTINATION_EXISTS",
        "Installation requires a new destination; existing or incomplete directories are never reused",
    )
}

#[cfg(test)]
#[path = "install/tests.rs"]
mod tests;
