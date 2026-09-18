//! Explicit, single-use LaunchAgent attempts. The service entry point repeats
//! the gate and execs Java while retaining the inherited instance flock.
use super::super::{
    launchd::{Job, Observation},
    runtime_http, runtime_result,
};
use super::*;
use std::{
    net::SocketAddr,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    process::{Command, Stdio},
    thread,
    time::Instant,
};

const SERVICE_JOURNAL: &str = "service-journal";
const MAX_CLI: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RuntimePhase {
    StartPrepared,
    GateConsumed,
    GateRefused,
    StoppedVerified,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Runtime {
    schema_version: u32,
    instance_id: String,
    control_directory: PathBuf,
    binding_sha256: String,
    attempt_id: String,
    phase: RuntimePhase,
    uid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    worker_sha256: String,
    node_sha256: String,
    chain_sha256: String,
    endpoint: SocketAddr,
    reason: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StopRequest {
    attempt_id: String,
    uid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    worker_sha256: String,
}
impl Runtime {
    fn save(&self, store: &mut Store) -> Result<()> {
        store
            .save(&serde_json::to_vec(self).map_err(|_| invalid())?)
            .map_err(|_| unknown())
    }
    fn workspace(&self, root: &Control) -> Result<Workspace> {
        Workspace::persistent(&root.path().join("operations").join(&self.attempt_id))
    }
    fn job(&self, root: &Control) -> Result<Job> {
        let workspace = self.workspace(root)?;
        Job::new(
            self.uid,
            root.path(),
            &self.attempt_id,
            &workspace.path.join("bxdl-worker"),
            &workspace.path,
        )
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSummary {
    pub instance_id: String,
    pub outcome: String,
    pub service_state: String,
    pub engine_state: String,
    pub runtime_readiness: String,
    pub reason: String,
    pub operation_busy: bool,
    pub development_only: bool,
    pub login_auto_start: bool,
    pub automatic_restart: bool,
    pub service_profile: &'static str,
    pub global_consensus: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<serde_json::Value>,
}
fn summary(binding: &Binding, runtime: Option<&Runtime>) -> ServiceSummary {
    ServiceSummary {
        instance_id: binding.instance_id.clone(),
        outcome: "UNKNOWN".into(),
        service_state: "UNKNOWN".into(),
        engine_state: "UNKNOWN".into(),
        runtime_readiness: "UNKNOWN".into(),
        reason: "SERVICE_OBSERVATION_UNKNOWN".into(),
        operation_busy: false,
        development_only: true,
        login_auto_start: false,
        automatic_restart: false,
        service_profile: "MACOS_USER_LAUNCH_AGENT",
        global_consensus: "NOT_CHECKED",
        attempt_id: runtime.map(|s| s.attempt_id.clone()),
        pid: runtime.and_then(|s| s.pid),
        node_instance_id: None,
        health: None,
    }
}
fn uid() -> Result<u32> {
    let uid = rustix::process::getuid().as_raw();
    if uid == 0 || uid != rustix::process::geteuid().as_raw() {
        return Err(fail(
            "SERVICE_USER_REQUIRED",
            "LaunchAgent는 로그인한 일반 사용자 권한으로 실행하세요.",
        ));
    }
    Ok(uid)
}
fn load_runtime(root: &Control, state: &State) -> Result<Option<(Store, Runtime)>> {
    let path = root.path().join(SERVICE_JOURNAL);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid()),
        Ok(_) => (),
    }
    let store = Store::open(&path).map_err(|_| invalid())?;
    let raw = store.read().map_err(|_| invalid())?.ok_or_else(invalid)?;
    let runtime: Runtime = json::decode(&raw).map_err(|_| invalid())?;
    if runtime.schema_version != 1
        || runtime.instance_id != state.instance_id
        || runtime.control_directory != root.path()
        || runtime.binding_sha256 != state.binding_sha256
        || !valid_attempt(&runtime.attempt_id)
        || runtime.uid != uid()?
        || !super::super::hash(&runtime.worker_sha256)
        || !super::super::hash(&runtime.node_sha256)
        || !super::super::hash(&runtime.chain_sha256)
        || !runtime.endpoint.ip().is_loopback()
        || runtime.endpoint.port() == 0
        || (runtime.phase == RuntimePhase::StartPrepared) != runtime.pid.is_none()
        || runtime.pid == Some(0)
    {
        return Err(invalid());
    }
    Ok(Some((store, runtime)))
}
fn expected<'a>(
    binding: &'a Binding,
    state: &'a State,
    runtime: &'a Runtime,
) -> Result<runtime_result::Expected<'a>> {
    Ok(runtime_result::Expected {
        attempt_id: &runtime.attempt_id,
        pid: runtime.pid.unwrap_or(0),
        identity: &binding.identity,
        backend: &binding.backend,
        chain_fingerprint: &binding.chain_fingerprint,
        node_identity: &binding.node_identity,
        data_directory: &binding.data_directory,
        genesis_hash: state.genesis_hash.as_deref().ok_or_else(precondition)?,
    })
}
fn initialized(binding: &Binding, state: &State, runtime: &Runtime) -> Result<Input> {
    if state.phase != Phase::Initialized {
        return Err(not_initialized());
    }
    let journal = Input::read(
        &binding.data_directory.join("engine-instance.json"),
        8192,
        false,
    )?;
    runtime_result::validate_initialized(&journal.raw, &expected(binding, state, runtime)?)?;
    for name in ["engine.lock", "ledger/CURRENT"] {
        let path = binding.data_directory.join(name);
        files::check_path(&path, false)?;
        if !fs::symlink_metadata(path)
            .map_err(|_| precondition())?
            .is_file()
        {
            return Err(precondition());
        }
    }
    Ok(journal)
}
fn busy(root: &Control) -> Result<bool> {
    match root.lock() {
        Ok(_) => Ok(false),
        Err(e) if e.code == "INSTANCE_BUSY" => Ok(true),
        Err(e) => Err(e),
    }
}
fn endpoint(native: &NativeInput) -> Result<SocketAddr> {
    let text = |name: &str| -> Result<String> {
        let v = native.value.node.get(name).ok_or_else(precondition)?;
        if let Some(s) = v.as_str() {
            Ok(s.into())
        } else if v.is_number() {
            Ok(v.to_string())
        } else {
            Err(precondition())
        }
    };
    let address = text("server.address")?
        .parse()
        .map_err(|_| precondition())?;
    let port = text("server.port")?.parse().map_err(|_| precondition())?;
    let address = SocketAddr::new(address, port);
    if !address.ip().is_loopback() || port == 0 {
        return Err(precondition());
    }
    Ok(address)
}

