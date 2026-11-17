//! Approved development installation. An unprivileged plan is content-bound;
//! the privileged helper snapshots it into a separate root transaction before
//! creating a dedicated account or registering the node. No user catalog writes.
mod accounts;
mod source;
#[cfg(test)]
mod tests;
use super::*;
use crate::install::RootIdentity;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};

pub const HELPER: &str = "/usr/local/libexec/bxdl/bxdl";
pub const REQUEST_LIMIT: u64 = 262_144;
const BASE: &str = "/var/lib/bxdl";
const QBFT: &str = "nigo.protocol.consensus.qbft.node.";
const SECRET_MAPPING: &[(&str, &str)] = &[
    ("validatorKeystore", "keystore-path"),
    ("validatorPasswordFile", "keystore-password-file"),
    ("tlsKeyStore", "mtls-key-store-path"),
    ("tlsKeyPasswordFile", "mtls-key-store-password-file"),
    ("tlsTrustStore", "mtls-trust-store-path"),
    ("tlsTrustPasswordFile", "mtls-trust-store-password-file"),
];
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Inputs {
    pub runtime_root: PathBuf,
    pub engine_jar: PathBuf,
    pub product_config: PathBuf,
    pub native_config: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pin {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub schema_version: u32,
    pub name: String,
    pub control: PathBuf,
    pub operator_uid: u32,
    pub inputs: Inputs,
    pub pins: Vec<Pin>,
    product: Value,
    native: Value,
    runtime_lock_sha256: String,
    engine_lock_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub schema_version: u32,
    pub name: String,
    pub control: PathBuf,
    pub request_sha256: String,
    pub operator_uid: u32,
    pub descriptor: super::management::Descriptor,
    pub service_uid: u32,
    pub service_gid: u32,
    pub development_only: bool,
    pub runtime_package_acceptance: String,
}
fn install_error(code: &str) -> BxdlError {
    BxdlError::new(
        code,
        match code {
            "LINUX_INSTALL_PRIVILEGE_REQUIRED" => {
                "관리자 인증을 확인한 뒤 같은 설치 준비를 다시 적용하세요."
            }
            "LINUX_INSTALL_INPUT_CHANGED" => {
                "승인한 설치 입력이 변경됐습니다. 준비 내용을 다시 검토하세요."
            }
            "LINUX_INSTALL_PARTIAL" => {
                "이 설치의 일부 결과가 남아 있습니다. 기존 폴더·계정·기록을 보존하고 진단하세요."
            }
            "LINUX_INSTALL_ACCOUNT_CONFLICT" => {
                "전용 계정 이름이나 UID/GID가 충돌합니다. 다른 계정을 임의 변경하지 않았습니다."
            }
            "LINUX_INSTALL_HELPER_REQUIRED" => {
                "관리자가 root 소유 Linux 설치 helper를 먼저 준비해야 합니다."
            }
            "LINUX_INSTALL_UNKNOWN" => {
                "설치 결과를 확정하지 못했습니다. 시도 기록을 보존하고 같은 준비로 결과를 다시 확인하세요."
            }
            _ => "Linux 설치 요청·권한·고정 공급물을 확인하세요.",
        },
    )
}
fn invalid() -> BxdlError {
    install_error("LINUX_INSTALL_REQUEST_INVALID")
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub fn control_path(name: &str) -> Result<PathBuf> {
    if !valid_name(name) {
        return Err(invalid());
    }
    Ok(Path::new(BASE).join("instances").join(name))
}
fn text(value: &Value) -> Result<&str> {
    value.as_str().ok_or_else(invalid)
}
fn reference_paths(product: &Value, native: &Value) -> Result<BTreeSet<PathBuf>> {
    let mut paths = BTreeSet::new();
    paths.insert(PathBuf::from(text(&product["chainDescription"])?));
    for (name, _) in SECRET_MAPPING {
        paths.insert(PathBuf::from(text(&product["secrets"][*name])?));
    }
    for (key, value) in native["node"].as_object().ok_or_else(invalid)? {
        if key.ends_with("-path") || key.ends_with("-file") {
            paths.insert(PathBuf::from(text(value)?));
        }
    }
    if paths.len() > 32 {
        return Err(invalid());
    }
    for path in &paths {
        boundary::safe_absolute(path)?;
    }
    Ok(paths)
}
/// Derive a reviewed native copy; preserve peers, transport policy and every
/// unrelated native setting. This never modifies the supplied templates.
fn derive(name: &str, control: &Path, product: &mut Value, native: &mut Value) -> Result<()> {
    product["instanceId"] = Value::String(name.into());
    product["storage"]["dataDirectory"] =
        Value::String(control.join("data").to_string_lossy().into());
    native["dataDirectory"] = product["storage"]["dataDirectory"].clone();
    native["chainFile"] = product["chainDescription"].clone();
    native["backend"] = product["storage"]["backend"].clone();
    let node = native["node"].as_object_mut().ok_or_else(invalid)?;
    for (key, value) in [
        (format!("{QBFT}node-id"), product["nodeId"].clone()),
        ("server.address".into(), product["http"]["address"].clone()),
        ("server.port".into(), product["http"]["port"].clone()),
        (
            format!("{QBFT}listen-host"),
            product["p2p"]["address"].clone(),
        ),
        (format!("{QBFT}listen-port"), product["p2p"]["port"].clone()),
    ] {
        node.insert(key, value);
    }
    for (field, suffix) in SECRET_MAPPING {
        node.insert(
            format!("{QBFT}{suffix}"),
            product["secrets"][*field].clone(),
        );
    }
    Ok(())
}

pub fn prepare(name: &str, inputs: &Inputs, operator_uid: u32) -> Result<Request> {
    if matches!(operator_uid, 0 | u32::MAX)
        || rustix::process::getuid().as_raw() != operator_uid
        || rustix::process::geteuid().as_raw() != operator_uid
    {
        return Err(invalid());
    }
    let control = control_path(name)?;
    let inputs = Inputs {
        runtime_root: files::absolute(&inputs.runtime_root)?,
        engine_jar: files::absolute(&inputs.engine_jar)?,
        product_config: files::absolute(&inputs.product_config)?,
        native_config: files::absolute(&inputs.native_config)?,
    };
    for path in [
        &inputs.runtime_root,
        &inputs.engine_jar,
        &inputs.product_config,
        &inputs.native_config,
    ] {
        boundary::safe_absolute(path)?;
    }
    let product_raw = source::read(&inputs.product_config, operator_uid, 262144)?;
    let native_raw = source::read(&inputs.native_config, operator_uid, 262144)?;
    let mut product: Value = json::decode(&crate::config::normalize_bytes(
        &product_raw,
        &inputs.product_config,
    )?)
    .map_err(|_| invalid())?;
    let mut imported = NativeInput::load(&inputs.native_config)?;
    if imported.config.raw != native_raw {
        return Err(install_error("LINUX_INSTALL_INPUT_CHANGED"));
    }
    let mut native = serde_json::to_value(&imported.value).map_err(|_| invalid())?;
    derive(name, &control, &mut product, &mut native)?;
    // Validate the derived copy with the same product/native contract consumed
    // by register; cold engine validation remains a separate privileged step.
    imported.value = json::decode(&encoded(&native)?).map_err(|_| invalid())?;
    imported.data = control.join("data");
    imported.chain = Input::read(
        Path::new(text(&product["chainDescription"])?),
        262144,
        false,
    )?;
    imported.references = reference_paths(&product, &native)?.into_iter().collect();
    super::super::product::check_document(&product, &inputs.product_config, &imported)?;
    let mut paths = reference_paths(&product, &native)?;
    paths.extend([
        inputs.product_config.clone(),
        inputs.native_config.clone(),
        inputs.engine_jar.clone(),
    ]);
    let mut pins = Vec::new();
    for path in paths {
        pins.push(source::pin(
            &path,
            operator_uid,
            if path == inputs.engine_jar {
                files::MAX_JAR
            } else {
                4 * 1024 * 1024
            },
        )?);
    }
    for (path, raw) in [
        (&inputs.product_config, &product_raw),
        (&inputs.native_config, &native_raw),
    ] {
        if !pins.iter().any(|p| {
            &p.path == path && p.sha256 == files::digest(raw) && p.size == raw.len() as u64
        }) {
            return Err(install_error("LINUX_INSTALL_INPUT_CHANGED"));
        }
    }
    let engine = lock()?;
    if !pins.iter().any(|p| {
        p.path == inputs.engine_jar
            && p.sha256 == engine.jar_sha256
            && p.size == engine.jar_size_bytes
    }) {
        return Err(install_error("LINUX_INSTALL_SUPPLY_MISMATCH"));
    }
    source::verify_runtime(&inputs.runtime_root, operator_uid)?;
    let request = Request {
        schema_version: 1,
        name: name.into(),
        control,
        operator_uid,
        inputs,
        pins,
        product,
        native,
        runtime_lock_sha256: files::digest(RUNTIME_LOCK),
        engine_lock_sha256: files::digest(ENGINE_LOCK),
    };
    validate(&request)?;
    Ok(request)
}
fn contract(product: &Value, native: &Value, source: &Path) -> Result<()> {
    let normalized: Value =
        json::decode(&crate::config::normalize_bytes(&encoded(product)?, source)?)
            .map_err(|_| invalid())?;
    if &normalized != product {
        return Err(invalid());
    }
    let _: super::super::files::NativeConfig =
        json::decode(&encoded(native)?).map_err(|_| invalid())?;
    let node = native["node"].as_object().ok_or_else(invalid)?;
    if product["role"] != "validator"
        || native["chainFile"] != product["chainDescription"]
        || native["backend"] != product["storage"]["backend"]
        || native["dataDirectory"] != product["storage"]["dataDirectory"]
        || node.get(&format!("{QBFT}role")) != Some(&Value::String("VALIDATOR".into()))
        || node.get(&format!("{QBFT}transport-security-scheme"))
            != Some(&Value::String("MTLS".into()))
        || node
            .values()
            .any(|v| !(v.is_string() || v.is_boolean() || v.is_number()))
    {
        return Err(invalid());
    }
    for (key, value) in [
        (format!("{QBFT}node-id"), &product["nodeId"]),
        ("server.address".into(), &product["http"]["address"]),
        ("server.port".into(), &product["http"]["port"]),
        (format!("{QBFT}listen-host"), &product["p2p"]["address"]),
        (format!("{QBFT}listen-port"), &product["p2p"]["port"]),
    ] {
        if node.get(&key) != Some(value) {
            return Err(invalid());
        }
    }
    for (field, suffix) in SECRET_MAPPING {
        if node.get(&format!("{QBFT}{suffix}")) != Some(&product["secrets"][*field]) {
            return Err(invalid());
        }
    }
    Ok(())
}
pub fn validate(request: &Request) -> Result<()> {
    if encoded(request)?.len() as u64 > REQUEST_LIMIT {
        return Err(install_error("LINUX_INSTALL_REQUEST_TOO_LARGE"));
    }
    contract(
        &request.product,
        &request.native,
        &request.inputs.product_config,
    )?;
    if request.schema_version != 1
        || request.control != control_path(&request.name)?
        || matches!(request.operator_uid, 0 | u32::MAX)
        || request.runtime_lock_sha256 != files::digest(RUNTIME_LOCK)
        || request.engine_lock_sha256 != files::digest(ENGINE_LOCK)
        || request.product["instanceId"] != request.name
        || request.product["storage"]["dataDirectory"]
            != request.control.join("data").to_string_lossy().as_ref()
        || request.native["dataDirectory"] != request.product["storage"]["dataDirectory"]
        || request.product["storage"]["backend"] != "rocksdb"
        || request.native["backend"] != "rocksdb"
    {
        return Err(invalid());
    }
    for path in [
        &request.inputs.runtime_root,
        &request.inputs.engine_jar,
        &request.inputs.product_config,
        &request.inputs.native_config,
    ] {
        boundary::safe_absolute(path)?;
        if path.starts_with(&request.control) || request.control.starts_with(path) {
            return Err(invalid());
        }
    }
    let mut expected = reference_paths(&request.product, &request.native)?;
    expected.extend([
        request.inputs.product_config.clone(),
        request.inputs.native_config.clone(),
        request.inputs.engine_jar.clone(),
    ]);
    if request.pins.len() != expected.len() || request.pins.len() > 40 {
        return Err(invalid());
    }
    for pin in &request.pins {
        if !expected.remove(&pin.path)
            || !super::super::hash(&pin.sha256)
            || pin.size == 0
            || pin.size
                > if pin.path == request.inputs.engine_jar {
                    files::MAX_JAR
                } else {
                    4 * 1024 * 1024
                }
        {
            return Err(invalid());
        }
        boundary::safe_absolute(&pin.path)?;
        if pin.path.starts_with(&request.control) || request.control.starts_with(&pin.path) {
            return Err(invalid());
        }
    }
    let engine = lock()?;
    if !request.pins.iter().any(|p| {
        p.path == request.inputs.engine_jar
            && p.sha256 == engine.jar_sha256
            && p.size == engine.jar_size_bytes
    }) {
        return Err(invalid());
    }
    if encoded(request)?.len() as u64 > REQUEST_LIMIT {
        return Err(invalid());
    }
    Ok(())
}
pub fn digest(request: &Request) -> Result<String> {
    validate(request)?;
    Ok(files::digest(&encoded(request)?))
}

/// Read a private immutable user request without following links.
pub fn read_request_user(path: &Path, operator_uid: u32) -> Result<Vec<u8>> {
    if matches!(operator_uid, 0 | u32::MAX) {
        return Err(invalid());
    }
    source::read_request(path, operator_uid)
}
pub fn recheck(request: &Request) -> Result<()> {
    validate(request)?;
    for pin in &request.pins {
        source::verify(pin, request.operator_uid)?;
    }
    source::verify_runtime(&request.inputs.runtime_root, request.operator_uid)
}
fn trusted_executable(path: &Path) -> Result<()> {
    boundary::trusted(path).map_err(|_| install_error("LINUX_INSTALL_HELPER_REQUIRED"))?;
    let m =
        fs::symlink_metadata(path).map_err(|_| install_error("LINUX_INSTALL_HELPER_REQUIRED"))?;
    if !m.is_file()
        || m.uid() != 0
        || m.nlink() != 1
        || m.mode() & 0o7022 != 0
        || m.mode() & 0o111 == 0
    {
        return Err(install_error("LINUX_INSTALL_HELPER_REQUIRED"));
    }
    Ok(())
}
pub fn check_installer() -> Result<()> {
    trusted_executable(Path::new(HELPER))
}
fn request_from_file(path: &Path, sha: &str, uid: u32) -> Result<Request> {
    if !super::super::hash(sha) {
        return Err(invalid());
    }
    let raw = read_request_user(path, uid)?;
    if files::digest(&raw) != sha {
        return Err(install_error("LINUX_INSTALL_INPUT_CHANGED"));
    }
    let request: Request = json::decode(&raw).map_err(|_| invalid())?;
    validate(&request)?;
    if request.operator_uid != uid || digest(&request)? != sha {
        return Err(invalid());
    }
    Ok(request)
}
/// Bounded helper/tool collection. Timeout preserves the live child and its
/// inherited transaction lock; no signal is sent to an account/install mutation.
fn capture_command(mut command: Command, timeout: Duration) -> Result<(i32, Vec<u8>)> {
    use std::{
        io::ErrorKind,
        os::{fd::OwnedFd, unix::net::UnixStream},
        sync::{Arc, atomic::AtomicBool},
        thread,
    };
    if timeout.is_zero() || timeout > Duration::from_secs(2640) {
        return Err(invalid());
    }
    let streams = || -> std::io::Result<(UnixStream, Stdio)> {
        let (read, write) = UnixStream::pair()?;
        read.set_read_timeout(Some(Duration::from_millis(50)))?;
        Ok((read, Stdio::from(OwnedFd::from(write))))
    };
    let (out_reader, out) = streams().map_err(|_| install_error("LINUX_INSTALL_UNKNOWN"))?;
    let (err_reader, err) = streams().map_err(|_| install_error("LINUX_INSTALL_UNKNOWN"))?;
    command.stdout(out).stderr(err);
    let mut child = command
        .spawn()
        .map_err(|_| install_error("LINUX_INSTALL_UNKNOWN"))?;
    drop(command);
    let done = Arc::new(AtomicBool::new(false));
    let overflow = Arc::new(AtomicBool::new(false));
    let readers = [out_reader, err_reader]
        .into_iter()
        .map(|mut stream| {
            let done = done.clone();
            let overflow = overflow.clone();
            thread::spawn(move || {
                let mut raw = Vec::new();
                let mut buffer = [0u8; 4096];
                let mut draining = None;
                loop {
                    if done.load(Ordering::Acquire)
                        && draining.get_or_insert_with(Instant::now).elapsed()
                            > Duration::from_millis(200)
                    {
                        return Err(ErrorKind::TimedOut.into());
                    }
                    match stream.read(&mut buffer) {
                        Ok(0) => return Ok::<_, std::io::Error>(raw),
                        Ok(n) if raw.len() + n <= REQUEST_LIMIT as usize => {
                            raw.extend_from_slice(&buffer[..n])
                        }
                        Ok(_) => {
                            overflow.store(true, Ordering::Release);
                        }
                        Err(e)
                            if matches!(
                                e.kind(),
                                ErrorKind::WouldBlock
                                    | ErrorKind::TimedOut
                                    | ErrorKind::Interrupted
                            ) => {}
                        Err(e) => return Err(e),
                    }
                }
            })
        })
        .collect::<Vec<_>>();
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Err(_) => break None,
            Ok(None) => {}
        }
        if started.elapsed() >= timeout || overflow.load(Ordering::Acquire) {
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    if status.is_none() {
        thread::spawn(move || {
            let _ = child.wait();
        });
    }
    done.store(true, Ordering::Release);
    let mut captured = Vec::new();
    for reader in readers {
        captured.push(
            reader
                .join()
                .map_err(|_| install_error("LINUX_INSTALL_UNKNOWN"))?
                .map_err(|_| install_error("LINUX_INSTALL_UNKNOWN"))?,
        );
    }
    if overflow.load(Ordering::Acquire) {
        return Err(install_error("LINUX_INSTALL_UNKNOWN"));
    }
    let code = status
        .and_then(|s| s.code())
        .ok_or_else(|| install_error("LINUX_INSTALL_UNKNOWN"))?;
    Ok((code, captured.remove(0)))
}
pub fn invoke(
    request_path: &Path,
    sha: &str,
    operator_uid: u32,
    timeout: Duration,
) -> Result<Receipt> {
    duration(timeout)?;
    if !cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        return Err(error("LINUX_SERVICE_PLATFORM_UNSUPPORTED"));
    }
    if rustix::process::getuid().as_raw() != operator_uid
        || rustix::process::geteuid().as_raw() != operator_uid
    {
        return Err(invalid());
    }
    let request = request_from_file(request_path, sha, operator_uid)?;
    check_installer()?;
    let worker = Input::read(Path::new(HELPER), MAX_WORKER, true)?;
    boundary::trusted(Path::new("/usr/bin/sudo"))?;
    let mut command = Command::new("/usr/bin/sudo");
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .current_dir("/")
        .stdin(Stdio::null())
        .args(["-n", "--", HELPER, "linux-install", "--request"])
        .arg(request_path)
        .args([
            "--sha256",
            sha,
            "--operator-uid",
            &operator_uid.to_string(),
            "--timeout-seconds",
            &timeout.as_secs().to_string(),
            "--json",
        ]);
    let supervision = Duration::from_secs(timeout.as_secs() * 4 + 240);
    let (exit, raw) = capture_command(command, supervision)?;
    check_installer()?;
    worker.recheck()?;
    let envelope: Value = crate::artifact::decode_strict_json(&raw)
        .map_err(|_| install_error("LINUX_INSTALL_UNKNOWN"))?;
    if envelope["command"] != "linux-install" {
        return Err(install_error("LINUX_INSTALL_UNKNOWN"));
    }
    if exit != 0 || envelope["outcome"] != "SUCCEEDED" {
        let code = envelope["reasonCode"]
            .as_str()
            .filter(|s| s.starts_with("LINUX_INSTALL_"))
            .unwrap_or("LINUX_INSTALL_UNKNOWN");
        return Err(install_error(code));
    }
    let receipt: Receipt = serde_json::from_value(envelope["data"].clone())
        .map_err(|_| install_error("LINUX_INSTALL_UNKNOWN"))?;
    validate_receipt(&receipt, &request, sha)?;
    if super::management::inspect(&request.control)? != receipt.descriptor {
        return Err(install_error("LINUX_INSTALL_UNKNOWN"));
    }
    Ok(receipt)
}
fn validate_receipt(receipt: &Receipt, request: &Request, sha: &str) -> Result<()> {
    if receipt.schema_version != 1
        || receipt.name != request.name
        || receipt.control != request.control
        || receipt.request_sha256 != sha
        || receipt.operator_uid != request.operator_uid
        || matches!(receipt.service_uid, 0 | u32::MAX)
        || matches!(receipt.service_gid, 0 | u32::MAX)
        || !receipt.development_only
        || receipt.runtime_package_acceptance != "NOT_ACCEPTED"
        || receipt.descriptor.schema_version != 1
        || receipt.descriptor.protocol != super::management::PROTOCOL
        || receipt.descriptor.control_inode == 0
        || !super::super::hash(&receipt.descriptor.worker_sha256)
        || receipt.descriptor.worker_size == 0
        || receipt.descriptor.worker_size > MAX_WORKER
        || receipt.descriptor.control_directory != request.control
        || receipt.descriptor.instance_id != request.name
    {
        return Err(install_error("LINUX_INSTALL_UNKNOWN"));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum InstallPhase {
    SnapshotIntent,
    SnapshotsReady,
    AccountIntent,
    RegisterIntent,
    Reserved,
    Complete,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallState {
    schema_version: u32,
    request_sha256: String,
    phase: InstallPhase,
    account: accounts::State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    control_identity: Option<RootIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    receipt: Option<Receipt>,
}
fn save(store: &mut Store, state: &InstallState) -> Result<()> {
    store.save(&encoded(state)?)
}
fn ensure_directory(path: &Path, mode: u32) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) => {
            boundary::trusted(path)?;
            if !m.is_dir() || m.uid() != 0 || m.gid() != 0 || m.mode() & 0o7777 != mode {
                return Err(invalid());
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => boundary::mkdir(path, 0, 0, mode),
        Err(_) => Err(invalid()),
    }
}
fn global_lock() -> Result<File> {
    let path = Path::new(BASE).join("install.lock");
    if fs::symlink_metadata(&path).is_err() {
        boundary::write_new(&path, b"", 0, 0o600)?;
    }
    boundary::trusted(&path)?;
    let m = fs::symlink_metadata(&path).map_err(|_| invalid())?;
    if !m.is_file() || m.len() != 0 || m.mode() & 0o7777 != 0o600 {
        return Err(invalid());
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC).bits() as i32)
        .open(&path)
        .map_err(|_| invalid())?;
    rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .map_err(|_| install_error("LINUX_INSTALL_BUSY"))?;
    Ok(file)
}
fn derived_snapshots(request: &Request, tx: &Path) -> Result<(Value, Value)> {
    let mapping: BTreeMap<_, _> = request
        .pins
        .iter()
        .enumerate()
        .map(|(index, pin)| {
            (
                pin.path.clone(),
                tx.join("inputs").join(format!("source-{index:03}")),
            )
        })
        .collect();
    let mut product = request.product.clone();
    let mut native = request.native.clone();
    let rewrite = |value: &mut Value| -> Result<()> {
        let replacement = mapping.get(Path::new(text(value)?)).ok_or_else(invalid)?;
        *value = Value::String(replacement.to_string_lossy().into());
        Ok(())
    };
    rewrite(&mut product["chainDescription"])?;
    rewrite(&mut native["chainFile"])?;
    for (field, _) in SECRET_MAPPING {
        rewrite(&mut product["secrets"][*field])?;
    }
    for (key, value) in native["node"].as_object_mut().ok_or_else(invalid)? {
        if key.ends_with("-path") || key.ends_with("-file") {
            rewrite(value)?;
        }
    }
    Ok((product, native))
}
fn verify_snapshots(request: &Request, tx: &Path) -> Result<()> {
    for (index, pin) in request.pins.iter().enumerate() {
        source::verify(
            &Pin {
                path: tx.join("inputs").join(format!("source-{index:03}")),
                sha256: pin.sha256.clone(),
                size: pin.size,
            },
            0,
        )?;
    }
    source::verify_runtime(&tx.join("runtime"), 0)?;
    let (product, native) = derived_snapshots(request, tx)?;
    for (name, raw) in [
        ("product.json", encoded(&product)?),
        ("native.json", encoded(&native)?),
        ("engine-lock.json", ENGINE_LOCK.to_vec()),
        ("runtime-lock.json", RUNTIME_LOCK.to_vec()),
    ] {
        if boundary::root_read(&tx.join(name), REQUEST_LIMIT)? != raw {
            return Err(install_error("LINUX_INSTALL_PARTIAL"));
        }
    }
    let imported = NativeInput::load(&tx.join("native.json"))?;
    ProductInput::load(&tx.join("product.json"))?.check(&imported)?;
    Ok(())
}
fn snapshot(request: &Request, tx: &Path) -> Result<()> {
    let input = tx.join("inputs");
    boundary::mkdir(&input, 0, 0, 0o700)?;
    for (index, pin) in request.pins.iter().enumerate() {
        let path = input.join(format!("source-{index:03}"));
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC).bits() as i32,
            )
            .open(&path)
            .map_err(|_| invalid())?;
        source::copy(pin, request.operator_uid, &mut out)?;
        out.sync_all().map_err(|_| invalid())?;
    }
    source::snapshot_runtime(
        &request.inputs.runtime_root,
        request.operator_uid,
        &tx.join("runtime"),
    )?;
    let (product, native) = derived_snapshots(request, tx)?;
    boundary::write_new(&tx.join("product.json"), &encoded(&product)?, 0, 0o600)?;
    boundary::write_new(&tx.join("native.json"), &encoded(&native)?, 0, 0o600)?;
    boundary::write_new(&tx.join("engine-lock.json"), ENGINE_LOCK, 0, 0o600)?;
    boundary::write_new(&tx.join("runtime-lock.json"), RUNTIME_LOCK, 0, 0o600)?;
    // Complete schema/native/chain validation precedes account creation.
    let native = NativeInput::load(&tx.join("native.json"))?;
    let product = ProductInput::load(&tx.join("product.json"))?;
    product.check(&native)?;
    if crate::config::preflight(&tx.join("product.json"))?.outcome == "FAIL" {
        return Err(invalid());
    }
    File::open(tx)
        .and_then(|f| f.sync_all())
        .map_err(|_| invalid())?;
    Ok(())
}
fn spec(request: &Request, tx: &Path, uid: u32, gid: u32) -> Result<Spec> {
    let index = request
        .pins
        .iter()
        .position(|p| p.path == request.inputs.engine_jar)
        .ok_or_else(invalid)?;
    Ok(Spec {
        schema_version: 1,
        runtime_root: tx.join("runtime"),
        runtime_lock: tx.join("runtime-lock.json"),
        engine_jar: tx.join("inputs").join(format!("source-{index:03}")),
        engine_lock: tx.join("engine-lock.json"),
        native_config: tx.join("native.json"),
        product_config: tx.join("product.json"),
        service_uid: uid,
        service_gid: gid,
    })
}
fn completed_registration(request: &Request, state: &InstallState) -> Result<()> {
    let identity = state
        .control_identity
        .ok_or_else(|| install_error("LINUX_INSTALL_PARTIAL"))?;
    let m = fs::symlink_metadata(&request.control)
        .map_err(|_| install_error("LINUX_INSTALL_PARTIAL"))?;
    if (m.dev(), m.ino()) != (identity.device, identity.inode) {
        return Err(install_error("LINUX_INSTALL_PARTIAL"));
    }
    let (binding, _, _) =
        load(&request.control).map_err(|_| install_error("LINUX_INSTALL_PARTIAL"))?;
    if binding.instance_id != request.name
        || Some(binding.service_uid) != state.account.uid
        || Some(binding.service_gid) != state.account.gid
    {
        return Err(install_error("LINUX_INSTALL_PARTIAL"));
    }
    verify_assets(&binding)?;
    Ok(())
}
/// Only the administrator-installed fixed helper accepts this internal command.
/// Unknown/partial results preserve every account, source snapshot and control.
pub fn apply(
    request_path: &Path,
    sha: &str,
    operator_uid: u32,
    timeout: Duration,
) -> Result<Receipt> {
    boundary::root()?;
    duration(timeout)?;
    check_installer()?;
    if std::env::current_exe().map_err(|_| invalid())? != Path::new(HELPER) {
        return Err(install_error("LINUX_INSTALL_HELPER_REQUIRED"));
    }
    if let Some(sudo_uid) = std::env::var_os("SUDO_UID") {
        if sudo_uid.to_str().and_then(|s| s.parse::<u32>().ok()) != Some(operator_uid) {
            return Err(invalid());
        }
    }
    let request = request_from_file(request_path, sha, operator_uid)?;
    baseline()?;
    ensure_directory(Path::new(BASE), 0o755)?;
    ensure_directory(&Path::new(BASE).join("instances"), 0o755)?;
    ensure_directory(&Path::new(BASE).join("installations"), 0o700)?;
    let guard = global_lock()?;
    let tx = Path::new(BASE)
        .join("installations")
        .join(format!("{}-{}", request.name, &sha[..24]));
    let (mut journal, mut state) = if fs::symlink_metadata(&tx).is_ok() {
        boundary::private_directory(&tx, 0, 0)?;
        if boundary::root_read(&tx.join("request.json"), REQUEST_LIMIT)? != encoded(&request)? {
            return Err(install_error("LINUX_INSTALL_PARTIAL"));
        }
        let journal = Store::open(&tx.join("journal"))?;
        let state: InstallState =
            json::decode(&journal.read()?.ok_or_else(invalid)?).map_err(|_| invalid())?;
        if state.schema_version != 1 || state.request_sha256 != sha {
            return Err(invalid());
        }
        (journal, state)
    } else {
        if fs::symlink_metadata(&request.control).is_ok() {
            return Err(install_error("LINUX_INSTALL_PARTIAL"));
        }
        recheck(&request)?;
        boundary::mkdir(&tx, 0, 0, 0o700)?;
        boundary::write_new(&tx.join("request.json"), &encoded(&request)?, 0, 0o600)?;
        let mut journal = Store::create(&tx.join("journal"))?;
        let mut state = InstallState {
            schema_version: 1,
            request_sha256: sha.into(),
            phase: InstallPhase::SnapshotIntent,
            account: accounts::State::default(),
            control_identity: None,
            receipt: None,
        };
        save(&mut journal, &state)?;
        snapshot(&request, &tx)?;
        state.phase = InstallPhase::SnapshotsReady;
        save(&mut journal, &state)?;
        (journal, state)
    };
    journal.require_capacity(12)?;
    if state.phase == InstallPhase::SnapshotIntent {
        return Err(install_error("LINUX_INSTALL_PARTIAL"));
    }
    if state.phase == InstallPhase::Complete {
        completed_registration(&request, &state)?;
        let receipt = state.receipt.ok_or_else(invalid)?;
        validate_receipt(&receipt, &request, sha)?;
        if super::management::inspect(&request.control)? != receipt.descriptor {
            return Err(install_error("LINUX_INSTALL_PARTIAL"));
        }
        return Ok(receipt);
    }
    if state.control_identity.is_none() {
        verify_snapshots(&request, &tx)?;
        if state.phase == InstallPhase::Reserved || fs::symlink_metadata(&request.control).is_ok() {
            return Err(install_error("LINUX_INSTALL_PARTIAL"));
        }
        state.phase = InstallPhase::AccountIntent;
        save(&mut journal, &state)?;
        let mut account = state.account.clone();
        let (uid, gid) =
            accounts::provision(&request.control, &mut account, &guard, timeout, |account| {
                state.account = account.clone();
                save(&mut journal, &state)
            })?;
        state.account = account;
        let spec = spec(&request, &tx, uid, gid)?;
        let spec_path = tx.join("spec.json");
        let raw = encoded(&spec)?;
        if fs::symlink_metadata(&spec_path).is_ok() {
            if boundary::root_read(&spec_path, 65536)? != raw {
                return Err(install_error("LINUX_INSTALL_PARTIAL"));
            }
        } else {
            boundary::write_new(&spec_path, &raw, 0, 0o600)?;
        }
        state.phase = InstallPhase::RegisterIntent;
        save(&mut journal, &state)?;
        register_owned(&request.control, &spec_path, timeout, |identity| {
            state.control_identity = Some(identity);
            state.phase = InstallPhase::Reserved;
            save(&mut journal, &state)
        })?;
    }
    completed_registration(&request, &state)?;
    let descriptor = super::management::publish(&request.control)?;
    let receipt = Receipt {
        schema_version: 1,
        name: request.name.clone(),
        control: request.control.clone(),
        request_sha256: sha.into(),
        operator_uid,
        descriptor,
        service_uid: state.account.uid.ok_or_else(invalid)?,
        service_gid: state.account.gid.ok_or_else(invalid)?,
        development_only: true,
        runtime_package_acceptance: "NOT_ACCEPTED".into(),
    };
    validate_receipt(&receipt, &request, sha)?;
    state.receipt = Some(receipt.clone());
    state.phase = InstallPhase::Complete;
    save(&mut journal, &state)?;
    Ok(receipt)
}
