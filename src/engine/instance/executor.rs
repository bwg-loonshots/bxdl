//! Versioned management adapter for the accepted, immutable RC.3 executor.
//! Only this pinned package may be executed. The package remains the writer of
//! its own private operation journals; this manager never reimplements them.
mod json;
mod process;
mod reply;

use super::{files, load, verified_package};
use crate::{
    artifact,
    engine::instance_fs::Control,
    error::{BxdlError, Result},
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

pub const PROFILE: &str = "BXDL_RC3_EXECUTOR_V1";
const ARCHIVE: &str = "8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017";
const MANIFEST: &str = "2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123";
const CLI: &str = "b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef";
const CLI_SIZE: u64 = 3_024_064;
const JAR: &str = "05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f";
const JAVA: &str = "9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c";

#[derive(Clone, Copy, Debug)]
pub enum ServiceAction {
    Start,
    Status,
    Stop,
}
#[derive(Clone, Debug)]
pub enum Request {
    Show,
    Preflight {
        timeout: Duration,
    },
    Initialize {
        resume: bool,
        timeout: Duration,
    },
    Service {
        action: ServiceAction,
        wait: bool,
        timeout: Duration,
    },
    Logs {
        tail: usize,
        max_bytes: u64,
        timeout: Duration,
    },
    Diagnose {
        output: PathBuf,
        tail: usize,
        max_bytes: u64,
        timeout: Duration,
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
                action: ServiceAction::Start,
                ..
            } => "start",
            Self::Service {
                action: ServiceAction::Status,
                ..
            } => "status",
            Self::Service {
                action: ServiceAction::Stop,
                ..
            } => "stop",
            Self::Logs { .. } => "logs",
            Self::Diagnose { .. } => "diagnose",
        }
    }
    fn mutating(&self) -> bool {
        matches!(
            self,
            Self::Initialize { .. }
                | Self::Service {
                    action: ServiceAction::Start | ServiceAction::Stop,
                    ..
                }
                | Self::Diagnose { .. }
        )
    }
    fn timeout(&self) -> Duration {
        match self {
            Self::Show => Duration::from_secs(10),
            Self::Preflight { timeout }
            | Self::Initialize { timeout, .. }
            | Self::Service { timeout, .. }
            | Self::Logs { timeout, .. }
            | Self::Diagnose { timeout, .. } => *timeout,
        }
    }
    fn validate(&self) -> Result<()> {
        let seconds = self.timeout().as_secs();
        let maximum = match self {
            Self::Show => 10,
            Self::Preflight { .. } => 120,
            Self::Initialize { .. } => 600,
            Self::Service {
                action: ServiceAction::Status,
                wait: false,
                ..
            } => 120,
            Self::Service { .. } => 600,
            Self::Logs { .. } | Self::Diagnose { .. } => 30,
        };
        if !(1..=maximum).contains(&seconds) || self.timeout().subsec_nanos() != 0 {
            return Err(options());
        }
        if matches!(
            self,
            Self::Service {
                action: ServiceAction::Start | ServiceAction::Stop,
                wait: true,
                ..
            }
        ) {
            return Err(options());
        }
        if let Self::Logs {
            tail, max_bytes, ..
        }
        | Self::Diagnose {
            tail, max_bytes, ..
        } = self
        {
            if !(1..=200).contains(tail) || !(4_096..=1_048_576).contains(max_bytes) {
                return Err(options());
            }
        }
        if let Self::Diagnose { output, .. } = self {
            let output = files::absolute(output)?;
            let text = output.to_str().ok_or_else(options)?;
            if text.len() > 4096 || text.chars().any(char::is_control) {
                return Err(options());
            }
        }
        Ok(())
    }
    fn arguments(&self, instance: &Path) -> Result<Vec<OsString>> {
        self.validate()?;
        let mut args: Vec<OsString> = self.command().split(' ').map(OsString::from).collect();
        args.extend([
            OsString::from("--instance"),
            instance.as_os_str().to_owned(),
            OsString::from("--json"),
        ]);
        if !matches!(self, Self::Show) {
            args.extend([
                "--timeout-seconds".into(),
                self.timeout().as_secs().to_string().into(),
            ]);
        }
        match self {
            Self::Initialize { resume, .. } => args.push(
                if *resume {
                    "--confirm-resume"
                } else {
                    "--confirm-initialize"
                }
                .into(),
            ),
            Self::Service { wait: true, .. } => args.push("--wait".into()),
            Self::Logs {
                tail, max_bytes, ..
            }
            | Self::Diagnose {
                tail, max_bytes, ..
            } => {
                args.extend([
                    "--tail".into(),
                    tail.to_string().into(),
                    "--max-bytes".into(),
                    max_bytes.to_string().into(),
                ]);
            }
            _ => (),
        }
        if let Self::Diagnose { output, .. } = self {
            args.extend(["--output".into(), files::absolute(output)?.into_os_string()]);
        }
        Ok(args)
    }
    /// This is a supervisor budget, not a promise about the old command's total
    /// deadline. Cold gates may each consume the command timeout separately.
    fn supervision(&self) -> Duration {
        match self {
            Self::Initialize { .. }
            | Self::Service {
                action: ServiceAction::Start,
                ..
            } => self.timeout() * 4 + Duration::from_secs(120),
            Self::Preflight { .. } => self.timeout() * 3 + Duration::from_secs(60),
            _ => self.timeout() + Duration::from_secs(30),
        }
    }
}

