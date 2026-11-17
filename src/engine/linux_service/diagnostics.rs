//! Offline latest-attempt event collection. No HTTP, systemctl, JVM, DB read or
//! recovery. Engine rows are reported evidence, never authority to retry.
use super::*;
use crate::engine::diagnostic_events;

const ACTION_STATUS: &str =
    "실시간 상태는 bxdl status <이름> 또는 --instance <경로>로 별도 확인하세요.";
const ACTION_UNKNOWN: &str =
    "불명 시도의 기록과 데이터를 보존하고 자동 초기화·재시작·삭제하지 마세요.";
const ACTION_PARTIAL: &str = "sources의 누락·변경·한도 이유를 확인하고 필요하면 다시 수집하세요.";
const RAW_POLICY: &str = "ALLOWLIST_EVENTS_ONLY_RAW_TEXT_AND_IDENTIFIERS_WITHHELD";

#[derive(Clone, Debug)]
pub struct Options {
    pub tail: usize,
    pub max_bytes: u64,
    pub timeout: Duration,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            tail: 50,
            max_bytes: 262144,
            timeout: Duration::from_secs(5),
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        if !(1..=200).contains(&self.tail)
            || !(4096..=1_048_576).contains(&self.max_bytes)
            || self.timeout.is_zero()
            || self.timeout > Duration::from_secs(30)
        {
            return Err(error("LINUX_DIAGNOSTIC_OPTIONS_INVALID"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub partial: bool,
    pub mode: String,
    pub scope: String,
    pub initialization_record: String,
    pub service_record: String,
    pub runtime_readiness: String,
    pub global_consensus: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_busy: Option<bool>,
    pub bytes_read: u64,
    pub events_omitted: usize,
    pub events: Vec<Event>,
    pub sources: Vec<Source>,
    pub next_actions: Vec<String>,
    pub raw_text_policy: String,
    pub output_created: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub source: String,
    pub sequence: u64,
    pub command: String,
    pub status: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub name: String,
    pub result: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InitCommand {
    schema_version: u32,
    attempt_id: String,
    command: String,
}

pub(super) fn record_init_command(b: &Binding, attempt: &str, command: &str) -> Result<()> {
    if !valid_attempt(attempt) || !matches!(command, "init" | "resume-init") {
        return Err(unknown());
    }
    boundary::write_new(
        &b.control
            .join("attempts")
            .join(attempt)
            .join("init-command.json"),
        &encoded(&InitCommand {
            schema_version: 1,
            attempt_id: attempt.into(),
            command: command.into(),
        })?,
        0,
        0o600,
    )
}
struct Collector {
    report: Report,
    remaining: u64,
    deadline: Instant,
}
impl Collector {
    fn new(options: &Options) -> Result<Self> {
        options.validate()?;
        Ok(Self {
            remaining: options.max_bytes,
            deadline: Instant::now() + options.timeout,
            report: Report {
                schema_version: 1,
                partial: false,
                mode: "OFFLINE_RECORDED_EVIDENCE".into(),
                scope: "LATEST_LINUX_INITIALIZATION_AND_SERVICE_ATTEMPTS".into(),
                initialization_record: "UNAVAILABLE".into(),
                service_record: "UNAVAILABLE".into(),
                runtime_readiness: "NOT_OBSERVED".into(),
                global_consensus: "NOT_CHECKED".into(),
                operation_busy: None,
                bytes_read: 0,
                events_omitted: 0,
                events: Vec::new(),
                sources: Vec::new(),
                next_actions: Vec::new(),
                raw_text_policy: RAW_POLICY.into(),
                output_created: false,
            },
        })
    }
    fn source(&mut self, name: &str, result: &str, reason: &str, size: Option<u64>) {
        self.report.sources.push(Source {
            name: name.into(),
            result: result.into(),
            reason: reason.into(),
            size_bytes: size,
        });
    }
    fn missing(&mut self, name: &str, reason: &str) {
        self.report.partial = true;
        self.source(name, "UNAVAILABLE", reason, None);
    }
    fn ready(&mut self, name: &str) -> bool {
        if Instant::now() >= self.deadline {
            self.missing(name, "TIME_BUDGET_EXCEEDED");
            false
        } else if self.remaining == 0 {
            self.missing(name, "BYTE_BUDGET_EXCEEDED");
            false
        } else {
            true
        }
    }
    fn read(&mut self, name: &str, path: &Path, uid: u32, limit: u64) -> Option<Vec<u8>> {
        if !self.ready(name) {
            return None;
        }
        let maximum = self.remaining.min(limit);
        self.remaining -= maximum;
        let bytes = if uid == 0 {
            boundary::root_read(path, maximum)
        } else {
            boundary::read_owned(path, uid, maximum)
        };
        match bytes {
            Ok(raw) => {
                self.remaining += maximum - raw.len() as u64;
                self.report.bytes_read += raw.len() as u64;
                self.source(
                    name,
                    "COLLECTED",
                    "BOUNDED_STABLE_SNAPSHOT",
                    Some(raw.len() as u64),
                );
                Some(raw)
            }
            Err(_) => {
                self.missing(name, "MISSING_UNSAFE_CHANGING_OR_LIMIT");
                None
            }
        }
    }
    fn projected(&mut self, name: &str, raw: &[u8], command: &str, attempt: &str) {
        let projected = diagnostic_events::project(raw, command, attempt);
        self.report.partial |= projected.partial;
        self.report.events_omitted += projected.omitted;
        self.report
            .events
            .extend(projected.events.into_iter().map(|e| Event {
                source: name.into(),
                sequence: e.sequence,
                command: e.command.into(),
                status: e.status.into(),
                reason: e.reason.into(),
            }));
        if projected.partial {
            self.source(name, "WITHHELD", "UNKNOWN_OR_PARTIAL_ROWS", None);
        }
    }
    fn finish(mut self, options: &Options) -> Result<Report> {
        if self.report.events.len() > options.tail {
            let count = self.report.events.len() - options.tail;
            self.report.events.drain(..count);
            self.report.events_omitted += count;
            self.report.partial = true;
            self.source("events", "TRUNCATED", "TAIL_LIMIT", None);
        }
        self.source(
            "rawOutput",
            "WITHHELD",
            "RAW_STDOUT_STDERR_SYSTEM_JOURNAL_EXCLUDED",
            None,
        );
        self.source(
            "historicalAttempts",
            "WITHHELD",
            "LATEST_BOUND_ATTEMPTS_ONLY",
            None,
        );
        self.report.next_actions.push(ACTION_STATUS.into());
        if matches!(
            self.report.initialization_record.as_str(),
            "INIT_INTENT" | "UNKNOWN" | "UNAVAILABLE"
        ) || matches!(
            self.report.service_record.as_str(),
            "START_PREPARED" | "GATE_REFUSED" | "UNAVAILABLE"
        ) {
            self.report.next_actions.push(ACTION_UNKNOWN.into());
        }
        if self.report.partial {
            self.report.next_actions.push(ACTION_PARTIAL.into());
        }
        let mut clipped = false;
        while encoded(&self.report)?.len() as u64 > options.max_bytes {
            if self.report.events.is_empty() {
                return Err(error("LINUX_DIAGNOSTIC_OUTPUT_LIMIT"));
            }
            if !clipped {
                self.source("events", "TRUNCATED", "OUTPUT_BYTE_LIMIT", None);
                clipped = true;
            }
            self.report.events.remove(0);
            self.report.events_omitted += 1;
            self.report.partial = true;
        }
        self.report.validate(options)?;
        Ok(self.report)
    }
}
/// Root administrative metadata/account lookups have their own fixed bounds;
/// max_bytes is the combined budget for collected binding/journal/report bytes.
pub fn collect(instance: &Path, options: &Options) -> Result<Report> {
    options.validate()?;
    boundary::root()?;
    let mut c = Collector::new(options)?;
    // Bound before the normal binding reader, which also checks the root-owned
    // account and control identity. Never read an oversized binding then clip.
    boundary::trusted(instance)?;
    let binding_path = instance.join("binding.json");
    boundary::trusted(&binding_path)?;
    let binding_len = fs::symlink_metadata(&binding_path)
        .map_err(|_| unknown())?
        .len();
    if binding_len > c.remaining || !c.ready("rootBinding") {
        c.missing("rootBinding", "BYTE_BUDGET_EXCEEDED");
        return c.finish(options);
    }
    let (b, raw) = read_binding(instance)?;
    if raw.len() as u64 > c.remaining {
        c.missing("rootBinding", "BYTE_BUDGET_EXCEEDED");
        return c.finish(options);
    }
    c.remaining -= raw.len() as u64;
    c.report.bytes_read += raw.len() as u64;
    c.source(
        "rootBinding",
        "COLLECTED",
        "ROOT_BINDING_VERIFIED",
        Some(raw.len() as u64),
    );
    let journal_path = instance.join("journal");
    boundary::trusted(&journal_path)?;
    for (count, entry) in fs::read_dir(&journal_path)
        .map_err(|_| unknown())?
        .enumerate()
    {
        if count >= 8192 || Instant::now() >= c.deadline {
            c.missing("rootJournal", "JOURNAL_UNAVAILABLE_OR_LIMIT");
            return c.finish(options);
        }
        boundary::trusted(&entry.map_err(|_| unknown())?.path())?;
    }
    if !c.ready("rootJournal") {
        return c.finish(options);
    }
    let store = match Store::open_bounded(&journal_path, (c.remaining / 2) as usize) {
        Ok(s) => s,
        Err(_) => {
            c.missing("rootJournal", "JOURNAL_UNAVAILABLE_OR_LIMIT");
            return c.finish(options);
        }
    };
    let snapshot = match store.loaded_snapshot() {
        Some(raw) => raw,
        None => {
            c.missing("rootJournal", "JOURNAL_UNAVAILABLE_OR_LIMIT");
            return c.finish(options);
        }
    };
    let state: State = json::decode(snapshot).map_err(|_| unknown())?;
    validate_state(&state, &files::digest(&raw))?;
    // Reserve the final bounded snapshot reread within the combined byte budget.
    let journal_bytes = snapshot.len();
    c.remaining -= 2 * journal_bytes as u64;
    c.report.bytes_read += journal_bytes as u64;
    c.source(
        "rootJournal",
        "COLLECTED",
        "ROOT_JOURNAL_VERIFIED",
        Some(snapshot.len() as u64),
    );
    c.report.initialization_record = match state.phase {
        Phase::Registered => "REGISTERED",
        Phase::InitIntent => "INIT_INTENT",
        Phase::Unknown => "UNKNOWN",
        Phase::Initialized => "INITIALIZED",
    }
    .into();
    c.report.service_record = match state.run.as_ref().map(|r| &r.phase) {
        None => "NOT_STARTED",
        Some(RunPhase::Prepared) => "START_PREPARED",
        Some(RunPhase::Consumed) => "GATE_CONSUMED",
        Some(RunPhase::Refused) => "GATE_REFUSED",
        Some(RunPhase::StoppedVerified) => "STOPPED_VERIFIED",
    }
    .into();
    c.report.operation_busy = boundary::busy(instance).ok();
    if c.report.operation_busy.is_none() {
        c.missing("rootJournal", "LOCK_OBSERVATION_UNKNOWN");
    }
    if let Some(attempt) = &state.init_attempt {
        let command_path = b
            .control
            .join("attempts")
            .join(attempt)
            .join("init-command.json");
        let command = if fs::symlink_metadata(&command_path).is_ok() {
            c.read("initializationCommand", &command_path, 0, 4096)
                .and_then(|raw| json::decode::<InitCommand>(&raw).ok())
                .filter(|v| {
                    v.schema_version == 1
                        && v.attempt_id == *attempt
                        && matches!(v.command.as_str(), "init" | "resume-init")
                })
                .map(|v| v.command)
        } else {
            // LNX03 success receipts retain the command. Pending legacy attempts
            // have no such root evidence and must remain unavailable, not guessed.
            let path = b
                .control
                .join("attempts")
                .join(attempt)
                .join("verified-result.json");
            c.read("initializationCommand", &path, 0, 4096)
                .and_then(|raw| json::decode::<Value>(&raw).ok())
                .filter(|v| {
                    v["attemptId"] == *attempt
                        && matches!(v["command"].as_str(), Some("init" | "resume-init"))
                })
                .and_then(|v| v["command"].as_str().map(str::to_owned))
        };
        if let Some(command) = command {
            let work = output(&b, attempt);
            if boundary::private_directory(&work, b.service_uid, b.service_gid).is_err() {
                c.missing("initializationReport", "MISSING_UNSAFE_CHANGING_OR_LIMIT");
            } else if let Some(raw) = c.read(
                "initializationReport",
                &work.join("report.jsonl"),
                b.service_uid,
                32768,
            ) {
                c.projected("initializationReport", &raw, &command, attempt);
            }
        } else {
            c.missing("initializationReport", "ROOT_COMMAND_BINDING_UNAVAILABLE");
        }
    } else {
        c.source(
            "initializationReport",
            "NOT_APPLICABLE",
            "NO_BOUND_ATTEMPT",
            None,
        );
    }
    if let Some(run) = &state.run {
        let work = output(&b, &run.attempt);
        if boundary::private_directory(&work, b.service_uid, b.service_gid).is_err() {
            c.missing("serviceReport", "MISSING_UNSAFE_CHANGING_OR_LIMIT");
        } else if let Some(raw) = c.read(
            "serviceReport",
            &work.join("report.jsonl"),
            b.service_uid,
            32768,
        ) {
            c.projected("serviceReport", &raw, "run", &run.attempt);
        }
    } else {
        c.source("serviceReport", "NOT_APPLICABLE", "NO_BOUND_ATTEMPT", None);
    }
    if Instant::now() >= c.deadline {
        c.missing("rootJournal", "TIME_BUDGET_EXCEEDED");
    } else if store.read_bounded(journal_bytes).is_err() {
        c.missing("rootJournal", "RECORD_CHANGED_DURING_COLLECTION");
    } else {
        c.report.bytes_read += journal_bytes as u64;
    }
    c.finish(options)
}
/// Publish a validated report as the invoking user. Root workers never accept an
/// output path. The anchored writer refuses links, replacements and overwrites.
pub fn write_export(control: &Path, destination: &Path, raw: &[u8]) -> Result<()> {
    let uid = rustix::process::getuid().as_raw();
    if uid == 0 || uid != rustix::process::geteuid().as_raw() {
        return Err(error("LINUX_DIAGNOSTIC_USER_OUTPUT_REQUIRED"));
    }
    boundary::safe_absolute(control)?;
    boundary::safe_absolute(destination)?;
    if destination.starts_with(control) || control.starts_with(destination) {
        return Err(error("LINUX_DIAGNOSTIC_OUTPUT_OVERLAP"));
    }
    crate::engine::diagnostic_io::write_export(destination, raw)
}
impl Report {
    /// Validate the narrow wire DTO before a manager displays or writes it.
    pub(crate) fn validate(&self, options: &Options) -> Result<()> {
        options.validate()?;
        if self.schema_version != 1
            || self.mode != "OFFLINE_RECORDED_EVIDENCE"
            || self.scope != "LATEST_LINUX_INITIALIZATION_AND_SERVICE_ATTEMPTS"
            || self.runtime_readiness != "NOT_OBSERVED"
            || self.global_consensus != "NOT_CHECKED"
            || self.raw_text_policy != RAW_POLICY
            || self.output_created
            || self.bytes_read > options.max_bytes
            || self.events.len() > options.tail
            || self.sources.len() > 24
            || self.next_actions.len() > 3
            || encoded(self)?.len() as u64 > options.max_bytes
            || (!self.partial && self.events_omitted != 0)
            || self
                .next_actions
                .iter()
                .any(|s| ![ACTION_STATUS, ACTION_UNKNOWN, ACTION_PARTIAL].contains(&s.as_str()))
            || !matches!(
                self.initialization_record.as_str(),
                "REGISTERED" | "INIT_INTENT" | "UNKNOWN" | "INITIALIZED" | "UNAVAILABLE"
            )
            || !matches!(
                self.service_record.as_str(),
                "NOT_STARTED"
                    | "START_PREPARED"
                    | "GATE_CONSUMED"
                    | "GATE_REFUSED"
                    | "STOPPED_VERIFIED"
                    | "UNAVAILABLE"
            )
        {
            return Err(error("LINUX_DIAGNOSTIC_RESPONSE_INVALID"));
        }
        for event in &self.events {
            if !matches!(
                event.source.as_str(),
                "initializationReport" | "serviceReport"
            ) || !matches!(event.command.as_str(), "init" | "resume-init" | "run")
                || !(1..=16).contains(&event.sequence)
                || (event.source == "serviceReport") != (event.command == "run")
                || !allowed_event(event)
            {
                return Err(error("LINUX_DIAGNOSTIC_RESPONSE_INVALID"));
            }
        }
        for source in &self.sources {
            if !matches!(
                source.name.as_str(),
                "rootBinding"
                    | "rootJournal"
                    | "initializationCommand"
                    | "initializationReport"
                    | "serviceReport"
                    | "events"
                    | "rawOutput"
                    | "historicalAttempts"
            ) || !matches!(
                source.result.as_str(),
                "COLLECTED" | "UNAVAILABLE" | "NOT_APPLICABLE" | "WITHHELD" | "TRUNCATED"
            ) || !matches!(
                source.reason.as_str(),
                "BOUNDED_STABLE_SNAPSHOT"
                    | "BYTE_BUDGET_EXCEEDED"
                    | "JOURNAL_UNAVAILABLE_OR_LIMIT"
                    | "LATEST_BOUND_ATTEMPTS_ONLY"
                    | "LOCK_OBSERVATION_UNKNOWN"
                    | "MISSING_UNSAFE_CHANGING_OR_LIMIT"
                    | "NO_BOUND_ATTEMPT"
                    | "OUTPUT_BYTE_LIMIT"
                    | "RAW_STDOUT_STDERR_SYSTEM_JOURNAL_EXCLUDED"
                    | "RECORD_CHANGED_DURING_COLLECTION"
                    | "ROOT_BINDING_VERIFIED"
                    | "ROOT_COMMAND_BINDING_UNAVAILABLE"
                    | "ROOT_JOURNAL_VERIFIED"
                    | "TAIL_LIMIT"
                    | "TIME_BUDGET_EXCEEDED"
                    | "UNKNOWN_OR_PARTIAL_ROWS"
            ) || source.size_bytes.is_some_and(|n| n > options.max_bytes)
                || (!self.partial && matches!(source.result.as_str(), "UNAVAILABLE" | "TRUNCATED"))
            {
                return Err(error("LINUX_DIAGNOSTIC_RESPONSE_INVALID"));
            }
        }
        Ok(())
    }
}
fn allowed_event(event: &Event) -> bool {
    match (event.status.as_str(), event.reason.as_str()) {
        ("CHECKING", "VALIDATING_CONFIGURATION")
        | ("STARTING", "OPENING_MANAGED_STORAGE")
        | ("FAILED", "STARTUP_OR_INITIALIZATION_FAILED") => true,
        ("INITIALIZED", "STORAGE_CLOSED") => event.command != "run",
        ("RUNNING", "LOCAL_STARTUP_COMPLETED")
        | ("STOPPING", "SHUTDOWN_REQUESTED")
        | ("STOPPED", "STORAGE_AND_CONSENSUS_CLOSED")
        | ("UNKNOWN", "REPORT_FAILURE")
        | ("UNKNOWN", "CLOSE_NOT_CONFIRMED")
        | ("UNKNOWN", "CLOSE_OR_REPORT_FAILURE") => event.command == "run",
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_reject_before_collecting() {
        for (tail, bytes, seconds) in [
            (0, 4096, 1),
            (201, 4096, 1),
            (1, 4095, 1),
            (1, 1_048_577, 1),
            (1, 4096, 0),
            (1, 4096, 31),
        ] {
            assert!(
                Options {
                    tail,
                    max_bytes: bytes,
                    timeout: Duration::from_secs(seconds)
                }
                .validate()
                .is_err()
            );
        }
    }
    #[test]
    fn unknown_rows_never_echo_private_content_or_complete_partial_report() {
        let options = Options::default();
        let mut c = Collector::new(&options).unwrap();
        c.projected(
            "serviceReport",
            b"{\"PRIVATE_CANARY\":\"/secret/path\"}\n",
            "run",
            "attempt",
        );
        let report = c.finish(&options).unwrap();
        assert!(report.partial);
        assert!(report.events.is_empty());
        assert!(
            !String::from_utf8(encoded(&report).unwrap())
                .unwrap()
                .contains("PRIVATE_CANARY")
        );
    }
    #[test]
    fn intentional_raw_exclusion_is_not_an_engine_failure() {
        let options = Options::default();
        let mut c = Collector::new(&options).unwrap();
        c.report.initialization_record = "REGISTERED".into();
        c.report.service_record = "NOT_STARTED".into();
        let report = c.finish(&options).unwrap();
        assert!(!report.partial);
        assert_eq!(report.runtime_readiness, "NOT_OBSERVED");
        assert!(report.sources.iter().all(|s| s.result == "WITHHELD"));
    }
    #[test]
    fn tail_limit_preserves_partial_and_omitted_count() {
        let options = Options {
            tail: 1,
            ..Options::default()
        };
        let mut c = Collector::new(&options).unwrap();
        for sequence in 1..=3 {
            c.report.events.push(Event {
                source: "serviceReport".into(),
                sequence,
                command: "run".into(),
                status: "CHECKING".into(),
                reason: "VALIDATING_CONFIGURATION".into(),
            });
        }
        let report = c.finish(&options).unwrap();
        assert!(report.partial);
        assert_eq!(report.events_omitted, 2);
        assert_eq!(report.events[0].sequence, 3);
    }

    fn event_rows(command: &str, attempt: &str, terminal: bool) -> Vec<u8> {
        let mut rows = vec![
            serde_json::json!({"attemptId":attempt,"command":command,"sequence":1,"pid":1234,"observedAt":1,"status":"CHECKING","reason":"VALIDATING_CONFIGURATION","contractStatus":"PROPOSED","details":{"build":{"private":"PRIVATE_CANARY"}}}),
        ];
        if terminal {
            rows.push(serde_json::json!({"attemptId":attempt,"command":command,"sequence":2,"pid":1234,"observedAt":2,"status":"FAILED","reason":"STARTUP_OR_INITIALIZATION_FAILED","contractStatus":"PROPOSED","details":{"resultMayBePartial":true}}));
        }
        let mut raw = Vec::new();
        for row in rows {
            raw.extend(serde_json::to_vec(&row).unwrap());
            raw.push(b'\n');
        }
        raw
    }
    #[test]
    fn latest_init_and_run_events_roundtrip_without_identifiers_or_success_inference() {
        let options = Options::default();
        let mut c = Collector::new(&options).unwrap();
        c.report.initialization_record = "UNKNOWN".into();
        c.report.service_record = "GATE_REFUSED".into();
        c.projected(
            "initializationReport",
            &event_rows("resume-init", "init-bound", true),
            "resume-init",
            "init-bound",
        );
        c.projected(
            "serviceReport",
            &event_rows("run", "run-bound", false),
            "run",
            "run-bound",
        );
        let report = c.finish(&options).unwrap();
        assert_eq!(report.events.len(), 3);
        assert!(!report.partial);
        assert_eq!(report.events[1].status, "FAILED");
        assert_eq!(report.runtime_readiness, "NOT_OBSERVED");
        let raw = encoded(&report).unwrap();
        let text = String::from_utf8(raw.clone()).unwrap();
        for secret in ["PRIVATE_CANARY", "init-bound", "run-bound", "1234", "null"] {
            assert!(!text.contains(secret));
        }
        let decoded: Report = json::decode(&raw).unwrap();
        decoded.validate(&options).unwrap();
        let mut changed = decoded;
        changed.events[0].reason = "PRIVATE_CANARY".into();
        assert!(changed.validate(&options).is_err());
    }
    #[test]
    fn changed_attempt_is_partial_and_cannot_reuse_old_events() {
        let options = Options::default();
        let mut c = Collector::new(&options).unwrap();
        c.projected(
            "initializationReport",
            &event_rows("init", "old", true),
            "init",
            "new",
        );
        let report = c.finish(&options).unwrap();
        assert!(report.partial);
        assert!(report.events.is_empty());
        assert_eq!(report.events_omitted, 2);
    }
    #[test]
    fn export_is_user_owned_exclusive_and_rejects_control_overlap() {
        if rustix::process::getuid().as_raw() == 0 {
            return;
        }
        let temporary = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let control = root.join("control");
        let target = root.join("diagnose.json");
        write_export(&control, &target, b"{}").unwrap();
        assert_eq!(fs::metadata(&target).unwrap().mode() & 0o7777, 0o600);
        assert!(write_export(&control, &target, b"replacement").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"{}");
        assert_eq!(
            write_export(&control, &control.join("report.json"), b"{}")
                .unwrap_err()
                .code,
            "LINUX_DIAGNOSTIC_OUTPUT_OVERLAP"
        );
        assert_eq!(
            write_export(&control, &root, b"{}").unwrap_err().code,
            "LINUX_DIAGNOSTIC_OUTPUT_OVERLAP"
        );
    }
}
