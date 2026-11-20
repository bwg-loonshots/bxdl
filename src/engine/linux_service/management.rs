//! Root-admitted development executor protocol. The user manager verifies a
//! public root descriptor and invokes only the originally registered worker.
//! This is independent CLI orchestration, not a signed/frozen release profile.
use super::*;
use std::{
    ffi::OsString,
    io::{ErrorKind, IsTerminal, Read, Write},
    os::{fd::OwnedFd, unix::net::UnixStream},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub const PROTOCOL: &str = "BXDL_LINUX_DEVELOPMENT_EXECUTOR_V1";
pub const LOG_EVENT_TIMES: &str = "linux-executor.logs.event-times.v1";
const DESCRIPTOR: &str = "management.json";
const SUDO: &str = "/usr/bin/sudo";
const MAX_REPLY: usize = 262_144;
const MAX_REQUEST: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Descriptor {
    pub schema_version: u32,
    pub protocol: String,
    pub instance_id: String,
    pub control_directory: PathBuf,
    pub control_device: u64,
    pub control_inode: u64,
    pub worker_sha256: String,
    pub worker_size: u64,
}
impl Descriptor {
    fn validate(&self, control: &Path) -> Result<()> {
        service_account(control)?;
        if self.schema_version != 1
            || self.protocol != PROTOCOL
            || self.control_directory != control
            || self.control_inode == 0
            || self.instance_id.is_empty()
            || self.instance_id.len() > 128
            || !self
                .instance_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || !super::super::hash(&self.worker_sha256)
            || self.worker_size == 0
            || self.worker_size > MAX_WORKER
        {
            return Err(protocol_error());
        }
        Ok(())
    }
}
pub fn descriptor_sha256(value: &Descriptor) -> Result<String> {
    Ok(files::digest(&encoded(value)?))
}
/// Read-only and nonprivileged. Root ownership is the development trust anchor;
/// user-owned directories/links/modified workers never become privileged code.
pub fn inspect(control: &Path) -> Result<Descriptor> {
    boundary::safe_absolute(control)?;
    boundary::trusted(control)?;
    let path = control.join(DESCRIPTOR);
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| error("LINUX_EXECUTOR_DESCRIPTOR_REQUIRED"))?;
    if metadata.mode() & 0o7777 != 0o644 {
        return Err(protocol_error());
    }
    let raw = boundary::root_read(&path, 8192)?;
    let descriptor: Descriptor = json::decode(&raw).map_err(|_| protocol_error())?;
    descriptor.validate(control)?;
    let m = fs::symlink_metadata(control).map_err(|_| protocol_error())?;
    if (m.dev(), m.ino()) != (descriptor.control_device, descriptor.control_inode)
        || m.mode() & 0o7777 != 0o755
    {
        return Err(protocol_error());
    }
    let worker = control.join("worker");
    boundary::trusted(&worker)?;
    let m = fs::symlink_metadata(&worker).map_err(|_| protocol_error())?;
    if m.mode() & 0o7777 != 0o755 || m.len() != descriptor.worker_size {
        return Err(protocol_error());
    }
    files::Binary::open(&worker, &descriptor.worker_sha256)?.recheck()?;
    if boundary::root_read(&path, 8192)? != raw {
        return Err(protocol_error());
    }
    Ok(descriptor)
}
/// Publish only after root registration completed. An older LNX03 worker cannot
/// be adopted by a newer manager: publication requires these exact own bytes.
pub fn publish(control: &Path) -> Result<Descriptor> {
    boundary::root()?;
    let (b, _) = read_binding(control)?;
    let worker = control.join("worker");
    let pin = b
        .pins
        .iter()
        .find(|p| p.path == worker)
        .ok_or_else(protocol_error)?;
    let current = std::env::current_exe().map_err(|_| protocol_error())?;
    boundary::trusted(&current)?;
    let bytes = boundary::root_read(&current, MAX_WORKER)?;
    if bytes.len() as u64 != pin.size
        || files::digest(&bytes) != pin.sha256
        || boundary::root_read(&worker, MAX_WORKER)? != bytes
    {
        return Err(protocol_error());
    }
    let descriptor = Descriptor {
        schema_version: 1,
        protocol: PROTOCOL.into(),
        instance_id: b.instance_id,
        control_directory: control.into(),
        control_device: b.device,
        control_inode: b.inode,
        worker_sha256: pin.sha256.clone(),
        worker_size: pin.size,
    };
    descriptor.validate(control)?;
    let raw = encoded(&descriptor)?;
    let path = control.join(DESCRIPTOR);
    if fs::symlink_metadata(&path).is_ok() {
        if boundary::root_read(&path, 8192)? != raw {
            return Err(protocol_error());
        }
    } else {
        boundary::write_new(&path, &raw, 0, 0o644)?;
    }
    inspect(control)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServiceAction {
    Start,
    Status,
    Stop,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Request {
    Show,
    Preflight {
        timeout_seconds: u64,
    },
    Initialize {
        resume: bool,
        timeout_seconds: u64,
    },
    Service {
        service_action: ServiceAction,
        wait: bool,
        timeout_seconds: u64,
    },
    Logs {
        tail: usize,
        max_bytes: u64,
        timeout_seconds: u64,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        event_times: bool,
    },
}
impl Request {
    pub fn command(&self) -> &'static str {
        match self {
            Self::Show => "instance show",
            Self::Preflight { .. } => "preflight",
            Self::Initialize { resume: false, .. } => "init",
            Self::Initialize { resume: true, .. } => "resume-init",
            Self::Service {
                service_action: ServiceAction::Start,
                ..
            } => "start",
            Self::Service {
                service_action: ServiceAction::Status,
                ..
            } => "status",
            Self::Service {
                service_action: ServiceAction::Stop,
                ..
            } => "stop",
            Self::Logs { .. } => "logs",
        }
    }
    fn seconds(&self) -> u64 {
        match self {
            Self::Show => 10,
            Self::Preflight { timeout_seconds }
            | Self::Initialize {
                timeout_seconds, ..
            }
            | Self::Service {
                timeout_seconds, ..
            }
            | Self::Logs {
                timeout_seconds, ..
            } => *timeout_seconds,
        }
    }
    pub fn validate(&self) -> Result<()> {
        let max = match self {
            Self::Show => 10,
            Self::Preflight { .. } => 120,
            Self::Logs { .. } => 30,
            _ => 600,
        };
        if !(1..=max).contains(&self.seconds())
            || matches!(
                self,
                Self::Service {
                    service_action: ServiceAction::Start | ServiceAction::Stop,
                    wait: true,
                    ..
                }
            )
        {
            return Err(error("LINUX_EXECUTOR_OPTIONS_INVALID"));
        }
        if let Self::Logs {
            tail,
            max_bytes,
            timeout_seconds,
            event_times,
        } = self
        {
            diagnostics::Options {
                tail: *tail,
                max_bytes: *max_bytes,
                timeout: Duration::from_secs(*timeout_seconds),
                event_times: *event_times,
            }
            .validate()?;
        }
        Ok(())
    }
    fn mutating(&self) -> bool {
        matches!(
            self,
            Self::Initialize { .. }
                | Self::Service {
                    service_action: ServiceAction::Start | ServiceAction::Stop,
                    ..
                }
        )
    }
    fn supervision(&self) -> Duration {
        match self {
            Self::Initialize { .. }
            | Self::Service {
                service_action: ServiceAction::Start,
                ..
            } => Duration::from_secs(self.seconds() * 4 + 240),
            Self::Preflight { .. } => Duration::from_secs(self.seconds() * 3 + 60),
            _ => Duration::from_secs(self.seconds() + 30),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestEnvelope {
    schema_version: u32,
    protocol: String,
    descriptor_sha256: String,
    request: Request,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reply {
    pub schema_version: u32,
    pub protocol: String,
    pub descriptor_sha256: String,
    pub worker_sha256: String,
    pub request_sha256: String,
    pub command: String,
    pub outcome: String,
    pub reason_code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    pub exit: i32,
}
pub struct Response {
    pub command: String,
    pub outcome: String,
    pub reason_code: String,
    pub message: String,
    pub data: Option<Value>,
    pub exit: i32,
}
fn protocol_error() -> BxdlError {
    BxdlError::new(
        "LINUX_EXECUTOR_PROTOCOL_INVALID",
        "고정 Linux 실행기와 관리 요청의 결속을 확인하지 못했습니다. 원문 출력은 표시하지 않았습니다.",
    )
}
fn execution_unknown() -> BxdlError {
    BxdlError::new(
        "LINUX_EXECUTOR_OPERATION_UNKNOWN",
        "고정 실행기에 전달한 작업 결과를 확정하지 못했습니다. 작업은 계속될 수 있으며 자동 재시도하지 않았습니다.",
    )
}
fn unsupported() -> BxdlError {
    BxdlError::new(
        "LINUX_SERVICE_PLATFORM_UNSUPPORTED",
        "Linux GNU x86_64 시스템 서비스에서만 실행할 수 있습니다.",
    )
}
fn host() -> Result<()> {
    if cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        Ok(())
    } else {
        Err(unsupported())
    }
}
fn prepare_request(descriptor: &Descriptor, request: &Request) -> Result<Vec<u8>> {
    request.validate()?;
    encoded(&RequestEnvelope {
        schema_version: 1,
        protocol: PROTOCOL.into(),
        descriptor_sha256: descriptor_sha256(descriptor)?,
        request: request.clone(),
    })
}

/// The CLI calls this only in an explicitly interactive terminal. Authentication
/// is separate from executing a mutation; credentials are never captured by BXDL.
pub fn authenticate() -> Result<()> {
    host()?;
    if rustix::process::getuid().as_raw() == 0 && rustix::process::geteuid().as_raw() == 0 {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(auth_required());
    }
    trusted_sudo()?;
    let mut command = Command::new(SUDO);
    command
        .arg("-v")
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .current_dir("/")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut child = command.spawn().map_err(|_| auth_required())?;
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(auth_required())
                };
            }
            Err(_) => return Err(auth_required()),
            Ok(None) => (),
        };
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(auth_required());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn auth_required() -> BxdlError {
    BxdlError::new(
        "LINUX_ADMIN_AUTHORIZATION_REQUIRED",
        "이 작업에는 관리자 권한이 필요합니다. 터미널에서 sudo -v로 인증한 뒤 같은 명령을 실행하세요.",
    )
}
fn trusted_sudo() -> Result<()> {
    boundary::trusted(Path::new(SUDO))?;
    let m = fs::symlink_metadata(SUDO).map_err(|_| auth_required())?;
    if !m.is_file() || m.mode() & 0o111 == 0 {
        return Err(auth_required());
    }
    Ok(())
}
fn authorization_command() -> Command {
    let mut command = Command::new(SUDO);
    command
        .args(["-n", "-v"])
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .current_dir("/");
    command
}
// A failed check occurs before the worker request. A mutation's stderr/exit
// cannot prove it did not execute and must never trigger an authentication retry.
fn check_authorization() -> Result<()> {
    let output = transport(authorization_command(), None, Duration::from_secs(15))
        .map_err(|_| auth_required())?;
    if output.exit != 0 || !output.stdout.is_empty() {
        return Err(auth_required());
    }
    Ok(())
}
fn invocation(control: &Path, privileged: bool) -> Command {
    let worker = control.join("worker");
    let mut command = if privileged {
        Command::new(worker)
    } else {
        let mut c = Command::new(SUDO);
        c.args([
            OsString::from("-n"),
            OsString::from("--"),
            worker.into_os_string(),
        ]);
        c
    };
    command
        .arg("linux-execute")
        .arg("--control")
        .arg(control)
        .arg("--json")
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("HOME", control)
        .env("TMPDIR", "/tmp")
        .current_dir("/");
    command
}
// Older V1 peers reject unknown request and response fields. Only ask for
// event times when the pinned worker advertises them through its public version.
fn worker_has_event_times(control: &Path) -> Result<bool> {
    let mut command = Command::new(control.join("worker"));
    command
        .args(["version", "--json"])
        .env_clear()
        .current_dir("/");
    let output = transport(command, None, Duration::from_secs(5))?;
    if output.exit != 0 || !output.stderr.is_empty() {
        return Err(protocol_error());
    }
    let version: Value = json::decode(&output.stdout).map_err(|_| protocol_error())?;
    if version["schemaVersion"] != 1
        || version["command"] != "version"
        || version["outcome"] != "SUCCEEDED"
        || version["reasonCode"] != "VERSION"
        || version["data"]["product"] != "BXDL"
        || version["data"]["primaryTarget"] != "linux-amd64"
    {
        return Err(protocol_error());
    }
    let capabilities = version["data"]["capabilities"]
        .as_array()
        .ok_or_else(protocol_error)?;
    Ok(capabilities
        .iter()
        .any(|capability| capability == LOG_EVENT_TIMES))
}

/// The user manager never interprets private journals or performs engine work.
pub fn execute(control: &Path, request: &Request) -> Result<Response> {
    host()?;
    request.validate()?;
    let uid = rustix::process::getuid().as_raw();
    let euid = rustix::process::geteuid().as_raw();
    if uid != euid {
        return Err(protocol_error());
    }
    let descriptor = inspect(control)?;
    let mut effective_request = request.clone();
    if let Request::Logs { event_times, .. } = &mut effective_request {
        *event_times = *event_times && worker_has_event_times(control)?;
    }
    let request = &effective_request;
    let request_raw = prepare_request(&descriptor, request)?;
    if uid != 0 {
        trusted_sudo()?;
        check_authorization()?;
    }
    let output = transport(
        invocation(control, uid == 0),
        Some(&request_raw),
        request.supervision(),
    )
    .map_err(|e| {
        if request.mutating() && e.code != "LINUX_EXECUTOR_SPAWN_FAILED" {
            execution_unknown()
        } else {
            e
        }
    })?;
    if inspect(control).ok().as_ref() != Some(&descriptor) {
        return Err(if request.mutating() {
            execution_unknown()
        } else {
            protocol_error()
        });
    }
    let reply = parse_reply(&descriptor, request, &request_raw, &output).map_err(|_| {
        if request.mutating() {
            execution_unknown()
        } else {
            protocol_error()
        }
    })?;
    Ok(Response {
        command: request.command().into(),
        outcome: reply.outcome,
        reason_code: reply.reason_code,
        message: reply.message,
        data: reply.data,
        exit: reply.exit,
    })
}
/// Root entrypoint invoked by a verified manager. The CLI serializes this Reply
/// directly; wrapping it in another command envelope would violate the protocol.
pub fn execute_internal(control: &Path, request_raw: &[u8]) -> Result<Reply> {
    boundary::root()?;
    if request_raw.is_empty() || request_raw.len() > MAX_REQUEST {
        return Err(protocol_error());
    }
    let envelope: RequestEnvelope = json::decode(request_raw).map_err(|_| protocol_error())?;
    envelope.request.validate()?;
    let descriptor = inspect(control)?;
    if envelope.schema_version != 1
        || envelope.protocol != PROTOCOL
        || envelope.descriptor_sha256 != descriptor_sha256(&descriptor)?
    {
        return Err(protocol_error());
    }
    let current = std::env::current_exe().map_err(|_| protocol_error())?;
    if current != control.join("worker") {
        return Err(protocol_error());
    }
    let (binding, _) = read_binding(control)?;
    let worker_pin = binding
        .pins
        .iter()
        .find(|p| p.path == current)
        .ok_or_else(protocol_error)?;
    if worker_pin.sha256 != descriptor.worker_sha256
        || worker_pin.size != descriptor.worker_size
        || binding.instance_id != descriptor.instance_id
    {
        return Err(protocol_error());
    }
    let mut reply = Reply {
        schema_version: 1,
        protocol: PROTOCOL.into(),
        descriptor_sha256: envelope.descriptor_sha256,
        worker_sha256: descriptor.worker_sha256,
        request_sha256: files::digest(request_raw),
        command: envelope.request.command().into(),
        outcome: "FAILED".into(),
        reason_code: "LINUX_EXECUTOR_REQUEST_FAILED".into(),
        message: "고정 Linux 실행기 요청을 처리했습니다.".into(),
        data: None,
        exit: 3,
    };
    match dispatch_local(control, &envelope.request) {
        Ok((outcome, reason, data, exit)) => {
            reply.outcome = outcome;
            reply.reason_code = reason;
            reply.data = Some(data);
            reply.exit = exit;
        }
        Err(e) => {
            reply.outcome = if e.code.contains("UNKNOWN") || e.code.ends_with("WAIT_EXPIRED") {
                "UNKNOWN"
            } else {
                "FAILED"
            }
            .into();
            reply.exit = if reply.outcome == "UNKNOWN" { 6 } else { 3 };
            reply.reason_code = e.code;
            reply.message =
                "고정 실행기가 요청을 완료하지 못했습니다. 기록을 보존하고 사유 코드를 확인하세요."
                    .into();
        }
    }
    Ok(reply)
}
fn dispatch_local(control: &Path, request: &Request) -> Result<(String, String, Value, i32)> {
    if let Request::Logs {
        tail,
        max_bytes,
        timeout_seconds,
        event_times,
    } = request
    {
        let report = diagnostics::collect(
            control,
            &diagnostics::Options {
                tail: *tail,
                max_bytes: *max_bytes,
                timeout: Duration::from_secs(*timeout_seconds),
                event_times: *event_times,
            },
        )?;
        let partial = report.partial;
        return Ok((
            if partial { "INCOMPLETE" } else { "SUCCEEDED" }.into(),
            if partial {
                "LINUX_DIAGNOSTIC_PARTIAL"
            } else {
                "LINUX_DIAGNOSTIC_COLLECTED"
            }
            .into(),
            serde_json::to_value(report).map_err(|_| protocol_error())?,
            if partial { 5 } else { 0 },
        ));
    }
    let timeout = Duration::from_secs(request.seconds());
    let value = match request {
        Request::Show => {
            let (b, _, s) = load(control)?;
            let mut value = summary(&b, &s, boundary::busy(control)?);
            value.outcome = "SUCCEEDED".into();
            value.reason = "LINUX_INSTANCE_RECORD_READ".into();
            value
        }
        Request::Preflight { .. } => {
            baseline()?;
            let guard = boundary::operation_lock(control)?;
            let (b, _, s) = load(control)?;
            let attempt = fresh_attempt()?;
            new_attempt(&b, &attempt, false)?;
            cold(&b, &output(&b, &attempt), timeout, Some(&guard))?;
            let mut value = summary(&b, &s, false);
            value.outcome = "INCOMPLETE".into();
            value.reason = "LINUX_COLD_RUNTIME_CHECKS_REQUIRED".into();
            value
        }
        Request::Initialize { resume, .. } => initialize(control, *resume, timeout)?,
        Request::Service {
            service_action: ServiceAction::Start,
            ..
        } => start(control, timeout)?,
        Request::Service {
            service_action: ServiceAction::Stop,
            ..
        } => stop(control, timeout)?,
        Request::Service {
            service_action: ServiceAction::Status,
            wait: true,
            ..
        } => wait_ready(control, timeout)?,
        Request::Service {
            service_action: ServiceAction::Status,
            wait: false,
            ..
        } => status(control, timeout)?,
        Request::Logs { .. } => return Err(protocol_error()),
    };
    let exit = match value.outcome.as_str() {
        "SUCCEEDED" => 0,
        "INCOMPLETE" => 5,
        "FAILED" => 4,
        _ => 6,
    };
    Ok((
        value.outcome.clone(),
        value.reason.clone(),
        summary_data(value)?,
        exit,
    ))
}
// Normalize legacy local Option::None health values at the producer boundary.
// The independently versioned wire intentionally never accepts JSON null.
fn summary_data(value: Summary) -> Result<Value> {
    let data: SummaryWire =
        serde_json::from_value(serde_json::to_value(value).map_err(|_| protocol_error())?)
            .map_err(|_| protocol_error())?;
    serde_json::to_value(data).map_err(|_| protocol_error())
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SummaryWire {
    instance_id: String,
    outcome: String,
    service_state: String,
    engine_state: String,
    runtime_readiness: String,
    reason: String,
    initialization: String,
    operation_busy: bool,
    development_only: bool,
    service_profile: String,
    automatic_restart: bool,
    boot_auto_start: bool,
    runtime_package_acceptance: String,
    global_consensus: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attempt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    node_instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    health: Option<HealthWire>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HealthWire {
    observed_at_epoch_millis: String,
    running: bool,
    lifecycle_running: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    readiness: Option<super::super::runtime_result::Readiness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_status: Option<String>,
}
fn code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}
fn parse_reply(
    descriptor: &Descriptor,
    request: &Request,
    request_raw: &[u8],
    output: &TransportOutput,
) -> Result<Reply> {
    if !output.stderr.is_empty() || output.stdout.len() > MAX_REPLY {
        return Err(protocol_error());
    }
    let mut reply: Reply = json::decode(&output.stdout).map_err(|_| protocol_error())?;
    if reply.schema_version != 1
        || reply.protocol != PROTOCOL
        || reply.descriptor_sha256 != descriptor_sha256(descriptor)?
        || reply.worker_sha256 != descriptor.worker_sha256
        || reply.request_sha256 != files::digest(request_raw)
        || reply.command != request.command()
        || reply.exit != output.exit
        || !code(&reply.reason_code)
        || !matches!(
            (reply.outcome.as_str(), reply.exit),
            ("SUCCEEDED", 0) | ("INCOMPLETE", 5) | ("FAILED", 3 | 4) | ("UNKNOWN", 6)
        )
        || reply.message.len() > 512
        || reply.message.chars().any(char::is_control)
    {
        return Err(protocol_error());
    }
    if let Some(data) = reply.data.take() {
        reply.data = Some(
            if let Request::Logs {
                tail,
                max_bytes,
                timeout_seconds,
                event_times,
            } = request
            {
                let data: diagnostics::Report =
                    serde_json::from_value(data).map_err(|_| protocol_error())?;
                data.validate(&diagnostics::Options {
                    tail: *tail,
                    max_bytes: *max_bytes,
                    timeout: Duration::from_secs(*timeout_seconds),
                    event_times: *event_times,
                })?;
                if data.partial != (reply.outcome == "INCOMPLETE") {
                    return Err(protocol_error());
                }
                serde_json::to_value(data).map_err(|_| protocol_error())?
            } else {
                let data: SummaryWire =
                    serde_json::from_value(data).map_err(|_| protocol_error())?;
                if data.instance_id != descriptor.instance_id
                    || data.outcome != reply.outcome
                    || data.reason != reply.reason_code
                    || !data.development_only
                    || data.service_profile != "LINUX_DEVELOPMENT_SYSTEM_SERVICE"
                    || data.automatic_restart
                    || data.boot_auto_start
                    || data.runtime_package_acceptance != "NOT_ACCEPTED"
                    || data.global_consensus != "NOT_CHECKED"
                    || ![
                        &data.service_state,
                        &data.engine_state,
                        &data.runtime_readiness,
                        &data.initialization,
                    ]
                    .iter()
                    .all(|v| code(v))
                    || data.attempt_id.as_ref().is_some_and(|v| !valid_attempt(v))
                    || data.pid == Some(0)
                    || data.node_instance_id.as_ref().is_some_and(|v| {
                        v.len() != 36 || !v.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
                    })
                {
                    return Err(protocol_error());
                }
                if let Some(health) = &data.health {
                    if health.observed_at_epoch_millis.is_empty()
                        || health.observed_at_epoch_millis.len() > 20
                        || !health
                            .observed_at_epoch_millis
                            .bytes()
                            .all(|b| b.is_ascii_digit())
                        || health.execution_status.as_ref().is_some_and(|v| !code(v))
                    {
                        return Err(protocol_error());
                    }
                }
                serde_json::to_value(data).map_err(|_| protocol_error())?
            },
        );
    } else if !matches!(reply.outcome.as_str(), "FAILED" | "UNKNOWN") {
        return Err(protocol_error());
    }
    // Public text is ours, never copied from an unknown version's arbitrary message.
    reply.message =
        "고정 Linux 실행기의 결속된 결과입니다. 로컬 운용과 전역 합의 인수는 별도입니다.".into();
    Ok(reply)
}
struct TransportOutput {
    exit: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
fn capture(
    mut stream: UnixStream,
    done: Arc<AtomicBool>,
    overflow: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        if done.load(Ordering::Acquire) {
            return Ok(bytes);
        }
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(bytes),
            Ok(n) => {
                if bytes.len() + n > MAX_REPLY {
                    overflow.store(true, Ordering::Release);
                    return Err(ErrorKind::InvalidData.into());
                }
                bytes.extend_from_slice(&buffer[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::TimedOut | ErrorKind::WouldBlock | ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e),
        }
    }
}
fn abandon(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}
fn transport(
    mut command: Command,
    request: Option<&[u8]>,
    timeout: Duration,
) -> Result<TransportOutput> {
    if request.is_some_and(|r| r.is_empty() || r.len() > MAX_REQUEST) {
        return Err(protocol_error());
    }
    let streams = || -> std::io::Result<(UnixStream, Stdio)> {
        let (read, write) = UnixStream::pair()?;
        read.set_read_timeout(Some(Duration::from_millis(25)))?;
        Ok((read, Stdio::from(OwnedFd::from(write))))
    };
    let (out, out_child) = streams().map_err(|_| protocol_error())?;
    let (err, err_child) = streams().map_err(|_| protocol_error())?;
    command
        .stdin(if request.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(out_child)
        .stderr(err_child);
    let mut child = command
        .spawn()
        .map_err(|_| error("LINUX_EXECUTOR_SPAWN_FAILED"))?;
    drop(command);
    if let Some(request) = request {
        let write = child
            .stdin
            .take()
            .ok_or_else(execution_unknown)
            .and_then(|mut input| input.write_all(request).map_err(|_| execution_unknown()));
        if write.is_err() {
            abandon(child);
            return Err(execution_unknown());
        }
    }
    let done = Arc::new(AtomicBool::new(false));
    let exceeded = Arc::new(AtomicBool::new(false));
    let threads: Vec<_> = [out, err]
        .into_iter()
        .map(|stream| {
            let done = done.clone();
            let exceeded = exceeded.clone();
            std::thread::spawn(move || capture(stream, done, exceeded))
        })
        .collect();
    let begin = Instant::now();
    let status = loop {
        if exceeded.load(Ordering::Acquire) || begin.elapsed() >= timeout {
            abandon(child);
            break Err(execution_unknown());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => (),
            Err(_) => {
                abandon(child);
                break Err(execution_unknown());
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if status.is_ok() {
        let drain = Instant::now();
        while threads.iter().any(|t| !t.is_finished())
            && drain.elapsed() < Duration::from_millis(150)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let complete = threads.iter().all(|t| t.is_finished());
    done.store(true, Ordering::Release);
    let results: Vec<_> = threads
        .into_iter()
        .map(|t| {
            t.join()
                .map_err(|_| execution_unknown())?
                .map_err(|_| execution_unknown())
        })
        .collect();
    let status = status?;
    if !complete || exceeded.load(Ordering::Acquire) {
        return Err(execution_unknown());
    }
    let mut results = results.into_iter();
    Ok(TransportOutput {
        exit: status.code().ok_or_else(execution_unknown)?,
        stdout: results.next().ok_or_else(execution_unknown)??,
        stderr: results.next().ok_or_else(execution_unknown)??,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn descriptor() -> Descriptor {
        Descriptor {
            schema_version: 1,
            protocol: PROTOCOL.into(),
            instance_id: "node-one".into(),
            control_directory: "/var/lib/bxdl/instances/node-one".into(),
            control_device: 1,
            control_inode: 2,
            worker_sha256: "a".repeat(64),
            worker_size: 100,
        }
    }
    fn request() -> Request {
        Request::Service {
            service_action: ServiceAction::Status,
            wait: false,
            timeout_seconds: 10,
        }
    }
    #[test]
    fn descriptor_identity_is_canonical_and_includes_worker() {
        let d = descriptor();
        assert!(d.validate(&d.control_directory).is_ok());
        let mut changed = d.clone();
        changed.worker_sha256 = "b".repeat(64);
        assert_ne!(
            descriptor_sha256(&d).unwrap(),
            descriptor_sha256(&changed).unwrap()
        );
        changed = d.clone();
        changed.control_directory = "/var/lib/bxdl/../one".into();
        assert!(changed.validate(&changed.control_directory).is_err());
        changed = d;
        changed.protocol = "future".into();
        assert!(changed.validate(&changed.control_directory).is_err());
    }
    #[test]
    fn logs_request_negotiates_times_with_old_and_new_workers() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let worker = directory.path().join("worker");
        for (caps, expected) in [
            (serde_json::json!(["linux-executor.development.v1"]), false),
            (
                serde_json::json!(["linux-executor.development.v1", LOG_EVENT_TIMES]),
                true,
            ),
        ] {
            let version = serde_json::json!({"schemaVersion":1,"command":"version",
                "outcome":"SUCCEEDED","reasonCode":"VERSION","data":{
                "product":"BXDL","primaryTarget":"linux-amd64","capabilities":caps}});
            fs::write(&worker, format!("#!/bin/sh\n[ \"$1 $2\" = 'version --json' ] || exit 1\nprintf '%s\\n' '{version}'\n")).unwrap();
            fs::set_permissions(&worker, fs::Permissions::from_mode(0o700)).unwrap();
            let event_times = worker_has_event_times(directory.path()).unwrap();
            assert_eq!(event_times, expected);
            let request = Request::Logs {
                tail: 1,
                max_bytes: 4096,
                timeout_seconds: 1,
                event_times,
            };
            let wire = serde_json::to_value(&request).unwrap();
            assert_eq!(wire.get("eventTimes").is_some(), expected);
            assert_eq!(serde_json::from_value::<Request>(wire).unwrap(), request);
        }
    }
    #[test]
    fn request_whitelist_refuses_arbitrary_programs_and_unbounded_options() {
        assert!(json::decode::<Request>(br#"{"action":"SHELL","program":"/bin/sh"}"#).is_err());
        assert!(
            Request::Service {
                service_action: ServiceAction::Start,
                wait: true,
                timeout_seconds: 1
            }
            .validate()
            .is_err()
        );
        assert!(
            Request::Initialize {
                resume: false,
                timeout_seconds: 601
            }
            .validate()
            .is_err()
        );
        assert!(
            Request::Logs {
                tail: 201,
                max_bytes: 4096,
                timeout_seconds: 1,
                event_times: false,
            }
            .validate()
            .is_err()
        );
        assert!(request().validate().is_ok());
    }
    #[test]
    fn invocation_uses_only_fixed_sudo_and_original_worker() {
        let d = descriptor();
        let c = invocation(&d.control_directory, false);
        assert_eq!(c.get_program(), SUDO);
        let args: Vec<_> = c.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(
            &args[..3],
            ["-n", "--", "/var/lib/bxdl/instances/node-one/worker"]
        );
        assert_eq!(
            &args[3..],
            [
                "linux-execute",
                "--control",
                "/var/lib/bxdl/instances/node-one",
                "--json"
            ]
        );
        let c = invocation(&d.control_directory, true);
        assert_eq!(c.get_program(), "/var/lib/bxdl/instances/node-one/worker");
    }
    #[test]
    fn response_is_bound_to_request_descriptor_worker_and_exit() {
        let d = descriptor();
        let request = request();
        let raw = prepare_request(&d, &request).unwrap();
        let reply = Reply {
            schema_version: 1,
            protocol: PROTOCOL.into(),
            descriptor_sha256: descriptor_sha256(&d).unwrap(),
            worker_sha256: d.worker_sha256.clone(),
            request_sha256: files::digest(&raw),
            command: "status".into(),
            outcome: "UNKNOWN".into(),
            reason_code: "LINUX_SERVICE_UNKNOWN".into(),
            message: "safe".into(),
            data: None,
            exit: 6,
        };
        let mut output = TransportOutput {
            exit: 6,
            stdout: encoded(&reply).unwrap(),
            stderr: Vec::new(),
        };
        assert!(parse_reply(&d, &request, &raw, &output).is_ok());
        output.exit = 0;
        assert!(parse_reply(&d, &request, &raw, &output).is_err());
        output.exit = 6;
        output.stderr = b"PRIVATE_CANARY".to_vec();
        assert!(parse_reply(&d, &request, &raw, &output).is_err());
        output.stderr.clear();
        let mut changed = reply;
        changed.request_sha256 = "f".repeat(64);
        output.stdout = encoded(&changed).unwrap();
        assert!(parse_reply(&d, &request, &raw, &output).is_err());
    }
    #[test]
    fn foreign_host_never_attempts_sudo_or_worker() {
        if !cfg!(all(
            target_os = "linux",
            target_arch = "x86_64",
            target_env = "gnu"
        )) {
            assert_eq!(
                execute(Path::new("/var/lib/bxdl/absent"), &request())
                    .err()
                    .unwrap()
                    .code,
                "LINUX_SERVICE_PLATFORM_UNSUPPORTED"
            );
        }
    }
    #[test]
    fn bounded_capture_does_not_wait_on_retained_descriptor() {
        let (reader, _writer) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_millis(10)))
            .unwrap();
        assert!(
            capture(
                reader,
                Arc::new(AtomicBool::new(true)),
                Arc::new(AtomicBool::new(false))
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn auth_probe_has_no_worker_or_user_arguments() {
        let command = authorization_command();
        assert_eq!(command.get_program(), SUDO);
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["-n", "-v"]);
        assert_eq!(command.get_current_dir(), Some(Path::new("/")));
    }
    #[test]
    fn local_nullable_health_is_normalized_before_strict_wire_decoding() {
        let summary = Summary {
            instance_id: "node-one".into(),
            outcome: "UNKNOWN".into(),
            service_state: "RUNNING".into(),
            engine_state: "RUNNING".into(),
            runtime_readiness: "NOT_CHECKED".into(),
            reason: "LINUX_SERVICE_UNKNOWN".into(),
            initialization: "INITIALIZED".into(),
            operation_busy: true,
            development_only: true,
            service_profile: "LINUX_DEVELOPMENT_SYSTEM_SERVICE",
            automatic_restart: false,
            boot_auto_start: false,
            runtime_package_acceptance: "NOT_ACCEPTED",
            global_consensus: "NOT_CHECKED",
            attempt_id: None,
            pid: None,
            node_instance_id: None,
            health: Some(
                serde_json::json!({"observedAtEpochMillis":"123","running":true,"lifecycleRunning":true,"readiness":null,"executionStatus":null}),
            ),
        };
        let data = summary_data(summary).unwrap();
        let raw = encoded(&data).unwrap();
        assert!(!String::from_utf8(raw.clone()).unwrap().contains("null"));
        let _: SummaryWire = json::decode(&raw).unwrap();
        let descriptor = descriptor();
        let request = request();
        let input = prepare_request(&descriptor, &request).unwrap();
        let reply = Reply {
            schema_version: 1,
            protocol: PROTOCOL.into(),
            descriptor_sha256: descriptor_sha256(&descriptor).unwrap(),
            worker_sha256: descriptor.worker_sha256.clone(),
            request_sha256: files::digest(&input),
            command: request.command().into(),
            outcome: "UNKNOWN".into(),
            reason_code: "LINUX_SERVICE_UNKNOWN".into(),
            message: "safe".into(),
            data: Some(data),
            exit: 6,
        };
        assert!(
            parse_reply(
                &descriptor,
                &request,
                &input,
                &TransportOutput {
                    exit: 6,
                    stdout: encoded(&reply).unwrap(),
                    stderr: vec![]
                }
            )
            .is_ok()
        );
    }
    #[test]
    fn transport_sends_only_bounded_stdin_and_captures_output() {
        let output = transport(
            Command::new("/bin/cat"),
            Some(b"bound request"),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(output.exit, 0);
        assert_eq!(output.stdout, b"bound request");
        assert!(output.stderr.is_empty());
        assert!(
            transport(
                Command::new("/bin/cat"),
                Some(&vec![0; MAX_REQUEST + 1]),
                Duration::from_secs(1)
            )
            .is_err()
        );
    }
    #[test]
    fn timeout_does_not_kill_an_inflight_operation_or_wait_for_its_exit() {
        let temporary = tempfile::tempdir().unwrap();
        let marker = temporary.path().join("finished");
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "sleep 0.2; printf finished > \"$1\"", "fixture"])
            .arg(&marker);
        let started = Instant::now();
        assert!(transport(command, None, Duration::from_millis(20)).is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if fs::read(&marker).is_ok_and(|raw| raw == b"finished") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "operation did not finish after supervisor timeout"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn output_overflow_is_rejected_without_returning_partial_raw_text() {
        let mut command = Command::new("/usr/bin/head");
        command.args(["-c", &(MAX_REPLY + 1).to_string(), "/dev/zero"]);
        assert!(transport(command, None, Duration::from_secs(2)).is_err());
    }
}
