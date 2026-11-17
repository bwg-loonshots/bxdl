//! Read-only Linux host observations and separately guarded system-unit control.
//! `inspect_host` never installs or starts a unit; mutations live in `control`.
//!
//! Host-baseline success is not Linux package, node, or service acceptance.
//! Unit metadata must be combined with process/cgroup identity and the engine's
//! authenticated STOPPED report before a controller can confirm shutdown.
//! Property names follow systemctl(1) and org.freedesktop.systemd1(5).
pub mod control;

use crate::error::{BxdlError, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{ErrorKind, Read},
    os::{fd::OwnedFd, unix::net::UnixStream},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 65_536;
const SYSTEMCTL: &str = "/usr/bin/systemctl";
const MIN_SYSTEMD: u32 = 255;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Distribution {
    pub id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagerState {
    pub version: String,
    pub major: u32,
    pub system_state: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostReport {
    pub outcome: &'static str,
    pub reason_code: &'static str,
    pub scope: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
    pub distribution: Option<Distribution>,
    pub kernel_release: Option<String>,
    pub wsl: bool,
    pub pid1_systemd: bool,
    pub manager: Option<ManagerState>,
    pub linux_service_control: &'static str,
    pub engine_acceptance: &'static str,
    pub service_mutations: u32,
}

impl HostReport {
    fn initial() -> Self {
        Self {
            outcome: "INCOMPLETE",
            reason_code: "HOST_PLATFORM_UNSUPPORTED",
            scope: "READ_ONLY_HOST_BASELINE",
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            distribution: None,
            kernel_release: None,
            wsl: false,
            pid1_systemd: false,
            manager: None,
            linux_service_control: "DEVELOPMENT_BACKEND_IMPLEMENTED",
            engine_acceptance: "NOT_RUN",
            service_mutations: 0,
        }
    }
}

/// Check only the selected Ubuntu/systemd host baseline. This does not request
/// sudo, write configuration, contact nodes, or inspect an engine installation.
/// WSL is explicitly deferred rather than counted as a native Linux acceptance.
pub fn inspect_host(timeout: Duration) -> Result<HostReport> {
    validate_timeout(timeout)?;
    let mut report = HostReport::initial();
    if !cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        return Ok(report);
    }
    let kernel = read_text(Path::new("/proc/sys/kernel/osrelease"))?;
    let kernel = kernel.trim_end_matches('\n');
    if !safe_value(kernel, 256) {
        return Err(observation_error());
    }
    report.kernel_release = Some(kernel.into());
    report.wsl = is_wsl_kernel(kernel);
    report.distribution = Some(parse_distribution(&read_text(Path::new(
        "/etc/os-release",
    ))?)?);
    if report.wsl {
        report.reason_code = "HOST_WSL_DEFERRED";
        return Ok(report);
    }
    let pid1 = read_text(Path::new("/proc/1/comm"))?;
    report.pid1_systemd = pid1 == "systemd\n" || pid1 == "systemd";
    if !report.pid1_systemd {
        report.reason_code = "HOST_SYSTEMD_PID1_REQUIRED";
        return Ok(report);
    }
    let mut command = Command::new(SYSTEMCTL);
    command.args([
        "--system",
        "--no-pager",
        "--no-ask-password",
        "show",
        "--all",
        "--property=Version",
        "--property=SystemState",
    ]);
    let output = run_read_only(command, timeout)?;
    if output.exit != 0 || !output.stderr.is_empty() {
        return Err(BxdlError::new(
            "HOST_SYSTEMD_QUERY_FAILED",
            "시스템 systemd 상태를 조회하지 못했습니다. 서비스는 변경하지 않았습니다.",
        ));
    }
    report.manager = Some(parse_manager(&output.stdout)?);
    classify(&mut report);
    Ok(report)
}

fn classify(report: &mut HostReport) {
    report.outcome = "INCOMPLETE";
    let Some(distribution) = &report.distribution else {
        return;
    };
    let Some(manager) = &report.manager else {
        return;
    };
    report.reason_code = if distribution.id != "ubuntu" || distribution.version_id != "24.04" {
        "HOST_DISTRIBUTION_UNSELECTED"
    } else if manager.major < MIN_SYSTEMD {
        "HOST_SYSTEMD_VERSION_UNSELECTED"
    } else if manager.system_state != "running" {
        "HOST_SYSTEMD_NOT_RUNNING"
    } else {
        report.outcome = "SUCCEEDED";
        "HOST_BASELINE_OBSERVED"
    };
}

fn read_text(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|_| observation_error())?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(OUTPUT_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| observation_error())?;
    if bytes.len() > OUTPUT_LIMIT {
        return Err(observation_error());
    }
    String::from_utf8(bytes).map_err(|_| observation_error())
}

fn is_wsl_kernel(release: &str) -> bool {
    release.to_ascii_lowercase().contains("microsoft")
}

fn safe_value(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn parse_distribution(raw: &str) -> Result<Distribution> {
    let mut values = BTreeMap::new();
    for line in raw.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(observation_error());
        };
        if !matches!(key, "ID" | "VERSION_ID") {
            continue;
        }
        let value = if let Some(value) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            value
        } else if let Some(value) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
            value
        } else {
            value
        };
        if !safe_value(value, 80)
            || !value.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
            })
            || values.insert(key, value).is_some()
        {
            return Err(observation_error());
        }
    }
    Ok(Distribution {
        id: values.get("ID").ok_or_else(observation_error)?.to_string(),
        version_id: values
            .get("VERSION_ID")
            .ok_or_else(observation_error)?
            .to_string(),
    })
}

