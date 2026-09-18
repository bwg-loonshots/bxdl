//! Pinned 303e163a init/resume-init result binding; no filesystem or process I/O.
//!
//! The caller must first establish the owned child's termination and exit 0,
//! pin the executable/inputs, and safely read the report and engine journal.
//! A valid terminal row alone cannot prove that resource closure succeeded:
//! the JVM can fail after writing it and before emitting the stdout result.

use super::{Identity, fail, hash, json, node_identity};
use crate::error::{BxdlError, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::Path;

const MAX_STDOUT_BYTES: usize = 4_096;
const MAX_REPORT_BYTES: usize = 16_384;
const MAX_EVENT_BYTES: usize = 8_192;
const MAX_INSTANCE_BYTES: usize = 8_192;

pub(super) struct Expected<'a> {
    pub command: &'a str,
    pub attempt_id: &'a str,
    pub pid: u32,
    pub identity: &'a Identity,
    pub backend: &'a str,
    pub chain_fingerprint: &'a str,
    pub node_identity: &'a str,
    pub data_directory: &'a Path,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Success {
    pub genesis_hash: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InitializedOutput {
    status: String,
    genesis_hash: String,
    attempt_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event<D> {
    attempt_id: String,
    command: String,
    sequence: u64,
    pid: u32,
    // Wall-clock time may move backwards. It is an i64 JSON integer in this
    // file contract, independently of HTTP's decimal-string media type.
    observed_at: i64,
    status: String,
    reason: String,
    contract_status: String,
    details: D,
}

impl<D> Event<D> {
    fn matches(&self, expected: &Expected<'_>, sequence: u64, status: &str, reason: &str) -> bool {
        let _observed_at = self.observed_at;
        self.attempt_id == expected.attempt_id
            && self.command == expected.command
            && self.sequence == sequence
            && self.pid == expected.pid
            && self.status == status
            && self.reason == reason
            && self.contract_status == "PROPOSED"
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Checking {
    build: Identity,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Starting {
    backend: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Initialized {
    genesis_hash: String,
    chain_fingerprint: String,
    node_identity: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Instance {
    status: String,
    backend: String,
    chain_fingerprint: String,
    node_identity: String,
    data_directory: String,
    genesis_hash: String,
}

/// Validate all three independent records against one pinned execution.
///
/// `chain_fingerprint` must be the engine's canonical cold result for the
/// frozen input, not the public chain file's raw SHA-256. This deliberately
/// accepts only the three-event successful one-shot contract; it never turns
/// partial, failed, stale or unfamiliar evidence into a successful result.
pub(super) fn validate_success(
    stdout: &[u8],
    report: &[u8],
    instance: &[u8],
    expected: &Expected<'_>,
) -> Result<Success> {
    let data_directory = expected_directory(expected)?;
    if expected.pid == 0 {
        return Err(invalid());
    }

    let output: InitializedOutput = decode(stdout, MAX_STDOUT_BYTES)?;
    let journal: Instance = decode(instance, MAX_INSTANCE_BYTES)?;
    if report.is_empty() || report.len() > MAX_REPORT_BYTES || !report.ends_with(b"\n") {
        return Err(invalid());
    }
    // A trailing partial row after INITIALIZED is not a successful report.
    // Even blank extra rows are outside the pinned three-row contract.
    let mut lines = report[..report.len() - 1].split(|b| *b == b'\n');
    let checking: Event<Checking> = decode(lines.next().ok_or_else(invalid)?, MAX_EVENT_BYTES)?;
    let starting: Event<Starting> = decode(lines.next().ok_or_else(invalid)?, MAX_EVENT_BYTES)?;
    let initialized: Event<Initialized> =
        decode(lines.next().ok_or_else(invalid)?, MAX_EVENT_BYTES)?;
    if lines.next().is_some()
        || !checking.matches(expected, 1, "CHECKING", "VALIDATING_CONFIGURATION")
        || !starting.matches(expected, 2, "STARTING", "OPENING_MANAGED_STORAGE")
        || !initialized.matches(expected, 3, "INITIALIZED", "STORAGE_CLOSED")
        || checking.details.build != *expected.identity
        || starting.details.backend != expected.backend
        || initialized.details.chain_fingerprint != expected.chain_fingerprint
        || initialized.details.node_identity != expected.node_identity
        || output.status != "INITIALIZED"
        || output.attempt_id != expected.attempt_id
        || !genesis_hash(&output.genesis_hash)
        || output.genesis_hash != initialized.details.genesis_hash
        || journal.status != "INITIALIZED"
        || journal.backend != expected.backend
        || journal.chain_fingerprint != expected.chain_fingerprint
        || journal.node_identity != expected.node_identity
        || journal.data_directory != data_directory
        || journal.genesis_hash != output.genesis_hash
    {
        return Err(invalid());
    }
    Ok(Success {
        genesis_hash: output.genesis_hash,
    })
}

/// Check the provider's six-field pending journal before an explicit resume.
/// The caller separately checks the lock/ledger and excludes concurrent use;
/// this function neither opens the database nor certifies recoverability.
pub(super) fn validate_resume(instance: &[u8], expected: &Expected<'_>) -> Result<()> {
    let data_directory = expected_directory(expected)?;
    let journal: Instance = decode(instance, MAX_INSTANCE_BYTES)?;
    if expected.command != "resume-init"
        || journal.status != "INITIALIZING"
        || journal.backend != expected.backend
        || journal.chain_fingerprint != expected.chain_fingerprint
        || journal.node_identity != expected.node_identity
        || journal.data_directory != data_directory
        || !journal.genesis_hash.is_empty()
    {
        return Err(invalid());
    }
    Ok(())
}

fn expected_directory<'a>(expected: &'a Expected<'_>) -> Result<&'a str> {
    let data_directory = expected.data_directory.to_str().ok_or_else(invalid)?;
    if !matches!(expected.command, "init" | "resume-init")
        || expected.attempt_id.is_empty()
        || expected.attempt_id.len() > 80
        || !expected
            .attempt_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        || expected.identity.build_info_status != "AVAILABLE"
        || !matches!(expected.backend, "rocksdb" | "h2")
        || !hash(expected.chain_fingerprint)
        || !node_identity(expected.node_identity)
        || !expected.data_directory.is_absolute()
    {
        return Err(invalid());
    }
    Ok(data_directory)
}

fn decode<T: DeserializeOwned>(bytes: &[u8], maximum: usize) -> Result<T> {
    if bytes.is_empty() || bytes.len() > maximum {
        return Err(invalid());
    }
    json::decode(bytes).map_err(|_| invalid())
}

fn genesis_hash(value: &str) -> bool {
    value.strip_prefix("0x").is_some_and(hash)
}

fn invalid() -> BxdlError {
    fail(
        "ENGINE_INIT_RESULT_INVALID",
        "초기화 출력·실행 보고·엔진 기록의 일치를 확인하지 못했습니다. 결과는 불명입니다.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const CANARY: &str = "PRIVATE_INIT_RESULT_CANARY";
    const PID: u32 = 41_234;
    const DATA: &str = "/private/bxdl fixture/data";

    struct Fixture {
        identity: Identity,
        command: String,
        stdout: Value,
        events: Vec<Value>,
        instance: Value,
        chain: String,
        node: String,
    }

    impl Fixture {
        fn new(command: &str) -> Self {
            // Actual clean provider DTO, with no executable or private input.
            let identity: Identity = json::decode(include_bytes!(
                "../../contracts/nigo/development-clean-2026-09-18/evidence/engine-info.json"
            ))
            .unwrap();
            let genesis = format!("0x{}", "a".repeat(64));
            let chain = "b".repeat(64);
            let node = format!("0x{}:0x{}", "c".repeat(64), "d".repeat(40));
            let event = |sequence, status, reason, details| {
                json!({"attemptId":"init-test_1","command":command,"sequence":sequence,
                    "pid":PID,"observedAt":1770000000123_i64,"status":status,"reason":reason,
                    "contractStatus":"PROPOSED","details":details})
            };
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
                    "INITIALIZED",
                    "STORAGE_CLOSED",
                    json!({"genesisHash":genesis,
                    "chainFingerprint":chain,"nodeIdentity":node}),
                ),
            ];
            Self {
                identity,
                command: command.into(),
                stdout: json!({"status":"INITIALIZED","genesisHash":genesis,"attemptId":"init-test_1"}),
                events,
                instance: json!({"status":"INITIALIZED","backend":"rocksdb","chainFingerprint":chain,
                    "nodeIdentity":node,"dataDirectory":DATA,"genesisHash":genesis}),
                chain,
                node,
            }
        }

        fn expected(&self) -> Expected<'_> {
            Expected {
                command: &self.command,
                attempt_id: "init-test_1",
                pid: PID,
                identity: &self.identity,
                backend: "rocksdb",
                chain_fingerprint: &self.chain,
                node_identity: &self.node,
                data_directory: Path::new(DATA),
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

        fn validate(&self) -> Result<Success> {
            validate_success(
                &serde_json::to_vec(&self.stdout).unwrap(),
                &self.report(),
                &serde_json::to_vec(&self.instance).unwrap(),
                &self.expected(),
            )
        }
    }

    fn rejected(result: Result<Success>) {
        let error = result.unwrap_err();
        assert_eq!(error.code, "ENGINE_INIT_RESULT_INVALID");
        assert!(!error.message.contains(CANARY));
        assert!(!error.message.contains(DATA));
    }

    #[test]
    fn init_and_resume_bind_all_records_and_serialize_only_genesis() {
        for command in ["init", "resume-init"] {
            let fixture = Fixture::new(command);
            let result = fixture.validate().unwrap();
            let value = serde_json::to_value(result.clone()).unwrap();
            assert_eq!(
                value,
                json!({"genesisHash":format!("0x{}", "a".repeat(64))})
            );
            assert_eq!(serde_json::from_value::<Success>(value).unwrap(), result);
        }
        let mut fixture = Fixture::new("init");
        fixture.instance["backend"] = json!("h2");
        fixture.events[1]["details"]["backend"] = json!("h2");
        let mut expected = fixture.expected();
        expected.backend = "h2";
        assert!(
            validate_success(
                &serde_json::to_vec(&fixture.stdout).unwrap(),
                &fixture.report(),
                &serde_json::to_vec(&fixture.instance).unwrap(),
                &expected
            )
            .is_ok()
        );
    }

    #[test]
    fn every_row_binds_attempt_command_sequence_and_owned_pid() {
        for row in 0..3 {
            for (key, value) in [
                ("attemptId", json!("previous-attempt")),
                ("command", json!("resume-init")),
                ("sequence", json!(0)),
                ("pid", json!(PID + 1)),
                ("contractStatus", json!("ACCEPTED")),
            ] {
                let mut fixture = Fixture::new("init");
                fixture.events[row][key] = value;
                rejected(fixture.validate());
            }
        }
        let mut fixture = Fixture::new("init");
        fixture.stdout["attemptId"] = json!("previous-attempt");
        rejected(fixture.validate());
    }

    #[test]
    fn required_event_order_status_reason_and_build_identity_cannot_drift() {
        let mut fixture = Fixture::new("init");
        fixture.events.swap(0, 1);
        rejected(fixture.validate());
        for row in 0..3 {
            for key in ["status", "reason"] {
                let mut fixture = Fixture::new("init");
                fixture.events[row][key] = json!(CANARY);
                rejected(fixture.validate());
            }
        }
        for (pointer, value) in [
            ("/details/build/source/dirty", json!(true)),
            ("/details/build/contract/fingerprint", json!("e".repeat(64))),
            ("/details/build/buildInfoStatus", json!("MISSING")),
        ] {
            let mut fixture = Fixture::new("init");
            *fixture.events[0].pointer_mut(pointer).unwrap() = value;
            rejected(fixture.validate());
        }
    }

    #[test]
    fn public_identity_and_journal_must_match_frozen_inputs() {
        for key in ["chainFingerprint", "nodeIdentity"] {
            let mut fixture = Fixture::new("init");
            fixture.events[2]["details"][key] = json!(CANARY);
            rejected(fixture.validate());
        }
        for (key, value) in [
            ("status", json!("INITIALIZING")),
            ("backend", json!("h2")),
            ("chainFingerprint", json!("e".repeat(64))),
            ("nodeIdentity", json!("INSTANT")),
            ("dataDirectory", json!("/private/different-instance/data")),
            ("genesisHash", json!(format!("0x{}", "e".repeat(64)))),
        ] {
            let mut fixture = Fixture::new("init");
            fixture.instance[key] = value;
            rejected(fixture.validate());
        }
        let mut fixture = Fixture::new("init");
        fixture.events[1]["details"]["backend"] = json!("h2");
        rejected(fixture.validate());
    }

    #[test]
    fn genesis_requires_exact_canonical_format_and_three_way_agreement() {
        for value in [
            "".to_owned(),
            "a".repeat(64),
            format!("0x{}", "A".repeat(64)),
            format!("0x{}", "a".repeat(63)),
            CANARY.into(),
        ] {
            let mut fixture = Fixture::new("init");
            fixture.stdout["genesisHash"] = json!(value);
            fixture.events[2]["details"]["genesisHash"] = json!(value);
            fixture.instance["genesisHash"] = json!(value);
            rejected(fixture.validate());
        }
        for target in 0..3 {
            let mut fixture = Fixture::new("init");
            let replacement = json!(format!("0x{}", "e".repeat(64)));
            match target {
                0 => fixture.stdout["genesisHash"] = replacement,
                1 => fixture.events[2]["details"]["genesisHash"] = replacement,
                _ => fixture.instance["genesisHash"] = replacement,
            }
            rejected(fixture.validate());
        }
    }

    #[test]
    fn missing_partial_extra_and_post_terminal_failure_rows_are_rejected() {
        let fixture = Fixture::new("init");
        let report = fixture.report();
        let stdout = serde_json::to_vec(&fixture.stdout).unwrap();
        let journal = serde_json::to_vec(&fixture.instance).unwrap();
        let mut variants = vec![
            Vec::new(),
            report[..report.len() - 1].to_vec(),
            report[..20].to_vec(),
        ];
        for suffix in [b"\n".as_slice(), b"{\"status\":", b"{}\n"] {
            let mut bytes = report.clone();
            bytes.extend_from_slice(suffix);
            variants.push(bytes);
        }
        let mut extra = Fixture::new("init");
        extra.events.push(json!({"attemptId":"init-test_1","command":"init","sequence":4,"pid":PID,
            "observedAt":1770000000124_i64,"status":"FAILED","reason":"STARTUP_OR_INITIALIZATION_FAILED",
            "contractStatus":"PROPOSED","details":{"resultMayBePartial":true}}));
        variants.push(extra.report());
        extra.events.truncate(2);
        variants.push(extra.report());
        for bytes in variants {
            rejected(validate_success(
                &stdout,
                &bytes,
                &journal,
                &fixture.expected(),
            ));
        }
    }

    #[test]
    fn unknown_missing_null_and_duplicate_fields_are_rejected_without_echo() {
        for target in 0..5 {
            for operation in 0..3 {
                let mut fixture = Fixture::new("init");
                let value = match target {
                    0 => &mut fixture.stdout,
                    1 => &mut fixture.instance,
                    _ => &mut fixture.events[target - 2],
                };
                match operation {
                    0 => {
                        value[CANARY] = json!(CANARY);
                    }
                    1 => {
                        value.as_object_mut().unwrap().remove("status");
                    }
                    _ => {
                        value["status"] = Value::Null;
                    }
                }
                rejected(fixture.validate());
            }
        }
        for row in 0..3 {
            let mut fixture = Fixture::new("init");
            fixture.events[row]["details"][CANARY] = json!(CANARY);
            rejected(fixture.validate());
        }
        let fixture = Fixture::new("init");
        let stdout = serde_json::to_vec(&fixture.stdout).unwrap();
        let journal = serde_json::to_vec(&fixture.instance).unwrap();
        let duplicate = |bytes: &[u8], field: &str| {
            let mut out = format!("{{\"{field}\":\"{CANARY}\",").into_bytes();
            out.extend_from_slice(&bytes[1..]);
            out
        };
        rejected(validate_success(
            &duplicate(&stdout, "status"),
            &fixture.report(),
            &journal,
            &fixture.expected(),
        ));
        rejected(validate_success(
            &stdout,
            &fixture.report(),
            &duplicate(&journal, "status"),
            &fixture.expected(),
        ));
        for row in 0..3 {
            let mut report = Vec::new();
            for (index, event) in fixture.events.iter().enumerate() {
                let bytes = serde_json::to_vec(event).unwrap();
                report.extend(if index == row {
                    duplicate(&bytes, "status")
                } else {
                    bytes
                });
                report.push(b'\n');
            }
            rejected(validate_success(
                &stdout,
                &report,
                &journal,
                &fixture.expected(),
            ));
        }
    }

    #[test]
    fn numeric_identity_is_strict_but_wall_clock_need_not_increase() {
        let mut fixture = Fixture::new("init");
        fixture.events[0]["observedAt"] = json!(i64::MAX);
        fixture.events[1]["observedAt"] = json!(0);
        fixture.events[2]["observedAt"] = json!(-1);
        assert!(fixture.validate().is_ok());
        for key in ["sequence", "pid", "observedAt"] {
            for value in [json!("1"), json!(1.5), json!(true), Value::Null] {
                let mut fixture = Fixture::new("init");
                fixture.events[0][key] = value;
                rejected(fixture.validate());
            }
        }
        let mut fixture = Fixture::new("init");
        fixture.events[0]["observedAt"] = json!(u64::MAX);
        rejected(fixture.validate());
    }

    #[test]
    fn bounded_inputs_and_extra_json_or_secret_text_never_form_success() {
        let fixture = Fixture::new("init");
        let stdout = serde_json::to_vec(&fixture.stdout).unwrap();
        let report = fixture.report();
        let journal = serde_json::to_vec(&fixture.instance).unwrap();
        for raw in [
            vec![b' '; MAX_STDOUT_BYTES + 1],
            [stdout.clone(), b" {}".to_vec()].concat(),
            CANARY.as_bytes().to_vec(),
        ] {
            rejected(validate_success(
                &raw,
                &report,
                &journal,
                &fixture.expected(),
            ));
        }
        rejected(validate_success(
            &stdout,
            &vec![b'\n'; MAX_REPORT_BYTES + 1],
            &journal,
            &fixture.expected(),
        ));
        rejected(validate_success(
            &stdout,
            &report,
            &vec![b' '; MAX_INSTANCE_BYTES + 1],
            &fixture.expected(),
        ));
        let mut long_event = vec![b' '; MAX_EVENT_BYTES + 1];
        long_event.extend_from_slice(&report);
        assert!(long_event.len() < MAX_REPORT_BYTES);
        rejected(validate_success(
            &stdout,
            &long_event,
            &journal,
            &fixture.expected(),
        ));
        let mut fixture = Fixture::new("init");
        fixture.stdout = json!({"status":"FAILED","reason":"STARTUP_OR_INITIALIZATION_FAILED","attemptId":"init-test_1"});
        rejected(fixture.validate());
    }

    #[test]
    fn invalid_expected_scope_cannot_authorize_a_forged_success() {
        let fixture = Fixture::new("init");
        let stdout = serde_json::to_vec(&fixture.stdout).unwrap();
        let report = fixture.report();
        let journal = serde_json::to_vec(&fixture.instance).unwrap();
        for command in ["run", "preflight", ""] {
            let mut expected = fixture.expected();
            expected.command = command;
            rejected(validate_success(&stdout, &report, &journal, &expected));
        }
        let mut expected = fixture.expected();
        expected.pid = 0;
        rejected(validate_success(&stdout, &report, &journal, &expected));
        let mut expected = fixture.expected();
        expected.data_directory = Path::new("relative/data");
        rejected(validate_success(&stdout, &report, &journal, &expected));
    }

    #[test]
    fn resume_requires_exact_pending_journal_and_empty_genesis() {
        let mut fixture = Fixture::new("resume-init");
        fixture.instance["status"] = json!("INITIALIZING");
        fixture.instance["genesisHash"] = json!("");
        let raw = serde_json::to_vec(&fixture.instance).unwrap();
        assert!(validate_resume(&raw, &fixture.expected()).is_ok());
        let mut before_spawn = fixture.expected();
        before_spawn.pid = 0;
        assert!(validate_resume(&raw, &before_spawn).is_ok());
        for (key, value) in [
            ("status", json!("INITIALIZED")),
            ("backend", json!("h2")),
            ("chainFingerprint", json!("e".repeat(64))),
            ("nodeIdentity", json!("INSTANT")),
            ("dataDirectory", json!("/private/other/data")),
            ("genesisHash", json!(format!("0x{}", "a".repeat(64)))),
            (CANARY, json!(CANARY)),
        ] {
            let mut journal = fixture.instance.clone();
            journal[key] = value;
            assert!(
                validate_resume(&serde_json::to_vec(&journal).unwrap(), &fixture.expected())
                    .is_err()
            );
        }
        for field in [
            "status",
            "backend",
            "chainFingerprint",
            "nodeIdentity",
            "dataDirectory",
            "genesisHash",
        ] {
            let mut journal = fixture.instance.clone();
            journal.as_object_mut().unwrap().remove(field);
            assert!(
                validate_resume(&serde_json::to_vec(&journal).unwrap(), &fixture.expected())
                    .is_err()
            );
            journal[field] = Value::Null;
            assert!(
                validate_resume(&serde_json::to_vec(&journal).unwrap(), &fixture.expected())
                    .is_err()
            );
        }
        let mut duplicate = b"{\"genesisHash\":\"\",".to_vec();
        duplicate.extend_from_slice(&raw[1..]);
        assert!(validate_resume(&duplicate, &fixture.expected()).is_err());
        let mut expected = fixture.expected();
        expected.command = "init";
        assert!(validate_resume(&raw, &expected).is_err());
    }
}