pub fn start(instance: &Path, timeout: Duration) -> Result<ServiceSummary> {
    host()?;
    check_timeout(timeout, 600)?;
    let root = Control::open(instance)?;
    let guard = root.lock()?;
    let (binding, binding_file, _journal, state) = load(&root)?;
    if state.phase != Phase::Initialized {
        return Err(not_initialized());
    }
    let mut runtime_store = match load_runtime(&root, &state)? {
        Some((store, old)) => {
            if old.phase != RuntimePhase::StoppedVerified {
                return Err(fail(
                    "SERVICE_ATTEMPT_UNRESOLVED",
                    "기존 실행의 종료가 확정되지 않았습니다. status와 명시 stop으로 확인하세요. 자동 재시작하지 않았습니다.",
                ));
            }
            let job = old.job(&root)?;
            match job.inspect(Duration::from_secs(5))? {
                Observation::NotLoaded => (),
                Observation::Loaded(observed) if observed.is_stopped() => {
                    job.bootout_stopped(Duration::from_secs(5))?
                }
                _ => return Err(unknown()),
            }
            Some(store)
        }
        None => None,
    };
    let inputs = binding.check_pins()?;
    let package = verified_package(&binding, true)?;
    bound_preflight(&binding, timeout)?;
    let native = NativeInput::load(&binding.native)?;
    let product = ProductInput::load(&binding.product)?;
    product.check(&native)?;
    // A future service entry point must be the exact CLI included in this
    // verified package, not an unrelated binary currently found on PATH.
    let cli = Input::read(&binding.package.join("bin/bxdl"), MAX_CLI, true)?;
    let current = Input::read(
        &std::env::current_exe().map_err(|_| precondition())?,
        MAX_CLI,
        true,
    )?;
    if cli.raw != current.raw {
        return Err(fail(
            "SERVICE_CLI_MISMATCH",
            "검증된 설치 패키지에 포함된 같은 버전의 BXDL 실행 파일로 start를 실행하세요.",
        ));
    }
    let mut runtime = Runtime {
        schema_version: 1,
        instance_id: binding.instance_id.clone(),
        control_directory: root.path().to_owned(),
        binding_sha256: state.binding_sha256.clone(),
        attempt_id: attempt_id()?,
        phase: RuntimePhase::StartPrepared,
        uid: uid()?,
        pid: None,
        worker_sha256: files::digest(&cli.raw),
        node_sha256: String::new(),
        chain_sha256: String::new(),
        endpoint: endpoint(&native)?,
        reason: "START_REQUEST_RECORDED".into(),
    };
    let engine_journal = initialized(&binding, &state, &runtime)?;
    let workspace = Workspace::persistent(&root.attempt(&runtime.attempt_id)?)?;
    let worker = workspace.write("bxdl-worker", &cli.raw)?;
    fs::set_permissions(&worker, fs::Permissions::from_mode(0o700)).map_err(|_| unknown())?;
    fs::File::open(&worker)
        .and_then(|f| f.sync_all())
        .map_err(|_| unknown())?;
    let lock = read_lock(&Input::read(&binding.lock, 65_536, false)?)?;
    workspace.snapshot_jar(
        &binding.options(timeout).jar,
        &lock.jar_sha256,
        lock.jar_size_bytes,
    )?;
    let config = native.snapshot(&workspace)?;
    runtime.node_sha256 = files::digest(&Input::read(&config, 262_144, false)?.raw);
    runtime.chain_sha256 =
        files::digest(&Input::read(&workspace.path.join("chain.json"), 262_144, false)?.raw);
    let job = runtime.job(&root)?;
    // Verify a GUI domain before publishing the durable start request.
    match job.inspect(Duration::from_secs(5))? {
        Observation::NotLoaded => (),
        Observation::GuiUnavailable => {
            return Err(fail(
                "SERVICE_GUI_SESSION_REQUIRED",
                "이 사용자의 Mac GUI 로그인 세션이 필요합니다. LaunchDaemon은 아직 제공하지 않습니다.",
            ));
        }
        _ => return Err(unknown()),
    }
    job.prepare()?;
    for input in &inputs {
        input.recheck()?;
    }
    native.recheck()?;
    product.recheck()?;
    binding_file.recheck()?;
    cli.recheck()?;
    current.recheck()?;
    engine_journal.recheck()?;
    root.recheck()?;
    install::verify_installed(&binding.package, &package)?;
    if runtime_store.is_none() {
        runtime_store =
            Some(Store::create(&root.path().join(SERVICE_JOURNAL)).map_err(|_| unknown())?);
    }
    let mut runtime_store = runtime_store.ok_or_else(invalid)?;
    runtime_store.require_capacity(4).map_err(|_| unknown())?;
    runtime.save(&mut runtime_store)?;
    // launchd is not our child. Its worker acquires a NEW lock, then passes
    // that lock to Java. Never hold our copy while bootstrapping the job.
    drop(guard);
    job.bootstrap(Duration::from_secs(10))?;
    wait_start(instance, &runtime.attempt_id, timeout)
}

