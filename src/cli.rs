//! Operator interface. Mutations require a registered instance and explicit intent.
use crate::{artifact, config, engine, error::BxdlError, install, runtime, setup};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, IsTerminal, Write},
    path::{Path, PathBuf},
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REVISION: &str = match option_env!("BXDL_REVISION") {
    Some(v) => v,
    None => "development",
};
pub const HELP: &str = "BXDL — 패키징·운영 CLI (development, Rust)\n\n사용법:\n  bxdl version [--json]\n  bxdl runtime stage --source <extracted-java-home> --lock <json> --output <new-dir> [--json]\n  bxdl package build --root <dir> --spec <json> --output <tar.gz>\n      (--signing-key <private.pem> | --allow-unsigned-development) [--json]\n  bxdl package verify <tar.gz>\n      (--public-key <trusted.pem> | --allow-unsigned-development) [--json]\n  bxdl install <tar.gz> --destination <new-dir>\n      (--public-key <trusted.pem> | --allow-unsigned-development) [--json]\n  bxdl engine inspect --jar <jar> --java <java> --lock <json> --allow-development\n      [--timeout-seconds <1..120>] [--json]\n  bxdl engine preflight --jar <jar> --java <java> --lock <json> --allow-development\n      --config <nigo-node.json> [--timeout-seconds <1..120>] [--json]\n  bxdl config validate --file <instance.json> [--json]\n  bxdl config fingerprint --certificate <leaf.pem|leaf.der> [--json]\n  bxdl preflight --config <instance.json> [--json]\n  bxdl preflight --config <instance.json> --engine-config <nigo-node.json>\n      --jar <jar> --java <java> --lock <json> --allow-development\n      [--timeout-seconds <1..120>] [--json]\n  bxdl setup --network --workspace <new-session> --from <instance.json>\n      --network-file <network.json> [--output <new-node.json>]\n  bxdl setup --network --workspace <session> --resume [--output <new-node.json>]\n  bxdl setup --install --workspace <new-session> [--from <instance.json>]\n  bxdl setup --install --workspace <session> --resume\n  bxdl setup [--workspace <dir>] [--resume] [--from <instance.json>]\n      [--output <new-instance.json>]\n  bxdl setup --workspace <dir> (--from <instance.json> | --resume)\n      --non-interactive [--output <new-instance.json>] [--json]\n\n  bxdl instance register --instance <new-dir> --package <installed-dir>\n      --archive <tar.gz> (--public-key <trusted.pem> | --allow-unsigned-development)\n      --config <instance.json> --engine-config <node.json> --lock <json>\n      --allow-development [--timeout-seconds <1..120>] [--json]\n  bxdl instance show --instance <dir> [--json]\n  bxdl preflight --instance <dir> [--timeout-seconds <1..120>] [--json]\n  bxdl init --instance <dir> --confirm-initialize [--timeout-seconds <1..600>] [--json]\n  bxdl resume-init --instance <dir> --confirm-resume [--timeout-seconds <1..600>] [--json]\n  bxdl start --instance <dir> [--timeout-seconds <1..600>] [--json]\n  bxdl status --instance <dir> [--timeout-seconds <1..120>] [--json]\n  bxdl status --instance <dir> --wait [--timeout-seconds <1..600>] [--json]\n  bxdl stop --instance <dir> [--timeout-seconds <1..600>] [--json]\n  bxdl logs --instance <dir> [--tail <1..200>] [--max-bytes <4096..1048576>]\n      [--timeout-seconds <1..30>] [--json]\n  bxdl diagnose --instance <dir> --output <new.json>\n      [--tail <1..200>] [--max-bytes <4096..1048576>]\n      [--timeout-seconds <1..30>] [--json]\n\n서명 검증 key는 패키지 밖의 신뢰한 경로에서 제공하세요.\ndevelopment package 검증은 엔진 실행·공식 공급·OS 서비스 지원 검증이 아닙니다.\nmacOS arm64를 첫 설치·운용 UX 대상으로 하며 Linux/Docker는 후속입니다.\n기본 setup은 설정 초안·로컬 검사·파일 저장만 수행합니다.\nsetup --network는 기존 제품 설정·공개 네트워크 자료로 RC1 native 설정을 준비합니다. 키 생성·엔진 실행은 하지 않습니다.\nsetup --install은 터미널에서 설치·등록·초기화·시작을 각각 명시 선택합니다. 기존 DB 업그레이드는 제공하지 않습니다.\npreflight는 기본적으로 로컬 정적 검사입니다. 전체 엔진 옵션을 지정하면 제품·native 설정을 대조한 뒤 cold 검사합니다.\nengine preflight는 NIGO node.json을 읽고 cold 검사를 수행하며 INCOMPLETE를 유지합니다.\ninstall은 macOS arm64 새 폴더에 검증한 파일만 설치합니다. 서비스·초기화는 수행하지 않습니다.\nstart/status/stop은 Mac 사용자 LaunchAgent를 명시 제어합니다. 로그인 자동 시작·자동 재시작은 하지 않습니다.\nstatus --wait는 현재 시작 시도만 최대 120초(기본) 관측하며 초기화·시작·중지는 실행하지 않습니다.\nlogs는 최신 초기화·서비스 시도의 정제한 저장 기록만 읽습니다. raw stdout/stderr나 실시간 follow는 제공하지 않습니다.\ndiagnose는 같은 기록을 새 JSON 지원 보고서에 저장합니다. JVM·네트워크·서비스 호출이나 현재 상태 검증은 수행하지 않습니다.\nstatus/diagnose의 시작 단계는 마지막 진입 기록입니다. 완료 여부나 파일 접근 지연의 원인을 확정하지 않습니다.\nlogs/diagnose 기본 한도는 tail 50개, 읽기 262144 bytes, 시간 5초입니다. 누락·예산 초과는 partial로 표시합니다.\nupgrade/uninstall은 아직 제공하지 않습니다.\n";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultEnvelope {
    pub schema_version: u32,
    pub command: String,
    pub outcome: String,
    pub reason_code: String,
    pub message: String,
    pub observed_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

fn result(
    command: &str,
    outcome: &str,
    code: &str,
    message: &str,
    data: Option<Value>,
) -> ResultEnvelope {
    ResultEnvelope {
        schema_version: 1,
        command: command.into(),
        outcome: outcome.into(),
        reason_code: code.into(),
        message: message.into(),
        observed_at: String::new(),
        data,
    }
}
fn invalid(command: &str) -> (ResultEnvelope, i32) {
    (
        result(
            command,
            "FAILED",
            "INVALID_ARGUMENTS",
            "명령 또는 인자가 올바르지 않습니다. bxdl help를 확인하세요.",
            None,
        ),
        2,
    )
}
fn failure(command: &str, error: BxdlError, exit: i32) -> (ResultEnvelope, i32) {
    (
        result(command, "FAILED", &error.code, &error.message, None),
        exit,
    )
}
fn report<T: Serialize>(
    command: &str,
    outcome: &str,
    code: &str,
    message: &str,
    data: T,
    exit: i32,
) -> (ResultEnvelope, i32) {
    match serde_json::to_value(data) {
        Ok(data) => (result(command, outcome, code, message, Some(data)), exit),
        Err(_) => (
            result(
                command,
                "FAILED",
                "INTERNAL_ERROR",
                "결과를 구성하지 못했습니다.",
                None,
            ),
            7,
        ),
    }
}

/// JSON mode emits exactly one document on stdout, including command failures.
/// A failed output write is exit 7, never a successful operation report.
pub fn run<'a>(args: &[String], stdout: &'a mut dyn Write, stderr: &'a mut dyn Write) -> i32 {
    let input = std::io::stdin();
    let interactive = input.is_terminal();
    run_with_input(args, &mut input.lock(), interactive, stdout, stderr)
}

