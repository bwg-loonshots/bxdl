//! Pure consumers for the pinned 303e163a managed-run and monitor contracts.
//! A report cannot prove process liveness/death. Health has no process identity:
//! the caller must bracket it with matching bootstrap observations and recheck
//! its owned process/lock. These independent samples are never quorum evidence.
use super::{Identity, fail, hash, json, node_identity};
use crate::error::{BxdlError, Result};
use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Value};
use std::{fmt, path::Path};

const MAX_REPORT: usize = 32_768;
const MAX_EVENT: usize = 8_192;
const MAX_HTTP: usize = 65_536;
const MEDIA: &str = "application/vnd.nigo.console+json";

pub(super) struct Expected<'a> {
    pub attempt_id: &'a str,
    pub pid: u32,
    pub identity: &'a Identity,
    pub backend: &'a str,
    pub chain_fingerprint: &'a str,
    pub node_identity: &'a str,
    pub data_directory: &'a Path,
    pub genesis_hash: &'a str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RunReport {
    pub status: String,
    pub reason: String,
    pub sequence: u64,
    pub node_instance_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HealthReport {
    pub observed_at_epoch_millis: String,
    pub running: bool,
    pub lifecycle_running: bool,
    pub readiness: Option<Readiness>,
    pub execution_status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Readiness {
    pub status: String,
    pub reason: String,
    pub role: String,
    pub startup_mode: String,
    pub startup_completed: bool,
    pub sync_status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    attempt_id: String,
    command: String,
    sequence: u64,
    pid: u32,
    observed_at: i64,
    status: String,
    reason: String,
    contract_status: String,
    details: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    status: String,
    backend: String,
    chain_fingerprint: String,
    node_identity: String,
    data_directory: String,
    genesis_hash: String,
}

/// May be used before spawning: PID zero and an empty attempt are allowed here.
pub(super) fn validate_initialized(raw: &[u8], expected: &Expected<'_>) -> Result<()> {
    scope(expected)?;
    if raw.is_empty() || raw.len() > 8_192 {
        return Err(invalid());
    }
    let journal: Journal = json::decode(raw).map_err(|_| invalid())?;
    if journal.status != "INITIALIZED"
        || journal.backend != expected.backend
        || journal.chain_fingerprint != expected.chain_fingerprint
        || journal.node_identity != expected.node_identity
        || journal.data_directory != expected.data_directory.to_str().ok_or_else(invalid)?
        || journal.genesis_hash != expected.genesis_hash
    {
        return Err(invalid());
    }
    Ok(())
}

/// A complete prefix is a startup observation; incomplete JSONL is an error.
/// STOPPED still requires independent confirmation that the owned JVM exited.
pub(super) fn validate_report(
    raw: &[u8],
    journal: &[u8],
    expected: &Expected<'_>,
) -> Result<RunReport> {
    validate_initialized(journal, expected)?;
    if expected.pid == 0
        || !attempt(expected.attempt_id)
        || raw.is_empty()
        || raw.len() > MAX_REPORT
        || !raw.ends_with(b"\n")
    {
        return Err(invalid());
    }
    let mut result = RunReport {
        status: String::new(),
        reason: String::new(),
        sequence: 0,
        node_instance_id: None,
    };
    for (index, line) in raw[..raw.len() - 1].split(|b| *b == b'\n').enumerate() {
        if index >= 6 || line.is_empty() || line.len() > MAX_EVENT {
            return Err(invalid());
        }
        let event: Event = json::decode(line).map_err(|_| invalid())?;
        let _wall_clock = event.observed_at;
        if event.attempt_id != expected.attempt_id
            || event.command != "run"
            || event.sequence != index as u64 + 1
            || event.pid != expected.pid
            || event.contract_status != "PROPOSED"
            || matches!(result.status.as_str(), "STOPPED" | "UNKNOWN" | "FAILED")
        {
            return Err(invalid());
        }
        match event.status.as_str() {
            "CHECKING" if index == 0 && event.reason == "VALIDATING_CONFIGURATION" => {
                let details = object(&event.details, &["build"])?;
                let build: Identity =
                    serde_json::from_value(details["build"].clone()).map_err(|_| invalid())?;
                if build != *expected.identity {
                    return Err(invalid());
                }
            }
            "STARTING"
                if result.status == "CHECKING" && event.reason == "OPENING_MANAGED_STORAGE" =>
            {
                let details = object(&event.details, &["backend"])?;
                if string(&details["backend"])? != expected.backend {
                    return Err(invalid());
                }
            }
            "RUNNING"
                if result.status == "STARTING" && event.reason == "LOCAL_STARTUP_COMPLETED" =>
            {
                let details = object(
                    &event.details,
                    &[
                        "genesisHash",
                        "chainFingerprint",
                        "nodeIdentity",
                        "nodeInstanceId",
                        "networkQuorumVerified",
                    ],
                )?;
                let node = string(&details["nodeInstanceId"])?;
                if string(&details["genesisHash"])? != expected.genesis_hash
                    || string(&details["chainFingerprint"])? != expected.chain_fingerprint
                    || string(&details["nodeIdentity"])? != expected.node_identity
                    || boolean(&details["networkQuorumVerified"])?
                    || !uuid(node)
                {
                    return Err(invalid());
                }
                result.node_instance_id = Some(node.into());
            }
            "STOPPING" if result.status == "RUNNING" && event.reason == "SHUTDOWN_REQUESTED" => {
                object(&event.details, &[])?;
            }
            "STOPPED"
                if result.status == "STOPPING"
                    && event.reason == "STORAGE_AND_CONSENSUS_CLOSED" =>
            {
                let details = object(&event.details, &["storageClosed", "consensusStopReturned"])?;
                if !boolean(&details["storageClosed"])?
                    || !boolean(&details["consensusStopReturned"])?
                {
                    return Err(invalid());
                }
            }
            "UNKNOWN" if matches!(result.status.as_str(), "RUNNING" | "STOPPING") => {
                match event.reason.as_str() {
                    "CLOSE_OR_REPORT_FAILURE" => {
                        object(&event.details, &[])?;
                    }
                    "REPORT_FAILURE" | "CLOSE_NOT_CONFIRMED" => {
                        let details =
                            object(&event.details, &["storageClosed", "consensusStopReturned"])?;
                        let closed = boolean(&details["storageClosed"])?;
                        let stopped = boolean(&details["consensusStopReturned"])?;
                        if event.reason == "CLOSE_NOT_CONFIRMED" && closed && stopped {
                            return Err(invalid());
                        }
                    }
                    _ => return Err(invalid()),
                }
            }
            "FAILED" if index > 0 && event.reason == "STARTUP_OR_INITIALIZATION_FAILED" => {
                let details = object(&event.details, &["resultMayBePartial"])?;
                if !boolean(&details["resultMayBePartial"])? {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
        result.status = event.status;
        result.reason = event.reason;
        result.sequence = event.sequence;
    }
    Ok(result)
}

/// Caller checks HTTP 200, response size and a single trusted local endpoint.
/// This binds bootstrap to RUNNING and the initialized journal's genesis;
/// none of the console capability flags grant operational authority.
pub(super) fn validate_bootstrap(
    raw: &[u8],
    media: &str,
    expected: &Expected<'_>,
    node_instance_id: &str,
) -> Result<()> {
    scope(expected)?;
    if !uuid(node_instance_id) {
        return Err(invalid());
    }
    let value = http(raw, media)?;
    let body = object(
        &value,
        &[
            "nodeInstanceId",
            "chainId",
            "chainIdHex",
            "genesisBlockHash",
            "clientVersion",
            "consensus",
            "storage",
            "head",
            "fee",
            "pendingTransactions",
            "sampledAt",
            "consoleEnabled",
            "access",
            "engine",
        ],
    )?;
    let commit = expected
        .identity
        .source
        .commit
        .get(..12)
        .ok_or_else(invalid)?;
    let client = format!(
        "NIGO/{}/{}{}",
        expected.identity.version,
        commit,
        if expected.identity.source.dirty {
            "-dirty"
        } else {
            ""
        }
    );
    if string(&body["nodeInstanceId"])? != node_instance_id
        || string(&body["genesisBlockHash"])? != expected.genesis_hash
        || string(&body["clientVersion"])? != client
    {
        return Err(invalid());
    }
    let chain = decimal(&body["chainId"], false)?;
    if chain == 0 || chain > i32::MAX as i64 {
        return Err(invalid());
    }
    let chain_hex = string(&body["chainIdHex"])?;
    if !chain_hex
        .strip_prefix("0x")
        .is_some_and(|s| !s.is_empty() && s.len() <= 8 && s.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(invalid());
    }
    if u32::from_str_radix(&chain_hex[2..], 16).map_err(|_| invalid())? != chain as u32 {
        return Err(invalid());
    }
    decimal(&body["sampledAt"], true)?;
    let pending = body["pendingTransactions"].as_u64().ok_or_else(invalid)?;
    if pending > i32::MAX as u64 {
        return Err(invalid());
    }
    let consensus = object(&body["consensus"], &["profileId", "protocol"])?;
    printable(&consensus["profileId"], 64)?;
    member(&consensus["protocol"], &["QBFT", "INSTANT"])?;
    let storage = object(&body["storage"], &["backend"])?;
    if string(&storage["backend"])?
        != if expected.backend == "h2" {
            "rdb"
        } else {
            expected.backend
        }
    {
        return Err(invalid());
    }
    let head = object(&body["head"], &["number", "hash"])?;
    let height = decimal(&head["number"], false)?;
    if !hash0x(string(&head["hash"])?)
        || (height == 0 && string(&head["hash"])? != expected.genesis_hash)
    {
        return Err(invalid());
    }
    let fee = object(
        &body["fee"],
        &[
            "profileHash",
            "evaluatedAtHeight",
            "mode",
            "policyId",
            "revision",
            "gasPrice",
            "settlementMode",
            "activationHeight",
        ],
    )?;
    if !hash0x(string(&fee["profileHash"])?)
        || decimal(&fee["evaluatedAtHeight"], false)?
            != height.checked_add(1).ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    member(&fee["mode"], &["ZERO", "FIXED"])?;
    member(&fee["settlementMode"], &["NONE", "BURN"])?;
    printable(&fee["policyId"], 64)?;
    let revision = decimal(&fee["revision"], false)?;
    if revision == 0 || revision > u32::MAX as i64 {
        return Err(invalid());
    }
    decimal(&fee["activationHeight"], false)?;
    let gas = string(&fee["gasPrice"])?;
    const UINT256_MAX: &str =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    if !unsigned_decimal(gas)
        || gas.len() > UINT256_MAX.len()
        || (gas.len() == UINT256_MAX.len() && gas > UINT256_MAX)
    {
        return Err(invalid());
    }
    let enabled = boolean(&body["consoleEnabled"])?;
    let access = object(
        &body["access"],
        &[
            "mode",
            "implementedAuth",
            "rpcExecution",
            "storageGcControl",
        ],
    )?;
    if string(&access["mode"])?
        != if enabled {
            "LOCAL_DEVELOPMENT_UNAUTHENTICATED"
        } else {
            "CONSOLE_DISABLED"
        }
        || boolean(&access["implementedAuth"])?
    {
        return Err(invalid());
    }
    for key in ["rpcExecution", "storageGcControl"] {
        if boolean(&access[key])? && !enabled {
            return Err(invalid());
        }
    }
    let engine = object(&body["engine"], &["build", "capabilities"])?;
    let build: Identity = serde_json::from_value(engine["build"].clone()).map_err(|_| invalid())?;
    if build != *expected.identity {
        return Err(invalid());
    }
    let capabilities = engine["capabilities"].as_array().ok_or_else(invalid)?;
    let expected_capabilities = [
        "CONSENSUS_HEALTH",
        "CONSENSUS_PROGRESS",
        "ROLE_AWARE_LOCAL_READINESS",
    ];
    if capabilities.len() != expected_capabilities.len()
        || capabilities
            .iter()
            .zip(expected_capabilities)
            .any(|(a, b)| a.as_str() != Some(b))
    {
        return Err(invalid());
    }
    Ok(())
}

/// No process identity exists in the health DTO. Never publish this result as
/// bound health until matching bootstrap/process observations surround it.
pub(super) fn validate_health(raw: &[u8], media: &str) -> Result<HealthReport> {
    let value = http(raw, media)?;
    let body = object(
        &value,
        &[
            "observedAtEpochMillis",
            "running",
            "lifecycleRunning",
            "runtimeInitialized",
            "role",
            "engineAttached",
            "engineRunning",
            "failure",
            "executionProgress",
            "readiness",
        ],
    )?;
    decimal(&body["observedAtEpochMillis"], true)?;
    let running = boolean(&body["running"])?;
    let lifecycle_running = boolean(&body["lifecycleRunning"])?;
    for field in ["runtimeInitialized", "engineAttached", "engineRunning"] {
        nullable(&body[field], |v| boolean(v).map(|_| ()))?;
    }
    nullable(&body["role"], |v| {
        member(v, &["VALIDATOR", "OBSERVER"]).map(|_| ())
    })?;
    nullable(&body["failure"], validate_failure)?;
    nullable(&body["executionProgress"], validate_execution)?;
    // These fields are projected from the same captured health snapshot. The
    // execution-progress sample is independent and is deliberately excluded.
    let projected_running = if let Some(initialized) = body["runtimeInitialized"].as_bool() {
        let attached = boolean(&body["engineAttached"])?;
        if attached != body["engineRunning"].is_boolean()
            || (!attached && !body["failure"].is_null())
        {
            return Err(invalid());
        }
        lifecycle_running
            && initialized
            && body["failure"].is_null()
            && body["engineRunning"]
                .as_bool()
                .unwrap_or(body["role"].as_str() == Some("OBSERVER"))
    } else {
        if [
            "role",
            "engineAttached",
            "engineRunning",
            "failure",
            "executionProgress",
        ]
        .iter()
        .any(|key| !body[*key].is_null())
        {
            return Err(invalid());
        }
        lifecycle_running
    };
    if running != projected_running {
        return Err(invalid());
    }
    let execution_status = body["executionProgress"]
        .get("status")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let readiness = if body["readiness"].is_null() {
        None
    } else {
        object(
            &body["readiness"],
            &[
                "status",
                "reason",
                "role",
                "startupMode",
                "startupCompleted",
                "syncStatus",
            ],
        )?;
        let r: Readiness =
            serde_json::from_value(body["readiness"].clone()).map_err(|_| invalid())?;
        validate_readiness(&r, body)?;
        Some(r)
    };
    Ok(HealthReport {
        observed_at_epoch_millis: string(&body["observedAtEpochMillis"])?.into(),
        running,
        lifecycle_running,
        readiness,
        execution_status,
    })
}

fn validate_readiness(r: &Readiness, health: &Map<String, Value>) -> Result<()> {
    if !["READY", "NOT_READY", "UNKNOWN", "FAILED"].contains(&r.status.as_str())
        || !["VALIDATOR", "OBSERVER", "INSTANT", "UNKNOWN"].contains(&r.role.as_str())
        || !["AUTO_START", "MANUAL_START"].contains(&r.startup_mode.as_str())
        || ![
            "UNKNOWN",
            "NEGOTIATING",
            "DOWNLOADING",
            "RETRYING",
            "FOLLOWING",
            "FAILED",
        ]
        .contains(&r.sync_status.as_str())
    {
        return Err(invalid());
    }
    // Mirror the fixed pinned priority rules. Do not upgrade an explicit UNKNOWN
    // or infer healthy operation from `running` alone.
    let role = health["role"].as_str().unwrap_or(if r.role == "INSTANT" {
        "INSTANT"
    } else {
        "UNKNOWN"
    });
    if role != r.role || (r.role == "INSTANT" && !health["runtimeInitialized"].is_null()) {
        return Err(invalid());
    }
    let execution = health["executionProgress"]
        .get("status")
        .and_then(Value::as_str);
    let (status, reason) = if !health["failure"].is_null() || execution == Some("FAILED") {
        ("FAILED", "TERMINAL_FAILURE")
    } else if r.sync_status == "FAILED" {
        ("FAILED", "SYNC_FAILED")
    } else if matches!(execution, Some("OPERATION_OVERDUE" | "TIMER_OVERDUE")) {
        ("NOT_READY", "LOCAL_EXECUTION_OVERDUE")
    } else if !r.startup_completed {
        ("NOT_READY", "STARTUP_INCOMPLETE")
    } else if r.role == "UNKNOWN" {
        ("UNKNOWN", "ROLE_UNKNOWN")
    } else if r.role != "INSTANT" && health["runtimeInitialized"].as_bool() != Some(true) {
        ("UNKNOWN", "RUNTIME_UNINITIALIZED")
    } else if r.sync_status == "NEGOTIATING" {
        ("NOT_READY", "SYNC_NEGOTIATING")
    } else if r.sync_status == "DOWNLOADING" {
        ("NOT_READY", "SYNC_DOWNLOADING")
    } else if r.sync_status == "RETRYING" {
        ("NOT_READY", "SYNC_RETRYING")
    } else if r.role == "OBSERVER" {
        ("READY", "OBSERVER_INITIALIZED")
    } else if r.startup_mode == "MANUAL_START" {
        ("READY", "MANUAL_START_INITIALIZED")
    } else if r.role == "VALIDATOR" && health["engineAttached"].as_bool() != Some(true) {
        ("NOT_READY", "ENGINE_MISSING")
    } else if health["running"].as_bool() != Some(true) {
        ("NOT_READY", "ENGINE_NOT_RUNNING")
    } else if r.role == "VALIDATOR"
        && matches!(
            execution,
            None | Some("NOT_STARTED" | "STOPPED" | "NO_TIMER")
        )
    {
        ("UNKNOWN", "EXECUTION_UNKNOWN")
    } else {
        (
            "READY",
            if r.role == "INSTANT" {
                "INSTANT_RUNNING"
            } else {
                "VALIDATOR_RUNNING"
            },
        )
    };
    let follower = r.role == "OBSERVER"
        || (r.role == "VALIDATOR"
            && r.startup_mode == "MANUAL_START"
            && health["engineRunning"].as_bool() != Some(true));
    if (!follower && r.sync_status != "UNKNOWN") || r.status != status || r.reason != reason {
        return Err(invalid());
    }
    Ok(())
}

fn validate_failure(value: &Value) -> Result<()> {
    let f = object(
        value,
        &[
            "kind",
            "phase",
            "startedAtEpochMillis",
            "startedAtNanos",
            "completedAtNanos",
            "messageType",
            "inputView",
            "beforeView",
            "beforeStep",
            "afterView",
            "afterStep",
            "causeCategory",
        ],
    )?;
    member(&f["kind"], KINDS)?;
    member(&f["phase"], PHASES)?;
    member(
        &f["causeCategory"],
        &[
            "SAFETY_VIOLATION",
            "INVALID_ARGUMENT",
            "ILLEGAL_STATE",
            "RUNTIME_FAILURE",
            "FATAL_ERROR",
        ],
    )?;
    for key in ["startedAtEpochMillis", "startedAtNanos", "completedAtNanos"] {
        decimal(&f[key], true)?;
    }
    for key in ["messageType", "beforeStep", "afterStep"] {
        nullable(&f[key], |v| member(v, STEPS).map(|_| ()))?;
    }
    for key in ["inputView", "beforeView", "afterView"] {
        nullable(&f[key], validate_view)?;
    }
    Ok(())
}
const KINDS: &[&str] = &["START", "MESSAGE", "TIMEOUT", "BLOCK_PERIOD"];
const PHASES: &[&str] = &["SUBSCRIBE", "TRANSITION", "DISPATCH"];
const STEPS: &[&str] = &["PROPOSAL", "PREPARE", "COMMIT", "ROUND_CHANGE"];
fn validate_execution(value: &Value) -> Result<()> {
    let e = object(
        value,
        &[
            "status",
            "kind",
            "phase",
            "view",
            "operationElapsedMillis",
            "lastCompletedAgeMillis",
            "nextTimerDueInMillis",
            "timerOverdueMillis",
            "operationTimeoutMillis",
            "timerGraceMillis",
        ],
    )?;
    member(
        &e["status"],
        &[
            "NOT_STARTED",
            "STOPPED",
            "FAILED",
            "WAITING_TIMER",
            "EXECUTING",
            "OPERATION_OVERDUE",
            "TIMER_OVERDUE",
            "NO_TIMER",
        ],
    )?;
    nullable(&e["kind"], |v| {
        member(v, &["START", "MESSAGE", "TIMEOUT", "BLOCK_PERIOD", "STOP"]).map(|_| ())
    })?;
    nullable(&e["phase"], |v| {
        member(
            v,
            &[
                "SUBSCRIBE",
                "TRANSITION",
                "DIAGNOSTICS",
                "DISPATCH",
                "CLEANUP",
            ],
        )
        .map(|_| ())
    })?;
    nullable(&e["view"], validate_view)?;
    for key in [
        "operationElapsedMillis",
        "lastCompletedAgeMillis",
        "timerOverdueMillis",
    ] {
        nullable(&e[key], |v| decimal(v, false).map(|_| ()))?;
    }
    nullable(&e["nextTimerDueInMillis"], |v| decimal(v, true).map(|_| ()))?;
    decimal(&e["operationTimeoutMillis"], false)?;
    decimal(&e["timerGraceMillis"], false)?;
    Ok(())
}
fn validate_view(value: &Value) -> Result<()> {
    let view = object(value, &["height", "round"])?;
    decimal(&view["height"], false)?;
    if decimal(&view["round"], false)? > u32::MAX as i64 {
        return Err(invalid());
    }
    Ok(())
}
fn scope(e: &Expected<'_>) -> Result<()> {
    if e.identity.build_info_status != "AVAILABLE"
        || !matches!(e.backend, "rocksdb" | "h2")
        || !hash(e.chain_fingerprint)
        || !node_identity(e.node_identity)
        || !hash0x(e.genesis_hash)
        || !e.data_directory.is_absolute()
        || e.data_directory.to_str().is_none()
    {
        return Err(invalid());
    }
    Ok(())
}
fn attempt(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 80
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
fn hash0x(v: &str) -> bool {
    v.strip_prefix("0x").is_some_and(hash)
}
fn uuid(v: &str) -> bool {
    v.len() == 36
        && v.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
        && v.as_bytes()[14] == b'4'
        && b"89ab".contains(&v.as_bytes()[19])
}
fn object<'a>(v: &'a Value, keys: &[&str]) -> Result<&'a Map<String, Value>> {
    let map = v.as_object().ok_or_else(invalid)?;
    if map.len() != keys.len() || keys.iter().any(|k| !map.contains_key(*k)) {
        return Err(invalid());
    }
    Ok(map)
}
fn string(v: &Value) -> Result<&str> {
    v.as_str().ok_or_else(invalid)
}
fn boolean(v: &Value) -> Result<bool> {
    v.as_bool().ok_or_else(invalid)
}
fn member<'a>(v: &'a Value, allowed: &[&str]) -> Result<&'a str> {
    let text = string(v)?;
    if !allowed.contains(&text) {
        return Err(invalid());
    }
    Ok(text)
}
fn printable(v: &Value, maximum: usize) -> Result<()> {
    let text = string(v)?;
    if text.is_empty() || text.len() > maximum || !text.bytes().all(|b| (0x21..=0x7e).contains(&b))
    {
        return Err(invalid());
    }
    Ok(())
}
fn nullable(v: &Value, f: impl FnOnce(&Value) -> Result<()>) -> Result<()> {
    if v.is_null() { Ok(()) } else { f(v) }
}
fn unsigned_decimal(v: &str) -> bool {
    v == "0" || (!v.is_empty() && !v.starts_with('0') && v.bytes().all(|b| b.is_ascii_digit()))
}
fn decimal(v: &Value, negative: bool) -> Result<i64> {
    let text = string(v)?;
    if !(unsigned_decimal(text)
        || (negative
            && text
                .strip_prefix('-')
                .is_some_and(|s| s != "0" && unsigned_decimal(s))))
    {
        return Err(invalid());
    }
    text.parse().map_err(|_| invalid())
}

fn http(raw: &[u8], media: &str) -> Result<Value> {
    let mut pieces = media.split(';');
    if raw.is_empty()
        || raw.len() > MAX_HTTP
        || media.len() > 128
        || !pieces
            .next()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case(MEDIA)
        || pieces.any(|p| {
            !matches!(
                p.trim().to_ascii_lowercase().as_str(),
                "charset=utf-8" | "charset=\"utf-8\""
            )
        })
    {
        return Err(invalid());
    }
    let mut deserializer = serde_json::Deserializer::from_slice(raw);
    let value = NullableSeed(0)
        .deserialize(&mut deserializer)
        .map_err(|_| invalid())?;
    deserializer.end().map_err(|_| invalid())?;
    Ok(value)
}

// Unlike engine::json, HTTP has contractually meaningful nulls. Keep duplicate
// rejection and a shallow bounded tree; shape consumers decide where null fits.
struct NullableSeed(usize);
impl<'de> DeserializeSeed<'de> for NullableSeed {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        reader: D,
    ) -> std::result::Result<Value, D::Error> {
        if self.0 > 16 {
            return Err(de::Error::custom("depth"));
        }
        reader.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for NullableSeed {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("bounded monitor JSON")
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(v.into()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Value, A::Error> {
        let mut result = Vec::new();
        while let Some(value) = seq.next_element_seed(NullableSeed(self.0 + 1))? {
            result.push(value);
        }
        Ok(Value::Array(result))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Value, A::Error> {
        let mut result = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if result.contains_key(&key) {
                return Err(de::Error::custom("duplicate"));
            }
            result.insert(key, map.next_value_seed(NullableSeed(self.0 + 1))?);
        }
        Ok(Value::Object(result))
    }
}
fn invalid() -> BxdlError {
    fail(
        "ENGINE_RUNTIME_RESULT_INVALID",
        "실행 보고 또는 HTTP 관측의 형식·실행 identity를 확인하지 못했습니다. 결과는 불명입니다.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NODE: &str = "b51f6f4d-e704-4be3-9630-38a93875f30a";
    const DATA: &str = "/private/bxdl runtime/data";
    const CANARY: &str = "PRIVATE_RUNTIME_CANARY";
    struct Fixture {
        identity: Identity,
        genesis: String,
        chain: String,
        node: String,
        journal: Value,
        events: Vec<Value>,
    }
    impl Fixture {
        fn new() -> Self {
            let identity: Identity = json::decode(include_bytes!(
                "../../contracts/nigo/development-clean-2026-09-18/evidence/engine-info.json"
            ))
            .unwrap();
            let genesis = format!("0x{}", "a".repeat(64));
            let chain = "b".repeat(64);
            let node = format!("0x{}:0x{}", "c".repeat(64), "d".repeat(40));
            let event = |sequence, status, reason, details| json!({"attemptId":"run-test_1","command":"run","sequence":sequence,"pid":12345,"observedAt":9007199254740993_i64,"status":status,"reason":reason,"contractStatus":"PROPOSED","details":details});
            let events = vec![
                event(
                    1,
                    "CHECKING",
                    "VALIDATING_CONFIGURATION",
                    json!({"build":identity}),
                ),
                event(
                    2,
                    "STARTING",
                    "OPENING_MANAGED_STORAGE",
                    json!({"backend":"rocksdb"}),
                ),
                event(
                    3,
                    "RUNNING",
                    "LOCAL_STARTUP_COMPLETED",
                    json!({"genesisHash":genesis,"chainFingerprint":chain,"nodeIdentity":node,"nodeInstanceId":NODE,"networkQuorumVerified":false}),
                ),
                event(4, "STOPPING", "SHUTDOWN_REQUESTED", json!({})),
                event(
                    5,
                    "STOPPED",
                    "STORAGE_AND_CONSENSUS_CLOSED",
                    json!({"storageClosed":true,"consensusStopReturned":true}),
                ),
            ];
            let journal = json!({"status":"INITIALIZED","backend":"rocksdb","chainFingerprint":chain,"nodeIdentity":node,"dataDirectory":DATA,"genesisHash":genesis});
            Self {
                identity,
                genesis,
                chain,
                node,
                journal,
                events,
            }
        }
        fn expected(&self) -> Expected<'_> {
            Expected {
                attempt_id: "run-test_1",
                pid: 12345,
                identity: &self.identity,
                backend: "rocksdb",
                chain_fingerprint: &self.chain,
                node_identity: &self.node,
                data_directory: Path::new(DATA),
                genesis_hash: &self.genesis,
            }
        }
        fn report(&self) -> Vec<u8> {
            let mut raw = Vec::new();
            for event in &self.events {
                raw.extend(serde_json::to_vec(event).unwrap());
                raw.push(b'\n');
            }
            raw
        }
        fn validate(&self) -> Result<RunReport> {
            validate_report(
                &self.report(),
                &serde_json::to_vec(&self.journal).unwrap(),
                &self.expected(),
            )
        }
        fn bootstrap(&self) -> Value {
            json!({"nodeInstanceId":NODE,"chainId":"11578","chainIdHex":"0x2d3a","genesisBlockHash":self.genesis,
                "clientVersion":format!("NIGO/{}/{}",self.identity.version,&self.identity.source.commit[..12]),
                "consensus":{"profileId":"QBFT_V1","protocol":"QBFT"},"storage":{"backend":"rocksdb"},
                "head":{"number":"9007199254740993","hash":self.genesis},
                "fee":{"profileHash":format!("0x{}","e".repeat(64)),"evaluatedAtHeight":"9007199254740994","mode":"ZERO","policyId":"ZERO_FEE","revision":"1","gasPrice":"0","settlementMode":"NONE","activationHeight":"0"},
                "pendingTransactions":0,"sampledAt":"9007199254740993","consoleEnabled":false,
                "access":{"mode":"CONSOLE_DISABLED","implementedAuth":false,"rpcExecution":false,"storageGcControl":false},
                "engine":{"build":self.identity,"capabilities":["CONSENSUS_HEALTH","CONSENSUS_PROGRESS","ROLE_AWARE_LOCAL_READINESS"]}})
        }
    }
    fn bytes(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }
    fn bad<T>(result: Result<T>) {
        let error = match result {
            Ok(_) => panic!("accepted invalid runtime evidence"),
            Err(e) => e,
        };
        assert_eq!(error.code, "ENGINE_RUNTIME_RESULT_INVALID");
        assert!(!error.message.contains(CANARY));
        assert!(!error.message.contains(DATA));
    }
    fn health_cases() -> Vec<Value> {
        let fixture: Value = serde_json::from_slice(include_bytes!(
            "../../contracts/nigo/development-clean-2026-09-18/health-cases.json"
        ))
        .unwrap();
        fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| case["health"].clone())
            .collect()
    }
    fn validator_health() -> Value {
        health_cases()
            .into_iter()
            .find(|v| v["executionProgress"]["view"].is_object())
            .unwrap()
    }

    #[test]
    fn exact_prefixes_and_stopped_keep_run_identity_without_claiming_health() {
        for (count, status) in [
            (1, "CHECKING"),
            (2, "STARTING"),
            (3, "RUNNING"),
            (4, "STOPPING"),
            (5, "STOPPED"),
        ] {
            let mut fixture = Fixture::new();
            fixture.events.truncate(count);
            let report = fixture.validate().unwrap();
            assert_eq!(report.status, status);
            assert_eq!(report.sequence, count as u64);
            assert_eq!(
                report.node_instance_id.as_deref(),
                (count >= 3).then_some(NODE)
            );
        }
        let mut fixture = Fixture::new();
        fixture.events[1]["observedAt"] = json!(-1);
        assert_eq!(fixture.validate().unwrap().status, "STOPPED");
    }

    #[test]
    fn failed_and_unknown_remain_explicit_even_with_successful_http_or_closed_storage() {
        for prefix in 1..=4 {
            let mut fixture = Fixture::new();
            fixture.events.truncate(prefix);
            let mut failure = fixture.events[0].clone();
            failure["sequence"] = json!(prefix + 1);
            failure["status"] = json!("FAILED");
            failure["reason"] = json!("STARTUP_OR_INITIALIZATION_FAILED");
            failure["details"] = json!({"resultMayBePartial":true});
            fixture.events.push(failure);
            assert_eq!(fixture.validate().unwrap().status, "FAILED");
        }
        for (reason, details) in [
            (
                "CLOSE_NOT_CONFIRMED",
                json!({"storageClosed":false,"consensusStopReturned":true}),
            ),
            (
                "REPORT_FAILURE",
                json!({"storageClosed":true,"consensusStopReturned":true}),
            ),
            ("CLOSE_OR_REPORT_FAILURE", json!({})),
        ] {
            let mut fixture = Fixture::new();
            fixture.events[4]["status"] = json!("UNKNOWN");
            fixture.events[4]["reason"] = json!(reason);
            fixture.events[4]["details"] = details;
            assert_eq!(fixture.validate().unwrap().status, "UNKNOWN");
        }
    }

    #[test]
    fn report_binds_every_attempt_pid_sequence_build_and_running_identity() {
        for row in 0..5 {
            for (key, value) in [
                ("pid", json!(12346)),
                ("attemptId", json!("old-attempt")),
                ("command", json!("init")),
                ("sequence", json!(0)),
                ("contractStatus", json!("ACCEPTED")),
            ] {
                let mut f = Fixture::new();
                f.events[row][key] = value;
                bad(f.validate());
            }
        }
        for (pointer, value) in [
            ("/details/build/source/dirty", json!(true)),
            ("/details/build/contract/fingerprint", json!("f".repeat(64))),
        ] {
            let mut f = Fixture::new();
            *f.events[0].pointer_mut(pointer).unwrap() = value;
            bad(f.validate());
        }
        for (key, value) in [
            ("genesisHash", json!(format!("0x{}", "f".repeat(64)))),
            ("chainFingerprint", json!("f".repeat(64))),
            ("nodeIdentity", json!("INSTANT")),
            ("nodeInstanceId", json!("stale-or-invalid")),
            ("networkQuorumVerified", json!(true)),
        ] {
            let mut f = Fixture::new();
            f.events[2]["details"][key] = value;
            bad(f.validate());
        }
    }

    #[test]
    fn strict_journal_gate_allows_prespan_scope_but_never_pending_or_different_genesis() {
        let f = Fixture::new();
        let mut expected = f.expected();
        expected.pid = 0;
        expected.attempt_id = "";
        validate_initialized(&bytes(&f.journal), &expected).unwrap();
        bad(validate_report(&f.report(), &bytes(&f.journal), &expected));
        for (key, value) in [
            ("status", json!("INITIALIZING")),
            ("genesisHash", json!("")),
            ("backend", json!("h2")),
            ("chainFingerprint", json!("f".repeat(64))),
            ("nodeIdentity", json!("INSTANT")),
            ("dataDirectory", json!("/private/other/data")),
        ] {
            let mut journal = f.journal.clone();
            journal[key] = value;
            bad(validate_initialized(&bytes(&journal), &expected));
        }
    }

    #[test]
    fn truncated_extra_duplicate_null_unknown_and_oversized_report_never_succeed() {
        let f = Fixture::new();
        let raw = f.report();
        let journal = bytes(&f.journal);
        for raw in [
            Vec::new(),
            raw[..raw.len() - 1].to_vec(),
            [raw.clone(), b"\n".to_vec()].concat(),
            [raw.clone(), b"{}\n".to_vec()].concat(),
            vec![b' '; MAX_REPORT + 1],
        ] {
            bad(validate_report(&raw, &journal, &f.expected()));
        }
        for row in 0..5 {
            let mut f = Fixture::new();
            f.events[row]["details"][CANARY] = json!(CANARY);
            bad(f.validate());
            f.events[row]["details"] = Value::Null;
            bad(f.validate());
            f.events[row]["status"] = json!("NEW_STATUS");
            bad(f.validate());
        }
        let mut duplicate = b"{\"pid\":12345,".to_vec();
        duplicate.extend(&raw[1..]);
        bad(validate_report(&duplicate, &journal, &f.expected()));
        let mut f = Fixture::new();
        f.events[4]["details"]["storageClosed"] = json!(false);
        bad(f.validate());
        let terminal = f.events[4].clone();
        f.events.push(terminal);
        bad(f.validate());
    }

    #[test]
    fn all_supplier_health_cases_preserve_ready_not_ready_unknown_failed_and_nulls() {
        for value in health_cases() {
            let parsed = validate_health(&bytes(&value), MEDIA).unwrap();
            let readiness = parsed.readiness.unwrap();
            assert_eq!(readiness.status, value["readiness"]["status"]);
            assert_eq!(readiness.reason, value["readiness"]["reason"]);
            assert_eq!(readiness.sync_status, value["readiness"]["syncStatus"]);
        }
        let mut value = validator_health();
        value["readiness"] = Value::Null;
        assert!(
            validate_health(&bytes(&value), MEDIA)
                .unwrap()
                .readiness
                .is_none()
        );
    }

    #[test]
    fn health_decimal_strings_preserve_above_js_precision_and_signed_monotonic_values() {
        let mut value = validator_health();
        value["observedAtEpochMillis"] = json!("9007199254740993");
        value["executionProgress"]["nextTimerDueInMillis"] = json!("-9223372036854775808");
        let report = validate_health(&bytes(&value), MEDIA).unwrap();
        assert_eq!(report.observed_at_epoch_millis, "9007199254740993");
        assert_eq!(
            http(&bytes(&value), MEDIA).unwrap()["executionProgress"]["view"]["height"],
            "9007199254740993"
        );
        for bad_value in [
            json!(9007199254740993_u64),
            json!("9007199254740993.0"),
            json!("01"),
            json!("-0"),
            json!("9223372036854775808"),
            Value::Null,
        ] {
            let mut bad_health = value.clone();
            bad_health["observedAtEpochMillis"] = bad_value;
            bad(validate_health(&bytes(&bad_health), MEDIA));
        }
        value["executionProgress"]["view"]["round"] = json!("4294967296");
        bad(validate_health(&bytes(&value), MEDIA));
    }

    #[test]
    fn nullable_does_not_mean_missing_or_zero_and_unknown_enums_are_rejected() {
        let mut value = validator_health();
        value.as_object_mut().unwrap().remove("runtimeInitialized");
        bad(validate_health(&bytes(&value), MEDIA));
        for pointer in [
            "/running",
            "/readiness/status",
            "/executionProgress/operationTimeoutMillis",
        ] {
            let mut value = validator_health();
            *value.pointer_mut(pointer).unwrap() = Value::Null;
            bad(validate_health(&bytes(&value), MEDIA));
        }
        for pointer in [
            "/role",
            "/readiness/status",
            "/readiness/reason",
            "/readiness/role",
            "/readiness/startupMode",
            "/readiness/syncStatus",
            "/executionProgress/status",
            "/executionProgress/kind",
            "/executionProgress/phase",
        ] {
            let mut value = validator_health();
            *value.pointer_mut(pointer).unwrap() = json!(CANARY);
            bad(validate_health(&bytes(&value), MEDIA));
        }
        let mut value = validator_health();
        value["executionProgress"]["kind"] = json!("STOP");
        value["executionProgress"]["phase"] = json!("CLEANUP");
        validate_health(&bytes(&value), MEDIA).unwrap();
    }

    #[test]
    fn forged_ready_cannot_override_unknown_runtime_terminal_failure_or_sync_retry() {
        for mut value in health_cases() {
            if value["readiness"]["status"] == "READY" {
                continue;
            }
            value["readiness"]["status"] = json!("READY");
            value["readiness"]["reason"] = json!("VALIDATOR_RUNNING");
            bad(validate_health(&bytes(&value), MEDIA));
        }
        let mut value = validator_health();
        value["executionProgress"]["status"] = json!("NO_TIMER");
        value["readiness"]["status"] = json!("UNKNOWN");
        value["readiness"]["reason"] = json!("EXECUTION_UNKNOWN");
        assert_eq!(
            validate_health(&bytes(&value), MEDIA)
                .unwrap()
                .readiness
                .unwrap()
                .status,
            "UNKNOWN"
        );
        let mut inconsistent = validator_health();
        inconsistent["engineRunning"] = json!(false);
        bad(validate_health(&bytes(&inconsistent), MEDIA));
    }

    #[test]
    fn instant_nullable_runtime_has_its_own_lifecycle_contract() {
        let mut value = json!({"observedAtEpochMillis":"1789600000000","running":true,"lifecycleRunning":true,
            "runtimeInitialized":null,"role":null,"engineAttached":null,"engineRunning":null,"failure":null,"executionProgress":null,
            "readiness":{"status":"READY","reason":"INSTANT_RUNNING","role":"INSTANT","startupMode":"AUTO_START","startupCompleted":true,"syncStatus":"UNKNOWN"}});
        assert_eq!(
            validate_health(&bytes(&value), MEDIA)
                .unwrap()
                .readiness
                .unwrap()
                .role,
            "INSTANT"
        );
        value["running"] = json!(false);
        value["lifecycleRunning"] = json!(false);
        value["readiness"]["startupMode"] = json!("MANUAL_START");
        value["readiness"]["reason"] = json!("MANUAL_START_INITIALIZED");
        assert_eq!(
            validate_health(&bytes(&value), MEDIA)
                .unwrap()
                .readiness
                .unwrap()
                .status,
            "READY"
        );
    }

    #[test]
    fn bootstrap_binds_uuid_build_client_version_and_initialized_genesis() {
        let f = Fixture::new();
        let boot = f.bootstrap();
        validate_bootstrap(&bytes(&boot), MEDIA, &f.expected(), NODE).unwrap();
        for (pointer, replacement) in [
            (
                "/nodeInstanceId",
                json!("e3cf0640-63e3-4a24-9790-62f83896bfe0"),
            ),
            ("/genesisBlockHash", Value::Null),
            ("/clientVersion", json!("NIGO/unavailable")),
            ("/engine/build/source/dirty", json!(true)),
            ("/storage/backend", json!("rdb")),
            ("/head/number", json!(9007199254740993_u64)),
            ("/fee/evaluatedAtHeight", json!("9007199254740993")),
        ] {
            let mut value = boot.clone();
            *value.pointer_mut(pointer).unwrap() = replacement;
            bad(validate_bootstrap(
                &bytes(&value),
                MEDIA,
                &f.expected(),
                NODE,
            ));
        }
    }
    #[test]
    fn http_rejects_wrong_media_duplicate_unknown_truncated_and_excessive_inputs() {
        let value = validator_health();
        let raw = bytes(&value);
        validate_health(&raw, "application/vnd.nigo.console+json; charset=UTF-8").unwrap();
        for media in [
            "application/json",
            "text/html",
            "",
            "application/vnd.nigo.console+json; charset=latin1",
        ] {
            bad(validate_health(&raw, media));
        }
        let mut duplicated = b"{\"running\":true,".to_vec();
        duplicated.extend(&raw[1..]);
        for bytes in [
            duplicated,
            raw[..raw.len() - 1].to_vec(),
            [raw.clone(), b" {}".to_vec()].concat(),
            vec![b' '; MAX_HTTP + 1],
        ] {
            bad(validate_health(&bytes, MEDIA));
        }
        let mut value = value.clone();
        value[CANARY] = json!(CANARY);
        bad(validate_health(&bytes(&value), MEDIA));
        let f = Fixture::new();
        let mut boot = f.bootstrap();
        boot["engine"]["capabilities"][0] = json!(CANARY);
        bad(validate_bootstrap(
            &bytes(&boot),
            MEDIA,
            &f.expected(),
            NODE,
        ));
    }
}
