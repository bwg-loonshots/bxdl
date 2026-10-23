//! Owned response DTOs for the pinned executor profile. Never forward unknown
//! data members or raw stderr. The original accepted envelope and exit survive.
use super::*;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    schema_version: u32,
    command: String,
    outcome: String,
    reason_code: String,
    message: String,
    observed_at: String,
    #[serde(default)]
    data: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Initialization {
    instance_id: String,
    initialization: String,
    operation_busy: Option<bool>,
    development_only: bool,
    service_registration: String,
    runtime_readiness: String,
    reason: String,
    attempt_id: Option<String>,
    genesis_hash: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Service {
    instance_id: String,
    outcome: String,
    service_state: String,
    engine_state: String,
    runtime_readiness: String,
    reason: String,
    last_observation_reason: Option<String>,
    startup_stage: Option<String>,
    startup_failure_reason: Option<String>,
    operation_busy: bool,
    development_only: bool,
    login_auto_start: bool,
    automatic_restart: bool,
    service_profile: String,
    global_consensus: String,
    attempt_id: Option<String>,
    pid: Option<u32>,
    node_instance_id: Option<String>,
    health: Option<Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)] // Typed fields validate the complete public DTO.
struct Health {
    observed_at_epoch_millis: String,
    running: bool,
    lifecycle_running: bool,
    readiness: Option<crate::engine::runtime_result::Readiness>,
    execution_status: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)] // Counters are preserved after type/bound validation.
struct Diagnostic {
    schema_version: u32,
    partial: bool,
    mode: String,
    scope: String,
    initialization_record: String,
    initialization_reason: String,
    service_record: String,
    startup_stage: Option<String>,
    startup_failure_reason: Option<String>,
    runtime_readiness: String,
    global_consensus: String,
    operation_busy: Option<bool>,
    bytes_read: u64,
    events_omitted: usize,
    events: Vec<Event>,
    sources: Vec<Source>,
    next_actions: Vec<String>,
    raw_text_policy: String,
    output_created: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    source: String,
    sequence: u64,
    command: String,
    status: String,
    reason: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)] // Optional size is validated as an unsigned number.