/// Injected input keeps wizard behavior testable without changing process-wide
/// stdin. Machine mode is always explicit and never consumes interactive input.
pub fn run_with_input<'a>(
    args: &[String],
    input: &mut dyn BufRead,
    interactive: bool,
    stdout: &'a mut dyn Write,
    stderr: &'a mut dyn Write,
) -> i32 {
    let (json_mode, args) = take_json(args);
    if !json_mode {
        if let Some(notice) = service_wait_notice(&args) {
            if writeln!(stderr, "{notice}")
                .and_then(|_| stderr.flush())
                .is_err()
            {
                return 7;
            }
        }
    }
    let (mut response, exit) = if args.first().is_some_and(|arg| arg == "setup") {
        dispatch_setup(&args[1..], json_mode, interactive, input, stderr)
    } else {
        dispatch(&args)
    };
    response.observed_at = match OffsetDateTime::now_utc().format(&Rfc3339) {
        Ok(time) => time,
        Err(_) => return 7,
    };
    if json_mode {
        return if serde_json::to_writer(&mut *stdout, &response).is_err()
            || writeln!(stdout).is_err()
        {
            7
        } else {
            exit
        };
    }
    if response.command == "help" && exit == 0 {
        return if stdout.write_all(HELP.as_bytes()).is_ok() {
            0
        } else {
            7
        };
    }
    let out = if exit == 0 { stdout } else { stderr };
    if writeln!(out, "{} [{}]", response.message, response.reason_code).is_err() {
        return 7;
    }
    if let Some(data) = response.data {
        if response.command == "setup network" {
            return if print_network_summary(out, &data).is_ok() {
                exit
            } else {
                7
            };
        }
        if response.command == "setup" {
            return if print_setup_summary(out, &data).is_ok() {
                exit
            } else {
                7
            };
        }
        if matches!(response.command.as_str(), "start" | "status" | "stop") {
            return if print_service_summary(out, &data).is_ok() {
                exit
            } else {
                7
            };
        }
        if matches!(response.command.as_str(), "logs" | "diagnose") {
            return if print_diagnostic_summary(out, &data).is_ok() {
                exit
            } else {
                7
            };
        }
        if serde_json::to_writer_pretty(&mut *out, &data).is_err() || writeln!(out).is_err() {
            return 7;
        }
    }
    exit
}

fn print_network_summary(out: &mut dyn Write, data: &Value) -> std::io::Result<()> {
    let text = |value: &Value| value.as_str().unwrap_or("UNAVAILABLE").to_owned();
    writeln!(out, "네트워크 준비 작업 폴더: {}", text(&data["workspace"]))?;
    writeln!(
        out,
        "선택 완료: {} / {}",
        data["completedFields"].as_array().map_or(0, Vec::len),
        data["totalFields"].as_u64().unwrap_or(0)
    )?;
    if let Some(configuration) = data.get("configuration") {
        writeln!(
            out,
            "validator {}개 / peer {}개 / 거래 권한 {}",
            configuration["validatorCount"].as_u64().unwrap_or(0),
            configuration["peerCount"].as_u64().unwrap_or(0),
            text(&configuration["transactionPermission"])
        )?;
    }
    if let Some(path) = data["outputPath"].as_str() {
        writeln!(out, "native 설정: {path}")?;
    }
    writeln!(out, "엔진 cold 검사·설치·실제 네트워크 검증: 미실행")
}

/// Only the collector's fixed projections are displayed, never arbitrary fields
/// or a fallback dump of private input. JSON mode returns before this formatter.
fn print_diagnostic_summary(out: &mut dyn Write, data: &Value) -> std::io::Result<()> {
    let text = |value: &Value| value.as_str().unwrap_or("UNAVAILABLE").to_owned();
    writeln!(
        out,
        "저장 상태: 초기화 {} / 서비스 {}",
        text(&data["initializationRecord"]),
        text(&data["serviceRecord"])
    )?;
    writeln!(out, "현재 runtime: 관측 안 함 (저장 기록만 수집)")?;
    print_startup_summary(out, data)?;
    let events = data["events"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    writeln!(
        out,
        "사건 기록: {}개 / 생략 {}개",
        events.len(),
        data["eventsOmitted"].as_u64().unwrap_or(0)
    )?;
    for event in events {
        writeln!(
            out,
            "  [{} #{}] {}: {} ({})",
            text(&event["source"]),
            event["sequence"].as_u64().unwrap_or(0),
            text(&event["command"]),
            text(&event["status"]),
            text(&event["reason"])
        )?;
    }
    if let Some(sources) = data["sources"].as_array() {
        for source in sources {
            let label = match source["result"].as_str() {
                Some("COLLECTED") => continue,
                Some("WITHHELD") => "제외",
                Some("UNAVAILABLE") => "누락",
                Some("TRUNCATED") => "일부 수집",
                Some("NOT_APPLICABLE") => "대상 없음",
                _ => "확인 필요",
            };
            writeln!(
                out,
                "자료 {label}: {} ({})",
                text(&source["name"]),
                text(&source["reason"])
            )?;
        }
    }
    if data["outputCreated"] == true {
        writeln!(out, "지원 보고서: 지정한 새 파일에 저장됨")?;
    }
    if let Some(actions) = data["nextActions"].as_array() {
        for action in actions.iter().filter_map(Value::as_str) {
            writeln!(out, "다음 조치: {action}")?;
        }
    }
    Ok(())
}

/// Render only public observation fields, never health payloads or source paths.
/// The install wizard uses the same projection as start/status/stop.
pub(crate) fn print_service_summary(out: &mut dyn Write, data: &Value) -> std::io::Result<()> {
    for (key, label) in [
        ("serviceState", "서비스"),
        ("engineState", "엔진"),
        ("runtimeReadiness", "로컬 준비 상태"),
    ] {
        let (description, code) = match data[key].as_str() {
            Some("RUNNING") => ("실행 중", "RUNNING"),
            Some("STARTING") => ("시작 중", "STARTING"),
            Some("PENDING") => ("준비 대기 중", "PENDING"),
            Some("START_PENDING") => ("시작 준비 대기 중", "START_PENDING"),
            Some("START_NOT_CONFIRMED") => ("시작 여부 미확인", "START_NOT_CONFIRMED"),
            Some("GATE_REFUSED") => ("실행 전 검사에서 거부됨", "GATE_REFUSED"),
            Some("GUI_UNAVAILABLE") => ("사용자 로그인 세션 확인 불가", "GUI_UNAVAILABLE"),
            Some("CHECKING") => ("확인 중", "CHECKING"),
            Some("READY") => ("준비됨", "READY"),
            Some("NOT_READY") => ("아직 준비되지 않음", "NOT_READY"),
            Some("STOPPED") => ("정지됨", "STOPPED"),
            Some("EXITED") => ("프로세스 종료됨", "EXITED"),
            Some("FAILED") => ("실패 확인됨", "FAILED"),
            Some("NOT_LOADED") => ("서비스에 등록되지 않음", "NOT_LOADED"),
            Some("NOT_REGISTERED") => ("서비스에 등록되지 않음", "NOT_REGISTERED"),
            Some("NOT_STARTED") => ("시작하지 않음", "NOT_STARTED"),
            Some("NOT_OBSERVED") => ("관측 전", "NOT_OBSERVED"),
            Some("NOT_CHECKED") => ("미검사", "NOT_CHECKED"),
            _ => ("확인되지 않음", "UNKNOWN"),
        };
        writeln!(out, "{label}: {description} ({code})")?;
    }
    let attempt = data["attemptId"]
        .as_str()
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 80
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        })
        .unwrap_or("확인되지 않음");
    writeln!(out, "시작 시도: {attempt}")?;
    let reason = data.get("reason").or_else(|| data.get("serviceReason"));
    let reason = reason
        .and_then(Value::as_str)
        .filter(|code| safe_reason_code(code))
        .unwrap_or("UNKNOWN");
    writeln!(out, "서비스 사유: {reason}")?;
    if let Some(last) = data["lastObservationReason"]
        .as_str()
        .filter(|code| safe_reason_code(code))
    {
        writeln!(out, "마지막 관측 사유: {last}")?;
    }
    print_startup_summary(out, data)?;
    match reason {
        "SERVICE_START_WAIT_EXPIRED" | "SERVICE_STATUS_WAIT_EXPIRED" => {
            writeln!(
                out,
                "관측 시간이 끝났습니다. 실행은 계속될 수 있으며, 시작 실패가 확정된 것은 아닙니다."
            )?;
            writeln!(
                out,
                "다음 조치: bxdl status --instance <인스턴스> --wait로 같은 시작 시도의 준비 상태를 이어서 확인하세요."
            )?;
        }
        "SERVICE_STATUS_QUERY_EXPIRED" => {
            writeln!(
                out,
                "이번 상태 조회의 관측 시간이 끝났습니다. 실행 여부를 확정하지 못했습니다."
            )?;
            writeln!(
                out,
                "다음 조치: bxdl status --instance <인스턴스> --wait로 현재 시도를 다시 확인하세요."
            )?;
        }
        _ if data["engineState"] == "FAILED" || data["outcome"] == "FAILED" => {
            writeln!(
                out,
                "실패가 관측됐습니다. logs 또는 diagnose의 정제한 기록을 확인하세요. 자동으로 다시 시작하지 않습니다."
            )?;
        }
        _ if data["engineState"] == "STARTING" => {
            writeln!(
                out,
                "엔진이 준비 중입니다. status --wait로 같은 시작 시도를 이어서 확인할 수 있습니다."
            )?;
        }
        _ => {}
    }
    writeln!(
        out,
        "네트워크 합의: 미검사 (로컬 READY만으로 quorum·거래 확정을 보장하지 않음)"
    )
}