#[derive(Debug)]
pub struct Response {
    pub command: String,
    pub outcome: String,
    pub reason_code: String,
    pub message: String,
    pub observed_at: String,
    pub data: Option<serde_json::Value>,
    pub exit: i32,
}

/// Compatibility is a tested, exact package profile, not semver guessing.
/// Call only with an already verified artifact report; this performs no I/O.
pub(crate) fn supports_package(report: &artifact::Report) -> bool {
    report.authenticity == "verified-external-ed25519"
        && report.archive_sha256 == ARCHIVE
        && report.manifest_sha256 == MANIFEST
        && report.manifest.engine.jar_sha256 == JAR
        && report.manifest.runtime.java_sha256 == JAVA
        && report
            .manifest
            .files
            .iter()
            .filter(|file| file.path == "bin/bxdl")
            .count()
            == 1
        && report
            .manifest
            .files
            .iter()
            .any(|file| file.path == "bin/bxdl" && file.sha256 == CLI && file.size == CLI_SIZE)
}

/// Unknown package profiles return None without executing any package file.
/// The caller retains its existing native behavior for those packages. The
/// callback rechecks a named selection immediately before dispatch; it must not
/// wait for user input. The legacy executor does not accept an expected-attempt
/// token, so its normal lock and current-attempt semantics remain authoritative.
pub fn try_execute(
    instance: &Path,
    request: &Request,
    before_dispatch: impl FnOnce() -> Result<()>,
) -> Result<Option<Response>> {
    request.validate()?;
    let Ok(root) = Control::open(instance) else {
        return Ok(None);
    };
    let Ok((binding, binding_file, journal, state)) = load(&root) else {
        return Ok(None);
    };
    if binding.archive_sha256 != ARCHIVE {
        return Ok(None);
    }
    if binding.manifest_sha256 != MANIFEST
        || binding.allow_unsigned_development
        || binding.public_key.is_none()
    {
        return Err(unsupported());
    }
    // Authentication inputs and the installed inventory are verified before
    // even probing version/capabilities. No untrusted program runs as a probe.
    let report = verified_package(&binding, true)?;
    if !supports_package(&report) {
        return Err(unsupported());
    }
    let cli = files::Input::read(&binding.package.join("bin/bxdl"), CLI_SIZE, true)?;
    if cli.raw.len() as u64 != CLI_SIZE || files::digest(&cli.raw) != CLI {
        return Err(unsupported());
    }
    let args = request.arguments(root.path())?;
    binding_file.recheck()?;
    journal.read().map_err(super::state_error)?;
    root.recheck()?;
    before_dispatch()?;
    binding_file.recheck()?;
    cli.recheck()?;
    root.recheck()?;
    let output = process::run(&cli, root.path(), &args, request.supervision())
        .map_err(|error| dispatched_error(request, error))?;
    // A completed child can legitimately advance journals, but not replace
    // its binding, the selected control root, or the pinned package executor.
    binding_file
        .recheck()
        .and_then(|_| root.recheck())
        .and_then(|_| cli.recheck())
        .map_err(|error| dispatched_error(request, error))?;
    let response = reply::parse(
        request,
        &state.instance_id,
        output.exit,
        &output.stdout,
        &output.stderr,
    )
    .map_err(|error| dispatched_error(request, error))?;
    Ok(Some(response))
}

fn dispatched_error(request: &Request, error: BxdlError) -> BxdlError {
    // EXECUTOR_SPAWN_FAILED is the one process error that proves no dispatch.
    if request.mutating() && error.code != "EXECUTOR_SPAWN_FAILED" {
        BxdlError::new(
            "EXECUTOR_OPERATION_UNKNOWN",
            "패키지 실행기에 요청한 작업 결과를 확정하지 못했습니다. 자동 재시도하지 않았습니다. 기존 기록과 현재 상태를 확인하세요.",
        )
    } else {
        error
    }
}
fn options() -> BxdlError {
    BxdlError::new(
        "EXECUTOR_OPTIONS_INVALID",
        "패키지 실행기에 전달할 명령 또는 옵션이 올바르지 않습니다.",
    )
}
fn unsupported() -> BxdlError {
    BxdlError::new(
        "EXECUTOR_PROFILE_UNSUPPORTED",
        "검증된 RC.3 패키지 실행기와 일치하지 않습니다. 설치 파일을 변경하거나 실행하지 않았습니다.",
    )
}
fn response_invalid() -> BxdlError {
    BxdlError::new(
        "EXECUTOR_RESPONSE_INVALID",
        "패키지 실행기의 결과 계약을 확인할 수 없습니다. 원문 출력은 표시하지 않았습니다.",
    )
}

#[cfg(test)]
mod tests;