struct Source {
    name: String,
    result: String,
    reason: String,
    size_bytes: Option<u64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Preflight {
    outcome: String,
    product: Product,
    configuration_binding: String,
    engine: Option<Engine>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Product {
    schema_version: u32,
    instance_id: String,
    outcome: String,
    checks: Vec<Check>,
    config_sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Check {
    name: String,
    status: String,
    reason_code: String,
    message: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Engine {
    outcome: String,
    engine_exit_code: i32,
    development_only: bool,
    identity: crate::engine::Identity,
    jar_sha256: String,
    java_sha256: String,
    config_sha256: Option<String>,
    chain_file_sha256: Option<String>,
    preflight: Option<crate::engine::ColdResult>,
}

fn decode<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T> {
    serde_json::from_value(value.clone()).map_err(|_| response_invalid())
}
fn code(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}
fn text(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn optional_code(s: &Option<String>) -> bool {
    s.as_ref().is_none_or(|s| code(s))
}
fn optional_genesis_hash(s: &Option<String>) -> bool {
    // Engine init results use 0x-prefixed genesis values, unlike artifact SHA-256.
    s.as_ref()
        .is_none_or(|s| s.strip_prefix("0x").is_some_and(crate::engine::hash))
}
fn tree(value: &Value, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match value {
        Value::String(s) => s.len() <= 4096 && !s.chars().any(char::is_control),
        Value::Array(v) => v.len() <= 512 && v.iter().all(|v| tree(v, depth + 1)),
        Value::Object(v) => {
            v.len() <= 128 && v.iter().all(|(k, v)| text(k, 100) && tree(v, depth + 1))
        }
        _ => true,
    }
}
fn initialization(data: &Initialization, instance: &str) -> bool {
    data.instance_id == instance
        && matches!(
            data.initialization.as_str(),
            "NOT_STARTED" | "INITIALIZED" | "UNKNOWN"
        )
        && data.development_only
        && matches!(
            data.service_registration.as_str(),
            "NOT_REGISTERED" | "NOT_OBSERVED"
        )
        && data.runtime_readiness == "NOT_CHECKED"
        && code(&data.reason)
        && data
            .attempt_id
            .as_ref()
            .is_none_or(|s| super::super::valid_attempt(s))
        && optional_genesis_hash(&data.genesis_hash)
}
fn service(data: &Service, instance: &str) -> bool {
    data.instance_id == instance
        && code(&data.outcome)
        && code(&data.service_state)
        && code(&data.engine_state)
        && code(&data.runtime_readiness)
        && code(&data.reason)
        && optional_code(&data.last_observation_reason)
        && optional_code(&data.startup_stage)
        && optional_code(&data.startup_failure_reason)
        && data.development_only
        && !data.login_auto_start
        && !data.automatic_restart
        && data.service_profile == "MACOS_USER_LAUNCH_AGENT"
        && data.global_consensus == "NOT_CHECKED"
        && data
            .attempt_id
            .as_ref()
            .is_none_or(|s| super::super::valid_attempt(s))
        && data.pid.is_none_or(|n| n > 0)
        && data.node_instance_id.as_ref().is_none_or(|s| text(s, 256))
}
fn health(data: &Health) -> bool {
    !data.observed_at_epoch_millis.is_empty()
        && data.observed_at_epoch_millis.len() <= 30
        && data
            .observed_at_epoch_millis
            .bytes()
            .all(|b| b.is_ascii_digit())
        && optional_code(&data.execution_status)
        && data.readiness.as_ref().is_none_or(|r| {
            code(&r.status)
                && code(&r.reason)
                && code(&r.role)
                && code(&r.startup_mode)
                && code(&r.sync_status)
        })
}

pub(super) fn parse(
    request: &Request,
    instance: &str,
    exit: i32,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<Response> {
    if stdout.is_empty() || stdout.len() > 2 * 1024 * 1024 || !stderr.is_empty() {
        return Err(response_invalid());
    }
    let value = super::json::decode(stdout).map_err(|_| response_invalid())?;
    if !tree(&value, 0) {
        return Err(response_invalid());
    }
    let envelope: Envelope = decode(&value)?;
    if envelope.schema_version != 1
        || envelope.command != request.command()
        || !code(&envelope.reason_code)
        || !text(&envelope.message, 2048)
        || envelope.observed_at.len() < 20
        || envelope.observed_at.len() > 40
        || !envelope.observed_at.is_ascii()
        || !envelope
            .observed_at
            .bytes()
            .all(|b| b.is_ascii_digit() || b"-T:.+Z".contains(&b))
        || !matches!(
            (exit, envelope.outcome.as_str()),
            (0, "SUCCEEDED")
                | (2..=4, "FAILED")
                | (4, "UNSUPPORTED")
                | (5, "INCOMPLETE")
                | (6, "UNKNOWN")
                | (7, "FAILED")
        )
    {
        return Err(response_invalid());
    }
    if let Some(data) = &envelope.data {
        match request {
            Request::Show | Request::Initialize { .. } => {
                let parsed: Initialization = decode(data)?;
                if !initialization(&parsed, instance)
                    || (!matches!(request, Request::Show)
                        && exit == 0
                        && parsed.initialization != "INITIALIZED")
                {
                    return Err(response_invalid());
                }
            }
            Request::Service { .. } => {
                let parsed: Service = decode(data)?;
                if !service(&parsed, instance)
                    || parsed.outcome != envelope.outcome
                    || parsed.reason != envelope.reason_code
                {
                    return Err(response_invalid());
                }
                if let Some(value) = &parsed.health {
                    if !health(&decode(value)?) {
                        return Err(response_invalid());
                    }
                }
            }
            Request::Logs { .. } | Request::Diagnose { .. } => {
                let parsed: Diagnostic = decode(data)?;
                if parsed.schema_version != 1
                    || parsed.mode != "OFFLINE_RECORDED_EVIDENCE"
                    || parsed.scope != "LATEST_INITIALIZATION_AND_SERVICE_ATTEMPTS"
                    || parsed.runtime_readiness != "NOT_OBSERVED"
                    || parsed.global_consensus != "NOT_CHECKED"
                    || parsed.partial != (exit == 5)
                    || parsed.output_created != matches!(request, Request::Diagnose { .. })
                    || !matches!(exit, 0 | 5)
                    || parsed.raw_text_policy
                        != "ALLOWLIST_EVENTS_ONLY_RAW_TEXT_AND_IDENTIFIERS_WITHHELD"
                    || !code(&parsed.initialization_record)
                    || !code(&parsed.initialization_reason)
                    || !code(&parsed.service_record)
                    || !optional_code(&parsed.startup_stage)
                    || !optional_code(&parsed.startup_failure_reason)
                    || parsed.events.len() > 200
                    || parsed.sources.len() > 64
                    || parsed.next_actions.len() > 32
                    || parsed.events.iter().any(|v| {
                        !text(&v.source, 100)
                            || v.sequence == 0
                            || !matches!(v.command.as_str(), "init" | "resume-init" | "run")
                            || !code(&v.status)
                            || !code(&v.reason)
                    })
                    || parsed
                        .sources
                        .iter()
                        .any(|v| !text(&v.name, 100) || !code(&v.result) || !code(&v.reason))
                {
                    return Err(response_invalid());
                }
            }
            Request::Preflight { .. } => {
                let parsed: Preflight = decode(data)?;
                if exit != 5
                    || envelope.reason_code != "ENGINE_RUNTIME_CHECKS_REQUIRED"
                    || parsed.outcome != "INCOMPLETE"
                    || parsed.configuration_binding != "MATCHED"
                    || parsed.engine.is_none()
                    || parsed.product.schema_version != 1
                    || parsed.product.instance_id != instance
                    || parsed.product.outcome != "INCOMPLETE"
                    || !crate::engine::hash(&parsed.product.config_sha256)
                    || parsed.product.checks.len() > 128
                    || parsed.product.checks.iter().any(|v| {
                        !text(&v.name, 100)
                            || !code(&v.status)
                            || !code(&v.reason_code)
                            || !text(&v.message, 2048)
                    })
                {
                    return Err(response_invalid());
                }
                if let Some(engine) = parsed.engine {
                    if engine.preflight.as_ref().is_none_or(|cold| {
                        cold.command != "preflight"
                            || cold.status != "INCOMPLETE"
                            || cold.contract_status != "PROPOSED"
                            || cold.backend != "rocksdb"
                    }) || engine.jar_sha256 != JAR
                        || engine.java_sha256 != JAVA
                        || !engine.development_only
                        || engine.outcome != "INCOMPLETE"
                        || engine.engine_exit_code != 3
                        || engine
                            .config_sha256
                            .as_ref()
                            .is_none_or(|s| !crate::engine::hash(s))
                        || engine
                            .chain_file_sha256
                            .as_ref()
                            .is_none_or(|s| !crate::engine::hash(s))
                        || engine.identity.source.commit
                            != "7ec5abd8c98e1cf070763a16f815e9281b74a5c1"
                    {
                        return Err(response_invalid());
                    }
                }
            }
        }
    } else if matches!(exit, 0 | 5) {
        return Err(response_invalid());
    }
    Ok(Response {
        command: envelope.command,
        outcome: envelope.outcome,
        reason_code: envelope.reason_code,
        message: envelope.message,
        observed_at: envelope.observed_at,
        data: envelope.data,
        exit,
    })
}

impl Response {
    pub fn into_initialization(self) -> Result<super::super::Summary> {
        if self.exit != 0 || self.outcome != "SUCCEEDED" {
            return Err(BxdlError::new(self.reason_code, self.message));
        }
        let Some(data) = self.data else {
            return Err(BxdlError::new(self.reason_code, self.message));
        };
        let data: Initialization = decode(&data)?;
        Ok(super::super::Summary {
            instance_id: data.instance_id,
            initialization: data.initialization,
            operation_busy: data.operation_busy,
            development_only: data.development_only,
            service_registration: match data.service_registration.as_str() {
                "NOT_REGISTERED" => "NOT_REGISTERED",
                "NOT_OBSERVED" => "NOT_OBSERVED",
                _ => return Err(response_invalid()),
            },
            runtime_readiness: "NOT_CHECKED",
            reason: data.reason,
            attempt_id: data.attempt_id,
            genesis_hash: data.genesis_hash,
        })
    }
    pub fn into_service(self) -> Result<super::super::service::ServiceSummary> {
        let Some(data) = self.data else {
            return Err(BxdlError::new(self.reason_code, self.message));
        };
        let data: Service = decode(&data)?;
        Ok(super::super::service::ServiceSummary {
            instance_id: data.instance_id,
            outcome: data.outcome,
            service_state: data.service_state,
            engine_state: data.engine_state,
            runtime_readiness: data.runtime_readiness,
            reason: data.reason,
            last_observation_reason: data.last_observation_reason,
            startup_stage: data.startup_stage,
            startup_failure_reason: data.startup_failure_reason,
            operation_busy: data.operation_busy,
            development_only: data.development_only,
            login_auto_start: data.login_auto_start,
            automatic_restart: data.automatic_restart,
            service_profile: "MACOS_USER_LAUNCH_AGENT",
            global_consensus: "NOT_CHECKED",
            attempt_id: data.attempt_id,
            pid: data.pid,
            node_instance_id: data.node_instance_id,
            health: data.health,
        })
    }
}