fn print_startup_summary(out: &mut dyn Write, data: &Value) -> std::io::Result<()> {
    if let Some(stage) = data["startupStage"].as_str() {
        let label = match stage {
            "INPUT_PINS" => Some("등록 입력 일치 검사"),
            "PACKAGE_VERIFICATION" => Some("공급 패키지 검증"),
            "COLD_PREFLIGHT" => Some("엔진 실행 전 검사"),
            "NATIVE_CONFIGURATION" => Some("노드 설정·초기화 기록 검사"),
            "SNAPSHOT_VERIFICATION" => Some("시작 자료 사본 검증"),
            "FINAL_RECHECK" => Some("실행 직전 입력 재검증"),
            "ENGINE_EXEC" => Some("엔진 실행 전환"),
            _ => None,
        };
        if let Some(label) = label {
            writeln!(out, "마지막 시작 진입 단계: {label} ({stage})")?;
        } else {
            writeln!(out, "마지막 시작 진입 단계: 기록 해석 불가")?;
        }
        writeln!(
            out,
            "단계는 마지막 진입 기록이며 완료 여부나 지연 원인을 확정하지 않습니다."
        )?;
    }
    if let Some(reason) = data["startupFailureReason"].as_str() {
        writeln!(
            out,
            "시작 gate 실패 코드: {}",
            engine::instance::service::startup_failure_code(reason)
        )?;
    }
    Ok(())
}

fn safe_reason_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 96
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

fn print_setup_summary(out: &mut dyn Write, data: &Value) -> std::io::Result<()> {
    if data["mode"] == "INSTALL" {
        for (key, label) in [
            ("installation", "패키지 설치"),
            ("registration", "인스턴스 등록"),
            ("initialization", "저장소 초기화"),
        ] {
            writeln!(out, "{label}: {}", data[key].as_str().unwrap_or("UNKNOWN"))?;
        }
        print_service_summary(out, data)?;
        if let Some(path) = data["workspace"].as_str() {
            writeln!(out, "설치 작업 폴더: {path}")?;
        }
        return writeln!(
            out,
            "같은 작업 폴더로 setup --install --resume을 실행하면 저장 결과를 재검증합니다. 원본 패키지와 참조 자료를 보존하세요."
        );
    }
    let completed = data["completedFields"].as_array().map_or(0, Vec::len);
    let total = data["totalFields"].as_u64().unwrap_or(0);
    writeln!(out, "저장한 입력: {completed}/{total}")?;
    if let Some(name) = data["nextField"].as_str() {
        if let Some(field) = setup::FIELDS.iter().find(|field| field.name == name) {
            writeln!(out, "다음 항목: {}", field.label)?;
        }
    }
    if let Some(path) = data["workspace"].as_str() {
        writeln!(out, "초안 작업 폴더: {path}")?;
    }
    if let Some(path) = data["configPath"].as_str() {
        writeln!(out, "저장한 설정 파일: {path}")?;
    }
    if let Some(checks) = data["preflight"]["checks"].as_array() {
        let failed = checks
            .iter()
            .filter(|check| check["status"] == "FAIL")
            .count();
        let unchecked = checks
            .iter()
            .filter(|check| check["status"] == "NOT_CHECKED")
            .count();
        writeln!(out, "사전 검사: 수정 필요 {failed}개, 미검사 {unchecked}개")?;
    }
    writeln!(
        out,
        "이어서 수정하려면 같은 작업 폴더로 setup --resume을 실행하세요."
    )
}

fn dispatch_setup(
    args: &[String],
    json_mode: bool,
    interactive: bool,
    input: &mut dyn BufRead,
    prompts: &mut dyn Write,
) -> (ResultEnvelope, i32) {
    if args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h") {
        return (
            result(
                "help",
                "SUCCEEDED",
                "HELP",
                "BXDL 설정 도우미 안내",
                Some(json!({"usage": HELP})),
            ),
            0,
        );
    }
    let Some((flags, positions)) = parse(
        args,
        &[
            ("workspace", true),
            ("resume", false),
            ("from", true),
            ("output", true),
            ("non-interactive", false),
            ("install", false),
            ("network", false),
            ("network-file", true),
        ],
    ) else {
        return invalid("setup");
    };
    let automated = flags.contains_key("non-interactive");
    let resume = flags.contains_key("resume");
    if flags.contains_key("network") {
        if !positions.is_empty()
            || json_mode
            || automated
            || flags.contains_key("install")
            || !flags.contains_key("workspace")
            || (resume && (flags.contains_key("from") || flags.contains_key("network-file")))
            || (!resume && (!flags.contains_key("from") || !flags.contains_key("network-file")))
        {
            return invalid("setup network");
        }
        if !interactive {
            return failure(
                "setup network",
                BxdlError::new(
                    "SETUP_INTERACTIVE_TERMINAL_REQUIRED",
                    "네트워크 준비 도우미는 터미널에서 실행하세요.",
                ),
                2,
            );
        }
        return dispatch_network_setup(&flags, input, prompts);
    }
    if flags.contains_key("network-file") {
        return invalid("setup");
    }
    if flags.contains_key("install") {
        if !positions.is_empty()
            || json_mode
            || automated
            || flags.contains_key("output")
            || !flags.contains_key("workspace")
            || (resume && flags.contains_key("from"))
        {
            return invalid("setup");
        }
        if !interactive {
            return failure(
                "setup",
                BxdlError::new(
                    "SETUP_INTERACTIVE_TERMINAL_REQUIRED",
                    "설치 도우미는 터미널에서 실행하세요. 자동화는 기존 개별 명령을 사용하세요.",
                ),
                2,
            );
        }
        return dispatch_install_setup(&flags, input, prompts);
    }
    if !positions.is_empty()
        || (resume && flags.contains_key("from"))
        || (json_mode && !automated)
        || (automated && !resume && !flags.contains_key("from"))
    {
        return invalid("setup");
    }
    if !automated && !interactive {
        return failure(
            "setup",
            BxdlError::new(
                "SETUP_INTERACTIVE_TERMINAL_REQUIRED",
                "대화형 setup은 터미널에서 실행하세요. 자동화는 --non-interactive와 --from 또는 --resume을 사용하세요.",
            ),
            2,
        );
    }
    let workspace = match flags
        .get("workspace")
        .map(|value| setup::absolute(Path::new(value)))
        .unwrap_or_else(setup::default_workspace)
    {
        Ok(path) => path,
        Err(error) => return failure("setup", error, 2),
    };
    let output = match flags
        .get("output")
        .map(|value| setup::absolute(Path::new(value)))
        .transpose()
    {
        Ok(path) => path,
        Err(error) => return failure("setup", error, 2),
    };
    let mut session = match if resume {
        setup::Session::resume(&workspace)
    } else {
        setup::Session::create(&workspace, flags.get("from").map(Path::new))
    } {
        Ok(value) => value,
        Err(error) => return failure("setup", error, 3),
    };
    let finish = if automated {
        if !session.complete() {
            return report(
                "setup",
                "INCOMPLETE",
                "SETUP_DRAFT_INCOMPLETE",
                "필수 입력이 남아 있습니다. 대화형 setup --resume으로 이어가세요.",
                session.summary(),
                5,
            );
        }
        match output.as_deref() {
            Some(path) => session.export(path).map(setup::Finish::Exported),
            None => Ok(setup::Finish::Saved),
        }
    } else {
        setup::interact(&mut session, input, prompts, output.as_deref())
    };
    let finish = match finish {
        Ok(value) => value,
        Err(error) => {
            let exit = if matches!(
                error.code.as_str(),
                "SETUP_INPUT_FAILED" | "SETUP_OUTPUT_FAILED"
            ) {
                7
            } else {
                3
            };
            return failure("setup", error, exit);
        }
    };
    let mut summary = session.summary();
    if session.complete() {
        let checks = match &finish {
            setup::Finish::Exported(path) => config::preflight(path),
            _ => session.preflight(),
        };
        summary.preflight = match checks {
            Ok(value) => Some(value),
            Err(error) => return failure("setup", error, 3),
        };
    }
    match finish {
        setup::Finish::Paused => report(
            "setup",
            "INCOMPLETE",
            "SETUP_PAUSED",
            "초안을 보존했습니다. 같은 작업 폴더에서 --resume으로 이어가세요. 설치·초기화·시작은 수행하지 않았습니다.",
            summary,
            5,
        ),
        setup::Finish::Saved => report(
            "setup",
            "SUCCEEDED",
            "SETUP_DRAFT_SAVED",
            "설정 초안을 저장했습니다. 로컬 검사 결과를 확인하세요. 엔진·설치·실행 검사는 남아 있습니다.",
            summary,
            0,
        ),
        setup::Finish::Exported(path) => {
            summary.config_path = path.to_str().map(str::to_owned);
            report(
                "setup",
                "SUCCEEDED",
                "SETUP_CONFIG_WRITTEN",
                "새 설정 파일을 저장했습니다. 엔진·설치·실행 검사는 남아 있습니다.",
                summary,
                0,
            )
        }
    }
}

