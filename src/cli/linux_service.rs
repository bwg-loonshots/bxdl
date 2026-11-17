//! Explicit development backend entry points. Linux package installation and
//! user-catalog delegation remain separate from this administrator interface.
use super::*;
use crate::engine::linux_service as service;
use std::io::Read as _;

pub(super) fn dispatch(args: &[String]) -> (ResultEnvelope, i32) {
    match args.first().map(String::as_str) {
        Some("account") => {
            let command = "linux-service account";
            let Some((flags, positions)) = parse(&args[1..], &[("instance", true)]) else {
                return invalid(command);
            };
            if !positions.is_empty() || !flags.contains_key("instance") {
                return invalid(command);
            }
            match service::service_account(Path::new(&flags["instance"])) {
                Ok(account) => report(
                    command,
                    "SUCCEEDED",
                    "LINUX_SERVICE_ACCOUNT_NAME",
                    "이 인스턴스 전용 계정 이름입니다. 계정·서비스를 생성하지 않았습니다.",
                    json!({"account":account,"serviceMutations":0}),
                    0,
                ),
                Err(error) => service_failure(command, error),
            }
        }
        Some("register") => {
            let command = "linux-service register";
            let Some((flags, positions)) = parse(
                &args[1..],
                &[
                    ("instance", true),
                    ("spec", true),
                    ("allow-development", false),
                    ("timeout-seconds", true),
                ],
            ) else {
                return invalid(command);
            };
            if !positions.is_empty()
                || ["instance", "spec", "allow-development"]
                    .iter()
                    .any(|key| !flags.contains_key(*key))
            {
                return invalid(command);
            }
            let Some(timeout) = timeout_flag(&flags, 120, 600) else {
                return invalid(command);
            };
            response(
                command,
                service::register(
                    Path::new(&flags["instance"]),
                    Path::new(&flags["spec"]),
                    timeout,
                )
                .and_then(|summary| {
                    service::management::publish(Path::new(&flags["instance"]))?;
                    Ok(summary)
                }),
            )
        }
        _ => invalid("linux-service"),
    }
}

pub(super) fn gate(args: &[String]) -> (ResultEnvelope, i32) {
    let command = "systemd-run";
    let Some((flags, positions)) = parse(args, &[("control", true), ("attempt", true)]) else {
        return invalid(command);
    };
    if !positions.is_empty() || !flags.contains_key("control") || !flags.contains_key("attempt") {
        return invalid(command);
    }
    match service::run_gate(Path::new(&flags["control"]), &flags["attempt"]) {
        Err(error) => service_failure(command, error),
        Ok(()) => service_failure(
            command,
            BxdlError::new(
                "LINUX_SERVICE_EXECUTION_UNKNOWN",
                "서비스 실행 전환을 확인하지 못했습니다.",
            ),
        ),
    }
}

pub(super) fn exit_hook(args: &[String]) -> (ResultEnvelope, i32) {
    let command = "systemd-exit";
    let Some((flags, positions)) = parse(args, &[("control", true), ("attempt", true)]) else {
        return invalid(command);
    };
    if !positions.is_empty() || !flags.contains_key("control") || !flags.contains_key("attempt") {
        return invalid(command);
    }
    match service::record_exit(Path::new(&flags["control"]), &flags["attempt"]) {
        Ok(()) => report(
            command,
            "SUCCEEDED",
            "LINUX_SERVICE_OS_EXIT_RECORDED",
            "OS 종료 근거를 기록했습니다. 엔진 정상 종료 판정은 별도입니다.",
            json!({"engineClosure":"NOT_CHECKED"}),
            0,
        ),
        Err(error) => service_failure(command, error),
    }
}

