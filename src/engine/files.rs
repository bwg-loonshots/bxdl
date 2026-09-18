use super::{fail, json};
use crate::error::{BxdlError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_JAR: u64 = 512 * 1024 * 1024;
const MAX_JAVA: u64 = 256 * 1024 * 1024;
#[cfg(target_os = "macos")]
const READ_FLAGS: i32 = 0x100 | 0x4; // O_NOFOLLOW | O_NONBLOCK
#[cfg(target_os = "linux")]
const READ_FLAGS: i32 = 0x20000 | 0x800;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    size: u64,
    mode: u32,
    modified: (i64, i64),
}
impl Stamp {
    fn of(m: &Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
            size: m.len(),
            mode: m.mode(),
            modified: (m.mtime(), m.mtime_nsec()),
        }
    }
}

pub struct Input {
    pub(super) path: PathBuf,
    stamp: Stamp,
    pub raw: Vec<u8>,
}
impl Input {
    pub fn read(path: &Path, maximum: u64, executable: bool) -> Result<Self> {
        let path = absolute(path)?;
        let (mut file, stamp) = open(&path, maximum, executable)?;
        let mut raw = Vec::new();
        Read::by_ref(&mut file)
            .take(maximum + 1)
            .read_to_end(&mut raw)
            .map_err(|_| io_error())?;
        if raw.len() as u64 != stamp.size {
            return Err(changed());
        }
        recheck(&path, &file, &stamp)?;
        Ok(Self { path, stamp, raw })
    }
    pub fn recheck(&self) -> Result<()> {
        let fresh = Self::read(&self.path, self.stamp.size, false)?;
        if self.stamp != fresh.stamp || self.raw != fresh.raw {
            return Err(changed());
        }
        Ok(())
    }
}

pub struct Binary {
    pub path: PathBuf,
    expected_hash: String,
    stamp: Stamp,
    executable: bool,
    maximum: u64,
}
impl Binary {
    pub fn open(path: &Path, expected_hash: &str) -> Result<Self> {
        Self::open_checked(path, expected_hash, true, MAX_JAVA)
    }
    pub(super) fn open_data(path: &Path, expected_hash: &str) -> Result<Self> {
        Self::open_checked(path, expected_hash, false, MAX_JAR)
    }
    fn open_checked(
        path: &Path,
        expected_hash: &str,
        executable: bool,
        maximum: u64,
    ) -> Result<Self> {
        let path = absolute(path)?;
        let (mut file, stamp) = open(&path, maximum, executable)?;
        if hash_file(&mut file, maximum)? != expected_hash {
            return Err(pin_mismatch());
        }
        recheck(&path, &file, &stamp)?;
        Ok(Self {
            path,
            expected_hash: expected_hash.into(),
            stamp,
            executable,
            maximum,
        })
    }
    pub fn recheck(&self) -> Result<()> {
        let fresh = Self::open_checked(
            &self.path,
            &self.expected_hash,
            self.executable,
            self.maximum,
        )?;
        if self.stamp != fresh.stamp {
            return Err(changed());
        }
        Ok(())
    }
}