fn dispatch_network_setup(
    flags: &BTreeMap<String, String>,
    input: &mut dyn BufRead,
    prompts: &mut dyn Write,
) -> (ResultEnvelope, i32) {
    let command = "setup network";
    let workspace = Path::new(&flags["workspace"]);
    let output = match flags
        .get("output")
        .map(|path| setup::absolute(Path::new(path)))
        .transpose()
    {
        Ok(value) => value,
        Err(error) => return failure(command, error, 2),
    };
    let session = if flags.contains_key("resume") {
        setup::network::Session::resume(workspace)
    } else {
        setup::network::Session::create(
            workspace,
            Path::new(&flags["from"]),
            Path::new(&flags["network-file"]),
        )
    };
    let mut session = match session {
        Ok(value) => value,
        Err(error) => return failure(command, error, 3),
    };
    match setup::network::interact(&mut session, input, prompts, output.as_deref()) {
        Ok(finish) => {
            let mut summary = session.summary();
            if let setup::network::Finish::Exported(path) = &finish {
                summary.output_path = Some(path.clone());
            }
            let (outcome, code, message, exit) = match finish {
                setup::network::Finish::Paused => (
                    "INCOMPLETE",
                    "SETUP_NETWORK_PAUSED",
                    "네트워크 준비 초안을 보존했습니다. 같은 작업 폴더에서 --resume으로 이어가세요.",
                    5,
                ),
                setup::network::Finish::Saved => (
                    "SUCCEEDED",
                    "SETUP_NETWORK_SAVED",
                    "네트워크 준비 초안을 저장했습니다. native 파일은 아직 게시하지 않았습니다.",
                    0,
                ),
                setup::network::Finish::Exported(_) => (
                    "SUCCEEDED",
                    "SETUP_NETWORK_WRITTEN",
                    "새 native 설정 파일을 준비했습니다. 엔진 cold 검사·설치·실제 네트워크 검증은 남아 있습니다.",
                    0,
                ),
            };
            report(command, outcome, code, message, summary, exit)
        }
        Err(error) => {
            if error.code == "SETUP_NETWORK_PUBLICATION_UNCERTAIN" {
                return (
                    result(command, "UNKNOWN", &error.code, &error.message, None),
                    6,
                );
            }
            let exit = if matches!(
                error.code.as_str(),
                "SETUP_INPUT_FAILED" | "SETUP_OUTPUT_FAILED"
            ) {
                7
            } else {
                3
            };
            failure(command, error, exit)
        }
    }
}

fn dispatch_install_setup(
    flags: &BTreeMap<String, String>,
    input: &mut dyn BufRead,
    prompts: &mut dyn Write,
) -> (ResultEnvelope, i32) {
    let Some(path) = flags.get("workspace") else {
        return invalid("setup");
    };
    let workflow = if flags.contains_key("resume") {
        setup::workflow::Workflow::resume(Path::new(path))
    } else {
        setup::workflow::Workflow::create(Path::new(path), flags.get("from").map(Path::new))
    };
    let mut workflow = match workflow {
        Ok(value) => value,
        Err(error) => return failure("setup", error, 3),
    };
    match setup::workflow::interact(&mut workflow, input, prompts) {
        Ok(finished) => report(
            "setup",
            if finished { "SUCCEEDED" } else { "INCOMPLETE" },
            if finished {
                "SETUP_INSTALL_SAVED"
            } else {
                "SETUP_INSTALL_PAUSED"
            },
            "설치 작업의 단계별 상태를 보존했습니다. 미실행 단계는 자동으로 진행하지 않습니다.",
            workflow.summary(),
            if finished { 0 } else { 5 },
        ),
        Err(error) => {
            let exit = if matches!(
                error.code.as_str(),
                "SETUP_INPUT_FAILED" | "SETUP_OUTPUT_FAILED"
            ) {
                7
            } else {
                3
            };
            failure("setup", error, exit)
        }
    }
}

fn take_json(args: &[String]) -> (bool, Vec<String>) {
    let mut json_mode = false;
    let mut literal = false;
    let mut remaining = Vec::new();
    for arg in args {
        if arg == "--" {
            literal = true;
        }
        if arg == "--json" && !literal {
            json_mode = true;
        } else {
            remaining.push(arg.clone());
        }
    }
    (json_mode, remaining)
}