fn properties<'a>(raw: &'a [u8], names: &[&str]) -> Result<BTreeMap<&'a str, &'a str>> {
    if raw.is_empty() || raw.len() > OUTPUT_LIMIT {
        return Err(observation_error());
    }
    let text = std::str::from_utf8(raw).map_err(|_| observation_error())?;
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line.split_once('=').ok_or_else(observation_error)?;
        if !names.contains(&key)
            || value.chars().any(char::is_control)
            || values.insert(key, value).is_some()
        {
            return Err(observation_error());
        }
    }
    if values.len() != names.len() {
        return Err(observation_error());
    }
    Ok(values)
}

/// Parse the bounded output of the exact manager property query above. Unknown,
/// duplicate, missing, or malformed properties remain unknown observations.
pub fn parse_manager(raw: &[u8]) -> Result<ManagerState> {
    let p = properties(raw, &["Version", "SystemState"])?;
    let version = p["Version"];
    if !safe_value(version, 256) {
        return Err(observation_error());
    }
    let numeric: String = version.chars().take_while(char::is_ascii_digit).collect();
    let major = numeric.parse::<u32>().map_err(|_| observation_error())?;
    if numeric.is_empty()
        || numeric.len() > 4
        || major == 0
        || !version[numeric.len()..]
            .chars()
            .next()
            .is_none_or(|c| matches!(c, '.' | '-' | ' ' | '~'))
        || !matches!(
            p["SystemState"],
            "initializing"
                | "starting"
                | "running"
                | "degraded"
                | "maintenance"
                | "stopping"
                | "offline"
                | "unknown"
        )
    {
        return Err(observation_error());
    }
    Ok(ManagerState {
        version: version.into(),
        major,
        system_state: p["SystemState"].into(),
    })
}

/// A systemd metadata binding for a previously recorded run. It is not itself
/// proof of /proc identity, cgroup membership, or the engine's authenticated ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedUnit {
    pub unit: String,
    pub invocation_id: String,
    pub main_pid: u32,
    pub control_group: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitProperties {
    pub unit: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub main_pid: u32,
    pub invocation_id: String,
    pub control_group: String,
    pub result: String,
    pub exec_main_code: u32,
    pub exec_main_status: u32,
}