/// Internal launchd entry point. Every entry consumes its durable attempt
/// before engine checks; a direct kickstart cannot replay it after any exit.
pub fn run_gate(instance: &Path, attempt: &str) -> Result<()> {
    host()?;
    let root = Control::open(instance)?;
    let guard = root.lock()?;
    let (binding, binding_file, _journal, state) = load(&root)?;
    let (mut store, mut runtime) = load_runtime(&root, &state)?.ok_or_else(invalid)?;
    if runtime.attempt_id != attempt || runtime.phase != RuntimePhase::StartPrepared {
        return Err(fail(
            "SERVICE_GATE_REPLAY_REJECTED",
            "이미 소비했거나 일치하지 않는 시작 시도입니다. 같은 LaunchAgent 작업을 다시 실행하지 않습니다.",
        ));
    }
    let job = runtime.job(&root)?;
    let self_file = Input::read(
        &std::env::current_exe().map_err(|_| invalid())?,
        MAX_CLI,
        true,
    )?;
    if self_file.path != job.program || files::digest(&self_file.raw) != runtime.worker_sha256 {
        return Err(invalid());
    }
    match job.inspect(Duration::from_secs(5))? {
        Observation::Loaded(observed) if observed.pid == Some(std::process::id()) => (),
        _ => {
            return Err(fail(
                "SERVICE_GATE_OWNER_MISMATCH",
                "정확한 LaunchAgent 실행 소유권을 확인하지 못했습니다.",
            ));
        }
    }
    runtime.phase = RuntimePhase::GateConsumed;
    runtime.pid = Some(std::process::id());
    runtime.reason = "START_GATE_CONSUMED".into();
    runtime.save(&mut store)?;
    let operation = (|| -> Result<()> {
        let inputs = binding.check_pins()?;
        let package = verified_package(&binding, true)?;
        bound_preflight(&binding, Duration::from_secs(120))?;
        let native = NativeInput::load(&binding.native)?;
        let product = ProductInput::load(&binding.product)?;
        product.check(&native)?;
        let engine_journal = initialized(&binding, &state, &runtime)?;
        let workspace = runtime.workspace(&root)?;
        let config = checked_snapshot(
            &workspace.path.join("node.json"),
            &runtime.node_sha256,
            262_144,
        )?;
        let chain = checked_snapshot(
            &workspace.path.join("chain.json"),
            &runtime.chain_sha256,
            262_144,
        )?;
        let lock = read_lock(&Input::read(&binding.lock, 65_536, false)?)?;
        // Rehash the staged JAR without allocating its full size in memory.
        let jar = files::Binary::open_data(&workspace.path.join("engine.jar"), &lock.jar_sha256)?;
        let java = files::Binary::open(
            &binding.options(Duration::from_secs(120)).java,
            &lock.java_sha256,
        )?;
        for input in &inputs {
            input.recheck()?;
        }
        native.recheck()?;
        product.recheck()?;
        binding_file.recheck()?;
        self_file.recheck()?;
        engine_journal.recheck()?;
        root.recheck()?;
        workspace.recheck()?;
        install::verify_installed(&binding.package, &package)?;
        config.recheck()?;
        chain.recheck()?;
        java.recheck()?;
        jar.recheck()?;
        if fs::symlink_metadata(workspace.path.join("stop-request.json")).is_ok() {
            return Err(fail(
                "SERVICE_START_CANCELLED",
                "시작 도중 명시 종료 요청이 있어 엔진을 실행하지 않았습니다.",
            ));
        }
        if fs::symlink_metadata(workspace.path.join("report.jsonl")).is_ok() {
            return Err(invalid());
        }
        let child_lock = guard.child_stdin()?;
        let mut command = Command::new(&java.path);
        command
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("HOME", &workspace.path)
            .env("TMPDIR", &workspace.path)
            .current_dir(&workspace.path)
            .stdin(Stdio::from(child_lock))
            .arg(format!("-Djava.io.tmpdir={}", workspace.path.display()))
            .arg("-jar")
            .arg(&jar.path)
            .arg("run")
            .arg(format!("--config={}", config.path.display()))
            .arg(format!(
                "--report={}",
                workspace.path.join("report.jsonl").display()
            ))
            .arg(format!("--attempt-id={}", runtime.attempt_id));
        // Successful exec keeps the launchd PID and stdin's flock. No shell,
        // supervisor PID guessing or destructor-based unlock is involved.
        let _error = command.exec();
        Err(fail(
            "SERVICE_EXEC_FAILED",
            "엔진 실행 전환에 실패했습니다. 시작 시도와 자료를 보존했습니다.",
        ))
    })();
    if let Err(error) = operation {
        runtime.phase = RuntimePhase::GateRefused;
        runtime.reason = error.code.clone();
        runtime.save(&mut store)?;
        return Err(error);
    }
    Err(unknown())
}
fn checked_snapshot(path: &Path, hash: &str, limit: u64) -> Result<Input> {
    let input = Input::read(path, limit, false)?;
    if files::digest(&input.raw) != hash {
        return Err(invalid());
    }
    Ok(input)
}
fn report_for(
    root: &Control,
    binding: &Binding,
    state: &State,
    runtime: &Runtime,
) -> Result<runtime_result::RunReport> {
    let report = Input::read(
        &runtime.workspace(root)?.path.join("report.jsonl"),
        65_536,
        false,
    )?;
    let journal = Input::read(
        &binding.data_directory.join("engine-instance.json"),
        8192,
        false,
    )?;
    let result = runtime_result::validate_report(
        &report.raw,
        &journal.raw,
        &expected(binding, state, runtime)?,
    )?;
    report.recheck()?;
    journal.recheck()?;
    Ok(result)
}