fn dispatch(args: &[String]) -> (ResultEnvelope, i32) {
    if args.is_empty() || (args.len() == 1 && matches!(args[0].as_str(), "help" | "--help" | "-h"))
    {
        return (
            result(
                "help",
                "SUCCEEDED",
                "HELP",
                "BXDL 명령 안내",
                Some(json!({"usage": HELP})),
            ),
            0,
        );
    }
    match args[0].as_str() {
        "version" => {
            if args.len() != 1 {
                return invalid("version");
            }
            (
                result(
                    "version",
                    "SUCCEEDED",
                    "VERSION",
                    &format!("BXDL {VERSION}"),
                    Some(json!({
                        "product": "BXDL", "version": VERSION, "revision": REVISION, "implementation": "rust",
                        "stage": "development-engine-integration", "bundleInspection": "NOT_PERFORMED",
                        "capabilities": ["runtime.stage", "package.build", "package.verify", "config.validate", "config.fingerprint", "preflight.local", "setup.draft", "setup.network", "install.macos", "engine.inspect", "engine.preflight.cold", "preflight.product-engine", "instance.register", "instance.show", "init", "resume-init", "start.macos-launchagent", "status", "status.wait", "stop.macos-launchagent", "logs.sanitized", "diagnose.offline"],
                        "primaryTarget": "darwin-arm64", "engineContractStatus": "PROPOSED_DEVELOPMENT",
                        "macosServiceAcceptance": "NOT_CHECKED", "linuxServiceAcceptance": "NOT_CHECKED"
                    })),
                ),
                0,
            )
        }
        "runtime" => {
            if args.get(1).map(String::as_str) != Some("stage") {
                return invalid("runtime");
            }
            let Some((flags, pos)) = parse(
                &args[2..],
                &[("source", true), ("lock", true), ("output", true)],
            ) else {
                return invalid("runtime stage");
            };
            if !pos.is_empty()
                || ["source", "lock", "output"]
                    .iter()
                    .any(|name| !flags.contains_key(*name))
            {
                return invalid("runtime stage");
            }
            let options = runtime::StageOptions {
                source: (&flags["source"]).into(),
                lock: (&flags["lock"]).into(),
                output: (&flags["output"]).into(),
            };
            match runtime::stage(&options) {
                Ok(value) => report(
                    "runtime stage",
                    "SUCCEEDED",
                    "RUNTIME_STAGED",
                    "신뢰 lock의 전체 runtime 파일을 검증해 새 폴더에 준비했습니다. archive 인증·JVM 실행·제품 호환성 검증은 포함하지 않습니다.",
                    value,
                    0,
                ),
                Err(error) => failure("runtime stage", error, 3),
            }
        }
        "package" => match args.get(1).map(String::as_str) {
            Some("verify") => {
                let Some((flags, pos)) = parse(
                    &args[2..],
                    &[("public-key", true), ("allow-unsigned-development", false)],
                ) else {
                    return invalid("package verify");
                };
                if pos.len() != 1
                    || flags.contains_key("public-key")
                        == flags.contains_key("allow-unsigned-development")
                {
                    return invalid("package verify");
                }
                let options = artifact::VerifyOptions {
                    public_key_path: flags.get("public-key").map(PathBuf::from),
                    allow_unsigned_development: flags.contains_key("allow-unsigned-development"),
                };
                match artifact::verify(Path::new(&pos[0]), &options) {
                    Ok(value) => report(
                        "package verify",
                        "SUCCEEDED",
                        "PACKAGE_CONTENT_VERIFIED",
                        "패키지 내용을 검증했습니다. 엔진 실행·공식 공급·호스트 호환성 검증은 포함하지 않습니다.",
                        value,
                        0,
                    ),
                    Err(error) => failure("package verify", error, 3),
                }
            }
            Some("build") => {
                let Some((flags, pos)) = parse(
                    &args[2..],
                    &[
                        ("root", true),
                        ("spec", true),
                        ("output", true),
                        ("signing-key", true),
                        ("allow-unsigned-development", false),
                    ],
                ) else {
                    return invalid("package build");
                };
                if !pos.is_empty()
                    || ["root", "spec", "output"]
                        .iter()
                        .any(|name| !flags.contains_key(*name))
                    || flags.contains_key("signing-key")
                        == flags.contains_key("allow-unsigned-development")
                {
                    return invalid("package build");
                }
                let options = artifact::BuildOptions {
                    root: PathBuf::from(&flags["root"]),
                    spec_path: PathBuf::from(&flags["spec"]),
                    output: PathBuf::from(&flags["output"]),
                    signing_key_path: flags.get("signing-key").map(PathBuf::from),
                    allow_unsigned_development: flags.contains_key("allow-unsigned-development"),
                };
                match artifact::build(&options) {
                    Ok(value) => report(
                        "package build",
                        "SUCCEEDED",
                        "DEVELOPMENT_PACKAGE_BUILT",
                        "개발용 패키지를 조립하고 검증했습니다. 공식 엔진 릴리스·실행 인수 완료가 아닙니다.",
                        value,
                        0,
                    ),
                    Err(error) => failure("package build", error, 3),
                }
            }
            _ => invalid("package"),
        },
        "install" => dispatch_install(&args[1..]),
        "engine" => dispatch_engine(&args[1..]),
        "config" => {
            if args.get(1).map(String::as_str) == Some("fingerprint") {
                let command = "config fingerprint";
                let Some((flags, pos)) = parse(&args[2..], &[("certificate", true)]) else {
                    return invalid(command);
                };
                let Some(path) = flags.get("certificate") else {
                    return invalid(command);
                };
                if !pos.is_empty() {
                    return invalid(command);
                }
                return match config::network::fingerprint(Path::new(path)) {
                    Ok(value) => report(
                        command,
                        "SUCCEEDED",
                        "TRANSPORT_FINGERPRINT_COMPUTED",
                        "공개 leaf 인증서 bytes에서 NIGO MTLS pin을 계산했습니다. 인증서 신뢰·만료·키 일치는 검증하지 않았습니다.",
                        value,
                        0,
                    ),
                    Err(error) => failure(command, error, 3),
                };
            }
            if args.get(1).map(String::as_str) != Some("validate") {
                return invalid("config");
            }
            let Some((flags, pos)) = parse(&args[2..], &[("file", true)]) else {
                return invalid("config validate");
            };
            let Some(path) = flags.get("file") else {
                return invalid("config validate");
            };
            if !pos.is_empty() {
                return invalid("config validate");
            }
            match config::validate_file(Path::new(path)) {
                Ok(value) => report(
                    "config validate",
                    "SUCCEEDED",
                    "PRODUCT_CONFIG_VALIDATED",
                    "제품 설정 형식을 검증했습니다. NIGO chain·key·DB 검증은 포함하지 않습니다.",
                    value,
                    0,
                ),
                Err(error) => failure("config validate", error, 2),
            }
        }
        "preflight" => dispatch_preflight(&args[1..]),
        "instance" => dispatch_instance(&args[1..]),
        command @ ("init" | "resume-init") => dispatch_initialization(command, &args[1..]),
        command @ ("start" | "stop" | "status") => dispatch_service(command, &args[1..]),
        "service-run" => dispatch_service_gate(&args[1..]),
        command @ ("logs" | "diagnose") => dispatch_diagnostics(command, &args[1..]),
        command @ ("upgrade" | "uninstall") => (
            result(
                command,
                "UNSUPPORTED",
                "CAPABILITY_NOT_IMPLEMENTED",
                "이 명령은 아직 제공하지 않습니다. 엔진·서비스·데이터에 작업을 수행하지 않았습니다.",
                Some(
                    json!({"required": ["NIGO 공급 계약", "macOS 설치·서비스 인수", "해당 운영 명령 구현"]}),
                ),
            ),
            4,
        ),
        _ => invalid("unknown"),
    }
}

fn dispatch_preflight(args: &[String]) -> (ResultEnvelope, i32) {
    if args
        .iter()
        .any(|arg| arg == "--instance" || arg.starts_with("--instance="))
    {
        return dispatch_registered_preflight(args);
    }
    let Some((flags, positions)) = parse(
        args,
        &[
            ("config", true),
            ("engine-config", true),
            ("jar", true),
            ("java", true),
            ("lock", true),
            ("allow-development", false),
            ("timeout-seconds", true),
        ],
    ) else {
        return invalid("preflight");
    };
    let Some(path) = flags.get("config") else {
        return invalid("preflight");
    };
    if !positions.is_empty() {
        return invalid("preflight");
    }
    let combined = flags.keys().any(|key| key != "config");
    if !combined {
        return match config::preflight(Path::new(path)) {
            Ok(value) if value.outcome == "FAIL" => report(
                "preflight",
                "FAILED",
                "LOCAL_CHECK_FAILED",
                "로컬 검사에서 문제를 발견했습니다. 검사 결과를 확인하세요.",
                value,
                4,
            ),
            Ok(value) => report(
                "preflight",
                "INCOMPLETE",
                "ENGINE_PREFLIGHT_NOT_RUN",
                "로컬 정적 검사만 수행했습니다. 엔진 검사를 연결하려면 --engine-config와 신뢰한 엔진 옵션을 함께 지정하세요.",
                value,
                5,
            ),
            Err(error) => failure("preflight", error, 2),
        };
    }
    if ["engine-config", "jar", "java", "lock", "allow-development"]
        .iter()
        .any(|key| !flags.contains_key(*key))
    {
        return invalid("preflight");
    }
    let timeout = match flags.get("timeout-seconds") {
        None => 30,
        Some(value) => match value.parse::<u64>() {
            Ok(seconds @ 1..=120) => seconds,
            _ => return invalid("preflight"),
        },
    };
    let options = engine::Options {
        jar: PathBuf::from(&flags["jar"]),
        java: PathBuf::from(&flags["java"]),
        lock: PathBuf::from(&flags["lock"]),
        timeout: std::time::Duration::from_secs(timeout),
    };
    match engine::preflight_product(
        &options,
        Path::new(path),
        Path::new(&flags["engine-config"]),
    ) {
        Ok(value) if value.outcome == "FAIL" => report(
            "preflight",
            "FAILED",
            "LOCAL_CHECK_FAILED",
            "제품 설정의 로컬 검사에서 문제를 발견했습니다. 엔진은 실행하지 않았습니다.",
            value,
            4,
        ),
        Ok(value) => report(
            "preflight",
            "INCOMPLETE",
            "ENGINE_RUNTIME_CHECKS_REQUIRED",
            "제품·엔진 설정의 일치와 엔진 cold 검사를 확인했습니다. DB/WAL·네트워크·서비스 검사는 남아 있습니다.",
            value,
            5,
        ),
        Err(error) if error.code == "ENGINE_TIMEOUT" => (
            result("preflight", "UNKNOWN", &error.code, &error.message, None),
            6,
        ),
        Err(error) => failure("preflight", error, 3),
    }
}

fn dispatch_install(args: &[String]) -> (ResultEnvelope, i32) {
    let Some((flags, positions)) = parse(
        args,
        &[
            ("destination", true),
            ("public-key", true),
            ("allow-unsigned-development", false),
        ],
    ) else {
        return invalid("install");
    };
    if positions.len() != 1
        || !flags.contains_key("destination")
        || flags.contains_key("public-key") == flags.contains_key("allow-unsigned-development")
    {
        return invalid("install");
    }
    let options = artifact::VerifyOptions {
        public_key_path: flags.get("public-key").map(PathBuf::from),
        allow_unsigned_development: flags.contains_key("allow-unsigned-development"),
    };
    match install::install(
        Path::new(&positions[0]),
        Path::new(&flags["destination"]),
        &options,
    ) {
        Ok(value) => report(
            "install",
            "SUCCEEDED",
            "PACKAGE_INSTALLED",
            "새 폴더에 패키지를 설치했습니다. 엔진 초기화·서비스 등록은 수행하지 않았습니다.",
            value,
            0,
        ),
        Err(error) if error.code == "INSTALL_COMMIT_UNCERTAIN" => (
            result("install", "UNKNOWN", &error.code, &error.message, None),
            6,
        ),
        Err(error) => failure("install", error, 3),
    }
}