fn valid_unit(unit: &str) -> bool {
    unit.len() <= 200
        && unit.starts_with("bxdl-")
        && unit.ends_with(".service")
        && unit.len() > "bxdl-.service".len()
        && !unit.contains("..")
        && unit.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
        })
}
fn valid_invocation(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && id.bytes().any(|b| b != b'0')
}
fn valid_cgroup(path: &str) -> bool {
    path.starts_with("/system.slice/")
        && path.len() <= 4096
        && !path.ends_with('/')
        && !path.contains("//")
        && path
            .split('/')
            .skip(1)
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && !path.chars().any(|c| c.is_control() || c.is_whitespace())
}
fn decimal(value: &str) -> Result<u32> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(observation_error());
    }
    value.parse().map_err(|_| observation_error())
}

/// Parse saved properties only. No arbitrary unit query or mutation is exposed.
/// The caller must still verify PID/cgroup and engine identity independently.
pub fn parse_unit(raw: &[u8]) -> Result<UnitProperties> {
    let p = properties(
        raw,
        &[
            "Id",
            "LoadState",
            "ActiveState",
            "SubState",
            "MainPID",
            "InvocationID",
            "ControlGroup",
            "Result",
            "ExecMainCode",
            "ExecMainStatus",
        ],
    )?;
    if !valid_unit(p["Id"])
        || !matches!(
            p["LoadState"],
            "loaded" | "not-found" | "masked" | "error" | "bad-setting"
        )
        || !matches!(
            p["ActiveState"],
            "active"
                | "inactive"
                | "failed"
                | "activating"
                | "deactivating"
                | "reloading"
                | "maintenance"
        )
        || !safe_value(p["SubState"], 80)
        || !safe_value(p["Result"], 80)
        || (!p["InvocationID"].is_empty() && !valid_invocation(p["InvocationID"]))
        || (!p["ControlGroup"].is_empty() && !valid_cgroup(p["ControlGroup"]))
    {
        return Err(observation_error());
    }
    let value = UnitProperties {
        unit: p["Id"].into(),
        load_state: p["LoadState"].into(),
        active_state: p["ActiveState"].into(),
        sub_state: p["SubState"].into(),
        main_pid: decimal(p["MainPID"])?,
        invocation_id: p["InvocationID"].into(),
        control_group: p["ControlGroup"].into(),
        result: p["Result"].into(),
        exec_main_code: decimal(p["ExecMainCode"])?,
        exec_main_status: decimal(p["ExecMainStatus"])?,
    };
    if value.main_pid > 0
        && (!valid_invocation(&value.invocation_id) || !valid_cgroup(&value.control_group))
    {
        return Err(observation_error());
    }
    Ok(value)
}

impl UnitProperties {
    /// Only metadata equality, never permission to signal a PID or stop a unit.
    pub fn matches_running_metadata(&self, expected: &ExpectedUnit) -> bool {
        valid_unit(&expected.unit)
            && valid_invocation(&expected.invocation_id)
            && valid_cgroup(&expected.control_group)
            && expected.main_pid > 0
            && self.unit == expected.unit
            && self.invocation_id == expected.invocation_id
            && self.main_pid == expected.main_pid
            && self.control_group == expected.control_group
            && self.load_state == "loaded"
            && self.active_state == "active"
            && self.sub_state == "running"
    }
    /// An OS exit observation still requires the engine STOPPED report and a
    /// separate process/cgroup absence audit; it does not prove clean DB closure.
    pub fn has_exit_metadata_for(&self, expected: &ExpectedUnit) -> bool {
        valid_unit(&expected.unit)
            && valid_invocation(&expected.invocation_id)
            && valid_cgroup(&expected.control_group)
            && expected.main_pid > 0
            && self.unit == expected.unit
            && self.invocation_id == expected.invocation_id
            && (self.control_group.is_empty() || self.control_group == expected.control_group)
            && self.load_state == "loaded"
            && self.main_pid == 0
            && matches!(
                (self.active_state.as_str(), self.sub_state.as_str()),
                ("inactive", "dead") | ("failed", "failed")
            )
            && matches!(self.exec_main_code, 1..=3)
    }
}