pub struct Workspace {
    pub path: PathBuf,
    device: u64,
    inode: u64,
    persistent: bool,
}
impl Workspace {
    pub fn create(native: Option<&NativeInput>) -> Result<Self> {
        #[cfg(target_os = "macos")]
        let parent = Path::new("/private/tmp");
        #[cfg(target_os = "linux")]
        let parent = Path::new("/tmp");
        check_path(parent, false)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| io_error())?
            .as_nanos();
        for _ in 0..64 {
            let id = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!("bxdl-engine-{}-{now:x}-{id:x}", std::process::id()));
            if let Some(native) = native {
                // Avoid even a temporary directory inside the operator's data.
                for forbidden in std::iter::once(&native.data)
                    .chain(std::iter::once(&native.config.path))
                    .chain(std::iter::once(&native.chain.path))
                    .chain(native.references.iter())
                {
                    if overlaps(forbidden, &path)? {
                        return Err(config_invalid());
                    }
                }
            }
            let result = fs::DirBuilder::new().mode(0o700).create(&path);
            match result {
                Ok(()) => {
                    let m = fs::symlink_metadata(&path).map_err(|_| io_error())?;
                    if !m.is_dir() || m.mode() & 0o777 != 0o700 {
                        return Err(unsafe_input());
                    }
                    return Ok(Self {
                        path,
                        device: m.dev(),
                        inode: m.ino(),
                        persistent: false,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(io_error()),
            }
        }
        Err(io_error())
    }
    pub fn persistent(path: &Path) -> Result<Self> {
        check_path(path, false)?;
        let m = fs::symlink_metadata(path).map_err(|_| io_error())?;
        let result = Self {
            path: path.into(),
            device: m.dev(),
            inode: m.ino(),
            persistent: true,
        };
        result.recheck()?;
        Ok(result)
    }
    pub fn recheck(&self) -> Result<()> {
        let m = fs::symlink_metadata(&self.path).map_err(|_| changed())?;
        if !m.is_dir()
            || m.dev() != self.device
            || m.ino() != self.inode
            || m.mode() & 0o777 != 0o700
        {
            return Err(changed());
        }
        Ok(())
    }
    pub fn write(&self, name: &str, raw: &[u8]) -> Result<PathBuf> {
        self.recheck()?;
        let path = self.path.join(name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|_| io_error())?;
        file.write_all(raw)
            .and_then(|_| file.sync_all())
            .map_err(|_| io_error())?;
        self.recheck()?;
        File::open(&self.path)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| io_error())?;
        Ok(path)
    }
    pub fn snapshot_jar(&self, path: &Path, expected: &str, size: u64) -> Result<PathBuf> {
        self.recheck()?;
        let path = absolute(path)?;
        let (mut input, stamp) = open(&path, MAX_JAR, false)?;
        if stamp.size != size {
            return Err(pin_mismatch());
        }
        let snapshot = self.path.join("engine.jar");
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&snapshot)
            .map_err(|_| io_error())?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut copied = 0u64;
        loop {
            let count = input.read(&mut buffer).map_err(|_| io_error())?;
            if count == 0 {
                break;
            }
            copied += count as u64;
            if copied > size {
                return Err(pin_mismatch());
            }
            hasher.update(&buffer[..count]);
            output.write_all(&buffer[..count]).map_err(|_| io_error())?;
        }
        output.sync_all().map_err(|_| io_error())?;
        recheck(&path, &input, &stamp)?;
        self.recheck()?;
        if copied != size || hex::encode(hasher.finalize()) != expected {
            return Err(pin_mismatch());
        }
        File::open(&self.path)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| io_error())?;
        Ok(snapshot)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        if !self.persistent && self.recheck().is_ok() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct NativeConfig {
    chain_file: String,
    data_directory: String,
    pub(super) backend: String,
    pub(super) node: BTreeMap<String, Value>,
}
pub struct NativeInput {
    pub config: Input,
    pub chain: Input,
    pub(super) value: NativeConfig,
    pub(super) data: PathBuf,
    pub(super) references: Vec<PathBuf>,
}
impl NativeInput {
    pub fn load(path: &Path) -> Result<Self> {
        let config = Input::read(path, 262_144, false)?;
        let mut value: NativeConfig = json::decode(&config.raw).map_err(|_| config_invalid())?;
        let base = config.path.parent().ok_or_else(config_invalid)?;
        let chain_path = reference(base, &value.chain_file)?;
        let data = reference(base, &value.data_directory)?;
        if data.parent().is_none()
            || crate::setup::paths::same(&data, base).map_err(|_| config_invalid())?
            || overlaps(&config.path, &data)?
            || overlaps(&chain_path, &data)?
        {
            return Err(config_invalid());
        }
        let chain = Input::read(&chain_path, 262_144, false)?;
        let mut references = Vec::new();
        for (key, item) in &mut value.node {
            if !(item.is_string() || item.is_boolean() || item.is_number()) {
                return Err(config_invalid());
            }
            // NIGO's documented native path properties, not BXDL config mapping.
            if key.ends_with("-path") || key.ends_with("-file") {
                let path = reference(base, item.as_str().ok_or_else(config_invalid)?)?;
                *item = Value::String(path_text(&path)?);
                references.push(path);
            }
        }
        value.data_directory = path_text(&data)?;
        Ok(Self {
            config,
            chain,
            value,
            data,
            references,
        })
    }
    pub fn snapshot(&self, workspace: &Workspace) -> Result<PathBuf> {
        self.recheck()?;
        let chain = workspace.write("chain.json", &self.chain.raw)?;
        let mut value = serde_json::to_value(&self.value).map_err(|_| config_invalid())?;
        value["chainFile"] = Value::String(path_text(&chain)?);
        let raw = serde_json::to_vec(&value).map_err(|_| config_invalid())?;
        workspace.write("node.json", &raw)
    }
    pub fn recheck(&self) -> Result<()> {
        self.config.recheck()?;
        self.chain.recheck()?;
        check_path(&self.data, true)?;
        for path in &self.references {
            check_path(path, true)?;
        }
        Ok(())
    }
}