fn dispatch_engine(args: &[String]) -> (ResultEnvelope, i32) {
    let Some(action) = args.first().map(String::as_str) else {
        return invalid("engine");
    };
    if !matches!(action, "inspect" | "preflight") {
        return invalid("engine");
    }
    let command = if action == "inspect" {
        "engine inspect"
    } else {
        "engine preflight"
    };
    let Some((flags, positions)) = parse(
        &args[1..],
        &[
            ("jar", true),
            ("java", true),
            ("lock", true),
            ("config", true),
            ("allow-development", false),
            ("timeout-seconds", true),
        ],
    ) else {
        return invalid(command);
    };
    if !positions.is_empty()
        || ["jar", "java", "lock", "allow-development"]
            .iter()
            .any(|key| !flags.contains_key(*key))
        || (action == "preflight") != flags.contains_key("config")
    {
        return invalid(command);
    }
    let timeout = match flags.get("timeout-seconds") {
        None => 30,
        Some(value) => match value.parse::<u64>() {
            Ok(seconds @ 1..=120) => seconds,
            _ => return invalid(command),
        },
    };
    let options = engine::Options {
        jar: PathBuf::from(&flags["jar"]),
        java: PathBuf::from(&flags["java"]),
        lock: PathBuf::from(&flags["lock"]),
        timeout: std::time::Duration::from_secs(timeout),
    };
    let response = if action == "inspect" {
        engine::inspect(&options)
    } else {
        engine::preflight(&options, Path::new(&flags["config"]))
    };
    match response {
        Ok(value) if action == "inspect" => report(
            command,
            "SUCCEEDED",
            "ENGINE_DEVELOPMENT_INSPECTED",
            "고정한 개발 후보 엔진의 식별 정보를 확인했습니다. 공식 릴리스·운영 인수 완료가 아닙니다.",
            value,
            0,
        ),
        Ok(value) => report(
            command,
            "INCOMPLETE",
            "ENGINE_RUNTIME_CHECKS_REQUIRED",
            "엔진 cold 검사를 수행했습니다. DB/WAL·네트워크·native 실행 검사는 남아 있습니다.",
            value,
            5,
        ),
        Err(error) if error.code == "ENGINE_TIMEOUT" => (
            result(command, "UNKNOWN", &error.code, &error.message, None),
            6,
        ),
        Err(error) => failure(command, error, 3),
    }
}

// Preserve positional arguments after -- and allow flags on either side of a path.
fn parse(
    args: &[String],
    allowed: &[(&str, bool)],
) -> Option<(BTreeMap<String, String>, Vec<String>)> {
    let mut flags = BTreeMap::new();
    let mut positions = Vec::new();
    let mut literal = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if literal {
            positions.push(arg.clone());
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        if !arg.starts_with('-') {
            positions.push(arg.clone());
            continue;
        }
        let raw = arg.strip_prefix("--")?;
        let (name, assigned) = match raw.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (raw, None),
        };
        let (_, needs_value) = allowed.iter().find(|(key, _)| *key == name)?;
        if flags.contains_key(name) {
            return None;
        }
        let value = if *needs_value {
            let value = match assigned {
                Some(value) => value.to_owned(),
                None => {
                    let next = iter.next()?;
                    if next.starts_with("--") {
                        return None;
                    }
                    next.clone()
                }
            };
            if value.is_empty() {
                return None;
            }
            value
        } else {
            if assigned.is_some() {
                return None;
            }
            "true".into()
        };
        flags.insert(name.into(), value);
    }
    Some((flags, positions))
}

fn timeout_flag(
    flags: &BTreeMap<String, String>,
    default: u64,
    max: u64,
) -> Option<std::time::Duration> {
    let seconds = match flags.get("timeout-seconds") {
        Some(value) => value.parse::<u64>().ok()?,
        None => default,
    };
    (1..=max)
        .contains(&seconds)
        .then_some(std::time::Duration::from_secs(seconds))
}
fn instance_failure(command: &str, error: BxdlError) -> (ResultEnvelope, i32) {
    if error.code == "INSTANCE_INITIALIZATION_UNKNOWN" || error.code == "ENGINE_TIMEOUT" {
        (
            result(command, "UNKNOWN", &error.code, &error.message, None),
            6,
        )
    } else {
        failure(command, error, 3)
    }
}
fn dispatch_instance(args: &[String]) -> (ResultEnvelope, i32) {
    match args.first().map(String::as_str) {
        Some("show") => {
            let Some((flags, pos)) = parse(&args[1..], &[("instance", true)]) else {
                return invalid("instance show");
            };
            if !pos.is_empty() || !flags.contains_key("instance") {
                return invalid("instance show");
            }
            match engine::instance::show(Path::new(&flags["instance"])) {
                Ok(value) => report(
                    "instance show",
                    "SUCCEEDED",
                    "INSTANCE_STATE_READ",
                    "저장된 인스턴스 상태입니다. 실행 중 노드의 건강 상태는 검사하지 않았습니다.",
                    value,
                    0,
                ),
                Err(e) => instance_failure("instance show", e),
            }
        }
        Some("register") => {
            let Some((flags, pos)) = parse(
                &args[1..],
                &[
                    ("instance", true),
                    ("package", true),
                    ("archive", true),
                    ("public-key", true),
                    ("allow-unsigned-development", false),
                    ("config", true),
                    ("engine-config", true),
                    ("lock", true),
                    ("allow-development", false),
                    ("timeout-seconds", true),
                ],
            ) else {
                return invalid("instance register");
            };
            if !pos.is_empty()
                || [
                    "instance",
                    "package",
                    "archive",
                    "config",
                    "engine-config",
                    "lock",
                    "allow-development",
                ]
                .iter()
                .any(|key| !flags.contains_key(*key))
                || flags.contains_key("public-key")
                    == flags.contains_key("allow-unsigned-development")
            {
                return invalid("instance register");
            }
            let Some(timeout) = timeout_flag(&flags, 30, 120) else {
                return invalid("instance register");
            };
            let options = engine::instance::RegisterOptions {
                instance: (&flags["instance"]).into(),
                package: (&flags["package"]).into(),
                archive: (&flags["archive"]).into(),
                public_key: flags.get("public-key").map(PathBuf::from),
                allow_unsigned_development: flags.contains_key("allow-unsigned-development"),
                product: (&flags["config"]).into(),
                native: (&flags["engine-config"]).into(),
                lock: (&flags["lock"]).into(),
                timeout,
            };
            match engine::instance::register(&options) {
                Ok(value) => report(
                    "instance register",
                    "SUCCEEDED",
                    "INSTANCE_REGISTERED",
                    "설치 패키지와 설정을 인스턴스로 등록했습니다. 다음 단계는 명시적 init입니다.",
                    value,
                    0,
                ),
                Err(e) => instance_failure("instance register", e),
            }
        }
        _ => invalid("instance"),
    }
}
fn dispatch_registered_preflight(args: &[String]) -> (ResultEnvelope, i32) {
    let Some((flags, pos)) = parse(args, &[("instance", true), ("timeout-seconds", true)]) else {
        return invalid("preflight");
    };
    if !pos.is_empty() || !flags.contains_key("instance") {
        return invalid("preflight");
    }
    let Some(timeout) = timeout_flag(&flags, 30, 120) else {
        return invalid("preflight");
    };
    match engine::instance::preflight(Path::new(&flags["instance"]), timeout) {
        Ok(value) => report(
            "preflight",
            "INCOMPLETE",
            "ENGINE_RUNTIME_CHECKS_REQUIRED",
            "등록한 패키지·설정과 엔진 cold 검사를 확인했습니다. DB/WAL·네트워크·서비스 검사는 남아 있습니다.",
            value,
            5,
        ),
        Err(e) => instance_failure("preflight", e),
    }
}
fn dispatch_initialization(command: &str, args: &[String]) -> (ResultEnvelope, i32) {
    let resume = command == "resume-init";
    let confirm = if resume {
        "confirm-resume"
    } else {
        "confirm-initialize"
    };
    let Some((flags, pos)) = parse(
        args,
        &[
            ("instance", true),
            (confirm, false),
            ("timeout-seconds", true),
        ],
    ) else {
        return invalid(command);
    };
    if !pos.is_empty() || !flags.contains_key("instance") || !flags.contains_key(confirm) {
        return invalid(command);
    }
    let Some(timeout) = timeout_flag(&flags, 120, 600) else {
        return invalid(command);
    };
    match engine::instance::initialize(Path::new(&flags["instance"]), resume, timeout) {
        Ok(value) if value.initialization == "INITIALIZED" => report(
            command,
            "SUCCEEDED",
            "INSTANCE_INITIALIZED",
            "저장소 초기화와 종료를 확인했습니다. 서비스 등록·노드 시작은 다음 단계입니다.",
            value,
            0,
        ),
        Ok(value) => report(
            command,
            "UNKNOWN",
            "INSTANCE_INITIALIZATION_UNKNOWN",
            "초기화 결과를 확정하지 못했습니다. 데이터와 작업 기록을 보존했습니다. instance show로 확인하세요.",
            value,
            6,
        ),
        Err(e) => instance_failure(command, e),
    }
}