#[derive(Debug)]
struct Output {
    exit: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
fn validate_timeout(timeout: Duration) -> Result<()> {
    if timeout.is_zero() || timeout > Duration::from_secs(30) {
        return Err(BxdlError::new(
            "HOST_OPTIONS_INVALID",
            "호스트 조회 제한 시간은 0초 초과 30초 이하여야 합니다.",
        ));
    }
    Ok(())
}
fn observation_error() -> BxdlError {
    BxdlError::new(
        "HOST_OBSERVATION_UNKNOWN",
        "호스트 관측 자료가 없거나 불명확합니다. 서비스는 변경하지 않았습니다.",
    )
}
fn timeout_error() -> BxdlError {
    BxdlError::new(
        "HOST_QUERY_TIMEOUT",
        "호스트 조회 제한 시간이 지났습니다. 조회용 프로세스만 종료하고 서비스는 변경하지 않았습니다.",
    )
}
fn capture(
    mut stream: UnixStream,
    done: Arc<AtomicBool>,
    exceeded: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut finishing = None;
    loop {
        if done.load(Ordering::Acquire)
            && finishing.get_or_insert_with(Instant::now).elapsed() >= Duration::from_millis(150)
        {
            return Err(ErrorKind::TimedOut.into());
        }
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(bytes),
            Ok(n) if bytes.len() + n <= OUTPUT_LIMIT && !exceeded.load(Ordering::Acquire) => {
                bytes.extend_from_slice(&buffer[..n])
            }
            Ok(_) => {
                exceeded.store(true, Ordering::Release);
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => (),
            Err(e)
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
                    && !done.load(Ordering::Acquire) => {}
            Err(e) => return Err(e),
        }
    }
}
fn run_read_only(mut command: Command, timeout: Duration) -> Result<Output> {
    validate_timeout(timeout)?;
    let streams = || -> std::io::Result<(UnixStream, Stdio)> {
        let (read, write) = UnixStream::pair()?;
        read.set_read_timeout(Some(Duration::from_millis(50)))?;
        Ok((read, Stdio::from(OwnedFd::from(write))))
    };
    let (stdout, child_stdout) = streams().map_err(|_| observation_error())?;
    let (stderr, child_stderr) = streams().map_err(|_| observation_error())?;
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(child_stdout)
        .stderr(child_stderr);
    let mut child = command.spawn().map_err(|_| observation_error())?;
    drop(command);
    let done = Arc::new(AtomicBool::new(false));
    let exceeded = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = [stdout, stderr]
        .into_iter()
        .map(|stream| {
            let done = done.clone();
            let exceeded = exceeded.clone();
            thread::spawn(move || capture(stream, done, exceeded))
        })
        .collect();
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if exceeded.load(Ordering::Acquire) => break Err(observation_error()),
            Ok(None) if start.elapsed() >= timeout => break Err(timeout_error()),
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(_) => break Err(observation_error()),
        }
    };
    if status.is_err() {
        // This PID belongs to our read-only helper, never a node or service.
        let _ = child.kill();
        thread::spawn(move || {
            let _ = child.wait();
        });
    }
    done.store(true, Ordering::Release);
    let captures: Vec<_> = readers.into_iter().map(|reader| reader.join()).collect();
    if exceeded.load(Ordering::Acquire) {
        return Err(observation_error());
    }
    let status = status?;
    let mut raw = Vec::new();
    for capture in captures {
        raw.push(
            capture
                .map_err(|_| observation_error())?
                .map_err(|_| observation_error())?,
        );
    }
    let stderr = raw.pop().ok_or_else(observation_error)?;
    let stdout = raw.pop().ok_or_else(observation_error)?;
    Ok(Output {
        exit: status.code().ok_or_else(observation_error)?,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const RUN: &str = "Id=bxdl-node1-attempt1.service\nLoadState=loaded\nActiveState=active\nSubState=running\nMainPID=1234\nInvocationID=1234567890abcdef1234567890abcdef\nControlGroup=/system.slice/bxdl-node1-attempt1.service\nResult=success\nExecMainCode=0\nExecMainStatus=0\n";
    fn expected() -> ExpectedUnit {
        ExpectedUnit {
            unit: "bxdl-node1-attempt1.service".into(),
            invocation_id: "1234567890abcdef1234567890abcdef".into(),
            main_pid: 1234,
            control_group: "/system.slice/bxdl-node1-attempt1.service".into(),
        }
    }
    #[test]
    fn manager_requires_every_unique_property_and_known_state() {
        let manager = parse_manager(b"Version=255.4-1ubuntu8.10\nSystemState=running\n").unwrap();
        assert_eq!(manager.major, 255);
        for bytes in [
            b"Version=255\n".as_slice(),
            b"Version=255\nVersion=255\nSystemState=running\n",
            b"Version=255\nSystemState=future-state\n",
            b"Version=255evil\nSystemState=running\n",
            b"Version=255\nSystemState=running\nUnexpected=yes\n",
            b"Version=255\nSystemState=running\0\n",
        ] {
            assert!(parse_manager(bytes).is_err());
        }
    }
    #[test]
    fn distribution_does_not_execute_or_expand_shell_values() {
        let d = parse_distribution("NAME=\"Ubuntu\"\nID=ubuntu\nVERSION_ID=\"24.04\"\n").unwrap();
        assert_eq!(d.id, "ubuntu");
        assert_eq!(d.version_id, "24.04");
        for raw in [
            "ID=ubuntu\nID=debian\nVERSION_ID=24.04\n",
            "ID=$(id)\nVERSION_ID=24.04\n",
            "ID=ubuntu\nVERSION_ID=\"24.04\n",
            "ID=ubuntu\n",
        ] {
            assert!(parse_distribution(raw).is_err());
        }
    }
    #[test]
    fn baseline_is_distinct_from_service_and_engine_acceptance() {
        let mut r = HostReport::initial();
        r.distribution = Some(Distribution {
            id: "ubuntu".into(),
            version_id: "24.04".into(),
        });
        r.manager = Some(parse_manager(b"Version=255\nSystemState=running\n").unwrap());
        classify(&mut r);
        assert_eq!(r.reason_code, "HOST_BASELINE_OBSERVED");
        assert_eq!(r.linux_service_control, "DEVELOPMENT_BACKEND_IMPLEMENTED");
        assert_eq!(r.engine_acceptance, "NOT_RUN");
        for (id, version, manager, code) in [
            (
                "debian",
                "12",
                "Version=255\nSystemState=running\n",
                "HOST_DISTRIBUTION_UNSELECTED",
            ),
            (
                "ubuntu",
                "24.04",
                "Version=254\nSystemState=running\n",
                "HOST_SYSTEMD_VERSION_UNSELECTED",
            ),
            (
                "ubuntu",
                "24.04",
                "Version=255\nSystemState=degraded\n",
                "HOST_SYSTEMD_NOT_RUNNING",
            ),
        ] {
            let mut r = HostReport::initial();
            r.distribution = Some(Distribution {
                id: id.into(),
                version_id: version.into(),
            });
            r.manager = Some(parse_manager(manager.as_bytes()).unwrap());
            classify(&mut r);
            assert_eq!(r.outcome, "INCOMPLETE");
            assert_eq!(r.reason_code, code);
        }
    }
    #[test]
    fn running_metadata_binds_all_run_identity_fields() {
        let p = parse_unit(RUN.as_bytes()).unwrap();
        let e = expected();
        assert!(p.matches_running_metadata(&e));
        assert!(!p.has_exit_metadata_for(&e));
        for wrong in [
            ExpectedUnit {
                main_pid: 1235,
                ..e.clone()
            },
            ExpectedUnit {
                invocation_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                ..e.clone()
            },
            ExpectedUnit {
                control_group: "/system.slice/other.service".into(),
                ..e.clone()
            },
            ExpectedUnit {
                unit: "bxdl-other.service".into(),
                ..e.clone()
            },
        ] {
            assert!(!p.matches_running_metadata(&wrong));
        }
    }
    #[test]
    fn exit_metadata_does_not_imply_engine_shutdown_or_allow_replaced_run() {
        let raw = RUN
            .replace("ActiveState=active", "ActiveState=inactive")
            .replace("SubState=running", "SubState=dead")
            .replace("MainPID=1234", "MainPID=0")
            .replace("ExecMainCode=0", "ExecMainCode=2")
            .replace("ExecMainStatus=0", "ExecMainStatus=9");
        let p = parse_unit(raw.as_bytes()).unwrap();
        assert!(p.has_exit_metadata_for(&expected()));
        assert!(!p.matches_running_metadata(&expected()));
        assert!(!p.has_exit_metadata_for(&ExpectedUnit {
            invocation_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ..expected()
        }));
    }
    #[test]
    fn malformed_or_ambiguous_unit_data_is_rejected() {
        for raw in [
            RUN.replace("MainPID=1234", "MainPID=-1"),
            RUN.replace("MainPID=1234", "MainPID=99999999999"),
            RUN.replace(
                "InvocationID=1234567890abcdef1234567890abcdef",
                "InvocationID=",
            ),
            RUN.replace(
                "/system.slice/bxdl-node1-attempt1.service",
                "/system.slice/../other",
            ),
            RUN.replace("Id=bxdl-node1-attempt1.service", "Id=ssh.service"),
            format!("{RUN}MainPID=1234\n"),
            RUN.replace("SubState=running\n", ""),
        ] {
            assert!(parse_unit(raw.as_bytes()).is_err());
        }
    }
    #[test]
    fn unit_cannot_inject_options_patterns_or_template_expansion() {
        for name in [
            "--all",
            "bxdl-*.service",
            "bxdl-foo@bar.service",
            "bxdl-%n.service",
            "bxdl-a\nb.service",
            "bxdl-../x.service",
        ] {
            assert!(!valid_unit(name));
        }
    }
    #[test]
    fn windows_subsystem_kernel_is_explicitly_distinct_from_native_baseline() {
        assert!(is_wsl_kernel("6.6.87.2-microsoft-standard-WSL2"));
        assert!(is_wsl_kernel("5.15.90.1-Microsoft-standard-WSL2"));
        assert!(!is_wsl_kernel("6.8.0-87-generic"));
    }
    #[test]
    fn timeout_validation_is_before_any_host_io() {
        assert_eq!(
            inspect_host(Duration::ZERO).unwrap_err().code,
            "HOST_OPTIONS_INVALID"
        );
        assert_eq!(
            inspect_host(Duration::from_secs(31)).unwrap_err().code,
            "HOST_OPTIONS_INVALID"
        );
    }
    #[test]
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu")))]
    fn unsupported_host_is_an_observation_not_fake_linux_success() {
        let r = inspect_host(Duration::from_secs(1)).unwrap();
        assert_eq!(r.outcome, "INCOMPLETE");
        assert_eq!(r.reason_code, "HOST_PLATFORM_UNSUPPORTED");
        assert!(r.manager.is_none());
        assert_eq!(r.service_mutations, 0);
    }
    #[test]
    fn bounded_reader_caps_stdout_and_stderr_without_service_calls() {
        let mut ok = Command::new("/bin/sh");
        ok.args(["-c", "printf 'Version=255\\nSystemState=running\\n'"]);
        let output = run_read_only(ok, Duration::from_secs(2)).unwrap();
        assert_eq!(output.exit, 0);
        assert!(output.stderr.is_empty());
        assert_eq!(parse_manager(&output.stdout).unwrap().major, 255);
        let mut large = Command::new("/bin/sh");
        large.args(["-c", "while :; do printf '0123456789012345678901234567890123456789012345678901234567890123456789' >&2; done"]);
        assert_eq!(
            run_read_only(large, Duration::from_secs(2))
                .unwrap_err()
                .code,
            "HOST_OBSERVATION_UNKNOWN"
        );
    }
    #[test]
    fn bounded_reader_timeout_only_kills_owned_query_process() {
        let mut query = Command::new("/bin/sleep");
        query.arg("2");
        let before = Instant::now();
        assert_eq!(
            run_read_only(query, Duration::from_millis(30))
                .unwrap_err()
                .code,
            "HOST_QUERY_TIMEOUT"
        );
        assert!(before.elapsed() < Duration::from_secs(1));
    }
}