pub fn digest(raw: &[u8]) -> String {
    hex::encode(Sha256::digest(raw))
}
fn hash_file(file: &mut File, maximum: u64) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|_| io_error())?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > maximum {
            return Err(unsafe_input());
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
fn open(path: &Path, maximum: u64, executable: bool) -> Result<(File, Stamp)> {
    check_path(path, false)?;
    let before = fs::symlink_metadata(path).map_err(|_| io_error())?;
    if !before.is_file()
        || before.len() == 0
        || before.len() > maximum
        || before.mode() & 0o022 != 0
        || (executable && before.mode() & 0o111 == 0)
    {
        return Err(unsafe_input());
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(READ_FLAGS)
        .open(path)
        .map_err(|_| io_error())?;
    let stamp = Stamp::of(&before);
    recheck(path, &file, &stamp)?;
    Ok((file, stamp))
}
fn recheck(path: &Path, file: &File, stamp: &Stamp) -> Result<()> {
    check_path(path, false)?;
    let visible = fs::symlink_metadata(path).map_err(|_| changed())?;
    let opened = file.metadata().map_err(|_| changed())?;
    if Stamp::of(&visible) != *stamp || Stamp::of(&opened) != *stamp {
        return Err(changed());
    }
    Ok(())
}
fn reference(base: &Path, text: &str) -> Result<PathBuf> {
    if text.contains("${") {
        return Err(config_invalid());
    }
    let raw = Path::new(text);
    let path = absolute(&if raw.is_absolute() {
        raw.into()
    } else {
        base.join(raw)
    })?;
    check_path(&path, true)?;
    Ok(path)
}
fn overlaps(a: &Path, b: &Path) -> Result<bool> {
    crate::setup::paths::overlaps(a, b).map_err(|_| config_invalid())
}
pub(super) fn absolute(path: &Path) -> Result<PathBuf> {
    let text = path_text(path)?;
    if text.is_empty()
        || text.len() > 4096
        || text.starts_with('~')
        || text.contains('\\')
        || text.chars().any(char::is_control)
    {
        return Err(path_invalid());
    }
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(|_| io_error())?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            c => normalized.push(c.as_os_str()),
        }
    }
    Ok(normalized)
}
pub(super) fn check_path(path: &Path, allow_missing: bool) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(m) => {
                if m.file_type().is_symlink() || (!m.is_file() && !m.is_dir()) {
                    return Err(path_invalid());
                }
                if current != path && !m.is_dir() {
                    return Err(path_invalid());
                }
                // Sticky shared temp parents are allowed; ordinary writable
                // ancestors cannot be the trust boundary for executable input.
                if m.is_dir() && m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0 {
                    return Err(unsafe_input());
                }
            }
            Err(e) if allow_missing && e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(io_error()),
        }
    }
    Ok(())
}
fn path_text(path: &Path) -> Result<String> {
    path.to_str().map(str::to_owned).ok_or_else(path_invalid)
}
fn config_invalid() -> BxdlError {
    fail(
        "ENGINE_CONFIG_INVALID",
        "명시적인 NIGO native 설정과 자료 경로가 필요합니다.",
    )
}
fn path_invalid() -> BxdlError {
    fail(
        "ENGINE_PATH_INVALID",
        "링크가 없는 일반 파일의 실제 경로를 사용하세요.",
    )
}
fn unsafe_input() -> BxdlError {
    fail(
        "ENGINE_INPUT_UNSAFE",
        "입력 파일 종류·권한·크기 또는 상위 경로를 확인하세요.",
    )
}
fn io_error() -> BxdlError {
    fail(
        "ENGINE_INPUT_IO",
        "엔진 입력 또는 비공개 임시 파일을 처리하지 못했습니다.",
    )
}
fn changed() -> BxdlError {
    fail(
        "ENGINE_INPUT_CHANGED",
        "검사 중 입력 또는 실행 경로가 변경되었습니다.",
    )
}
fn pin_mismatch() -> BxdlError {
    fail(
        "ENGINE_PIN_MISMATCH",
        "실제 JAR 또는 Java bytes가 trusted lock과 다릅니다.",
    )
}
