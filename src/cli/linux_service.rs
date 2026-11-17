//! Explicit development backend entry points. Linux package installation and
//! user-catalog delegation remain separate from this administrator interface.
use super::*;
use crate::engine::linux_service as service;

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
                ),
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

pub(super) fn explicit(target: &TargetSpec) -> crate::error::Result<&Path> {
    target.instance.as_deref().ok_or_else(|| BxdlError::new("LINUX_SERVICE_EXPLICIT_INSTANCE_REQUIRED","Linux 개발용 서비스는 관리자 권한과 --instance <등록 경로>를 명시하세요. 이름 기반 설치·운용 연결은 후속 단계입니다."))
}

pub(super) fn diagnostics(
    command: &str,
    instance: &Path,
    output: Option<&Path>,
    max_bytes: u64,
) -> (ResultEnvelope, i32) {
    let operation = (|| -> crate::error::Result<Value> {
        let summary = service::logs(instance)?;
        let report_collected = summary.reason == "LINUX_SERVICE_SAVED_REPORT";
        let mut data = json!({
            "schemaVersion":1,"scope":"LINUX_OFFLINE_RECORD_SUMMARY","partial":true,
            "initializationRecord":summary.initialization,"serviceRecord":summary.engine_state,
            "runtimeObservation":"NOT_CHECKED","operationBusy":summary.operation_busy,
            "events":[],"eventsOmitted":0,"record":summary,"outputCreated":false,
            "sources":[{"name":"ROOT_CONTROL_RECORD","result":"COLLECTED","reason":"BOUNDED_STATE_PROJECTION"},
                {"name":"TERMINAL_ENGINE_REPORT","result":if report_collected {"COLLECTED"} else {"UNAVAILABLE"},"reason":if report_collected {"VALIDATED_SAVED_REPORT"} else {"NOT_VERIFIED"}},
                {"name":"FULL_EVENT_HISTORY","result":"UNAVAILABLE","reason":"NOT_COLLECTED"},
                {"name":"SYSTEM_JOURNAL_AND_RAW_OUTPUT","result":"WITHHELD","reason":"RAW_OUTPUT_EXCLUDED"}],
            "nextActions":["실시간 상태는 status --instance <등록 경로>로 별도 확인하세요.","이 단계의 로그는 저장 상태·최종 report 요약이며 전체 사건/시스템 로그 수집은 후속입니다."]
        });
        if let Some(output) = output {
            data["outputCreated"] = json!(true);
            let raw = serde_json::to_vec_pretty(&data).map_err(|_| {
                BxdlError::new("LINUX_DIAGNOSTIC_INVALID", "정제 결과를 만들지 못했습니다.")
            })?;
            if raw.len() as u64 > max_bytes {
                return Err(BxdlError::new(
                    "LINUX_DIAGNOSTIC_LIMIT",
                    "정제 결과가 지정한 크기를 초과했습니다.",
                ));
            }
            write_diagnostic(instance, output, &raw)?;
        }
        Ok(data)
    })();
    match operation {
        Ok(data) => report(
            command,
            "INCOMPLETE",
            "LINUX_DIAGNOSTIC_SUMMARY_PARTIAL",
            "저장 상태와 최종 보고서 요약만 수집했습니다. 현재 노드 상태·전체 로그는 검증하지 않았습니다.",
            data,
            5,
        ),
        Err(error) => service_failure(command, error),
    }
}
fn write_diagnostic(instance: &Path, destination: &Path, raw: &[u8]) -> crate::error::Result<()> {
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
    };
    let invalid = || {
        BxdlError::new(
            "LINUX_DIAGNOSTIC_OUTPUT_UNSAFE",
            "제어·데이터 영역 밖의 새 출력 파일과 root가 보호하는 상위 경로를 사용하세요.",
        )
    };
    let path = setup::absolute(destination).map_err(|_| invalid())?;
    let control = setup::absolute(instance).map_err(|_| invalid())?;
    if path.starts_with(&control) || control.starts_with(&path) {
        return Err(invalid());
    }
    for ancestor in path.ancestors().skip(1) {
        let m = fs::symlink_metadata(ancestor).map_err(|_| invalid())?;
        // A root-owned sticky /tmp may hold a new root file. Non-root directory
        // ancestors are never admitted, even if the final file is create_new.
        if !m.is_dir() || m.uid() != 0 || (m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0) {
            return Err(invalid());
        }
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC).bits() as i32)
        .open(&path)
        .map_err(|_| invalid())?;
    file.write_all(raw)
        .and_then(|_| file.sync_all())
        .map_err(|_| invalid())?;
    std::fs::File::open(path.parent().ok_or_else(invalid)?)
        .and_then(|f| f.sync_all())
        .map_err(|_| invalid())
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
    #[test]
    fn explicit_target_cannot_fall_back_to_user_catalog() {
        let flags = BTreeMap::new();
        let target = TargetSpec::parse(&flags, &["node".into()]).unwrap();
        assert_eq!(
            explicit(&target).unwrap_err().code,
            "LINUX_SERVICE_EXPLICIT_INSTANCE_REQUIRED"
        );
    }
}
