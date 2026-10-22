//! Human guidance derived only from public result projections. This module does
//! not authorize an operation, query a service, or change persisted state.
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct View {
    pub description: &'static str,
    pub next_actions: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    NotObserved,
    NotStarted,
    Starting,
    Ready,
    NotReady,
    WaitExpired,
    QueryExpired,
    StopUnconfirmed,
    StoppedNeedsConfirmation,
    StoppedVerified,
    Failed,
    GuiUnavailable,
    Unknown,
    ColdIncomplete,
    LocalIncomplete,
    InputCorrection,
    CliMismatch,
    InitializationUnknown,
    OperationFailed,
}

impl State {
    fn view(self) -> View {
        let (description, next_actions): (_, &'static [&'static str]) = match self {
            Self::NotObserved => (
                "서비스 상태를 아직 관측하지 않았습니다.",
                &[
                    "등록된 인스턴스가 있다면 bxdl status --instance <인스턴스>로 현재 상태를 확인하세요.",
                ],
            ),
            Self::NotStarted => (
                "이 인스턴스에서 서비스 시작을 요청한 기록이 없습니다.",
                &[
                    "instance show로 초기화 결과를 확인하세요. 초기화가 확인된 경우에만 별도로 시작을 선택하세요.",
                ],
            ),
            Self::Starting => (
                "엔진이 준비 중입니다. 시작 완료는 아직 확인되지 않았습니다.",
                &["bxdl status --instance <인스턴스> --wait로 같은 시작 시도를 이어서 확인하세요."],
            ),
            Self::Ready => (
                "현재 관측에서 로컬 노드가 준비됐습니다. 네트워크 quorum·거래 확정은 별도 확인이 필요합니다.",
                &[
                    "상태가 필요할 때 다시 조회하세요. 이후 관측 없이 이 결과를 계속 현재 상태로 사용하지 마세요.",
                ],
            ),
            Self::NotReady => (
                "프로세스는 실행 중이지만 로컬 준비 조건을 아직 충족하지 못했습니다.",
                &[
                    "bxdl status --instance <인스턴스> --wait로 같은 시작 시도를 관측하세요. 계속 지연되면 logs 또는 diagnose의 정제한 기록을 확인하세요.",
                ],
            ),
            Self::WaitExpired => (
                "관측 시간이 끝났습니다. 실행은 계속될 수 있으며, 시작 실패가 확정된 것은 아닙니다.",
                &[
                    "bxdl status --instance <인스턴스> --wait로 같은 시작 시도의 준비 상태를 이어서 확인하세요.",
                ],
            ),
            Self::QueryExpired => (
                "이번 상태 조회의 관측 시간이 끝났습니다. 실행 여부를 확정하지 못했습니다.",
                &["bxdl status --instance <인스턴스> --wait로 현재 시도를 다시 확인하세요."],
            ),
            Self::StopUnconfirmed => (
                "종료 대기 시간이 끝났습니다. 노드 종료는 아직 확인되지 않았습니다.",
                &[
                    "status로 현재 시도를 확인하고 필요하면 stop을 명시적으로 다시 요청하세요. 종료가 확인되기 전에는 새 start를 요청하지 마세요.",
                ],
            ),
            Self::StoppedNeedsConfirmation => (
                "엔진 종료는 관측됐지만, 다음 시작에 필요한 정상 종료 확인 절차가 남아 있습니다.",
                &[
                    "bxdl stop --instance <인스턴스>으로 정상 종료 확인을 요청하세요. 확인 전에는 다시 시작하지 마세요.",
                ],
            ),
            Self::StoppedVerified => (
                "정상 종료가 확인됐습니다.",
                &[
                    "같은 DB로 다시 실행하려면 별도로 start를 선택하세요. 실행 직전에 대상과 입력을 다시 검사합니다.",
                ],
            ),
            Self::Failed => (
                "실패가 관측됐습니다. 자동으로 다시 시작하지 않습니다.",
                &[
                    "logs 또는 diagnose의 정제한 기록에서 실패 사유를 확인하세요. 시작 진입 단계만으로 원인을 단정하지 마세요.",
                ],
            ),
            Self::GuiUnavailable => (
                "Mac 사용자 로그인 세션을 확인할 수 없어 서비스 상태를 확정하지 못했습니다.",
                &[
                    "설치한 사용자의 GUI 로그인 세션에서 status를 다시 실행하세요. 저장 기록은 logs 또는 diagnose로 확인할 수 있습니다.",
                ],
            ),
            Self::Unknown => (
                "현재 근거만으로 작업 또는 서비스 상태를 확정할 수 없습니다.",
                &[
                    "기존 자료와 기록을 보존하세요. 등록된 인스턴스는 status 또는 instance show로 다시 확인하고, 불명이 계속되면 정제한 diagnose 보고서로 지원 검토를 받으세요. 초기화·시작을 반복하지 마세요.",
                ],
            ),
            Self::ColdIncomplete => (
                "엔진 cold 검사는 확인됐습니다. INCOMPLETE는 남은 runtime 검사를 뜻하며 노드 시작 실패를 뜻하지 않습니다.",
                &[
                    "설치·등록 상태를 확인한 뒤 DB 초기화와 서비스 시작을 각각 명시적으로 선택하세요. 로컬 READY와 네트워크 합의는 별도 검증합니다.",
                ],
            ),
            Self::LocalIncomplete => (
                "로컬 정적 검사만 수행했습니다. 엔진 cold 검사는 아직 실행하지 않았습니다.",
                &["필요한 자료를 준비하고 preflight --help에서 엔진 검사 입력을 확인하세요."],
            ),
            Self::InputCorrection => (
                "입력 또는 준비자료를 수정해야 합니다.",
                &[
                    "해당 명령의 --help를 확인하세요. 도우미에서는 edit으로 문제가 있는 항목만 수정하고 다시 검사하세요.",
                ],
            ),
            Self::CliMismatch => (
                "현재 CLI와 패키지에 고정된 CLI의 실행 파일이 일치하지 않습니다. 같은 버전명만으로 호환을 판단하지 않습니다.",
                &[
                    "검증한 해당 패키지의 bin/bxdl을 사용하세요. 새 CLI를 기존 설치본에 덮어쓰지 마세요.",
                ],
            ),
            Self::InitializationUnknown => (
                "DB 초기화 결과가 확정되지 않았습니다.",
                &[
                    "데이터와 작업 기록을 보존하고 instance show로 확인하세요. resume-init은 기존 재개 조건을 검사한 뒤 별도로 승인하며, 새 init으로 반복하지 마세요.",
                ],
            ),
            Self::OperationFailed => (
                "요청한 작업을 완료하지 못했습니다.",
                &[
                    "사유 코드와 해당 명령의 --help를 확인하세요. 생성된 자료나 실행 결과가 불명확하면 보존한 기록으로 지원 검토를 받으세요.",
                ],
            ),
        };
        View {
            description,
            next_actions,
        }
    }
}