fn service_failure(command: &str, error: BxdlError) -> (ResultEnvelope, i32) {
    if error.code.contains("UNKNOWN")
        || matches!(
            error.code.as_str(),
            "ENGINE_TIMEOUT"
                | "SERVICE_ATTEMPT_CHANGED"
                | "SERVICE_LAUNCHCTL_WAIT_EXPIRED"
                | "SERVICE_HTTP_WAIT_EXPIRED"
        )
    {
        (
            result(command, "UNKNOWN", &error.code, &error.message, None),
            6,
        )
    } else {
        failure(command, error, 3)
    }
}
fn dispatch_diagnostics(command: &str, args: &[String]) -> (ResultEnvelope, i32) {
    let mut accepted_flags = vec![
        ("instance", true),
        ("tail", true),
        ("max-bytes", true),
        ("timeout-seconds", true),
    ];
    if command == "diagnose" {
        accepted_flags.push(("output", true));
    }
    let Some((flags, positions)) = parse(args, &accepted_flags) else {
        return invalid(command);
    };
    if !positions.is_empty()
        || !flags.contains_key("instance")
        || (command == "diagnose" && !flags.contains_key("output"))
    {
        return invalid(command);
    }
    let tail = match flags.get("tail") {
        None => 50,
        Some(value) => match value.parse::<usize>() {
            Ok(tail @ 1..=200) => tail,
            _ => return invalid(command),
        },
    };
    let max_bytes = match flags.get("max-bytes") {
        None => 262_144,
        Some(value) => match value.parse::<u64>() {
            Ok(bytes @ 4_096..=1_048_576) => bytes,
            _ => return invalid(command),
        },
    };
    let Some(timeout) = timeout_flag(&flags, 5, 30) else {
        return invalid(command);
    };
    let options = engine::instance::diagnostics::Options {
        tail,
        max_bytes,
        timeout,
    };
    let instance = Path::new(&flags["instance"]);
    let collected = if command == "diagnose" {
        engine::instance::diagnostics::diagnose(instance, Path::new(&flags["output"]), &options)
    } else {
        engine::instance::diagnostics::logs(instance, &options)
    };
    match collected {
        Ok(value) => {
            let (outcome, exit) = if value.partial {
                ("INCOMPLETE", 5)
            } else {
                ("SUCCEEDED", 0)
            };
            let code = match (command, value.partial) {
                ("diagnose", true) => "DIAGNOSTIC_PARTIAL",
                ("diagnose", false) => "DIAGNOSTIC_WRITTEN",
                (_, true) => "LOGS_PARTIAL",
                (_, false) => "LOGS_COLLECTED",
            };
            let message = if command == "diagnose" {
                "정제한 지원 보고서를 새 파일에 저장했습니다. 누락 사유를 확인하세요. 현재 상태·실시간 health 검증은 수행하지 않았습니다."
            } else {
                "정제한 저장 기록을 수집했습니다. 누락 사유를 확인하세요. 현재 상태·실시간 health 검증은 수행하지 않았습니다."
            };
            report(command, outcome, code, message, value, exit)
        }
        Err(error) => failure(command, error, 3),
    }
}
struct ServiceOptions {
    instance: PathBuf,
    timeout: std::time::Duration,
    wait: bool,
}

fn service_options(command: &str, args: &[String]) -> Option<ServiceOptions> {
    let (flags, positions) = parse(
        args,
        &[
            ("instance", true),
            ("timeout-seconds", true),
            ("wait", false),
        ],
    )?;
    let wait = flags.contains_key("wait");
    if !positions.is_empty() || !flags.contains_key("instance") || (wait && command != "status") {
        return None;
    }
    let (default, maximum) = match command {
        "start" => (120, 600),
        "stop" => (60, 600),
        "status" if wait => (120, 600),
        "status" => (10, 120),
        _ => return None,
    };
    Some(ServiceOptions {
        instance: PathBuf::from(&flags["instance"]),
        timeout: timeout_flag(&flags, default, maximum)?,
        wait,
    })
}

fn service_wait_notice(args: &[String]) -> Option<String> {
    let command = args.first()?.as_str();
    let options = service_options(command, &args[1..])?;
    let seconds = options.timeout.as_secs();
    let action = match command {
        "start" => format!("사전 검증과 시작 요청 후 준비 상태를 최대 {seconds}초 관측합니다."),
        "status" if options.wait => format!(
            "현재 시작 시도의 준비 상태를 기다립니다. 새 실행은 요청하지 않습니다 (최대 {seconds}초)."
        ),
        _ => return None,
    };
    Some(format!(
        "{action} 관측 기한이 끝나도 실행은 계속될 수 있습니다."
    ))
}

fn dispatch_service(command: &str, args: &[String]) -> (ResultEnvelope, i32) {
    let Some(options) = service_options(command, args) else {
        return invalid(command);
    };
    let instance = options.instance.as_path();
    let result_value = match command {
        "start" => engine::instance::service::start(instance, options.timeout),
        "stop" => engine::instance::service::stop(instance, options.timeout),
        _ if options.wait => engine::instance::service::wait_ready(instance, options.timeout),
        _ => engine::instance::service::status(instance, options.timeout),
    };
    match result_value {
        Ok(value) => {
            let exit = match value.outcome.as_str() {
                "SUCCEEDED" => 0,
                "INCOMPLETE" => 5,
                "FAILED" => 4,
                _ => 6,
            };
            let outcome = value.outcome.clone();
            let reason = value.reason.clone();
            report(
                command,
                &outcome,
                &reason,
                "Mac 서비스와 해당 실행 시도의 엔진 관측 결과입니다. 로컬 READY는 네트워크 quorum·거래 확정 보장이 아닙니다.",
                value,
                exit,
            )
        }
        Err(error) => service_failure(command, error),
    }
}
fn dispatch_service_gate(args: &[String]) -> (ResultEnvelope, i32) {
    let Some((flags, positions)) = parse(args, &[("instance", true), ("attempt", true)]) else {
        return invalid("service-run");
    };
    if !positions.is_empty() || !flags.contains_key("instance") || !flags.contains_key("attempt") {
        return invalid("service-run");
    }
    match engine::instance::service::run_gate(Path::new(&flags["instance"]), &flags["attempt"]) {
        // Successful service execution replaces this process and never returns.
        Ok(()) => service_failure(
            "service-run",
            BxdlError::new(
                "SERVICE_OPERATION_UNKNOWN",
                "서비스 실행 전환을 확인하지 못했습니다.",
            ),
        ),
        Err(error) => service_failure("service-run", error),
    }
}

#[cfg(test)]
mod diagnostic_summary_tests {
    use super::*;

