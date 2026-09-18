//! Registered development instances. Registration never initializes data;
//! explicit one-shot init/resume-init preserves uncertain attempts for inspection.
use super::{
    Identity, Lock, Options, ProductReport, fail,
    files::{self, Input, NativeInput, Workspace},
    init_result::{self, Expected},
    instance_fs::Control,
    json, managed,
    product::ProductInput,
};
use crate::{
    artifact,
    error::{BxdlError, Result},
    install,
    setup::{
        paths,
        store::{self, Store},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct RegisterOptions {
    pub instance: PathBuf,
    pub package: PathBuf,
    pub archive: PathBuf,
    pub public_key: Option<PathBuf>,
    pub allow_unsigned_development: bool,
    pub product: PathBuf,
    pub native: PathBuf,
    pub lock: PathBuf,
    pub timeout: Duration,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Pin {
    path: PathBuf,
    sha256: String,
}
// Private 0600 binding: paths and reference hashes never enter public summaries.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    schema_version: u32,
    instance_id: String,
    package: PathBuf,
    archive: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    public_key: Option<PathBuf>,
    allow_unsigned_development: bool,
    product: PathBuf,
    native: PathBuf,
    lock: PathBuf,
    archive_sha256: String,
    manifest_sha256: String,
    identity: Identity,
    backend: String,
    chain_fingerprint: String,
    node_identity: String,
    data_directory: PathBuf,
    pins: Vec<Pin>,
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Phase {
    Registered,
    InitIntent,
    Initialized,
    Unknown,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Attempt {
    id: String,
    command: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct State {
    schema_version: u32,
    instance_id: String,
    binding_sha256: String,
    phase: Phase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attempt: Option<Attempt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    genesis_hash: Option<String>,
    reason: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub instance_id: String,
    pub initialization: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_busy: Option<bool>,
    pub development_only: bool,
    pub service_registration: &'static str,
    pub runtime_readiness: &'static str,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genesis_hash: Option<String>,
}
impl State {
    fn summary(&self, busy: Option<bool>) -> Summary {
        Summary {
            instance_id: self.instance_id.clone(),
            initialization: match self.phase {
                Phase::Registered => "NOT_STARTED",
                Phase::Initialized => "INITIALIZED",
                _ => "UNKNOWN",
            }
            .into(),
            operation_busy: busy,
            development_only: true,
            service_registration: "NOT_REGISTERED",
            runtime_readiness: "NOT_CHECKED",
            reason: self.reason.clone(),
            attempt_id: self.attempt.as_ref().map(|a| a.id.clone()),
            genesis_hash: self.genesis_hash.clone(),
        }
    }
    fn save(&self, store: &mut Store) -> Result<()> {
        let raw = serde_json::to_vec(self).map_err(|_| corrupt())?;
        store.save(&raw).map_err(state_error)
    }
}

pub fn register(options: &RegisterOptions) -> Result<Summary> {
    host()?;
    let instance = files::absolute(&options.instance)?;
    files::check_path(&instance, true)?;
    if fs::symlink_metadata(&instance).is_ok() {
        return Err(fail(
            "INSTANCE_EXISTS",
            "인스턴스 폴더가 이미 있습니다. instance show로 확인하거나 새 경로를 사용하세요.",
        ));
    }
    let native = NativeInput::load(&options.native)?;
    let product = Input::read(&options.product, 262_144, false)?;
    let lock = Input::read(&options.lock, 65_536, false)?;
    let mut binding = Binding {
        schema_version: 1,
        instance_id: String::new(),
        package: files::absolute(&options.package)?,
        archive: files::absolute(&options.archive)?,
        public_key: options
            .public_key
            .as_deref()
            .map(files::absolute)
            .transpose()?,
        allow_unsigned_development: options.allow_unsigned_development,
        product: product.path.clone(),
        native: native.config.path.clone(),
        lock: lock.path.clone(),
        archive_sha256: String::new(),
        manifest_sha256: String::new(),
        identity: read_lock(&lock)?.expected,
        backend: native.value.backend.clone(),
        chain_fingerprint: String::new(),
        node_identity: String::new(),
        data_directory: native.data.clone(),
        pins: Vec::new(),
    };
    let mut inputs = vec![
        product,
        lock,
        Input::read(&native.config.path, 262_144, false)?,
        Input::read(&native.chain.path, 262_144, false)?,
    ];
    for path in &native.references {
        inputs.push(Input::read(path, 4 * 1024 * 1024, false)?);
    }
    if let Some(key) = &binding.public_key {
        inputs.push(Input::read(key, 65_536, false)?);
    }
    let mut seen = BTreeSet::new();
    for input in &inputs {
        if seen.insert(input.path.clone()) {
            binding.pins.push(Pin {
                path: input.path.clone(),
                sha256: files::digest(&input.raw),
            });
        }
    }
    disjoint(&instance, &binding)?;
    // No persistent control or data paths are created until every check passes.
    let package = verified_package(&binding, false)?;
    binding.archive_sha256 = package.archive_sha256.clone();
    binding.manifest_sha256 = package.manifest_sha256.clone();
    let report = super::preflight_product(
        &binding.options(options.timeout),
        &binding.product,
        &binding.native,
    )?;
    let engine = report.engine.as_ref().ok_or_else(precondition)?;
    let cold = engine.preflight.as_ref().ok_or_else(precondition)?;
    if report.configuration_binding != "MATCHED" || engine.identity != binding.identity {
        return Err(precondition());
    }
    binding.instance_id = report.product.instance_id;
    binding.chain_fingerprint = cold.chain_fingerprint.clone();
    binding.node_identity = cold.node_identity.clone();
    if binding.backend != "rocksdb" || cold.backend != binding.backend {
        return Err(precondition());
    }
    for input in &inputs {
        input.recheck()?;
    }
    native.recheck()?;
    data_precondition(&binding, false)?;
    install::verify_installed(&binding.package, &package)?;
    let root = Control::create(&instance)?;
    let _guard = root.lock()?;
    let raw = serde_json::to_vec(&binding).map_err(|_| corrupt())?;
    store::write_new(&root.path().join("binding.json"), &raw).map_err(state_error)?;
    let mut journal = Store::create(&root.path().join("journal")).map_err(state_error)?;
    let state = State {
        schema_version: 1,
        instance_id: binding.instance_id,
        binding_sha256: files::digest(&raw),
        phase: Phase::Registered,
        attempt: None,
        genesis_hash: None,
        reason: "REGISTERED_COLD_CHECKED".into(),
    };
    state.save(&mut journal)?;
    root.recheck()?;
    Ok(state.summary(Some(false)))
}

impl Binding {
    fn options(&self, timeout: Duration) -> Options {
        Options {
            jar: self.package.join("engine/nigo-node.jar"),
            java: self.package.join("runtime/bin/java"),
            lock: self.lock.clone(),
            timeout,
        }
    }
    fn check_pins(&self) -> Result<Vec<Input>> {
        if self.pins.is_empty() || self.pins.len() > 128 {
            return Err(corrupt());
        }
        self.pins.iter().map(|p| { let input=Input::read(&p.path,4*1024*1024,false)?; if files::digest(&input.raw)!=p.sha256 {return Err(fail("INSTANCE_INPUT_CHANGED","등록한 설정·키 참조 또는 신뢰 입력이 변경되었습니다. 기존 인스턴스에서 초기화를 실행하지 않았습니다."));} Ok(input) }).collect()
    }
}
fn verified_package(binding: &Binding, pinned: bool) -> Result<artifact::Report> {
    let expected = artifact::verify(
        &binding.archive,
        &artifact::VerifyOptions {
            public_key_path: binding.public_key.clone(),
            allow_unsigned_development: binding.allow_unsigned_development,
        },
    )?;
    if pinned
        && (expected.archive_sha256 != binding.archive_sha256
            || expected.manifest_sha256 != binding.manifest_sha256)
    {
        return Err(precondition());
    }
    let lock = read_lock(&Input::read(&binding.lock, 65_536, false)?)?;
    if expected.manifest.engine.jar_sha256 != lock.jar_sha256
        || expected.manifest.runtime.java_sha256 != lock.java_sha256
        || expected.manifest.engine.revision != binding.identity.source.commit
        || expected.manifest.engine.contract_revision != binding.identity.contract.fingerprint
        || lock.expected != binding.identity
    {
        return Err(precondition());
    }
    install::verify_installed(&binding.package, &expected)?;
    Ok(expected)
}
fn read_lock(input: &Input) -> Result<Lock> {
    let lock: Lock = json::decode(&input.raw).map_err(|_| corrupt())?;
    super::validate_lock(&lock)?;
    Ok(lock)
}
fn disjoint(instance: &Path, b: &Binding) -> Result<()> {
    for trust in [&b.lock, &b.archive].into_iter().chain(b.public_key.iter()) {
        if paths::overlaps(&b.package, trust)? {
            return Err(fail(
                "INSTANCE_TRUST_PATH_OVERLAP",
                "신뢰 lock·공개 key·원본 archive는 설치 패키지 밖에서 제공하세요.",
            ));
        }
    }
    for path in b
        .pins
        .iter()
        .map(|p| p.path.as_path())
        .chain([b.package.as_path(), b.archive.as_path()])
    {
        if paths::overlaps(instance, path)? || paths::overlaps(&b.data_directory, path)? {
            return Err(fail(
                "INSTANCE_PATH_OVERLAP",
                "인스턴스 기록·데이터·설치 패키지·입력 경로를 서로 분리하세요.",
            ));
        }
    }
    if paths::overlaps(instance, &b.data_directory)? {
        return Err(precondition());
    }
    Ok(())
}
fn load(root: &Control) -> Result<(Binding, Input, Store, State)> {
    let binding_file = Input::read(&root.path().join("binding.json"), 262_144, false)?;
    let binding: Binding = json::decode(&binding_file.raw).map_err(|_| corrupt())?;
    let journal = Store::open(&root.path().join("journal")).map_err(state_error)?;
    let raw = journal.read().map_err(state_error)?.ok_or_else(corrupt)?;
    let state: State = json::decode(&raw).map_err(|_| corrupt())?;
    if binding.schema_version != 1
        || state.schema_version != 1
        || binding.instance_id != state.instance_id
        || files::digest(&binding_file.raw) != state.binding_sha256
        || binding.backend != "rocksdb"
        || !super::hash(&binding.chain_fingerprint)
        || (state.phase == Phase::Registered) != (state.attempt.is_none())
        || (state.phase == Phase::Initialized) != (state.genesis_hash.is_some())
    {
        return Err(corrupt());
    }
    if let Some(a) = &state.attempt {
        if !valid_attempt(&a.id) || !matches!(a.command.as_str(), "init" | "resume-init") {
            return Err(corrupt());
        }
    }
    for path in [
        &binding.package,
        &binding.archive,
        &binding.product,
        &binding.native,
        &binding.lock,
        &binding.data_directory,
    ] {
        if !path.is_absolute() || files::absolute(path)? != *path {
            return Err(corrupt());
        }
    }
    disjoint(root.path(), &binding)?;
    Ok((binding, binding_file, journal, state))
}
pub fn show(instance: &Path) -> Result<Summary> {
    let root = Control::open(instance)?;
    let (_binding, _file, _journal, state) = load(&root)?;
    let busy = match root.lock() {
        Ok(_guard) => false,
        Err(e) if e.code == "INSTANCE_BUSY" => true,
        Err(e) => return Err(e),
    };
    Ok(state.summary(Some(busy)))
}
pub fn preflight(instance: &Path, timeout: Duration) -> Result<ProductReport> {
    host()?;
    let root = Control::open(instance)?;
    let _guard = root.lock()?;
    let (binding, file, _journal, _state) = load(&root)?;
    let inputs = binding.check_pins()?;
    let package = verified_package(&binding, true)?;
    let result = bound_preflight(&binding, timeout)?;
    install::verify_installed(&binding.package, &package)?;
    for input in &inputs {
        input.recheck()?;
    }
    file.recheck()?;
    root.recheck()?;
    Ok(result)
}
fn bound_preflight(binding: &Binding, timeout: Duration) -> Result<ProductReport> {
    let report = super::preflight_product(
        &binding.options(timeout.min(Duration::from_secs(120))),
        &binding.product,
        &binding.native,
    )?;
    let engine = report.engine.as_ref().ok_or_else(precondition)?;
    let cold = engine.preflight.as_ref().ok_or_else(precondition)?;
    if report.configuration_binding != "MATCHED"
        || report.product.instance_id != binding.instance_id
        || engine.identity != binding.identity
        || cold.chain_fingerprint != binding.chain_fingerprint
        || cold.node_identity != binding.node_identity
        || cold.backend != binding.backend
    {
        return Err(precondition());
    }
    Ok(report)
}

pub fn initialize(instance: &Path, resume: bool, timeout: Duration) -> Result<Summary> {
    host()?;
    if timeout.is_zero() || timeout > Duration::from_secs(600) {
        return Err(precondition());
    }
    let root = Control::open(instance)?;
    let guard = root.lock()?;
    let (binding, binding_file, mut journal, mut state) = load(&root)?;
    if (!resume && state.phase != Phase::Registered)
        || (resume && !matches!(state.phase, Phase::InitIntent | Phase::Unknown))
    {
        return Err(fail(
            "INSTANCE_INITIALIZATION_STATE",
            "init은 등록 직후 한 번만, resume-init은 미완료 시도에만 실행할 수 있습니다. instance show로 확인하세요.",
        ));
    }
    let inputs = binding.check_pins()?;
    let package = verified_package(&binding, true)?;
    bound_preflight(&binding, timeout)?;
    let native = NativeInput::load(&binding.native)?;
    let product = ProductInput::load(&binding.product)?;
    product.check(&native)?;
    if native.data != binding.data_directory {
        return Err(precondition());
    }
    data_precondition(&binding, resume)?;
    // Reserve both durable intent and terminal checkpoint before touching data.
    journal.require_capacity(2).map_err(state_error)?;
    let attempt = Attempt {
        id: attempt_id()?,
        command: if resume { "resume-init" } else { "init" }.into(),
    };
    let workspace = Workspace::persistent(&root.attempt(&attempt.id)?)?;
    let options = binding.options(timeout);
    let lock = read_lock(&Input::read(&binding.lock, 65_536, false)?)?;
    let java = files::Binary::open(&options.java, &lock.java_sha256)?;
    let jar = workspace.snapshot_jar(&options.jar, &lock.jar_sha256, lock.jar_size_bytes)?;
    let config = native.snapshot(&workspace)?;
    for input in &inputs {
        input.recheck()?;
    }
    product.recheck()?;
    native.recheck()?;
    binding_file.recheck()?;
    root.recheck()?;
    install::verify_installed(&binding.package, &package)?;
    let child_lock = guard.child_stdin()?;
    state.phase = Phase::InitIntent;
    state.attempt = Some(attempt.clone());
    state.reason = "INITIALIZATION_ATTEMPT_UNRESOLVED".into();
    // save() may have published before a sync failure: never spawn on Err.
    state.save(&mut journal).map_err(|_| fail("INSTANCE_INITIALIZATION_UNKNOWN", "시도 기록의 게시 여부를 확정하지 못했습니다. 엔진은 시작하지 않았으며 기록을 보존했습니다."))?;
    let operation = (|| -> Result<init_result::Success> {
        if !resume {
            super::instance_fs::prepare_data(&binding.data_directory)?;
        }
        let output = managed::run(managed::Request {
            java: &java,
            jar: &jar,
            workspace: &workspace,
            command: &attempt.command,
            config: &config,
            attempt: &attempt.id,
            lock: child_lock,
            timeout,
        })?;
        if output.exit != 0 {
            return Err(fail(
                "ENGINE_INITIALIZATION_EXIT_FAILED",
                "엔진이 초기화 성공 종료 코드를 반환하지 않았습니다.",
            ));
        }
        let report = Input::read(&workspace.path.join("report.jsonl"), 16_384, false)?;
        let instance = Input::read(
            &binding.data_directory.join("engine-instance.json"),
            8192,
            false,
        )?;
        let expected = expected(&binding, &attempt, output.pid);
        let success =
            init_result::validate_success(&output.stdout, &report.raw, &instance.raw, &expected)?;
        for input in &inputs {
            input.recheck()?;
        }
        binding_file.recheck()?;
        root.recheck()?;
        native.recheck()?;
        report.recheck()?;
        instance.recheck()?;
        install::verify_installed(&binding.package, &package)?;
        let evidence = serde_json::json!({"attemptId":attempt.id,"command":attempt.command,"pid":output.pid,"exitCode":output.exit,"genesisHash":success.genesis_hash});
        workspace.write(
            "verified-result.json",
            &serde_json::to_vec(&evidence).map_err(|_| corrupt())?,
        )?;
        Ok(success)
    })();
    match operation {
        Ok(success) => {
            state.phase = Phase::Initialized;
            state.genesis_hash = Some(success.genesis_hash);
            state.reason = "INITIALIZATION_VERIFIED".into();
        }
        Err(error) => {
            state.phase = Phase::Unknown;
            state.reason = error.code;
        }
    }
    if state.save(&mut journal).is_err() {
        return Err(fail(
            "INSTANCE_INITIALIZATION_UNKNOWN",
            "초기화 결과 기록을 확정하지 못했습니다. 데이터와 작업 기록을 보존했습니다. instance show로 확인하세요.",
        ));
    }
    Ok(state.summary((state.phase == Phase::Initialized).then_some(false)))
}
fn data_precondition(binding: &Binding, resume: bool) -> Result<()> {
    files::check_path(&binding.data_directory, true)?;
    if !resume {
        let parent = binding.data_directory.parent().ok_or_else(precondition)?;
        files::check_path(parent, false)?;
        if !fs::symlink_metadata(parent)
            .map_err(|_| precondition())?
            .is_dir()
        {
            return Err(precondition());
        }
        match fs::read_dir(&binding.data_directory) {
            Ok(mut entries) => {
                if entries.next().is_none() {
                    Ok(())
                } else {
                    Err(fail(
                        "INSTANCE_DATA_NOT_EMPTY",
                        "init은 신규 또는 빈 데이터 폴더만 사용할 수 있습니다. 기존 데이터는 보존했습니다.",
                    ))
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(fail(
                "INSTANCE_DATA_NOT_EMPTY",
                "init은 신규 또는 빈 데이터 폴더만 사용할 수 있습니다. 기존 데이터는 보존했습니다.",
            )),
        }
    } else {
        let instance = Input::read(
            &binding.data_directory.join("engine-instance.json"),
            8192,
            false,
        )?;
        let dummy = Attempt {
            id: "resume-check".into(),
            command: "resume-init".into(),
        };
        init_result::validate_resume(&instance.raw,&expected(binding,&dummy,0)).map_err(|_|fail("INSTANCE_RESUME_NOT_ELIGIBLE","같은 identity의 INITIALIZING journal과 기존 ledger가 있어야 재개할 수 있습니다. INITIALIZED 또는 불일치 상태는 자동 조정하지 않습니다."))?;
        for name in ["engine.lock", "ledger/CURRENT"] {
            let path = binding.data_directory.join(name);
            files::check_path(&path, false)?;
            if !fs::symlink_metadata(&path)
                .map_err(|_| precondition())?
                .is_file()
            {
                return Err(precondition());
            }
        }
        instance.recheck()?;
        Ok(())
    }
}
fn expected<'a>(b: &'a Binding, a: &'a Attempt, pid: u32) -> Expected<'a> {
    Expected {
        command: &a.command,
        attempt_id: &a.id,
        pid,
        identity: &b.identity,
        backend: &b.backend,
        chain_fingerprint: &b.chain_fingerprint,
        node_identity: &b.node_identity,
        data_directory: &b.data_directory,
    }
}
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
fn attempt_id() -> Result<String> {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| corrupt())?
        .as_nanos();
    Ok(format!(
        "bxdl-{}-{time:x}-{:x}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}
fn valid_attempt(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn host() -> Result<()> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Ok(())
    } else {
        Err(fail(
            "INSTANCE_PLATFORM_UNSUPPORTED",
            "인스턴스 초기 운용은 macOS arm64에서 제공합니다.",
        ))
    }
}
fn state_error(_: BxdlError) -> BxdlError {
    fail(
        "INSTANCE_STATE_IO",
        "인스턴스 기록을 안전하게 읽거나 저장하지 못했습니다. 기존 기록을 보존했습니다.",
    )
}
fn corrupt() -> BxdlError {
    fail(
        "INSTANCE_STATE_INVALID",
        "인스턴스 등록 정보 또는 작업 기록이 유효하지 않습니다.",
    )
}
fn precondition() -> BxdlError {
    fail(
        "INSTANCE_PRECONDITION_FAILED",
        "패키지·설정·엔진 identity 또는 데이터 상태가 등록 조건과 일치하지 않습니다.",
    )
}

#[cfg(test)]
#[path = "instance_tests.rs"]
mod tests;