/// Text is compiled here; values, paths, engine payloads and arbitrary messages
/// from the input are never interpolated into human guidance.
pub(crate) fn service(data: &Value) -> View {
    service_state(data).view()
}

fn service_state(data: &Value) -> State {
    let reason = data
        .get("reason")
        .or_else(|| data.get("serviceReason"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let service = data["serviceState"].as_str().unwrap_or("");
    let engine = data["engineState"].as_str().unwrap_or("");
    let readiness = data["runtimeReadiness"].as_str().unwrap_or("");
    let outcome = data["outcome"].as_str();
    if service == "GUI_UNAVAILABLE" {
        return State::GuiUnavailable;
    }
    // Uncertain observation reasons win over any partial or stale state fields.
    match reason {
        "SERVICE_START_WAIT_EXPIRED" | "SERVICE_STATUS_WAIT_EXPIRED" => return State::WaitExpired,
        "SERVICE_STATUS_QUERY_EXPIRED" => return State::QueryExpired,
        "SERVICE_STOP_WAIT_EXPIRED" => return State::StopUnconfirmed,
        "SERVICE_OBSERVATION_UNKNOWN"
        | "SERVICE_PID_MISMATCH"
        | "SERVICE_REPORT_UNKNOWN"
        | "SERVICE_HTTP_IDENTITY_UNKNOWN"
        | "SERVICE_ATTEMPT_CHANGED"
        | "SERVICE_OPERATION_UNKNOWN"
        | "SERVICE_LAUNCHCTL_WAIT_EXPIRED"
        | "SERVICE_HTTP_WAIT_EXPIRED" => return State::Unknown,
        _ => {}
    }
    let no_busy = data["operationBusy"].as_bool() != Some(true);
    let has_attempt = data["attemptId"].as_str().is_some_and(|id| !id.is_empty());
    let successful = outcome.is_none() || outcome == Some("SUCCEEDED");
    if matches!(
        reason,
        "SERVICE_STOPPED_VERIFIED" | "SERVICE_STOPPED_REQUIRES_CONFIRMATION"
    ) {
        return if engine == "STOPPED"
            && readiness == "STOPPED"
            && matches!(service, "EXITED" | "NOT_LOADED")
            && has_attempt
            && no_busy
            && successful
        {
            if reason == "SERVICE_STOPPED_VERIFIED" {
                State::StoppedVerified
            } else {
                State::StoppedNeedsConfirmation
            }
        } else {
            State::Unknown
        };
    }
    if outcome == Some("FAILED")
        || engine == "FAILED"
        || readiness == "FAILED"
        || matches!(reason, "SERVICE_GATE_REFUSED" | "SERVICE_LAUNCH_FAILED")
    {
        return State::Failed;
    }
    if reason == "SERVICE_NOT_STARTED" {
        return if service == "NOT_REGISTERED"
            && engine == "NOT_STARTED"
            && readiness == "NOT_CHECKED"
            && !has_attempt
            && no_busy
        {
            State::NotStarted
        } else {
            State::Unknown
        };
    }
    if reason == "NOT_OBSERVED" && service == "NOT_OBSERVED" && engine == "NOT_OBSERVED" {
        return State::NotObserved;
    }
    if service == "RUNNING"
        && engine == "RUNNING"
        && readiness == "READY"
        && has_attempt
        && successful
        && matches!(
            reason,
            "OBSERVER_INITIALIZED"
                | "MANUAL_START_INITIALIZED"
                | "INSTANT_RUNNING"
                | "VALIDATOR_RUNNING"
        )
    {
        return State::Ready;
    }
    if service == "RUNNING"
        && engine == "RUNNING"
        && readiness == "NOT_READY"
        && has_attempt
        && matches!(
            reason,
            "LOCAL_EXECUTION_OVERDUE"
                | "STARTUP_INCOMPLETE"
                | "SYNC_NEGOTIATING"
                | "SYNC_DOWNLOADING"
                | "SYNC_RETRYING"
                | "ENGINE_MISSING"
                | "ENGINE_NOT_RUNNING"
        )
    {
        return State::NotReady;
    }
    if matches!(
        (engine, reason),
        ("START_PENDING", "SERVICE_START_PENDING")
            | ("CHECKING", "VALIDATING_CONFIGURATION")
            | ("STARTING", "OPENING_MANAGED_STORAGE")
    ) {
        return State::Starting;
    }
    State::Unknown
}

/// Common command guidance. Unknown results retain uncertainty even when an
/// input-shaped reason is present; a generic INCOMPLETE is not a cold success.
pub(crate) fn command(outcome: &str, reason: &str) -> Option<View> {
    let state = if reason == "INSTANCE_INITIALIZATION_UNKNOWN" {
        State::InitializationUnknown
    } else if outcome == "UNKNOWN" {
        State::Unknown
    } else if matches!(reason, "SETUP_CLI_MISMATCH" | "SERVICE_CLI_MISMATCH") {
        State::CliMismatch
    } else if outcome == "INCOMPLETE" && reason == "ENGINE_RUNTIME_CHECKS_REQUIRED" {
        State::ColdIncomplete
    } else if outcome == "INCOMPLETE" && reason == "ENGINE_PREFLIGHT_NOT_RUN" {
        State::LocalIncomplete
    } else if matches!(
        reason,
        "INVALID_ARGUMENTS"
            | "SETUP_INPUT_INVALID"
            | "SETUP_PATH_INVALID"
            | "SETUP_PATH_UNAVAILABLE"
            | "INSTANCE_ID_INVALID"
            | "NODE_ID_INVALID"
            | "HTTP_ADDRESS_INVALID"
            | "P2P_ADDRESS_INVALID"
            | "PORTS_INVALID"
            | "REFERENCE_PATH_INVALID"
            | "REFERENCE_PATH_CONFLICT"
            | "SETUP_PATH_CONFLICT"
            | "CONFIG_SCHEMA_INVALID"
            | "NETWORK_INPUT_INVALID"
            | "SETUP_PRODUCT_CHECK_FAILED"
            | "LOCAL_CHECK_FAILED"
            | "SETUP_DRAFT_INCOMPLETE"
            | "SETUP_NETWORK_INCOMPLETE"
    ) {
        State::InputCorrection
    } else if outcome == "FAILED" {
        State::OperationFailed
    } else {
        return None;
    };
    Some(state.view())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ready() -> Value {
        json!({"serviceState":"RUNNING","engineState":"RUNNING","runtimeReadiness":"READY",
            "outcome":"SUCCEEDED","reason":"VALIDATOR_RUNNING","attemptId":"run-example","operationBusy":true})
    }

    #[test]
    fn observation_uncertainty_cannot_be_promoted_by_stale_ready_fields() {
        for reason in [
            "SERVICE_PID_MISMATCH",
            "SERVICE_REPORT_UNKNOWN",
            "SERVICE_HTTP_IDENTITY_UNKNOWN",
            "SERVICE_ATTEMPT_CHANGED",
        ] {
            let mut value = ready();
            value["reason"] = json!(reason);
            assert_eq!(service_state(&value), State::Unknown);
        }
        let mut value = ready();
        value["serviceState"] = json!("GUI_UNAVAILABLE");
        assert_eq!(service_state(&value), State::GuiUnavailable);
        value = ready();
        value["reason"] = json!("FUTURE_REASON");
        assert_eq!(service_state(&value), State::Unknown);
    }

    #[test]
    fn timeouts_offer_observation_not_a_start_retry_or_a_failure_claim() {
        for (reason, expected) in [
            ("SERVICE_START_WAIT_EXPIRED", State::WaitExpired),
            ("SERVICE_STATUS_WAIT_EXPIRED", State::WaitExpired),
            ("SERVICE_STATUS_QUERY_EXPIRED", State::QueryExpired),
            ("SERVICE_STOP_WAIT_EXPIRED", State::StopUnconfirmed),
        ] {
            let mut value = ready();
            value["reason"] = json!(reason);
            value["outcome"] = json!("UNKNOWN");
            assert_eq!(service_state(&value), expected);
            assert_ne!(service(&value), State::Failed.view());
            assert_ne!(service(&value), State::StoppedVerified.view());
        }
    }

    #[test]
    fn stopped_requires_concordant_evidence_and_exact_confirmation_reason() {
        let mut value = json!({"serviceState":"NOT_LOADED","engineState":"STOPPED","runtimeReadiness":"STOPPED",
            "reason":"SERVICE_STOPPED_REQUIRES_CONFIRMATION","outcome":"SUCCEEDED","operationBusy":false,"attemptId":"run-example"});
        assert_eq!(service_state(&value), State::StoppedNeedsConfirmation);
        value["reason"] = json!("SERVICE_STOPPED_VERIFIED");
        assert_eq!(service_state(&value), State::StoppedVerified);
        for (key, replacement) in [
            ("operationBusy", json!(true)),
            ("serviceState", json!("RUNNING")),
            ("engineState", json!("UNKNOWN")),
            ("attemptId", Value::Null),
        ] {
            let mut inconsistent = value.clone();
            inconsistent[key] = replacement;
            assert_eq!(service_state(&inconsistent), State::Unknown);
        }
    }

    #[test]
    fn workflow_alias_preserves_readiness_without_manufacturing_missing_evidence() {
        let mut value = ready();
        value["serviceReason"] = value["reason"].take();
        value.as_object_mut().unwrap().remove("reason");
        value.as_object_mut().unwrap().remove("outcome");
        value.as_object_mut().unwrap().remove("operationBusy");
        assert_eq!(service_state(&value), State::Ready);
        value["engineState"] = json!("UNKNOWN");
        assert_eq!(service_state(&value), State::Unknown);
        assert_eq!(
            service_state(&json!({"outcome":"SUCCEEDED"})),
            State::Unknown
        );
    }

    #[test]
    fn startup_stage_alone_is_not_a_failure_and_never_started_is_not_stopped() {
        let value = json!({"serviceState":"NOT_REGISTERED","engineState":"NOT_STARTED","runtimeReadiness":"NOT_CHECKED",
            "reason":"SERVICE_NOT_STARTED","startupStage":"ENGINE_EXEC"});
        assert_eq!(service_state(&value), State::NotStarted);
        assert_eq!(
            service_state(&json!({"startupStage":"ENGINE_EXEC"})),
            State::Unknown
        );
        assert_eq!(
            service_state(&json!({"engineState":"FAILED","outcome":"FAILED"})),
            State::Failed
        );
    }

    #[test]
    fn only_known_cold_result_is_described_as_cold_checked() {
        assert_eq!(
            command("INCOMPLETE", "ENGINE_RUNTIME_CHECKS_REQUIRED"),
            Some(State::ColdIncomplete.view())
        );
        assert_eq!(
            command("INCOMPLETE", "ENGINE_PREFLIGHT_NOT_RUN"),
            Some(State::LocalIncomplete.view())
        );
        assert_eq!(command("INCOMPLETE", "FUTURE_REASON"), None);
        assert_eq!(
            command("FAILED", "ENGINE_RUNTIME_CHECKS_REQUIRED"),
            Some(State::OperationFailed.view())
        );
        assert_eq!(
            command("UNKNOWN", "SETUP_INPUT_INVALID"),
            Some(State::Unknown.view())
        );
    }

    #[test]
    fn arbitrary_values_are_not_part_of_guidance() {
        let secret = "PRIVATE_CANARY\n/secret/path";
        let value = json!({"serviceState":secret,"engineState":secret,"runtimeReadiness":secret,"reason":secret,"health":secret,"attemptId":secret});
        let view = service(&value);
        assert_eq!(view, State::Unknown.view());
        assert!(!format!("{view:?}").contains("PRIVATE_CANARY"));
        assert!(!format!("{:?}", command("FAILED", secret)).contains("PRIVATE_CANARY"));
    }
}
