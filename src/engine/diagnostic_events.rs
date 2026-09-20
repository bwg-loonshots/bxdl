//! Bounded, secret-free projections of one init/resume-init/run report.
//!
//! These are reported events, not verified engine identity, operation success,
//! current liveness, safe shutdown, or permission to retry. The caller must read
//! only its latest bound attempt with a safe, bounded, stable file read. It must
//! never substitute stderr, stdout, or an arbitrary discovered JSONL file.
use super::{hash, json, node_identity};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

const MAX_BYTES: usize = 32 * 1024;
const MAX_ROWS: usize = 16;
const MAX_ROW_BYTES: usize = 8 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Projection {
    pub events: Vec<Event>,
    /// Withheld rows/fragments. An uninspected oversized input or row-limit
    /// remainder counts as one; this is deliberately not a raw line count.
    pub omitted: usize,
    /// Some input was missing a newline, invalid, unknown, or outside bounds.
    pub partial: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct Event {
    pub sequence: u64,
    pub command: &'static str,
    pub status: &'static str,
    pub reason: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Row {
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

/// All output strings come from compiled constants. Never echoes identifiers,
/// timestamps, filenames, arbitrary categories, or any `details` value.
/// A truncated tail cannot erase a preceding failure or become a success.
pub(super) fn project(raw: &[u8], command: &str, attempt: &str) -> Projection {
    let mut result = Projection {
        events: Vec::new(),
        omitted: 0,
        partial: false,
    };
    let command = match command {
        "init" => "init",
        "resume-init" => "resume-init",
        "run" => "run",
        _ => return withheld(),
    };
    if attempt.is_empty()
        || attempt.len() > 80
        || !attempt
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        || raw.is_empty()
        || raw.len() > MAX_BYTES
    {
        return withheld();
    }
    let mut remaining = raw;
    let mut pid = None;
    let mut broken_header = false;
    let mut count = 0;
    while !remaining.is_empty() {
        if count == MAX_ROWS {
            result.omitted += 1;
            result.partial = true;
            break;
        }
        let Some(end) = remaining.iter().position(|b| *b == b'\n') else {
            result.omitted += 1;
            result.partial = true;
            break;
        };
        let line = &remaining[..end];
        remaining = &remaining[end + 1..];
        count += 1;
        let row = if line.is_empty() || line.len() > MAX_ROW_BYTES || broken_header {
            None
        } else {
            json::decode::<Row>(line).ok()
        };
        let Some(row) = row else {
            // Do not invent sequence/PID continuity after an unreadable row.
            broken_header = true;
            result.omitted += 1;
            result.partial = true;
            continue;
        };
        let _wall_clock = row.observed_at;
        if row.command != command
            || row.attempt_id != attempt
            || row.sequence != count as u64
            || row.pid == 0
            || pid.is_some_and(|pid| pid != row.pid)
            || row.contract_status != "PROPOSED"
        {
            broken_header = true;
            result.omitted += 1;
            result.partial = true;
            continue;
        }
        pid = Some(row.pid);
        match allowed(&row, command) {
            Some((status, reason)) => result.events.push(Event {
                sequence: row.sequence,
                command,
                status,
                reason,
            }),
            None => {
                result.omitted += 1;
                result.partial = true;
            }
        }
    }
    result
}

fn withheld() -> Projection {
    Projection {
        events: Vec::new(),
        omitted: 1,
        partial: true,
    }
}

// Fixed pairs emitted by 303e163a EngineCommand.managed()/shutdown(). Outer
// INVALID_CONFIGURATION and PRECONDITION_OR_IO_FAILURE are stdout-only DTOs.
fn allowed(row: &Row, command: &str) -> Option<(&'static str, &'static str)> {
    match (row.status.as_str(), row.reason.as_str()) {
        ("CHECKING", "VALIDATING_CONFIGURATION") => {
            let d = object(&row.details, &["build"])?;
            d["build"].as_object()?;
            Some(("CHECKING", "VALIDATING_CONFIGURATION"))
        }
        ("STARTING", "OPENING_MANAGED_STORAGE") => {
            let d = object(&row.details, &["backend"])?;
            if !matches!(d["backend"].as_str()?, "h2" | "rocksdb") {
                return None;
            }
            Some(("STARTING", "OPENING_MANAGED_STORAGE"))
        }
        ("INITIALIZED", "STORAGE_CLOSED") if command != "run" => {
            let d = object(
                &row.details,
                &["genesisHash", "chainFingerprint", "nodeIdentity"],
            )?;
            identity_shape(d)?;
            Some(("INITIALIZED", "STORAGE_CLOSED"))
        }
        ("RUNNING", "LOCAL_STARTUP_COMPLETED") if command == "run" => {
            let d = object(
                &row.details,
                &[
                    "genesisHash",
                    "chainFingerprint",
                    "nodeIdentity",
                    "nodeInstanceId",
                    "networkQuorumVerified",
                ],
            )?;
            identity_shape(d)?;
            let node = d["nodeInstanceId"].as_str()?;
            if !uuid(node) || d["networkQuorumVerified"].as_bool()? {
                return None;
            }
            Some(("RUNNING", "LOCAL_STARTUP_COMPLETED"))
        }
        ("STOPPING", "SHUTDOWN_REQUESTED") if command == "run" => {
            object(&row.details, &[])?;
            Some(("STOPPING", "SHUTDOWN_REQUESTED"))
        }
        ("STOPPED", "STORAGE_AND_CONSENSUS_CLOSED") if command == "run" => {
            let (storage, consensus) = closed(&row.details)?;
            if !storage || !consensus {
                return None;
            }
            Some(("STOPPED", "STORAGE_AND_CONSENSUS_CLOSED"))
        }
        ("FAILED", "STARTUP_OR_INITIALIZATION_FAILED") => {
            let d = object(&row.details, &["resultMayBePartial"])?;
            if !d["resultMayBePartial"].as_bool()? {
                return None;
            }
            Some(("FAILED", "STARTUP_OR_INITIALIZATION_FAILED"))
        }
        ("UNKNOWN", "REPORT_FAILURE") if command == "run" => {
            closed(&row.details)?;
            Some(("UNKNOWN", "REPORT_FAILURE"))
        }
        ("UNKNOWN", "CLOSE_NOT_CONFIRMED") if command == "run" => {
            let (storage, consensus) = closed(&row.details)?;
            if storage && consensus {
                return None;
            }
            Some(("UNKNOWN", "CLOSE_NOT_CONFIRMED"))
        }
        ("UNKNOWN", "CLOSE_OR_REPORT_FAILURE") if command == "run" => {
            object(&row.details, &[])?;
            Some(("UNKNOWN", "CLOSE_OR_REPORT_FAILURE"))
        }
        _ => None,
    }
}
fn object<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    let d = v.as_object()?;
    (d.len() == keys.len() && keys.iter().all(|key| d.contains_key(*key))).then_some(d)
}
fn identity_shape(d: &Map<String, Value>) -> Option<()> {
    if !d["genesisHash"]
        .as_str()?
        .strip_prefix("0x")
        .is_some_and(hash)
        || !hash(d["chainFingerprint"].as_str()?)
        || !node_identity(d["nodeIdentity"].as_str()?)
    {
        return None;
    }
    Some(())
}
fn closed(value: &Value) -> Option<(bool, bool)> {
    let d = object(value, &["storageClosed", "consensusStopReturned"])?;
    Some((
        d["storageClosed"].as_bool()?,
        d["consensusStopReturned"].as_bool()?,
    ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const CANARY: &str = "PRIVATE_DIAGNOSTIC_CANARY\u{1b}[31m";
    const ATTEMPT: &str = "bxdl-run_123";
    fn row(command: &str, sequence: u64, status: &str, reason: &str, details: Value) -> Value {
        json!({"attemptId":ATTEMPT,"command":command,"sequence":sequence,"pid":12345,
            "observedAt":9007199254740993_i64,"status":status,"reason":reason,"contractStatus":"PROPOSED","details":details})
    }
    fn checking(command: &str, sequence: u64) -> Value {
        row(
            command,
            sequence,
            "CHECKING",
            "VALIDATING_CONFIGURATION",
            json!({"build":{"privateCanary":CANARY}}),
        )
    }
    fn identity() -> Value {
        json!({"genesisHash":format!("0x{}","a".repeat(64)),"chainFingerprint":"b".repeat(64),"nodeIdentity":"INSTANT"})
    }
    fn rows(values: &[Value]) -> Vec<u8> {
        let mut raw = Vec::new();
        for value in values {
            raw.extend(serde_json::to_vec(value).unwrap());
            raw.push(b'\n');
        }
        raw
    }
    fn check_secret_free(result: &Projection) {
        let raw = serde_json::to_string(result).unwrap();
        for secret in [
            "PRIVATE_DIAGNOSTIC_CANARY",
            "privateCanary",
            ATTEMPT,
            "12345",
            "observedAt",
            "details",
            "nodeIdentity",
            "genesisHash",
            "pid",
            "\\u001b",
        ] {
            assert!(
                !raw.contains(secret),
                "unexpected value or raw field in projection"
            );
        }
        for event in &result.events {
            assert!(["init", "resume-init", "run"].contains(&event.command));
        }
    }
    #[test]
    fn complete_init_and_resume_events_are_fixed_projection_not_success_proof() {
        for command in ["init", "resume-init"] {
            let raw = rows(&[
                checking(command, 1),
                row(
                    command,
                    2,
                    "STARTING",
                    "OPENING_MANAGED_STORAGE",
                    json!({"backend":"rocksdb"}),
                ),
                row(command, 3, "INITIALIZED", "STORAGE_CLOSED", identity()),
            ]);
            let projected = project(&raw, command, ATTEMPT);
            assert_eq!(projected.events.len(), 3);
            assert_eq!(projected.omitted, 0);
            assert!(!projected.partial);
            assert_eq!(
                serde_json::to_value(&projected.events[2]).unwrap(),
                json!({"sequence":3,"command":command,"status":"INITIALIZED","reason":"STORAGE_CLOSED"})
            );
            check_secret_free(&projected);
        }
    }
    #[test]
    fn exact_run_and_all_terminal_pairs_are_allowed() {
        let mut running = identity();
        running["nodeInstanceId"] = json!("b51f6f4d-e704-4be3-9630-38a93875f30a");
        running["networkQuorumVerified"] = json!(false);
        for (status, reason, details) in [
            (
                "STOPPED",
                "STORAGE_AND_CONSENSUS_CLOSED",
                json!({"storageClosed":true,"consensusStopReturned":true}),
            ),
            (
                "UNKNOWN",
                "REPORT_FAILURE",
                json!({"storageClosed":true,"consensusStopReturned":true}),
            ),
            (
                "UNKNOWN",
                "CLOSE_NOT_CONFIRMED",
                json!({"storageClosed":false,"consensusStopReturned":true}),
            ),
            ("UNKNOWN", "CLOSE_OR_REPORT_FAILURE", json!({})),
            (
                "FAILED",
                "STARTUP_OR_INITIALIZATION_FAILED",
                json!({"resultMayBePartial":true}),
            ),
        ] {
            let raw = rows(&[
                checking("run", 1),
                row(
                    "run",
                    2,
                    "STARTING",
                    "OPENING_MANAGED_STORAGE",
                    json!({"backend":"h2"}),
                ),
                row(
                    "run",
                    3,
                    "RUNNING",
                    "LOCAL_STARTUP_COMPLETED",
                    running.clone(),
                ),
                row("run", 4, "STOPPING", "SHUTDOWN_REQUESTED", json!({})),
                row("run", 5, status, reason, details),
            ]);
            let result = project(&raw, "run", ATTEMPT);
            assert_eq!(result.events.len(), 5);
            assert!(!result.partial);
            assert_eq!(result.events[4].status, status);
            check_secret_free(&result);
        }
    }
    #[test]
    fn unknown_code_details_and_stdout_only_errors_are_withheld() {
        for (status, reason, details) in [
            ("FAILED", CANARY, json!({"resultMayBePartial":true})),
            (
                CANARY,
                "STARTUP_OR_INITIALIZATION_FAILED",
                json!({"resultMayBePartial":true}),
            ),
            (
                "FAILED",
                "STARTUP_OR_INITIALIZATION_FAILED",
                json!({"error":CANARY}),
            ),
            (
                "FAILED",
                "STARTUP_OR_INITIALIZATION_FAILED",
                json!({"resultMayBePartial":true,"error":CANARY}),
            ),
            ("FAILED", "PRECONDITION_OR_IO_FAILURE", json!({})),
            (
                "INVALID_CONFIGURATION",
                "INVALID_ARGUMENTS_OR_CONFIGURATION",
                json!({}),
            ),
            (
                "STOPPED",
                "STORAGE_AND_CONSENSUS_CLOSED",
                json!({"storageClosed":false,"consensusStopReturned":true}),
            ),
        ] {
            let result = project(
                &rows(&[checking("run", 1), row("run", 2, status, reason, details)]),
                "run",
                ATTEMPT,
            );
            assert_eq!(result.events.len(), 1);
            assert_eq!(result.omitted, 1);
            assert!(result.partial);
            check_secret_free(&result);
        }
        let run_only = row("init", 1, "STOPPING", "SHUTDOWN_REQUESTED", json!({}));
        assert!(
            project(&rows(&[run_only]), "init", ATTEMPT)
                .events
                .is_empty()
        );
    }
    #[test]
    fn missing_duplicate_null_and_malformed_header_break_continuity() {
        for key in [
            "attemptId",
            "command",
            "sequence",
            "pid",
            "observedAt",
            "status",
            "reason",
            "contractStatus",
            "details",
        ] {
            let mut first = checking("run", 1);
            first.as_object_mut().unwrap().remove(key);
            let result = project(&rows(&[first, checking("run", 2)]), "run", ATTEMPT);
            assert!(result.events.is_empty());
            assert_eq!(result.omitted, 2);
            assert!(result.partial);
            check_secret_free(&result);
        }
        let good = rows(&[checking("run", 1)]);
        let mut duplicate = b"{\"pid\":12345,".to_vec();
        duplicate.extend_from_slice(&good[1..]);
        for raw in [
            duplicate,
            format!("{CANARY}\n").into_bytes(),
            b"null\n".to_vec(),
            b"{}\n".to_vec(),
            b"\xff\n".to_vec(),
        ] {
            let result = project(&raw, "run", ATTEMPT);
            assert!(result.events.is_empty());
            assert!(result.partial);
            check_secret_free(&result);
        }
    }
    #[test]
    fn stale_attempt_command_sequence_pid_and_contract_are_never_echoed() {
        for (key, value) in [
            ("attemptId", json!(CANARY)),
            ("command", json!("init")),
            ("sequence", json!(4)),
            ("pid", json!(12346)),
            ("contractStatus", json!("ACCEPTED")),
        ] {
            let mut second = checking("run", 2);
            second[key] = value;
            let result = project(
                &rows(&[checking("run", 1), second, checking("run", 3)]),
                "run",
                ATTEMPT,
            );
            assert_eq!(result.events.len(), 1);
            assert_eq!(result.omitted, 2);
            assert!(result.partial);
            check_secret_free(&result);
        }
        assert!(
            project(&rows(&[checking("run", 1)]), "run", "different")
                .events
                .is_empty()
        );
        assert!(project(b"", "unknown", CANARY).partial);
    }
    #[test]
    fn split_lines_keep_only_complete_prefix_and_never_downgrade_failure() {
        let failed = row(
            "init",
            2,
            "FAILED",
            "STARTUP_OR_INITIALIZATION_FAILED",
            json!({"resultMayBePartial":true}),
        );
        let raw = rows(&[checking("init", 1), failed]);
        for cut in 1..raw.len() {
            let result = project(&raw[..cut], "init", ATTEMPT);
            check_secret_free(&result);
            if raw[cut - 1] != b'\n' {
                assert!(result.partial);
                assert!(result.omitted >= 1);
            }
            assert!(
                result
                    .events
                    .iter()
                    .all(|event| event.status != "INITIALIZED")
            );
        }
        let mut raw = raw;
        raw.extend(format!("{{\"secret\":\"{CANARY}").as_bytes());
        let result = project(&raw, "init", ATTEMPT);
        assert_eq!(result.events.last().unwrap().status, "FAILED");
        assert!(result.partial);
        assert_eq!(result.omitted, 1);
        check_secret_free(&result);
    }
    #[test]
    fn size_row_depth_and_timestamp_bounds_are_enforced_without_secret_output() {
        let result = project(&vec![b'x'; MAX_BYTES + 1], "run", ATTEMPT);
        assert!(result.events.is_empty());
        assert_eq!(result.omitted, 1);
        assert!(result.partial);
        let values: Vec<_> = (1..=17).map(|n| checking("run", n)).collect();
        let result = project(&rows(&values), "run", ATTEMPT);
        assert_eq!(result.events.len(), 16);
        assert_eq!(result.omitted, 1);
        assert!(result.partial);
        let mut deep = checking("run", 1);
        let mut nested = json!(CANARY);
        for _ in 0..20 {
            nested = json!({"n":nested});
        }
        deep["details"]["build"] = nested;
        assert!(project(&rows(&[deep]), "run", ATTEMPT).events.is_empty());
        let mut long = checking("run", 1);
        long["details"]["build"] = json!({"value":"x".repeat(MAX_ROW_BYTES)});
        assert!(project(&rows(&[long]), "run", ATTEMPT).events.is_empty());
        for timestamp in [
            json!("9007199254740993"),
            json!(1.5),
            Value::Null,
            json!(u64::MAX),
        ] {
            let mut row = checking("run", 1);
            row["observedAt"] = timestamp;
            assert!(project(&rows(&[row]), "run", ATTEMPT).events.is_empty());
        }
        let mut row = checking("run", 1);
        row["observedAt"] = json!(i64::MIN);
        assert_eq!(project(&rows(&[row]), "run", ATTEMPT).events.len(), 1);
        let empty = project(b"", "run", ATTEMPT);
        assert!(empty.events.is_empty());
        assert!(empty.partial);
        assert_eq!(empty.omitted, 1);
    }
}
