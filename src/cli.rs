//! Local operator interface. No engine, service manager, or network is invoked.
use crate::{artifact, config, error::BxdlError};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REVISION: &str = match option_env!("BXDL_REVISION") {
    Some(v) => v,
    None => "development",
};
pub const HELP: &str = "BXDL — 패키징·운영 CLI (development, Rust)\n\n사용법:\n  bxdl version [--json]\n  bxdl package build --root <dir> --spec <json> --output <tar.gz>\n      (--signing-key <private.pem> | --allow-unsigned-development) [--json]\n  bxdl package verify <tar.gz>\n      (--public-key <trusted.pem> | --allow-unsigned-development) [--json]\n  bxdl config validate --file <instance.json> [--json]\n  bxdl preflight --config <instance.json> [--json]\n\n서명 검증 key는 패키지 밖의 신뢰한 경로에서 제공하세요.\ndevelopment package 검증은 엔진 실행·공식 공급·OS 서비스 지원 검증이 아닙니다.\nmacOS arm64를 첫 설치·운용 UX 대상으로 하며 Linux/Docker는 후속입니다.\npreflight는 로컬 정적 검사입니다. NIGO canonical 검사는 NOT_CHECKED입니다.\ninstall/init/start/stop/status/logs/diagnose/upgrade/uninstall은 아직 제공하지 않습니다.\n";

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
    let (json_mode, args) = take_json(args);
    let (mut response, exit) = dispatch(&args);
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
        if serde_json::to_writer_pretty(&mut *out, &data).is_err() || writeln!(out).is_err() {
            return 7;
        }
    }
    exit
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
                        "stage": "development-foundation", "bundleInspection": "NOT_PERFORMED",
                        "capabilities": ["package.build", "package.verify", "config.validate", "preflight.local"],
                        "primaryTarget": "darwin-arm64", "engineContractStatus": "NOT_DELIVERED",
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
                    "ENGINE_PREFLIGHT_UNAVAILABLE",
                    "로컬 정적 검사만 수행했습니다. NIGO cold 검사 계약이 없어 기동 가능 여부는 미확인입니다.",
                    value,
                    5,
                ),
                Err(error) => failure("preflight", error, 2),
            }
        }
        command @ ("install" | "init" | "start" | "stop" | "status" | "logs" | "diagnose"
        | "upgrade" | "uninstall") => (
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