pub fn status(instance: &Path, timeout: Duration) -> Result<ServiceSummary> {
    host()?;
    check_timeout(timeout, 120)?;
    let deadline = Instant::now() + timeout;
    let root = Control::open(instance)?;
    let (binding, _file, _journal, state) = load(&root)?;
    let runtime = load_runtime(&root, &state)?;
    let mut result = summary(&binding, runtime.as_ref().map(|(_, r)| r));
    result.operation_busy = busy(&root)?;
    let Some((_store, runtime)) = runtime else {
        result.outcome = "INCOMPLETE".into();
        result.service_state = "NOT_REGISTERED".into();
        result.engine_state = "NOT_STARTED".into();
        result.runtime_readiness = "NOT_CHECKED".into();
        result.reason = "SERVICE_NOT_STARTED".into();
        return Ok(result);
    };
    let job = runtime.job(&root)?;
    let observation = match job.inspect(remaining(deadline)?.min(Duration::from_secs(5))) {
        Ok(value) => value,
        Err(_) => return Ok(result),
    };
    let live_pid = match observation {
        Observation::GuiUnavailable => {
            result.service_state = "GUI_UNAVAILABLE".into();
            return Ok(result);
        }
        Observation::NotLoaded => {
            result.service_state = "NOT_LOADED".into();
            None
        }
        Observation::Loaded(ref observed) => {
            result.service_state = if observed.pid.is_some() {
                "RUNNING"
            } else if observed.is_stopped() {
                "EXITED"
            } else {
                "PENDING"
            }
            .into();
            observed.pid
        }
    };
    if runtime.phase == RuntimePhase::GateRefused {
        result.engine_state = "GATE_REFUSED".into();
        result.reason = "SERVICE_GATE_REFUSED".into();
        result.outcome = "FAILED".into();
        return Ok(result);
    }
    if runtime.phase == RuntimePhase::StartPrepared {
        if matches!(&observation, Observation::Loaded(observed) if observed.is_stopped()) {
            result.engine_state = "START_NOT_CONFIRMED".into();
            result.reason = "SERVICE_LAUNCH_FAILED".into();
            result.outcome = "FAILED".into();
            return Ok(result);
        }
        result.engine_state = "START_PENDING".into();
        result.reason = "SERVICE_START_PENDING".into();
        return Ok(result);
    }
    if live_pid.is_some() && live_pid != runtime.pid {
        result.reason = "SERVICE_PID_MISMATCH".into();
        return Ok(result);
    }
    let report = match report_for(&root, &binding, &state, &runtime) {
        Ok(value) => value,
        Err(_) => {
            result.reason = "SERVICE_REPORT_UNKNOWN".into();
            return Ok(result);
        }
    };
    result.engine_state = report.status.clone();
    result.reason = report.reason.clone();
    result.node_instance_id = report.node_instance_id.clone();
    if report.status == "STOPPED" && live_pid.is_none() && !result.operation_busy {
        result.outcome = "SUCCEEDED".into();
        result.runtime_readiness = "STOPPED".into();
        result.reason = if runtime.phase == RuntimePhase::StoppedVerified {
            "SERVICE_STOPPED_VERIFIED"
        } else {
            "SERVICE_STOPPED_REQUIRES_CONFIRMATION"
        }
        .into();
        return Ok(result);
    }
    if report.status == "FAILED" {
        result.outcome = "FAILED".into();
        return Ok(result);
    }
    if report.status != "RUNNING"
        || live_pid != runtime.pid
        || live_pid.is_none()
        || !result.operation_busy
    {
        return Ok(result);
    }
    let Some(node_instance_id) = report.node_instance_id.as_deref() else {
        return Ok(result);
    };
    let expected = expected(&binding, &state, &runtime)?;
    let health_result = (|| -> Result<runtime_result::HealthReport> {
        let earliest = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| unknown())?
            .as_millis();
        let first = runtime_http::get(
            runtime.endpoint,
            "/monitor/api/console/bootstrap",
            remaining(deadline)?.min(Duration::from_secs(2)),
        )?;
        runtime_result::validate_bootstrap(&first.body, &first.media, &expected, node_instance_id)?;
        let health = runtime_http::get(
            runtime.endpoint,
            "/monitor/api/consensus/health",
            remaining(deadline)?.min(Duration::from_secs(2)),
        )?;
        let health = runtime_result::validate_health(&health.body, &health.media)?;
        validate_health_role(&health, &binding.node_identity)?;
        let observed = health
            .observed_at_epoch_millis
            .parse::<u128>()
            .map_err(|_| unknown())?;
        let latest = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| unknown())?
            .as_millis();
        if observed < earliest.saturating_sub(5000) || observed > latest.saturating_add(5000) {
            return Err(unknown());
        }
        let last = runtime_http::get(
            runtime.endpoint,
            "/monitor/api/console/bootstrap",
            remaining(deadline)?.min(Duration::from_secs(2)),
        )?;
        runtime_result::validate_bootstrap(&last.body, &last.media, &expected, node_instance_id)?;
        match job.inspect(remaining(deadline)?.min(Duration::from_secs(2)))? {
            Observation::Loaded(observed) if observed.pid == runtime.pid => (),
            _ => return Err(unknown()),
        }
        if !busy(&root)? || report_for(&root, &binding, &state, &runtime)?.status != "RUNNING" {
            return Err(unknown());
        }
        root.recheck()?;
        Ok(health)
    })();
    match health_result {
        Ok(health) => {
            if let Some(readiness) = &health.readiness {
                result.runtime_readiness = readiness.status.clone();
                result.reason = readiness.reason.clone();
                result.outcome = match readiness.status.as_str() {
                    "READY" => "SUCCEEDED",
                    "NOT_READY" => "INCOMPLETE",
                    "FAILED" => "FAILED",
                    _ => "UNKNOWN",
                }
                .into();
            }
            result.health = Some(serde_json::to_value(health).map_err(|_| invalid())?);
        }
        Err(_) => result.reason = "SERVICE_HTTP_IDENTITY_UNKNOWN".into(),
    }
    Ok(result)
}
fn wait_start(instance: &Path, attempt: &str, timeout: Duration) -> Result<ServiceSummary> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let mut result = status(
            instance,
            remaining
                .max(Duration::from_millis(1))
                .min(Duration::from_secs(8)),
        )?;
        if result.attempt_id.as_deref() != Some(attempt) {
            return Err(unknown());
        }
        if result.engine_state == "STOPPED" {
            result.outcome = "INCOMPLETE".into();
            result.reason = "SERVICE_EXITED_DURING_START".into();
            return Ok(result);
        }
        if result.runtime_readiness == "READY" || result.outcome == "FAILED" {
            return Ok(result);
        }
        if Instant::now() >= deadline {
            if result.outcome == "UNKNOWN" {
                result.reason = "SERVICE_START_WAIT_EXPIRED".into();
            }
            return Ok(result);
        }
        thread::sleep(Duration::from_millis(150));
    }
}

