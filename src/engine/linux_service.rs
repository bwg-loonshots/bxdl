//! Explicit development Linux service backend. Root protects authority; every
//! JVM executes as an instance-specific non-login UID. This is not admission of
//! a frozen Linux runtime package. Setup/catalog delegation uses the separate
//! root-admitted development installer and executor modules.
mod boundary;
pub mod diagnostics;
mod exit;
pub mod install;
pub mod management;
pub use exit::record_exit;
pub(crate) mod runner;
use super::{
    Identity, Lock,
    files::{self, Input, NativeInput},
    init_result, json,
    product::ProductInput,
    runtime_http, runtime_result,
    systemd::{
        self,
        control::{StopAuthorization, Unit},
    },
};
use crate::{
    error::{BxdlError, Result},
    runtime::{RuntimeLock, StageOptions},
    setup::store::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    net::SocketAddr,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const MAX_WORKER: u64 = 64 * 1024 * 1024;
const ENGINE_LOCK: &[u8] =
    include_bytes!("../../packaging/engine-lock.rc4-temurin-linux-development.json");
const RUNTIME_LOCK: &[u8] =
    include_bytes!("../../packaging/runtime-lock.temurin-21-linux-amd64.json");
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spec {
    pub schema_version: u32,
    pub runtime_root: PathBuf,
    pub runtime_lock: PathBuf,
    pub engine_jar: PathBuf,
    pub engine_lock: PathBuf,
    pub native_config: PathBuf,
    pub product_config: PathBuf,
    pub service_uid: u32,
    pub service_gid: u32,
}
/// Pure admission checks run before creating any root control files.
fn validate_spec(spec: &Spec) -> Result<()> {
    if spec.schema_version != 1
        || matches!(spec.service_uid, 0 | u32::MAX)
        || matches!(spec.service_gid, 0 | u32::MAX)
    {
        return Err(error("LINUX_SERVICE_SPEC_INVALID"));
    }
    for path in [
        &spec.runtime_root,
        &spec.runtime_lock,
        &spec.engine_jar,
        &spec.engine_lock,
        &spec.native_config,
        &spec.product_config,
    ] {
        boundary::safe_absolute(path)?;
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Pin {
    path: PathBuf,
    sha256: String,
    size: u64,
    mode: u32,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    schema_version: u32,
    kind: String,
    control: PathBuf,
    device: u64,
    inode: u64,
    instance_id: String,
    service_uid: u32,
    service_gid: u32,
    pins: Vec<Pin>,
    identity: Identity,
    chain_fingerprint: String,
    node_identity: String,
    endpoint: SocketAddr,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Phase {
    Registered,
    InitIntent,
    Unknown,
    Initialized,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RunPhase {
    Prepared,
    Consumed,
    Refused,
    StoppedVerified,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Run {
    attempt: String,
    phase: RunPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    invocation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    control_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start_ticks: Option<u64>,
    reason: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct State {
    schema_version: u32,
    binding_sha256: String,
    phase: Phase,
    reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    init_attempt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    genesis_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data_identity: Option<(u64, u64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    run: Option<Run>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub instance_id: String,
    pub outcome: String,
    pub service_state: String,
    pub engine_state: String,
    pub runtime_readiness: String,
    pub reason: String,
    pub initialization: String,
    pub operation_busy: bool,
    pub development_only: bool,
    pub service_profile: &'static str,
    pub automatic_restart: bool,
    pub boot_auto_start: bool,
    pub runtime_package_acceptance: &'static str,
    pub global_consensus: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<Value>,
}
fn error(code: &str) -> BxdlError {
    BxdlError::new(
        code,
        match code {
            "LINUX_SERVICE_ROOT_REQUIRED" => {
                "Linux 개발 서비스 제어는 명시적인 sudo/root 권한이 필요합니다."
            }
            "LINUX_SERVICE_PLATFORM_UNSUPPORTED" => {
                "Linux GNU x86_64 시스템 서비스에서만 실행할 수 있습니다."
            }
            "LINUX_SERVICE_ACCOUNT_INVALID" => {
                "인스턴스 전용 비로그인 계정·그룹과 UID/GID를 확인하세요."
            }
            "LINUX_SERVICE_BUSY" => "초기화 또는 서비스 실행이 제어 잠금을 보유하고 있습니다.",
            "LINUX_SERVICE_GATE_REPLAY" => {
                "이미 소비했거나 일치하지 않는 시작 요청입니다. 자동으로 재시도하지 않았습니다."
            }
            "LINUX_SERVICE_UNRESOLVED" => {
                "이전 시도 종료가 확정되지 않았습니다. 자료를 보존하고 status/stop으로 확인하세요."
            }
            "LINUX_SERVICE_NOT_INITIALIZED" => {
                "검증된 초기화 완료 기록이 필요합니다. 명시적으로 init 또는 resume-init을 실행하세요."
            }
            _ => "Linux 서비스 증거가 없거나 검증되지 않았습니다. 기존 자료를 보존했습니다.",
        },
    )
}
fn unknown() -> BxdlError {
    error("LINUX_SERVICE_UNKNOWN")
}
fn encoded(value: &impl Serialize) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|_| unknown())
}
fn duration(timeout: Duration) -> Result<()> {
    if timeout.is_zero() || timeout > Duration::from_secs(600) {
        Err(error("LINUX_SERVICE_OPTIONS_INVALID"))
    } else {
        Ok(())
    }
}
fn fresh_attempt() -> Result<String> {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| unknown())?
        .as_nanos();
    Ok(format!(
        "lnx-{n:x}-{:x}",
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}
fn valid_attempt(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
pub fn service_account(instance: &Path) -> Result<String> {
    boundary::safe_absolute(instance)?;
    Unit::new(
        instance,
        "validation",
        &instance.join("attempts/validation/bxdl-worker"),
    )?;
    Ok(format!(
        "bxdl-{}",
        &files::digest(instance.as_os_str().as_encoded_bytes())[..16]
    ))
}
fn unit(b: &Binding, r: &Run) -> Result<Unit> {
    Unit::new(
        &b.control,
        &r.attempt,
        &b.control
            .join("attempts")
            .join(&r.attempt)
            .join("bxdl-worker"),
    )
}
fn output(b: &Binding, attempt: &str) -> PathBuf {
    b.control.join("attempts").join(attempt).join("output")
}
fn java(b: &Binding) -> PathBuf {
    b.control.join("supply/runtime/bin/java")
}
fn jar(b: &Binding) -> PathBuf {
    b.control.join("engine.jar")
}
fn config(b: &Binding) -> PathBuf {
    b.control.join("config/node.json")
}
fn data(b: &Binding) -> PathBuf {
    b.control.join("data")
}
fn lock() -> Result<Lock> {
    let lock: Lock = json::decode(ENGINE_LOCK).map_err(|_| unknown())?;
    super::validate_lock(&lock)?;
    Ok(lock)
}
fn runtime_lock() -> Result<RuntimeLock> {
    json::decode(RUNTIME_LOCK).map_err(|_| unknown())
}
fn save(store: &mut Store, state: &State) -> Result<()> {
    store.save(&encoded(state)?).map_err(|_| unknown())
}
fn read_binding(instance: &Path) -> Result<(Binding, Vec<u8>)> {
    boundary::root()?;
    boundary::trusted(instance)?;
    let raw = boundary::root_read(&instance.join("binding.json"), 262144)?;
    let b: Binding = json::decode(&raw).map_err(|_| unknown())?;
    let m = fs::symlink_metadata(instance).map_err(|_| unknown())?;
    if b.schema_version != 1
        || b.kind != "bxdl-linux-development-service"
        || b.control != instance
        || (b.device, b.inode) != (m.dev(), m.ino())
        || m.mode() & 0o7777 != 0o755
        || b.service_uid == 0
        || b.service_gid == 0
        || b.identity != lock()?.expected
        || !b.endpoint.ip().is_loopback()
        || b.endpoint.port() == 0
        || b.pins.is_empty()
    {
        return Err(unknown());
    }
    boundary::account(instance, b.service_uid, b.service_gid)?;
    Ok((b, raw))
}
fn load(instance: &Path) -> Result<(Binding, Store, State)> {
    let (b, raw) = read_binding(instance)?;
    boundary::trusted(&instance.join("journal"))?;
    for entry in fs::read_dir(instance.join("journal")).map_err(|_| unknown())? {
        boundary::trusted(&entry.map_err(|_| unknown())?.path())?;
    }
    let store = Store::open(&instance.join("journal"))?;
    let state: State = json::decode(&store.read()?.ok_or_else(unknown)?).map_err(|_| unknown())?;
    validate_state(&state, &files::digest(&raw))?;
    Ok((b, store, state))
}
fn validate_state(state: &State, digest: &str) -> Result<()> {
    if state.schema_version != 1
        || state.binding_sha256 != digest
        || (state.phase == Phase::Initialized
            && (state.genesis_hash.is_none() || state.data_identity.is_none()))
        || state
            .init_attempt
            .as_ref()
            .is_some_and(|s| !valid_attempt(s))
    {
        return Err(unknown());
    }
    if let Some(r) = &state.run {
        if !valid_attempt(&r.attempt)
            || (r.phase == RunPhase::Prepared) != (r.pid.is_none())
            || r.pid == Some(0)
            || (r.pid.is_some()
                && (r.invocation_id.as_ref().is_none_or(|s| {
                    s.len() != 32
                        || !s.bytes().all(|b| b.is_ascii_hexdigit())
                        || s.bytes().all(|b| b == b'0')
                }) || r.control_group.is_none()
                    || r.start_ticks.is_none()))
        {
            return Err(unknown());
        }
    }
    Ok(())
}
fn verify_assets(b: &Binding) -> Result<()> {
    let mut paths = BTreeSet::new();
    for p in &b.pins {
        if !p.path.starts_with(&b.control) || !paths.insert(p.path.clone()) {
            return Err(unknown());
        }
        boundary::trusted(&p.path)?;
        let m = fs::symlink_metadata(&p.path).map_err(|_| unknown())?;
        if m.mode() & 0o7777 != p.mode || m.len() != p.size {
            return Err(unknown());
        }
        files::Binary::open_data(&p.path, &p.sha256)?.recheck()?;
    }
    // The inventory and admission lock bytes remain independently fixed.
    if boundary::root_read(&b.control.join("config/engine-lock.json"), 65536)? != ENGINE_LOCK
        || boundary::root_read(&b.control.join("config/runtime-lock.json"), 262144)? != RUNTIME_LOCK
    {
        return Err(unknown());
    }
    let runtime = runtime_lock()?;
    for entry in runtime.files {
        if !b.pins.iter().any(|p| {
            p.path == b.control.join("supply/runtime").join(&entry.path)
                && p.sha256 == entry.sha256
                && p.size == entry.size
                && p.mode == entry.mode
        }) {
            return Err(unknown());
        }
    }
    Ok(())
}
fn pin(path: &Path, maximum: u64) -> Result<Pin> {
    boundary::trusted(path)?;
    let input = Input::read(path, maximum, false)?;
    let m = fs::symlink_metadata(path).map_err(|_| unknown())?;
    Ok(Pin {
        path: path.into(),
        sha256: files::digest(&input.raw),
        size: m.len(),
        mode: m.mode() & 0o7777,
    })
}
fn readable_runtime(root: &Path) -> Result<()> {
    boundary::trusted(root)?;
    for entry in fs::read_dir(root).map_err(|_| unknown())? {
        let path = entry.map_err(|_| unknown())?.path();
        let m = fs::symlink_metadata(&path).map_err(|_| unknown())?;
        if m.is_dir() {
            readable_runtime(&path)?;
        } else {
            boundary::trusted(&path)?;
        }
    }
    fs::set_permissions(root, fs::Permissions::from_mode(0o755)).map_err(|_| unknown())?;
    boundary::sync(root)
}
fn new_attempt(b: &Binding, attempt: &str, worker: bool) -> Result<()> {
    let path = b.control.join("attempts").join(attempt);
    boundary::mkdir(&path, 0, 0, 0o755)?;
    if worker {
        let raw = boundary::root_read(&b.control.join("worker"), MAX_WORKER)?;
        boundary::write_new(&path.join("bxdl-worker"), &raw, 0, 0o755)?;
    }
    boundary::mkdir(&path.join("output"), b.service_uid, b.service_gid, 0o700)
}
fn cold(
    b: &Binding,
    work: &Path,
    timeout: Duration,
    operation_lock: Option<&File>,
) -> Result<super::ColdResult> {
    verify_assets(b)?;
    let j = java(b);
    let a = jar(b);
    let c = config(b);
    let invoke = |command, configuration| {
        runner::run(runner::Request {
            java: &j,
            jar: &a,
            command,
            config: configuration,
            report: None,
            attempt: None,
            work,
            uid: b.service_uid,
            gid: b.service_gid,
            timeout,
            lock: operation_lock,
        })
    };
    let identity = invoke("engine-info", None)?;
    if identity.exit != 0
        || json::decode::<Identity>(&identity.stdout).map_err(|_| unknown())? != b.identity
    {
        return Err(error("ENGINE_IDENTITY_MISMATCH"));
    }
    let native = NativeInput::load(&c)?;
    let product = ProductInput::load(&b.control.join("config/product.json"))?;
    product.check(&native)?;
    let response = invoke("preflight", Some(c.as_path()))?;
    let cold = super::consume_cold(response.exit, &response.stdout)?;
    product.check_result(&cold)?;
    if cold.backend != "rocksdb"
        || (!b.chain_fingerprint.is_empty()
            && (b.chain_fingerprint != cold.chain_fingerprint
                || b.node_identity != cold.node_identity))
    {
        return Err(unknown());
    }
    native.recheck()?;
    product.recheck()?;
    verify_assets(b)?;
    Ok(cold)
}

pub fn register(instance: &Path, spec: &Path, timeout: Duration) -> Result<Summary> {
    register_owned(instance, spec, timeout, |_| Ok(()))
}

/// Record exclusive ownership before any persistent payload is written.
pub(crate) fn register_owned(
    instance: &Path,
    spec: &Path,
    timeout: Duration,
    mut on_reserved: impl FnMut(crate::install::RootIdentity) -> Result<()>,
) -> Result<Summary> {
    boundary::root()?;
    duration(timeout)?;
    boundary::safe_absolute(instance)?;
    boundary::trusted(instance.parent().ok_or_else(unknown)?)?;
    if fs::symlink_metadata(instance).is_ok() {
        return Err(error("LINUX_SERVICE_PATH_EXISTS"));
    }
    baseline()?;
    let request: Spec = json::decode(&boundary::root_read(spec, 65536)?).map_err(|_| unknown())?;
    validate_spec(&request)?;
    boundary::account(instance, request.service_uid, request.service_gid)?;
    let engine_input = Input::read(&request.engine_lock, 65536, false)?;
    let runtime_input = Input::read(&request.runtime_lock, 262144, false)?;
    if engine_input.raw != ENGINE_LOCK || runtime_input.raw != RUNTIME_LOCK {
        return Err(error("LINUX_SERVICE_SUPPLY_MISMATCH"));
    }
    let native = NativeInput::load(&request.native_config)?;
    let product = ProductInput::load(&request.product_config)?;
    product.check(&native)?;
    if native.data != instance.join("data") || native.value.backend != "rocksdb" {
        return Err(error("LINUX_SERVICE_SPEC_INVALID"));
    }
    let local = crate::config::preflight(&request.product_config)?;
    if local.outcome == "FAIL" {
        return Err(error("LINUX_SERVICE_SPEC_INVALID"));
    }
    let mut product_json: Value = json::decode(&crate::config::normalize_bytes(
        &Input::read(&request.product_config, 262144, false)?.raw,
        &request.product_config,
    )?)
    .map_err(|_| unknown())?;
    let endpoint = endpoint(&native)?;
    let current = std::env::current_exe().map_err(|_| unknown())?;
    boundary::trusted(&current)?;
    let worker = Input::read(&current, MAX_WORKER, true)?;
    boundary::mkdir(instance, 0, 0, 0o755)?;
    let reserved = fs::symlink_metadata(instance).map_err(|_| unknown())?;
    on_reserved(crate::install::RootIdentity {
        device: reserved.dev(),
        inode: reserved.ino(),
    })?;
    boundary::write_new(&instance.join("operation.lock"), b"", 0, 0o600)?;
    let guard = boundary::operation_lock(instance)?;
    boundary::mkdir(&instance.join("config"), 0, request.service_gid, 0o750)?;
    boundary::mkdir(&instance.join("attempts"), 0, 0, 0o755)?;
    boundary::write_new(&instance.join("worker"), &worker.raw, 0, 0o755)?;
    boundary::write_new(
        &instance.join("config/engine-lock.json"),
        ENGINE_LOCK,
        request.service_gid,
        0o640,
    )?;
    boundary::write_new(
        &instance.join("config/runtime-lock.json"),
        RUNTIME_LOCK,
        request.service_gid,
        0o640,
    )?;
    crate::runtime::stage(&StageOptions {
        source: request.runtime_root.clone(),
        lock: request.runtime_lock.clone(),
        output: instance.join("supply"),
    })?;
    readable_runtime(&instance.join("supply"))?;
    let engine = lock()?;
    let jar_path = instance.join("engine.jar");
    let mut jar_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&jar_path)
        .map_err(|_| unknown())?;
    files::copy_pinned(
        &request.engine_jar,
        &engine.jar_sha256,
        engine.jar_size_bytes,
        &mut jar_file,
    )?;
    if jar_file.metadata().map_err(|_| unknown())?.len() != engine.jar_size_bytes {
        return Err(unknown());
    }
    jar_file
        .set_permissions(fs::Permissions::from_mode(0o644))
        .map_err(|_| unknown())?;
    jar_file.sync_all().map_err(|_| unknown())?;
    drop(jar_file);
    let chain_path = instance.join("config/chain.json");
    boundary::write_new(&chain_path, &native.chain.raw, request.service_gid, 0o640)?;
    let mut replacements = BTreeMap::new();
    for source in &native.references {
        if replacements.contains_key(source) {
            continue;
        }
        let input = Input::read(source, 4 * 1024 * 1024, false)?;
        let target = instance
            .join("config")
            .join(format!("secret-{}", replacements.len()));
        boundary::write_new(&target, &input.raw, request.service_gid, 0o640)?;
        input.recheck()?;
        replacements.insert(source.clone(), target);
    }
    let mut native_json = serde_json::to_value(&native.value).map_err(|_| unknown())?;
    native_json["chainFile"] = Value::String(chain_path.to_string_lossy().into());
    for value in native_json["node"]
        .as_object_mut()
        .ok_or_else(unknown)?
        .values_mut()
    {
        if let Some(replacement) = value.as_str().and_then(|s| replacements.get(Path::new(s))) {
            *value = Value::String(replacement.to_string_lossy().into());
        }
    }
    product_json["chainDescription"] = Value::String(chain_path.to_string_lossy().into());
    for value in product_json["secrets"]
        .as_object_mut()
        .ok_or_else(unknown)?
        .values_mut()
    {
        if let Some(replacement) = value.as_str().and_then(|s| replacements.get(Path::new(s))) {
            *value = Value::String(replacement.to_string_lossy().into());
        }
    }
    boundary::write_new(
        &instance.join("config/node.json"),
        &encoded(&native_json)?,
        request.service_gid,
        0o640,
    )?;
    boundary::write_new(
        &instance.join("config/product.json"),
        &encoded(&product_json)?,
        request.service_gid,
        0o640,
    )?;
    native.recheck()?;
    product.recheck()?;
    worker.recheck()?;
    engine_input.recheck()?;
    runtime_input.recheck()?;
    let mut pins = vec![
        pin(&instance.join("worker"), MAX_WORKER)?,
        pin(&jar_path, files::MAX_JAR)?,
    ];
    for entry in fs::read_dir(instance.join("config")).map_err(|_| unknown())? {
        pins.push(pin(&entry.map_err(|_| unknown())?.path(), 4 * 1024 * 1024)?);
    }
    for entry in runtime_lock()?.files {
        pins.push(pin(
            &instance.join("supply/runtime").join(entry.path),
            files::MAX_JAR,
        )?);
    }
    let m = fs::symlink_metadata(instance).map_err(|_| unknown())?;
    let mut binding = Binding {
        schema_version: 1,
        kind: "bxdl-linux-development-service".into(),
        control: instance.into(),
        device: m.dev(),
        inode: m.ino(),
        instance_id: local.instance_id,
        service_uid: request.service_uid,
        service_gid: request.service_gid,
        pins,
        identity: engine.expected,
        chain_fingerprint: String::new(),
        node_identity: String::new(),
        endpoint,
    };
    let attempt = fresh_attempt()?;
    new_attempt(&binding, &attempt, false)?;
    let cold = cold(
        &binding,
        &output(&binding, &attempt),
        timeout.min(Duration::from_secs(120)),
        Some(&guard),
    )?;
    binding.chain_fingerprint = cold.chain_fingerprint;
    binding.node_identity = cold.node_identity;
    let raw = encoded(&binding)?;
    boundary::write_new(&instance.join("binding.json"), &raw, 0, 0o600)?;
    let mut store = Store::create(&instance.join("journal"))?;
    let state = State {
        schema_version: 1,
        binding_sha256: files::digest(&raw),
        phase: Phase::Registered,
        reason: "LINUX_SERVICE_REGISTERED".into(),
        init_attempt: None,
        genesis_hash: None,
        data_identity: None,
        run: None,
    };
    save(&mut store, &state)?;
    let mut result = summary(&binding, &state, false);
    result.outcome = "SUCCEEDED".into();
    Ok(result)
}
fn endpoint(native: &NativeInput) -> Result<SocketAddr> {
    let text = |name: &str| -> Result<String> {
        let v = native.value.node.get(name).ok_or_else(unknown)?;
        if let Some(s) = v.as_str() {
            Ok(s.into())
        } else if v.is_number() {
            Ok(v.to_string())
        } else {
            Err(unknown())
        }
    };
    let address = SocketAddr::new(
        text("server.address")?.parse().map_err(|_| unknown())?,
        text("server.port")?.parse().map_err(|_| unknown())?,
    );
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err(unknown());
    }
    Ok(address)
}
fn baseline() -> Result<()> {
    if systemd::inspect_host(Duration::from_secs(5))?.outcome != "SUCCEEDED" {
        return Err(error("LINUX_SERVICE_HOST_BASELINE_REQUIRED"));
    }
    Ok(())
}
fn summary(b: &Binding, s: &State, busy: bool) -> Summary {
    Summary {
        instance_id: b.instance_id.clone(),
        outcome: "UNKNOWN".into(),
        service_state: if s.run.is_some() {
            "UNKNOWN"
        } else {
            "NOT_STARTED"
        }
        .into(),
        engine_state: if s.phase == Phase::Initialized {
            "INITIALIZED"
        } else {
            "NOT_STARTED"
        }
        .into(),
        runtime_readiness: "NOT_CHECKED".into(),
        reason: s.reason.clone(),
        initialization: match s.phase {
            Phase::Registered => "NOT_STARTED",
            Phase::Initialized => "INITIALIZED",
            _ => "UNKNOWN",
        }
        .into(),
        operation_busy: busy,
        development_only: true,
        service_profile: "LINUX_DEVELOPMENT_SYSTEM_SERVICE",
        automatic_restart: false,
        boot_auto_start: false,
        runtime_package_acceptance: "NOT_ACCEPTED",
        global_consensus: "NOT_CHECKED",
        attempt_id: s
            .run
            .as_ref()
            .map(|r| r.attempt.clone())
            .or_else(|| s.init_attempt.clone()),
        pid: s.run.as_ref().and_then(|r| r.pid),
        node_instance_id: None,
        health: None,
    }
}
fn data_check(b: &Binding, s: &State) -> Result<()> {
    if boundary::private_directory(&data(b), b.service_uid, b.service_gid)?
        != s.data_identity.ok_or_else(unknown)?
    {
        return Err(unknown());
    }
    Ok(())
}
pub fn initialize(instance: &Path, resume: bool, timeout: Duration) -> Result<Summary> {
    boundary::root()?;
    duration(timeout)?;
    baseline()?;
    let guard = boundary::operation_lock(instance)?;
    let (b, mut store, mut state) = load(instance)?;
    if (!resume && state.phase != Phase::Registered)
        || (resume && !matches!(state.phase, Phase::InitIntent | Phase::Unknown))
        || state.run.is_some()
    {
        return Err(error("LINUX_SERVICE_NOT_INITIALIZED"));
    }
    let attempt = fresh_attempt()?;
    new_attempt(&b, &attempt, false)?;
    cold(
        &b,
        &output(&b, &attempt),
        timeout.min(Duration::from_secs(120)),
        Some(&guard),
    )?;
    if resume {
        resume_eligible(&b, &state)?;
    } else if fs::symlink_metadata(data(&b)).is_ok() {
        return Err(error("LINUX_SERVICE_PATH_EXISTS"));
    }
    store.require_capacity(3)?;
    diagnostics::record_init_command(&b, &attempt, if resume { "resume-init" } else { "init" })?;
    state.phase = Phase::InitIntent;
    state.init_attempt = Some(attempt.clone());
    state.reason = "LINUX_SERVICE_INIT_UNRESOLVED".into();
    save(&mut store, &state)?;
    let operation = (|| -> Result<init_result::Success> {
        if !resume {
            boundary::mkdir(&data(&b), b.service_uid, b.service_gid, 0o700)?;
            state.data_identity = Some(boundary::private_directory(
                &data(&b),
                b.service_uid,
                b.service_gid,
            )?);
            save(&mut store, &state)?;
        }
        data_check(&b, &state)?;
        verify_assets(&b)?;
        if resume {
            resume_eligible(&b, &state)?;
        }
        let j = java(&b);
        let a = jar(&b);
        let c = config(&b);
        let work = output(&b, &attempt);
        let report = work.join("report.jsonl");
        let command = if resume { "resume-init" } else { "init" };
        let out = runner::run(runner::Request {
            java: &j,
            jar: &a,
            command,
            config: Some(&c),
            report: Some(&report),
            attempt: Some(&attempt),
            work: &work,
            uid: b.service_uid,
            gid: b.service_gid,
            timeout,
            lock: Some(&guard),
        })?;
        if out.exit != 0 {
            return Err(error("ENGINE_INITIALIZATION_EXIT_FAILED"));
        }
        let report_bytes = boundary::read_owned(&report, b.service_uid, 16384)?;
        let journal =
            boundary::read_owned(&data(&b).join("engine-instance.json"), b.service_uid, 8192)?;
        let result = init_result::validate_success(
            &out.stdout,
            &report_bytes,
            &journal,
            &init_result::Expected {
                command,
                attempt_id: &attempt,
                pid: out.pid,
                identity: &b.identity,
                backend: "rocksdb",
                chain_fingerprint: &b.chain_fingerprint,
                node_identity: &b.node_identity,
                data_directory: &data(&b),
            },
        )?;
        let _database = boundary::database_unlocked(&data(&b), b.service_uid)?;
        data_check(&b, &state)?;
        verify_assets(&b)?;
        let evidence = serde_json::json!({"attemptId":attempt,"command":command,"pid":out.pid,"exitCode":out.exit,"genesisHash":result.genesis_hash,"stdoutSha256":files::digest(&out.stdout),"reportSha256":files::digest(&report_bytes),"engineJournalSha256":files::digest(&journal)});
        boundary::write_new(
            &b.control
                .join("attempts")
                .join(&attempt)
                .join("verified-result.json"),
            &encoded(&evidence)?,
            0,
            0o600,
        )?;
        Ok(result)
    })();
    match operation {
        Ok(success) => {
            state.phase = Phase::Initialized;
            state.genesis_hash = Some(success.genesis_hash);
            state.reason = "LINUX_SERVICE_INITIALIZED".into();
        }
        Err(e) => {
            state.phase = Phase::Unknown;
            state.reason = e.code;
        }
    }
    save(&mut store, &state)?;
    drop(guard);
    let mut result = summary(&b, &state, boundary::busy(instance)?);
    if state.phase == Phase::Initialized {
        result.outcome = "SUCCEEDED".into();
    }
    Ok(result)
}

fn resume_eligible(b: &Binding, s: &State) -> Result<()> {
    data_check(b, s)?;
    let data_path = data(b);
    let journal =
        boundary::read_owned(&data_path.join("engine-instance.json"), b.service_uid, 8192)?;
    init_result::validate_resume(
        &journal,
        &init_result::Expected {
            command: "resume-init",
            attempt_id: "resume-check",
            pid: 0,
            identity: &b.identity,
            backend: "rocksdb",
            chain_fingerprint: &b.chain_fingerprint,
            node_identity: &b.node_identity,
            data_directory: &data_path,
        },
    )
    .map_err(|_| error("LINUX_SERVICE_RESUME_NOT_ELIGIBLE"))?;
    boundary::read_owned(&data_path.join("ledger/CURRENT"), b.service_uid, 4096)?;
    let _database = boundary::database_unlocked(&data_path, b.service_uid)?;
    Ok(())
}

fn initialized(b: &Binding, s: &State) -> Result<()> {
    if s.phase != Phase::Initialized {
        return Err(error("LINUX_SERVICE_NOT_INITIALIZED"));
    }
    data_check(b, s)?;
    let data_path = data(b);
    let expected = runtime_result::Expected {
        attempt_id: "",
        pid: 0,
        identity: &b.identity,
        backend: "rocksdb",
        chain_fingerprint: &b.chain_fingerprint,
        node_identity: &b.node_identity,
        data_directory: &data_path,
        genesis_hash: s.genesis_hash.as_deref().ok_or_else(unknown)?,
    };
    runtime_result::validate_initialized(
        &boundary::read_owned(&data_path.join("engine-instance.json"), b.service_uid, 8192)?,
        &expected,
    )
}
fn expected_unit(b: &Binding, r: &Run) -> Result<systemd::ExpectedUnit> {
    let unit = unit(b, r)?;
    let value = systemd::ExpectedUnit {
        unit: unit.name().into(),
        invocation_id: r.invocation_id.clone().ok_or_else(unknown)?,
        main_pid: r.pid.ok_or_else(unknown)?,
        control_group: r.control_group.clone().ok_or_else(unknown)?,
    };
    if value.control_group != format!("/system.slice/{}", unit.name()) {
        return Err(unknown());
    }
    Ok(value)
}
fn java_args(b: &Binding, r: &Run) -> Vec<String> {
    let work = output(b, &r.attempt);
    vec![
        java(b).to_string_lossy().into(),
        format!("-Djava.io.tmpdir={}", work.display()),
        "-jar".into(),
        jar(b).to_string_lossy().into(),
        "run".into(),
        format!("--config={}", config(b).display()),
        format!("--report={}", work.join("report.jsonl").display()),
        format!("--attempt-id={}", r.attempt),
    ]
}
fn process_witness(b: &Binding, r: &Run) -> Result<()> {
    boundary::witness(
        r.pid.ok_or_else(unknown)?,
        r.start_ticks.ok_or_else(unknown)?,
        r.control_group.as_deref().ok_or_else(unknown)?,
        b.service_uid,
        b.service_gid,
        &java(b),
        &java_args(b, r),
    )
}
fn report(b: &Binding, s: &State, r: &Run) -> Result<runtime_result::RunReport> {
    initialized(b, s)?;
    let data_path = data(b);
    let expected = runtime_result::Expected {
        attempt_id: &r.attempt,
        pid: r.pid.ok_or_else(unknown)?,
        identity: &b.identity,
        backend: "rocksdb",
        chain_fingerprint: &b.chain_fingerprint,
        node_identity: &b.node_identity,
        data_directory: &data_path,
        genesis_hash: s.genesis_hash.as_deref().ok_or_else(unknown)?,
    };
    let work = output(b, &r.attempt);
    boundary::private_directory(&work, b.service_uid, b.service_gid)?;
    runtime_result::validate_report(
        &boundary::read_owned(&work.join("report.jsonl"), b.service_uid, 32768)?,
        &boundary::read_owned(&data_path.join("engine-instance.json"), b.service_uid, 8192)?,
        &expected,
    )
}
pub fn start(instance: &Path, timeout: Duration) -> Result<Summary> {
    boundary::root()?;
    duration(timeout)?;
    baseline()?;
    let guard = boundary::operation_lock(instance)?;
    let (b, mut store, mut state) = load(instance)?;
    initialized(&b, &state)?;
    if state
        .run
        .as_ref()
        .is_some_and(|r| r.phase != RunPhase::StoppedVerified)
    {
        return Err(error("LINUX_SERVICE_UNRESOLVED"));
    }
    if let Some(old) = &state.run {
        verify_stopped(&b, &state, old, Duration::from_secs(5))?;
    }
    let attempt = fresh_attempt()?;
    new_attempt(&b, &attempt, true)?;
    cold(
        &b,
        &output(&b, &attempt),
        timeout.min(Duration::from_secs(120)),
        Some(&guard),
    )?;
    initialized(&b, &state)?;
    {
        let _database = boundary::database_unlocked(&data(&b), b.service_uid)?;
    }
    let run = Run {
        attempt: attempt.clone(),
        phase: RunPhase::Prepared,
        pid: None,
        invocation_id: None,
        control_group: None,
        start_ticks: None,
        reason: "LINUX_SERVICE_START_PREPARED".into(),
    };
    let service = unit(&b, &run)?;
    service.install(Duration::from_secs(10))?;
    store.require_capacity(4)?;
    state.run = Some(run);
    state.reason = "LINUX_SERVICE_START_PREPARED".into();
    save(&mut store, &state)?;
    // Gate gets a separate open-description lock. Never wait while retaining ours.
    drop(guard);
    service.start(Duration::from_secs(10))?;
    wait_for(instance, Some(&attempt), timeout)
}
/// Root-only fixed systemd entrypoint. A consumed request cannot authorize a
/// direct restart; root gate and JVM retain the same manager MainPID.
pub fn run_gate(control: &Path, attempt: &str) -> Result<()> {
    boundary::root()?;
    // Read-only status probes briefly test the same flock. Only the fixed
    // internal gate retries that benign contention; user mutations stay fail-fast.
    let lock_deadline = Instant::now() + Duration::from_secs(5);
    let guard = loop {
        match boundary::operation_lock(control) {
            Ok(g) => break g,
            Err(e) if e.code == "LINUX_SERVICE_BUSY" && Instant::now() < lock_deadline => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(e) => return Err(e),
        }
    };
    let (b, mut store, mut state) = load(control)?;
    let mut run = state.run.clone().ok_or_else(unknown)?;
    if run.attempt != attempt || run.phase != RunPhase::Prepared {
        return Err(error("LINUX_SERVICE_GATE_REPLAY"));
    }
    let service = unit(&b, &run)?;
    let self_path = std::env::current_exe().map_err(|_| unknown())?;
    if self_path != control.join("attempts").join(attempt).join("bxdl-worker")
        || boundary::root_read(&self_path, MAX_WORKER)?
            != boundary::root_read(&control.join("worker"), MAX_WORKER)?
    {
        return Err(unknown());
    }
    let invocation = std::env::var("INVOCATION_ID").map_err(|_| unknown())?;
    let group = boundary::cgroup(std::process::id())?;
    let expected = systemd::ExpectedUnit {
        unit: service.name().into(),
        invocation_id: invocation.clone(),
        main_pid: std::process::id(),
        control_group: group.clone(),
    };
    if group != format!("/system.slice/{}", service.name()) {
        return Err(unknown());
    }
    let publication_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let observed =
            service.inspect(remaining(publication_deadline)?.min(Duration::from_secs(2)))?;
        if observed.matches_running_metadata(&expected) {
            break;
        }
        if observed.unit != expected.unit
            || observed.main_pid != expected.main_pid
            || observed.invocation_id != expected.invocation_id
            || observed.control_group != expected.control_group
            || observed.active_state != "activating"
        {
            return Err(unknown());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    store.require_capacity(3)?;
    run.phase = RunPhase::Consumed;
    run.pid = Some(std::process::id());
    run.invocation_id = Some(invocation);
    run.control_group = Some(group);
    run.start_ticks = Some(boundary::ticks(std::process::id())?);
    run.reason = "LINUX_SERVICE_GATE_CONSUMED".into();
    state.run = Some(run.clone());
    state.reason = run.reason.clone();
    save(&mut store, &state)?;
    let operation = (|| -> Result<()> {
        baseline()?;
        initialized(&b, &state)?;
        verify_assets(&b)?;
        let work = output(&b, attempt);
        cold(&b, &work, Duration::from_secs(120), Some(&guard))?;
        initialized(&b, &state)?;
        {
            let _database = boundary::database_unlocked(&data(&b), b.service_uid)?;
        }
        if fs::symlink_metadata(control.join("attempts").join(attempt).join("stop.request")).is_ok()
        {
            return Err(error("LINUX_SERVICE_START_CANCELLED"));
        }
        let j = java(&b);
        let a = jar(&b);
        let c = config(&b);
        let report = work.join("report.jsonl");
        verify_assets(&b)?;
        // Store and all ordinary files are CLOEXEC. Only this readonly flock
        // is explicitly inherited on stdin; engine cannot write root records.
        runner::exec(runner::Request {
            java: &j,
            jar: &a,
            command: "run",
            config: Some(&c),
            report: Some(&report),
            attempt: Some(attempt),
            work: &work,
            uid: b.service_uid,
            gid: b.service_gid,
            timeout: Duration::from_secs(120),
            lock: Some(&guard),
        })
    })();
    if operation.is_err() {
        run.phase = RunPhase::Refused;
        run.reason = "LINUX_SERVICE_GATE_REFUSED".into();
        state.run = Some(run);
        state.reason = "LINUX_SERVICE_GATE_REFUSED".into();
        save(&mut store, &state)?;
    }
    operation
}
fn verify_stopped(b: &Binding, s: &State, r: &Run, timeout: Duration) -> Result<()> {
    let os = unit(b, r)?.inspect(timeout.min(Duration::from_secs(5)))?;
    exit::verify(b, r)?;
    let expected = expected_unit(b, r)?;
    // Inactive units may be garbage-collected; the root ExecStopPost receipt
    // retains this invocation's actual exit while current metadata proves no
    // replacement invocation is running. Empty reset metadata is not exit proof.
    if os.main_pid != 0
        || os.load_state != "loaded"
        || os.active_state != "inactive"
        || os.sub_state != "dead"
        || (!os.invocation_id.is_empty() && os.invocation_id != expected.invocation_id)
        || (!os.control_group.is_empty() && os.control_group != expected.control_group)
        || report(b, s, r)?.status != "STOPPED"
    {
        return Err(unknown());
    }
    boundary::absent(
        r.pid.ok_or_else(unknown)?,
        r.start_ticks.ok_or_else(unknown)?,
        r.control_group.as_deref().ok_or_else(unknown)?,
    )?;
    let _database = boundary::database_unlocked(&data(b), b.service_uid)?;
    initialized(b, s)?;
    Ok(())
}
pub fn status(instance: &Path, timeout: Duration) -> Result<Summary> {
    boundary::root()?;
    duration(timeout)?;
    let deadline = Instant::now() + timeout;
    let (b, _store, state) = load(instance)?;
    let mut result = summary(&b, &state, boundary::busy(instance)?);
    let Some(run) = state.run.as_ref() else {
        result.outcome = if state.phase == Phase::Unknown || state.phase == Phase::InitIntent {
            "UNKNOWN"
        } else {
            "INCOMPLETE"
        }
        .into();
        return Ok(result);
    };
    if run.phase == RunPhase::Refused {
        result.outcome = "FAILED".into();
        result.engine_state = "GATE_REFUSED".into();
        result.reason = "LINUX_SERVICE_GATE_REFUSED".into();
        return Ok(result);
    }
    let service = unit(&b, run)?;
    let os = match service.inspect(remaining(deadline)?.min(Duration::from_secs(5))) {
        Ok(o) => o,
        Err(_) => {
            result.reason = "LINUX_SERVICE_OS_UNKNOWN".into();
            return Ok(result);
        }
    };
    result.service_state = match (os.active_state.as_str(), os.sub_state.as_str()) {
        ("active", "running") => "RUNNING",
        ("inactive", "dead") => "INACTIVE",
        ("activating", _) => "STARTING",
        ("deactivating", _) => "STOPPING",
        ("failed", _) => "FAILED",
        _ => "UNKNOWN",
    }
    .into();
    if run.phase == RunPhase::Prepared {
        result.reason = "LINUX_SERVICE_START_PENDING".into();
        return Ok(result);
    }
    let observation = match report(&b, &state, run) {
        Ok(r) => r,
        Err(_) => {
            result.reason = "LINUX_SERVICE_REPORT_UNKNOWN".into();
            return Ok(result);
        }
    };
    result.engine_state = observation.status.clone();
    result.node_instance_id = observation.node_instance_id.clone();
    if observation.status == "STOPPED" {
        if !result.operation_busy && verify_stopped(&b, &state, run, remaining(deadline)?).is_ok() {
            result.outcome = if run.phase == RunPhase::StoppedVerified {
                "SUCCEEDED"
            } else {
                "INCOMPLETE"
            }
            .into();
            result.runtime_readiness = "STOPPED".into();
            result.reason = if run.phase == RunPhase::StoppedVerified {
                "LINUX_SERVICE_STOPPED_VERIFIED"
            } else {
                "LINUX_SERVICE_STOP_REQUIRES_CONFIRMATION"
            }
            .into();
        }
        return Ok(result);
    }
    if observation.status == "FAILED" {
        result.outcome = "FAILED".into();
        result.reason = "LINUX_SERVICE_ENGINE_FAILED".into();
        return Ok(result);
    }
    if observation.status != "RUNNING"
        || !result.operation_busy
        || !os.matches_running_metadata(&expected_unit(&b, run)?)
        || process_witness(&b, run).is_err()
    {
        result.reason = "LINUX_SERVICE_PROCESS_UNKNOWN".into();
        return Ok(result);
    }
    let health = (|| -> Result<runtime_result::HealthReport> {
        let data_path = data(&b);
        let expected = runtime_result::Expected {
            attempt_id: &run.attempt,
            pid: run.pid.ok_or_else(unknown)?,
            identity: &b.identity,
            backend: "rocksdb",
            chain_fingerprint: &b.chain_fingerprint,
            node_identity: &b.node_identity,
            data_directory: &data_path,
            genesis_hash: state.genesis_hash.as_deref().ok_or_else(unknown)?,
        };
        let node = observation
            .node_instance_id
            .as_deref()
            .ok_or_else(unknown)?;
        let budget =
            || -> Result<Duration> { Ok(remaining(deadline)?.min(Duration::from_secs(2))) };
        let earliest = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| unknown())?
            .as_millis();
        let first = runtime_http::get(b.endpoint, "/monitor/api/console/bootstrap", budget()?)?;
        runtime_result::validate_bootstrap(&first.body, &first.media, &expected, node)?;
        let raw = runtime_http::get(b.endpoint, "/monitor/api/consensus/health", budget()?)?;
        let health = runtime_result::validate_health(&raw.body, &raw.media)?;
        let last = runtime_http::get(b.endpoint, "/monitor/api/console/bootstrap", budget()?)?;
        runtime_result::validate_bootstrap(&last.body, &last.media, &expected, node)?;
        let observed = health
            .observed_at_epoch_millis
            .parse::<u128>()
            .map_err(|_| unknown())?;
        let latest = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| unknown())?
            .as_millis();
        if observed < earliest.saturating_sub(5000)
            || observed > latest.saturating_add(5000)
            || health
                .readiness
                .as_ref()
                .is_some_and(|r| r.role != "VALIDATOR")
        {
            return Err(unknown());
        }
        if !service
            .inspect(budget()?)?
            .matches_running_metadata(&expected_unit(&b, run)?)
            || !boundary::busy(instance)?
            || report(&b, &state, run)?.status != "RUNNING"
        {
            return Err(unknown());
        }
        process_witness(&b, run)?;
        Ok(health)
    })();
    match health {
        Ok(health) => {
            if let Some(ready) = &health.readiness {
                result.runtime_readiness = ready.status.clone();
                result.reason = ready.reason.clone();
                result.outcome = match ready.status.as_str() {
                    "READY" => "SUCCEEDED",
                    "NOT_READY" => "INCOMPLETE",
                    "FAILED" => "FAILED",
                    _ => "UNKNOWN",
                }
                .into();
            }
            result.health = Some(serde_json::to_value(health).map_err(|_| unknown())?);
        }
        Err(_) => result.reason = "LINUX_SERVICE_HTTP_IDENTITY_UNKNOWN".into(),
    }
    Ok(result)
}
fn remaining(deadline: Instant) -> Result<Duration> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err(error("LINUX_SERVICE_WAIT_EXPIRED"))
    } else {
        Ok(left)
    }
}
fn wait_for(instance: &Path, attempt: Option<&str>, timeout: Duration) -> Result<Summary> {
    duration(timeout)?;
    let deadline = Instant::now() + timeout;
    let mut wanted = attempt.map(str::to_owned);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let mut current = status(
            instance,
            left.max(Duration::from_millis(1))
                .min(Duration::from_secs(5)),
        )?;
        if wanted.is_none() {
            wanted = current.attempt_id.clone();
        }
        if current.attempt_id != wanted {
            return Err(error("LINUX_SERVICE_ATTEMPT_CHANGED"));
        }
        if matches!(current.outcome.as_str(), "SUCCEEDED" | "FAILED") {
            return Ok(current);
        }
        if Instant::now() >= deadline {
            current.outcome = "UNKNOWN".into();
            current.reason = "LINUX_SERVICE_WAIT_EXPIRED".into();
            return Ok(current);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}
pub fn wait_ready(instance: &Path, timeout: Duration) -> Result<Summary> {
    wait_for(instance, None, timeout)
}
pub fn stop(instance: &Path, timeout: Duration) -> Result<Summary> {
    boundary::root()?;
    duration(timeout)?;
    let (b, _, state) = load(instance)?;
    let Some(run) = state.run.as_ref() else {
        return status(instance, timeout);
    };
    let request = b
        .control
        .join("attempts")
        .join(&run.attempt)
        .join("stop.request");
    let bytes = encoded(&run.attempt)?;
    if fs::symlink_metadata(&request).is_ok() {
        if boundary::root_read(&request, 256)? != bytes {
            return Err(unknown());
        }
    } else {
        boundary::write_new(&request, &bytes, 0, 0o600)?;
    }
    let deadline = Instant::now() + timeout;
    let mut sent = false;
    loop {
        let (latest_b, _store, latest) = load(instance)?;
        let active = latest.run.clone().ok_or_else(unknown)?;
        if active.attempt != run.attempt {
            return Err(error("LINUX_SERVICE_ATTEMPT_CHANGED"));
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            let mut result = summary(&latest_b, &latest, boundary::busy(instance)?);
            result.reason = "LINUX_SERVICE_STOP_WAIT_EXPIRED".into();
            return Ok(result);
        }
        if active.phase == RunPhase::Prepared {
            std::thread::sleep(Duration::from_millis(150));
            continue;
        }
        if active.phase == RunPhase::Refused {
            return status(instance, left.min(Duration::from_secs(5)));
        }
        let service = unit(&latest_b, &active)?;
        let os = service.inspect(left.min(Duration::from_secs(3)))?;
        if os.matches_running_metadata(&expected_unit(&latest_b, &active)?) {
            if !sent
                && report(&latest_b, &latest, &active)
                    .is_ok_and(|r| matches!(r.status.as_str(), "CHECKING" | "STARTING" | "RUNNING"))
                && process_witness(&latest_b, &active).is_ok()
            {
                let proof =
                    StopAuthorization::after_process_audit(expected_unit(&latest_b, &active)?)?;
                service.stop(&proof, left.min(Duration::from_secs(5)))?;
                sent = true;
            }
        } else if os.main_pid == 0 {
            if verify_stopped(&latest_b, &latest, &active, left).is_ok() {
                let _guard = boundary::operation_lock(instance)?;
                let (latest_b, mut store, mut latest) = load(instance)?;
                let active = latest.run.clone().ok_or_else(unknown)?;
                if active.attempt != run.attempt {
                    return Err(error("LINUX_SERVICE_ATTEMPT_CHANGED"));
                }
                // Repeat closure under the exclusive operation lock before publishing.
                verify_stopped(&latest_b, &latest, &active, remaining(deadline)?)?;
                if active.phase != RunPhase::StoppedVerified {
                    let mut done = active;
                    done.phase = RunPhase::StoppedVerified;
                    done.reason = "LINUX_SERVICE_STOPPED_VERIFIED".into();
                    latest.run = Some(done);
                    latest.reason = "LINUX_SERVICE_STOPPED_VERIFIED".into();
                    save(&mut store, &latest)?;
                }
                let mut result = summary(&latest_b, &latest, false);
                result.outcome = "SUCCEEDED".into();
                result.service_state = "INACTIVE".into();
                result.engine_state = "STOPPED".into();
                result.runtime_readiness = "STOPPED".into();
                return Ok(result);
            }
            // ExecStopPost runs after MainPID becomes zero. It still owns the
            // cgroup and may not yet have published its durable exit receipt.
            // Wait through that transition; no OS-only success or early retry.
            if os.active_state == "failed" {
                return status(instance, left.min(Duration::from_secs(5)));
            }
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}
/// Offline, bounded, redacted evidence projection. No raw engine stderr, key
/// material, control paths or credentials are returned to the caller.
pub fn logs(instance: &Path) -> Result<Summary> {
    let (b, _, state) = load(instance)?;
    let mut result = summary(&b, &state, boundary::busy(instance)?);
    if let Some(run) = &state.run {
        if let Ok(observed) = report(&b, &state, run) {
            result.engine_state = observed.status;
            result.node_instance_id = observed.node_instance_id;
            result.reason = "LINUX_SERVICE_SAVED_REPORT".into();
        }
    }
    result.outcome = "INCOMPLETE".into();
    result.runtime_readiness = "NOT_CHECKED".into();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> State {
        State {
            schema_version: 1,
            binding_sha256: "abc".into(),
            phase: Phase::Registered,
            reason: "registered".into(),
            init_attempt: None,
            genesis_hash: None,
            data_identity: None,
            run: None,
        }
    }
    #[test]
    fn private_state_roundtrips_through_strict_decoder_without_nulls() {
        let mut value = state();
        let raw = encoded(&value).unwrap();
        assert!(!String::from_utf8(raw.clone()).unwrap().contains("null"));
        assert!(json::decode::<State>(&raw).is_ok());
        value.run = Some(Run {
            attempt: "lnx-test".into(),
            phase: RunPhase::Prepared,
            pid: None,
            invocation_id: None,
            control_group: None,
            start_ticks: None,
            reason: "LINUX_SERVICE_START_PREPARED".into(),
        });
        let raw = encoded(&value).unwrap();
        assert!(!String::from_utf8(raw.clone()).unwrap().contains("null"));
        assert!(json::decode::<State>(&raw).is_ok());
    }
    #[test]
    fn account_is_per_control_and_stable() {
        assert_eq!(
            service_account(Path::new("/var/lib/bxdl/a")).unwrap(),
            service_account(Path::new("/var/lib/bxdl/a")).unwrap()
        );
        assert_ne!(
            service_account(Path::new("/var/lib/bxdl/a")).unwrap(),
            service_account(Path::new("/var/lib/bxdl/b")).unwrap()
        );
        assert!(service_account(Path::new("relative")).is_err());
    }
    #[test]
    fn journal_rejects_partial_initialized_and_replayed_metadata() {
        let mut s = state();
        assert!(validate_state(&s, "abc").is_ok());
        s.phase = Phase::Initialized;
        assert!(validate_state(&s, "abc").is_err());
        s.genesis_hash = Some("genesis".into());
        s.data_identity = Some((1, 2));
        assert!(validate_state(&s, "abc").is_ok());
        s.run = Some(Run {
            attempt: "lnx-a".into(),
            phase: RunPhase::Prepared,
            pid: Some(42),
            invocation_id: None,
            control_group: None,
            start_ticks: None,
            reason: "bad".into(),
        });
        assert!(validate_state(&s, "abc").is_err());
        assert!(validate_state(&state(), "different").is_err());
    }
    #[test]
    fn shipped_linux_locks_match_engine_and_platform() {
        let engine = lock().unwrap();
        let runtime = runtime_lock().unwrap();
        assert_eq!(engine.java_sha256, runtime.java_sha256);
        assert_eq!(runtime.platform.os, "linux");
        assert_eq!(runtime.platform.arch, "amd64");
        assert_eq!(
            engine.expected.source.commit,
            "037c98c315d5322fa7da266ca0254d61f3863622"
        );
    }
    fn spec() -> Spec {
        Spec {
            schema_version: 1,
            runtime_root: "/opt/bxdl-input/jre".into(),
            runtime_lock: "/opt/bxdl-input/runtime-lock.json".into(),
            engine_jar: "/opt/bxdl-input/engine.jar".into(),
            engine_lock: "/opt/bxdl-input/engine-lock.json".into(),
            native_config: "/etc/bxdl-input/node.json".into(),
            product_config: "/etc/bxdl-input/product.json".into(),
            service_uid: 987,
            service_gid: 987,
        }
    }
    #[test]
    fn spec_rejects_missing_fields_root_ids_and_unknown_schema_before_admission() {
        assert!(json::decode::<Spec>(br#"{"schemaVersion":1,"serviceUid":0}"#).is_err());
        assert!(validate_spec(&spec()).is_ok());
        for invalid in [0, u32::MAX] {
            let mut input = spec();
            input.service_uid = invalid;
            assert!(validate_spec(&input).is_err());
            let mut input = spec();
            input.service_gid = invalid;
            assert!(validate_spec(&input).is_err());
        }
        for schema in [0, 2] {
            let mut input = spec();
            input.schema_version = schema;
            assert!(validate_spec(&input).is_err());
        }
    }
    #[test]
    fn every_spec_source_requires_an_absolute_canonical_path() {
        for index in 0..6 {
            for invalid in [
                "relative/input",
                "/opt/../etc/input",
                "/opt/./input",
                "/opt//input",
                "/opt/input/",
            ] {
                let mut input = spec();
                let paths = [
                    &mut input.runtime_root,
                    &mut input.runtime_lock,
                    &mut input.engine_jar,
                    &mut input.engine_lock,
                    &mut input.native_config,
                    &mut input.product_config,
                ];
                *paths.into_iter().nth(index).unwrap() = invalid.into();
                assert!(
                    validate_spec(&input).is_err(),
                    "accepted source {index}: {invalid}"
                );
            }
        }
    }
}