pub(super) fn response(
    command: &str,
    result: crate::error::Result<service::Summary>,
) -> (ResultEnvelope, i32) {
    match result {
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
                "Linux 개발용 시스템 서비스의 관측 결과입니다. 실제 Linux 인수·전역 합의는 별도 검증 대상입니다.",
                value,
                exit,
            )
        }
        Err(error) => service_failure(command, error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(input: &[&str]) -> Vec<String> {
        input.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn registration_requires_explicit_development_approval() {
        let (_, exit) = dispatch(&args(&[
            "register",
            "--instance",
            "/var/lib/bxdl/test",
            "--spec",
            "/tmp/spec.json",
        ]));
        assert_ne!(exit, 0);
    }
    #[test]
    fn account_name_is_a_read_only_planning_operation() {
        let (first, exit) = dispatch(&args(&["account", "--instance", "/var/lib/bxdl/test"]));
        assert_eq!(exit, 0);
        let data = first.data.unwrap();
        assert_eq!(data["serviceMutations"], 0);
        assert!(data["account"].as_str().unwrap().starts_with("bxdl-"));
    }
    #[test]
    fn gate_rejects_extra_arguments_before_touching_control() {
        let (_, exit) = gate(&args(&[
            "--control",
            "/var/lib/bxdl/test",
            "--attempt",
            "test",
            "--java",
            "/bin/sh",
        ]));
        assert_ne!(exit, 0);
    }
}

/// The internal worker reply is intentionally not a normal CLI envelope.
pub(super) fn execute_internal(
    args: &[String],
    json_mode: bool,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> i32 {
    let operation = (|| -> crate::error::Result<service::management::Reply> {
        let (flags, pos) = parse(args, &[("control", true)]).ok_or_else(protocol_arguments)?;
        if !json_mode || !pos.is_empty() || !flags.contains_key("control") {
            return Err(protocol_arguments());
        }
        let mut raw = Vec::new();
        std::io::Read::take(input, 8193)
            .read_to_end(&mut raw)
            .map_err(|_| protocol_arguments())?;
        service::management::execute_internal(Path::new(&flags["control"]), &raw)
    })();
    match operation {
        Ok(reply) => {
            if serde_json::to_writer(&mut *out, &reply).is_ok() && writeln!(out).is_ok() {
                reply.exit
            } else {
                7
            }
        }
        Err(error) => {
            let (mut response, exit) = service_failure("linux-execute", error);
            response.observed_at = OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_default();
            if serde_json::to_writer(&mut *out, &response).is_ok() && writeln!(out).is_ok() {
                exit
            } else {
                7
            }
        }
    }
}
fn protocol_arguments() -> BxdlError {
    BxdlError::new(
        "LINUX_EXECUTOR_ARGUMENTS_INVALID",
        "고정 실행기 내부 요청의 형식이 올바르지 않습니다.",
    )
}

pub(super) fn install(args: &[String]) -> (ResultEnvelope, i32) {
    let command = "linux-install";
    let Some((flags, pos)) = parse(
        args,
        &[
            ("request", true),
            ("sha256", true),
            ("operator-uid", true),
            ("timeout-seconds", true),
        ],
    ) else {
        return invalid(command);
    };
    if !pos.is_empty()
        || ["request", "sha256", "operator-uid"]
            .iter()
            .any(|k| !flags.contains_key(*k))
    {
        return invalid(command);
    }
    let Some(timeout) = timeout_flag(&flags, 600, 600) else {
        return invalid(command);
    };
    let Ok(uid) = flags["operator-uid"].parse::<u32>() else {
        return invalid(command);
    };
    match service::install::apply(Path::new(&flags["request"]), &flags["sha256"], uid, timeout) {
        Ok(receipt) => report(
            command,
            "SUCCEEDED",
            "LINUX_INSTALL_REGISTERED",
            "Linux 개발용 등록을 완료했습니다. 초기화·시작은 별도입니다.",
            receipt,
            0,
        ),
        Err(error) => service_failure(command, error),
    }
}

pub(super) fn read_command(command: &str, args: &[String]) -> (ResultEnvelope, i32) {
    let Some((flags, pos)) = parse(
        args,
        &[
            ("instance", true),
            ("home", true),
            ("timeout-seconds", true),
        ],
    ) else {
        return invalid(command);
    };
    let Some(target) = TargetSpec::parse(&flags, &pos) else {
        return invalid(command);
    };
    let Some(timeout) = timeout_flag(&flags, 30, 120) else {
        return invalid(command);
    };
    let request = if command == "instance show" {
        service::management::Request::Show
    } else {
        service::management::Request::Preflight {
            timeout_seconds: timeout.as_secs(),
        }
    };
    managed(command, target, &request, None)
}
pub(super) fn managed(
    command: &str,
    target: TargetSpec,
    request: &service::management::Request,
    output: Option<&Path>,
) -> (ResultEnvelope, i32) {
    let operation = (|| -> crate::error::Result<service::management::Response> {
        let target = target.resolve()?;
        target.recheck()?;
        let descriptor = service::management::inspect(target.path())?;
        // Normal commands never consume stdin; a missing sudo credential
        // produces an actionable sudo -v instruction before operation dispatch.
        target.recheck()?;
        if service::management::inspect(target.path())? != descriptor {
            return Err(protocol_arguments());
        }
        let mut response = service::management::execute(target.path(), request)?;
        target.recheck()?;
        if let Some(output) = output {
            if matches!(response.exit, 0 | 5) {
                let data = response.data.as_mut().ok_or_else(protocol_arguments)?;
                data["outputCreated"] = json!(true);
                let raw = serde_json::to_vec_pretty(data).map_err(|_| protocol_arguments())?;
                if let service::management::Request::Logs { max_bytes, .. } = request {
                    if raw.len() as u64 > *max_bytes {
                        return Err(BxdlError::new(
                            "LINUX_DIAGNOSTIC_LIMIT",
                            "정제 보고서가 지정한 출력 한도를 초과했습니다.",
                        ));
                    }
                }
                service::diagnostics::write_export(target.path(), output, &raw)?;
            }
        }
        Ok(response)
    })();
    match operation {
        Ok(value) => (
            result(
                command,
                &value.outcome,
                &value.reason_code,
                &value.message,
                value.data,
            ),
            value.exit,
        ),
        Err(error) => service_failure(command, error),
    }
}

pub(super) fn setup(
    args: &[String],
    json_mode: bool,
    interactive: bool,
    input: &mut dyn BufRead,
    prompts: &mut dyn Write,
) -> (ResultEnvelope, i32) {
    use operator::linux_setup as ux;
    let Some(action) = args.first().map(String::as_str) else {
        return invalid("setup");
    };
    let command = format!("setup {action}");
    let mut accepted = vec![("home", true), ("linux", false)];
    if action == "new" {
        accepted.extend([
            ("from", true),
            ("runtime", true),
            ("jar", true),
            ("native-config", true),
        ]);
    }
    if action == "edit" {
        accepted.push(("field", true));
    }
    let Some((flags, pos)) = parse(&args[1..], &accepted) else {
        return invalid(&command);
    };
    if (action == "list" && !pos.is_empty())
        || (action != "list" && (pos.len() != 1 || !operator::catalog::valid_name(&pos[0])))
        || (action == "edit" && !flags.contains_key("field"))
    {
        return invalid(&command);
    }
    if !matches!(action, "list" | "show") && (json_mode || !interactive) {
        return failure(
            &command,
            BxdlError::new(
                "SETUP_INTERACTIVE_TERMINAL_REQUIRED",
                "Linux 설치 문답·적용은 일반 사용자 터미널에서 실행하세요. 조회에는 list/show --json을 사용하세요.",
            ),
            2,
        );
    }
    // Linux control paths are evaluated on the actual target filesystem.
    if !cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) && !matches!(action, "list" | "show")
    {
        return failure(
            &command,
            BxdlError::new(
                "LINUX_SETUP_PLATFORM_UNSUPPORTED",
                "Linux 설치 준비·적용은 실제 Linux 머신에서 실행하세요.",
            ),
            3,
        );
    }
    let home = match flags
        .get("home")
        .map(|v| setup::absolute(Path::new(v)))
        .unwrap_or_else(operator::home::default_home)
    {
        Ok(v) => v,
        Err(e) => return failure(&command, e, 2),
    };
    let value = match action {
        "list" | "show" => {
            let value = if action == "list" {
                ux::list(&home)
            } else {
                ux::show(&home, &pos[0])
            };
            return match value {
                Ok(data) => report(
                    &command,
                    "SUCCEEDED",
                    "LINUX_SETUP_RECORD_READ",
                    "저장된 Linux 설치 준비입니다. 현재 서비스 상태는 조회하지 않았습니다.",
                    data,
                    0,
                ),
                Err(e) => failure(&command, e, 3),
            };
        }
        "new" | "resume" => ux::run(
            ux::Options {
                home: &home,
                name: &pos[0],
                resume: action == "resume",
                from: flags.get("from").map(Path::new),
                runtime: flags.get("runtime").map(Path::new),
                jar: flags.get("jar").map(Path::new),
                native: flags.get("native-config").map(Path::new),
            },
            input,
            prompts,
        ),
        "edit" => ux::edit(&home, &pos[0], &flags["field"], input, prompts),
        "apply" => ux::apply(&home, &pos[0], input, prompts),
        _ => return invalid(&command),
    };
    match value {
        Ok(v) => report(&command, v.outcome, v.code, v.message, v.data, v.exit),
        Err(e) => service_failure(&command, e),
    }
}
pub(super) fn print_setup(out: &mut dyn Write, data: &Value) -> std::io::Result<()> {
    if let Some(entries) = data["entries"].as_array() {
        if entries.is_empty() {
            writeln!(
                out,
                "저장한 Linux 설치 준비가 없습니다. bxdl setup new <이름>으로 시작하세요."
            )?;
        }
        for e in entries {
            writeln!(
                out,
                "{} · 준비 {} · 설치 {}",
                e["name"].as_str().unwrap_or("?"),
                e["preparation"].as_str().unwrap_or("UNKNOWN"),
                e["installation"].as_str().unwrap_or("NOT_STARTED")
            )?;
        }
    } else {
        for (key, label) in [
            ("name", "이름"),
            ("preparation", "준비"),
            ("installation", "설치"),
            ("control", "시스템 노드"),
            ("dataDirectory", "데이터"),
            ("serviceAccount", "전용 계정"),
            ("reviewedInputs", "검토 자료"),
        ] {
            if let Some(value) = data[key].as_str() {
                writeln!(out, "{label}: {value}")?;
            }
        }
        if let Some(fields) = data["fields"].as_array() {
            for field in fields {
                if let Some(value) = field["value"].as_str() {
                    writeln!(
                        out,
                        "{}: {}",
                        field["label"].as_str().unwrap_or("?"),
                        operator::prompt::safe_default(value)
                    )?;
                }
            }
        }
        for key in ["missingInputs", "missingFields"] {
            if let Some(values) = data[key].as_array().filter(|v| !v.is_empty()) {
                writeln!(
                    out,
                    "남은 입력: {}",
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }
        }
    }
    writeln!(
        out,
        "실시간 상태: 미조회 · Linux 개발용 공급물 (정식 패키지 인수 전)"
    )
}

#[cfg(test)]
mod ux_tests {
    use super::*;
    use std::io::Cursor;
    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn machine_or_nonterminal_setup_never_reads_input_or_creates_home() {
        let temp = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(temp.path())
            .unwrap()
            .join("absent-home");
        for (json, interactive) in [(false, false), (true, true)] {
            let mut words = args(&[
                "setup",
                "new",
                "local",
                "--linux",
                "--home",
                home.to_str().unwrap(),
            ]);
            if json {
                words.push("--json".into());
            }
            let mut input = Cursor::new(b"yes\nyes\n");
            let (mut out, mut err) = (Vec::new(), Vec::new());
            assert_eq!(
                run_with_input(&words, &mut input, interactive, &mut out, &mut err),
                2
            );
            assert_eq!(input.position(), 0);
            assert!(!home.exists());
            if json {
                let value: Value = serde_json::from_slice(&out).unwrap();
                assert_eq!(value["reasonCode"], "SETUP_INTERACTIVE_TERMINAL_REQUIRED");
            }
        }
    }
    #[test]
    fn linux_list_missing_home_is_read_only_single_json() {
        let temp = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(temp.path()).unwrap().join("absent");
        let mut input = Cursor::new(b"must not read");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let exit = run_with_input(
            &args(&[
                "setup",
                "list",
                "--linux",
                "--home",
                home.to_str().unwrap(),
                "--json",
            ]),
            &mut input,
            false,
            &mut out,
            &mut err,
        );
        assert_eq!(exit, 0);
        assert_eq!(input.position(), 0);
        assert!(!home.exists());
        assert!(err.is_empty());
        let value: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["data"]["platform"], "linux-amd64");
        assert_eq!(value["data"]["entries"], json!([]));
    }
    #[test]
    fn internal_executor_rejects_injected_flags_before_reading_stdin() {
        let mut input = Cursor::new(vec![b'x'; 9000]);
        let mut out = Vec::new();
        assert_ne!(
            execute_internal(
                &args(&[
                    "--control",
                    "/var/lib/bxdl/instances/local",
                    "--shell",
                    "/bin/sh"
                ]),
                true,
                &mut input,
                &mut out
            ),
            0
        );
        assert_eq!(input.position(), 0);
        let value: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["reasonCode"], "LINUX_EXECUTOR_ARGUMENTS_INVALID");
    }
    #[test]
    fn internal_executor_stdin_is_bounded() {
        let mut input = Cursor::new(vec![b'x'; 9000]);
        let mut out = Vec::new();
        assert_ne!(
            execute_internal(
                &args(&["--control", "/var/lib/bxdl/instances/local"]),
                true,
                &mut input,
                &mut out
            ),
            0
        );
        assert_eq!(input.position(), 8193);
        assert!(out.len() < 2048);
    }
    #[test]
    fn system_registration_list_does_not_claim_initialization() {
        let mut out = Vec::new();
        print_nodes_summary(
            &mut out,
            &json!({"name":"local","recordedState":"SYSTEM_REGISTERED"}),
            true,
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("초기화/실행 상태 별도 조회"));
        assert!(!text.contains("초기화 전"));
    }
}