pub fn stop(instance: &Path, timeout: Duration) -> Result<ServiceSummary> {
    host()?;
    check_timeout(timeout, 600)?;
    let root = Control::open(instance)?;
    let (binding, binding_file, _journal, state) = load(&root)?;
    let Some((_store, mut runtime)) = load_runtime(&root, &state)? else {
        return status(instance, Duration::from_secs(5));
    };
    let job = runtime.job(&root)?;
    let workspace = runtime.workspace(&root)?;
    let request = StopRequest {
        attempt_id: runtime.attempt_id.clone(),
        uid: runtime.uid,
        pid: runtime.pid,
        worker_sha256: runtime.worker_sha256.clone(),
    };
    let request_path = workspace.path.join("stop-request.json");
    match fs::symlink_metadata(&request_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            store::write_new(
                &request_path,
                &serde_json::to_vec(&request).map_err(|_| invalid())?,
            )
            .map_err(|_| unknown())?;
        }
        Ok(_) => {
            // A request only proves intent, never signal delivery. A later
            // explicit stop may re-observe and send TERM to the same attempt.
            let existing: StopRequest = json::decode(&Input::read(&request_path, 4096, false)?.raw)
                .map_err(|_| invalid())?;
            if existing.attempt_id != request.attempt_id
                || existing.uid != request.uid
                || existing.worker_sha256 != request.worker_sha256
                || (existing.pid.is_some() && existing.pid != request.pid)
            {
                return Err(invalid());
            }
        }
        Err(_) => return Err(unknown()),
    }
    let deadline = Instant::now() + timeout;
    let mut signalled = false;
    loop {
        let (_, current) = load_runtime(&root, &state)?.ok_or_else(invalid)?;
        if current.attempt_id != runtime.attempt_id {
            return Err(unknown());
        }
        runtime = current;
        let observation = job.inspect(remaining(deadline)?.min(Duration::from_secs(3)))?;
        let report = report_for(&root, &binding, &state, &runtime).ok();
        match observation {
            Observation::Loaded(ref observed) if observed.pid.is_some() => {
                if observed.pid != runtime.pid {
                    return Err(unknown());
                }
                if !signalled
                    && report.as_ref().is_some_and(|r| {
                        matches!(r.status.as_str(), "CHECKING" | "STARTING" | "RUNNING")
                    })
                {
                    job.terminate(
                        runtime.pid.ok_or_else(unknown)?,
                        remaining(deadline)?.min(Duration::from_secs(5)),
                    )?;
                    signalled = true;
                }
            }
            Observation::GuiUnavailable => return Err(unknown()),
            Observation::NotLoaded | Observation::Loaded(_) => {
                if report.as_ref().is_some_and(|r| r.status == "STOPPED") {
                    return confirm_stopped(
                        &root,
                        &binding,
                        &binding_file,
                        &state,
                        &runtime,
                        &job,
                        deadline,
                    );
                }
                // A gate refusal or missing report is preserved, not converted
                // into successful storage closure merely because PID is gone.
                return status(instance, remaining(deadline)?.min(Duration::from_secs(5)));
            }
        }
        if Instant::now() >= deadline {
            let mut result = summary(&binding, Some(&runtime));
            result.operation_busy = busy(&root)?;
            result.outcome = "UNKNOWN".into();
            result.reason = "SERVICE_STOP_WAIT_EXPIRED".into();
            return Ok(result);
        }
        thread::sleep(Duration::from_millis(150));
    }
}
fn confirm_stopped(
    root: &Control,
    binding: &Binding,
    file: &Input,
    state: &State,
    runtime: &Runtime,
    job: &Job,
    deadline: Instant,
) -> Result<ServiceSummary> {
    let _guard = root.lock().map_err(|_| unknown())?;
    let (mut store, mut current) = load_runtime(root, state)?.ok_or_else(invalid)?;
    if current.attempt_id != runtime.attempt_id
        || current.pid != runtime.pid
        || current.phase == RuntimePhase::StartPrepared
    {
        return Err(unknown());
    }
    match job.inspect(remaining(deadline)?.min(Duration::from_secs(3)))? {
        Observation::NotLoaded => (),
        Observation::Loaded(observed) if observed.is_stopped() => (),
        _ => return Err(unknown()),
    }
    if report_for(root, binding, state, &current)?.status != "STOPPED" {
        return Err(unknown());
    }
    file.recheck()?;
    root.recheck()?;
    if current.phase != RuntimePhase::StoppedVerified {
        current.phase = RuntimePhase::StoppedVerified;
        current.reason = "SERVICE_STOPPED_VERIFIED".into();
        current.save(&mut store)?;
    }
    // No bootout on a live or uncertain process. A removal failure retains
    // the verified stop journal and can be retried explicitly.
    job.bootout_stopped(remaining(deadline)?.min(Duration::from_secs(5)))?;
    let mut result = summary(binding, Some(&current));
    result.outcome = "SUCCEEDED".into();
    result.service_state = "NOT_LOADED".into();
    result.engine_state = "STOPPED".into();
    result.runtime_readiness = "STOPPED".into();
    result.reason = "SERVICE_STOPPED_VERIFIED".into();
    Ok(result)
}
fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(unknown)
}
fn validate_health_role(health: &runtime_result::HealthReport, node_identity: &str) -> Result<()> {
    let role = if node_identity == "INSTANT" {
        "INSTANT"
    } else if node_identity.ends_with(":OBSERVER") {
        "OBSERVER"
    } else {
        "VALIDATOR"
    };
    if health.readiness.as_ref().is_some_and(|r| r.role != role) {
        return Err(unknown());
    }
    Ok(())
}
fn check_timeout(timeout: Duration, max: u64) -> Result<()> {
    if timeout.is_zero() || timeout > Duration::from_secs(max) {
        Err(invalid())
    } else {
        Ok(())
    }
}
fn invalid() -> BxdlError {
    fail(
        "SERVICE_STATE_INVALID",
        "서비스 제어 기록·실행 자료가 유효하지 않습니다. 기존 시도와 데이터를 보존했습니다.",
    )
}
fn unknown() -> BxdlError {
    fail(
        "SERVICE_OPERATION_UNKNOWN",
        "서비스 실행·종료를 확정하지 못했습니다. status로 확인하세요. 강제 종료·자동 재시작은 하지 않았습니다.",
    )
}
fn not_initialized() -> BxdlError {
    fail(
        "SERVICE_INITIALIZATION_REQUIRED",
        "BXDL에서 초기화 완료가 확인된 인스턴스만 시작할 수 있습니다. 먼저 instance show를 확인하세요.",
    )
}

#[cfg(test)]
mod tests;