    #[test]
    fn human_summary_separates_recorded_events_from_missing_and_excluded_sources() {
        let report = json!({
            "initializationRecord": "INITIALIZED",
            "serviceRecord": "GATE_CONSUMED",
            "eventsOmitted": 2,
            "events": [{
                "source": "initialization.report", "sequence": 3, "command": "init",
                "status": "INITIALIZED", "reason": "STORAGE_CLOSED",
                "privateDetails": "PRIVATE-CANARY"
            }],
            "sources": [
                {"name": "initialization.report", "result": "COLLECTED", "reason": "PRIVATE_SNAPSHOT"},
                {"name": "service.stderr", "result": "WITHHELD", "reason": "RAW_TEXT_EXCLUDED_BY_POLICY"},
                {"name": "service.report", "result": "UNAVAILABLE", "reason": "MISSING_UNSAFE_OR_CHANGING_INPUT"}
            ],
            "outputCreated": true,
            "nextActions": ["현재 상태는 status로 별도 확인하세요."],
            "privatePath": "PRIVATE-CANARY"
        });
        let mut output = Vec::new();
        print_diagnostic_summary(&mut output, &report).unwrap();
        let text = String::from_utf8(output).unwrap();
        for expected in [
            "저장 상태: 초기화 INITIALIZED / 서비스 GATE_CONSUMED",
            "현재 runtime: 관측 안 함",
            "사건 기록: 1개 / 생략 2개",
            "[initialization.report #3] init: INITIALIZED (STORAGE_CLOSED)",
            "자료 제외: service.stderr (RAW_TEXT_EXCLUDED_BY_POLICY)",
            "자료 누락: service.report (MISSING_UNSAFE_OR_CHANGING_INPUT)",
            "지원 보고서: 지정한 새 파일에 저장됨",
            "다음 조치: 현재 상태는 status로 별도 확인하세요.",
        ] {
            assert!(text.contains(expected), "missing {expected}");
        }
        assert!(!text.contains("PRIVATE-CANARY"));
        assert!(!text.contains("PRIVATE_SNAPSHOT"));
        assert!(!text.starts_with('{'));
    }
}

#[cfg(test)]
mod service_ux_tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn wait_bounds_are_independent_from_single_query_and_mutation_options() {
        let single = service_options("status", &args(&["--instance", "x"])).unwrap();
        assert_eq!(single.timeout.as_secs(), 10);
        assert!(!single.wait);
        let wait = service_options("status", &args(&["--instance", "x", "--wait"])).unwrap();
        assert_eq!(wait.timeout.as_secs(), 120);
        assert!(wait.wait);
        assert!(
            service_options(
                "status",
                &args(&["--instance", "x", "--wait", "--timeout-seconds", "600"])
            )
            .is_some()
        );
        for command in ["start", "stop"] {
            assert!(service_options(command, &args(&["--instance", "x", "--wait"])).is_none());
        }
        for seconds in ["0", "601"] {
            assert!(
                service_options(
                    "status",
                    &args(&["--instance", "x", "--wait", "--timeout-seconds", seconds])
                )
                .is_none()
            );
        }
        assert!(
            service_options(
                "status",
                &args(&["--instance", "x", "--timeout-seconds", "121"])
            )
            .is_none()
        );
        assert!(service_options("status", &args(&["--instance", "x", "--wait=true"])).is_none());
        assert!(
            service_options("status", &args(&["--instance", "x", "--wait", "--wait"])).is_none()
        );
        assert!(
            service_wait_notice(&args(&[
                "start",
                "--instance",
                "x",
                "--timeout-seconds",
                "42"
            ]))
            .unwrap()
            .contains("사전 검증과 시작 요청 후 준비 상태를 최대 42초 관측합니다.")
        );
        assert!(service_wait_notice(&args(&["status", "--instance", "x"])).is_none());
        assert!(
            service_wait_notice(&args(&["status", "--instance", "x", "--wait"]))
                .unwrap()
                .contains("새 실행은 요청하지 않습니다")
        );
    }

    #[test]
    fn human_service_projection_distinguishes_expiry_from_failure_without_dumping_private_data() {
        for reason in ["SERVICE_START_WAIT_EXPIRED", "SERVICE_STATUS_WAIT_EXPIRED"] {
            let value = json!({
                "serviceState": "RUNNING", "engineState": "STARTING", "runtimeReadiness": "UNKNOWN",
                "outcome": "UNKNOWN", "reason": reason, "lastObservationReason": "SERVICE_START_PENDING",
                "attemptId": "bxdl-123-abcd-1", "health": {"token": "PRIVATE_CANARY"},
                "privatePath": "/PRIVATE_CANARY", "credential": "PRIVATE_CANARY"
            });
            let mut output = Vec::new();
            print_service_summary(&mut output, &value).unwrap();
            let text = String::from_utf8(output).unwrap();
            for expected in [
                "서비스: 실행 중 (RUNNING)",
                "엔진: 시작 중 (STARTING)",
                "시작 시도: bxdl-123-abcd-1",
                "마지막 관측 사유: SERVICE_START_PENDING",
                "관측 시간이 끝났습니다",
                "실행은 계속될 수",
                "status --instance <인스턴스> --wait",
            ] {
                assert!(text.contains(expected), "missing {expected}: {text}");
            }
            assert!(!text.contains("실패가 관측됐습니다"));
            assert!(!text.contains("PRIVATE_CANARY"));
            assert!(!text.contains("\"health\""));
        }
        let mut output = Vec::new();
        print_service_summary(&mut output, &json!({"serviceState":"EXITED", "engineState":"FAILED", "runtimeReadiness":"NOT_READY", "outcome":"FAILED", "reason":"SERVICE_EXITED_DURING_START"})).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("실패가 관측됐습니다"));
        assert!(!text.contains("관측 시간이 끝났습니다"));

        let mut output = Vec::new();
        print_service_summary(
            &mut output,
            &json!({
                "serviceState": "NOT_REGISTERED", "engineState": "NOT_STARTED",
                "runtimeReadiness": "NOT_CHECKED", "reason": "SERVICE_NOT_STARTED"
            }),
        )
        .unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("서비스: 서비스에 등록되지 않음 (NOT_REGISTERED)"));
        assert!(text.contains("엔진: 시작하지 않음 (NOT_STARTED)"));
        assert!(!text.contains("서비스: 확인되지 않음"));
    }

    #[test]
    fn projection_rejects_unrecognized_states_and_unsafe_observation_strings() {
        let mut output = Vec::new();
        print_service_summary(&mut output, &json!({
            "serviceState":"/PRIVATE_CANARY", "engineState":"PRIVATE_CANARY", "runtimeReadiness":"PRIVATE_CANARY",
            "serviceReason":"secret\nPRIVATE_CANARY", "lastObservationReason":"/PRIVATE_CANARY",
            "attemptId":"PRIVATE_CANARY\n", "health":"PRIVATE_CANARY"
        })).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(!text.contains("PRIVATE_CANARY"));
        assert!(text.contains("엔진: 확인되지 않음 (UNKNOWN)"));
        assert!(text.contains("서비스 사유: UNKNOWN"));
    }

    #[test]
    fn uncertain_service_errors_do_not_claim_engine_failure() {
        for code in [
            "ENGINE_TIMEOUT",
            "SERVICE_ATTEMPT_CHANGED",
            "SERVICE_LAUNCHCTL_WAIT_EXPIRED",
            "SERVICE_HTTP_WAIT_EXPIRED",
        ] {
            for command in ["start", "status", "stop"] {
                let (value, exit) = service_failure(
                    command,
                    BxdlError::new(code, "현재 실행 상태를 확정하지 못했습니다."),
                );
                assert_eq!(exit, 6, "{command} {code}");
                assert_eq!(value.outcome, "UNKNOWN", "{command} {code}");
                assert_eq!(value.reason_code, code);
            }
        }
    }
}

#[cfg(test)]
mod startup_summary_tests {
    use super::*;

    #[test]
    fn gate_step_and_failure_are_fixed_projections_not_a_cause_or_completion_claim() {
        let mut output = Vec::new();
        print_startup_summary(
            &mut output,
            &json!({
                "startupStage": "PACKAGE_VERIFICATION", "startupFailureReason": "ENGINE_INPUT_IO",
                "path": "/PRIVATE_CANARY", "rawError": "PRIVATE_CANARY"
            }),
        )
        .unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("마지막 시작 진입 단계: 공급 패키지 검증 (PACKAGE_VERIFICATION)"));
        assert!(text.contains("시작 gate 실패 코드: ENGINE_INPUT_IO"));
        assert!(text.contains("완료 여부나 지연 원인을 확정하지 않습니다"));
        assert!(!text.contains("PRIVATE_CANARY"));
        let mut output = Vec::new();
        print_startup_summary(
            &mut output,
            &json!({"startupStage":"PRIVATE_CANARY", "startupFailureReason":"PRIVATE_CANARY"}),
        )
        .unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("기록 해석 불가"));
        assert!(text.contains("DETAIL_WITHHELD"));
        assert!(!text.contains("PRIVATE_CANARY"));
    }
}
