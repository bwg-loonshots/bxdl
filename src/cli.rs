//! Operator interface. Engine commands invoke only pinned, bounded cold JVM commands.
use crate::{artifact, config, engine, error::BxdlError, install, setup};
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
pub const HELP: &str = "BXDL — 패키징·운영 CLI (development, Rust)\n\n사용법:\n  bxdl version [--json]\n  bxdl package build --root <dir> --spec <json> --output <tar.gz>\n      (--signing-key <private.pem> | --allow-unsigned-development) [--json]\n  bxdl package verify <tar.gz>\n      (--public-key <trusted.pem> | --allow-unsigned-development) [--json]\n  bxdl install <tar.gz> --destination <new-dir>\n      (--public-key <trusted.pem> | --allow-unsigned-development) [--json]\n  bxdl engine inspect --jar <jar> --java <java> --lock <json> --allow-development\n      [--timeout-seconds <1..120>] [--json]\n  bxdl engine preflight --jar <jar> --java <java> --lock <json> --allow-development\n      --config <nigo-node.json> [--timeout-seconds <1..120>] [--json]\n  bxdl config validate --file <instance.json> [--json]\n  bxdl preflight --config <instance.json> [--json]\n  bxdl setup [--workspace <dir>] [--resume] [--from <instance.json>]\n      [--output <new-instance.json>]\n  bxdl setup --workspace <dir> (--from <instance.json> | --resume)\n      --non-interactive [--output <new-instance.json>] [--json]\n\n서명 검증 key는 패키지 밖의 신뢰한 경로에서 제공하세요.\ndevelopment package 검증은 엔진 실행·공식 공급·OS 서비스 지원 검증이 아닙니다.\nmacOS arm64를 첫 설치·운용 UX 대상으로 하며 Linux/Docker는 후속입니다.\nsetup은 설정 초안·로컬 검사·파일 저장만 수행합니다. 설치·초기화·시작은 하지 않습니다.\npreflight는 BXDL 제품 설정의 로컬 정적 검사입니다.\nengine preflight는 NIGO node.json을 읽고 cold 검사를 수행하며 INCOMPLETE를 유지합니다.\ninstall은 macOS arm64 새 폴더에 검증한 파일만 설치합니다. 서비스·초기화는 수행하지 않습니다.\ninit/start/stop/status/logs/diagnose/upgrade/uninstall은 아직 제공하지 않습니다.\n";

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
        if response.command == "setup" {
            return if print_setup_summary(out, &data).is_ok() {
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

fn print_setup_summary(out: &mut dyn Write, data: &Value) -> std::io::Result<()> {
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
        ],
    ) else {
        return invalid("setup");
    };
    let automated = flags.contains_key("non-interactive");
    let resume = flags.contains_key("resume");
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
                        "capabilities": ["package.build", "package.verify", "config.validate", "preflight.local", "setup.draft", "install.macos", "engine.inspect", "engine.preflight.cold"],
                        "primaryTarget": "darwin-arm64", "engineContractStatus": "PROPOSED_DEVELOPMENT",
                        "macosServiceAcceptance": "NOT_CHECKED", "linuxServiceAcceptance": "NOT_CHECKED"
                    })),
                ),
                0,
            )
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
        "preflight" => {
            let Some((flags, pos)) = parse(&args[1..], &[("config", true)]) else {
                return invalid("preflight");
            };
            let Some(path) = flags.get("config") else {
                return invalid("preflight");
            };
            if !pos.is_empty() {
                return invalid("preflight");
            }
            match config::preflight(Path::new(path)) {
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
                    "로컬 정적 검사만 수행했습니다. NIGO node.json과 신뢰 lock으로 engine preflight를 별도 실행하세요.",
                    value,
                    5,
                ),
                Err(error) => failure("preflight", error, 2),
            }
        }
        command @ ("init" | "start" | "stop" | "status" | "logs" | "diagnose" | "upgrade"
        | "uninstall") => (
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
