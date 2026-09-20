//! Offline, bounded projections of private records. No engine, network, service
//! control, database read, or recovery is performed. Recorded events are never
//! used as proof of current liveness, readiness, or successful initialization.
use super::super::{diagnostic_events, diagnostic_io};
use super::*;
use std::time::Instant;

pub struct Options {
    pub tail: usize,
    pub max_bytes: u64,
    pub timeout: Duration,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            tail: 50,
            max_bytes: 262_144,
            timeout: Duration::from_secs(5),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema_version: u32,
    pub partial: bool,
    pub mode: &'static str,
    pub scope: &'static str,
    pub initialization_record: &'static str,
    pub initialization_reason: &'static str,
    pub service_record: &'static str,
    pub runtime_readiness: &'static str,
    pub global_consensus: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_busy: Option<bool>,
    pub bytes_read: u64,
    pub events_omitted: usize,
    pub events: Vec<RecordedEvent>,
    pub sources: Vec<Source>,
    pub next_actions: Vec<&'static str>,
    pub raw_text_policy: &'static str,
    pub output_created: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedEvent {
    pub source: &'static str,
    #[serde(flatten)]
    event: diagnostic_events::Event,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub name: &'static str,
    pub result: &'static str,
    pub reason: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}
struct Collector {
    report: Report,
    deadline: Instant,
    remaining: u64,
}
impl Collector {
    fn new(options: &Options) -> Result<Self> {
        if !(1..=200).contains(&options.tail)
            || !(4096..=1_048_576).contains(&options.max_bytes)
            || options.timeout.is_zero()
            || options.timeout > Duration::from_secs(30)
        {
            return Err(error(
                "DIAGNOSTIC_OPTIONS_INVALID",
                "진단 수집 범위가 올바르지 않습니다.",
            ));
        }
        Ok(Self {
            report: Report {
                schema_version: 1,
                partial: false,
                mode: "OFFLINE_RECORDED_EVIDENCE",
                scope: "LATEST_INITIALIZATION_AND_SERVICE_ATTEMPTS",
                initialization_record: "UNAVAILABLE",
                initialization_reason: "NOT_OBSERVED",
                service_record: "UNAVAILABLE",
                runtime_readiness: "NOT_OBSERVED",
                global_consensus: "NOT_CHECKED",
                operation_busy: None,
                bytes_read: 0,
                events_omitted: 0,
                events: Vec::new(),
                sources: Vec::new(),
                next_actions: Vec::new(),
                raw_text_policy: "ALLOWLIST_EVENTS_ONLY_RAW_TEXT_AND_IDENTIFIERS_WITHHELD",
                output_created: false,
            },
            deadline: Instant::now() + options.timeout,
            remaining: options.max_bytes,
        })
    }
    fn source(
        &mut self,
        name: &'static str,
        result: &'static str,
        reason: &'static str,
        bytes: Option<u64>,
    ) {
        self.report.sources.push(Source {
            name,
            result,
            reason,
            size_bytes: bytes,
        });
    }
    fn incomplete(&mut self, name: &'static str, reason: &'static str) {
        self.report.partial = true;
        self.source(name, "UNAVAILABLE", reason, None);
    }
    fn ready(&mut self, name: &'static str) -> bool {
        if Instant::now() >= self.deadline {
            self.incomplete(name, "TIME_BUDGET_EXCEEDED");
            false
        } else if self.remaining == 0 {
            self.incomplete(name, "BYTE_BUDGET_EXCEEDED");
            false
        } else {
            true
        }
    }
    fn read(&mut self, name: &'static str, path: &Path, limit: u64) -> Option<Vec<u8>> {
        if !self.ready(name) {
            return None;
        }
        let maximum = self.remaining.min(limit);
        // Reserve even failed reads: errors may have consumed their entire cap.
        self.remaining -= maximum;
        match diagnostic_io::read_private(path, maximum) {
            Ok(input) => {
                self.remaining += maximum - input.raw.len() as u64;
                self.report.bytes_read += input.raw.len() as u64;
                if input.truncated {
                    self.report.partial = true;
                    self.source(name, "TRUNCATED", "BYTE_LIMIT", Some(input.bytes));
                } else {
                    self.source(name, "COLLECTED", "PRIVATE_SNAPSHOT", Some(input.bytes));
                }
                Some(input.raw)
            }
            Err(_) => {
                self.incomplete(name, "MISSING_UNSAFE_OR_CHANGING_INPUT");
                None
            }
        }
    }
    fn journal(&mut self, name: &'static str, path: &Path) -> Option<(Store, Vec<u8>)> {
        if !self.ready(name) {
            return None;
        }
        let maximum = self.remaining.min(262_144) as usize;
        self.remaining -= maximum as u64;
        let store = match Store::open_bounded(path, maximum) {
            Ok(store) => store,
            Err(_) => {
                self.incomplete(name, "JOURNAL_UNAVAILABLE_OR_LIMIT");
                return None;
            }
        };
        let Some(raw) = store.loaded_snapshot().map(<[u8]>::to_vec) else {
            self.incomplete(name, "JOURNAL_UNAVAILABLE");
            return None;
        };
        self.remaining += maximum as u64 - raw.len() as u64;
        self.report.bytes_read += raw.len() as u64;
        self.source(
            name,
            "COLLECTED",
            "RECORDED_STATE_ONLY",
            Some(raw.len() as u64),
        );
        Some((store, raw))
    }
    fn recheck_journal(&mut self, name: &'static str, store: &Store) {
        if !self.ready(name) {
            return;
        }
        let maximum = self.remaining.min(262_144) as usize;
        self.remaining -= maximum as u64;
        match store.read_bounded(maximum) {
            Ok(Some(raw)) => {
                self.remaining += maximum as u64 - raw.len() as u64;
                self.report.bytes_read += raw.len() as u64;
            }
            _ => self.incomplete(name, "STATE_CHANGED_UNAVAILABLE_OR_LIMIT"),
        }
    }
    fn events(&mut self, name: &'static str, path: &Path, command: &str, attempt: &str) {
        if let Some(raw) = self.read(name, path, 32_768) {
            let projected = diagnostic_events::project(&raw, command, attempt);
            self.report.partial |= projected.partial;
            self.report.events_omitted += projected.omitted;
            self.report
                .events
                .extend(projected.events.into_iter().map(|event| RecordedEvent {
                    source: name,
                    event,
                }));
            if projected.partial {
                self.source(
                    name,
                    "PARTIAL",
                    "UNRECOGNIZED_OR_INCOMPLETE_EVENTS_WITHHELD",
                    None,
                );
            }
        }
    }
    fn raw_log(&mut self, name: &'static str, path: &Path) {
        if !self.ready(name) {
            return;
        }
        match diagnostic_io::private_size(path) {
            Ok(bytes) => self.source(name, "WITHHELD", "RAW_TEXT_EXCLUDED_BY_POLICY", Some(bytes)),
            Err(_) => self.incomplete(name, "MISSING_UNSAFE_OR_CHANGING_INPUT"),
        }
    }
}

pub fn logs(instance: &Path, options: &Options) -> Result<Report> {
    let (mut collector, _, _) = collect(instance, options)?;
    finish(&mut collector.report, options)?;
    Ok(collector.report)
}
pub fn diagnose(instance: &Path, output: &Path, options: &Options) -> Result<Report> {
    let (mut collector, root, binding) = collect(instance, options)?;
    let binding = binding.ok_or_else(|| error("DIAGNOSTIC_EXPORT_SCOPE_UNKNOWN", "등록 경로를 안전하게 확인할 수 없어 파일을 생성하지 않았습니다. logs로 읽을 수 있는 기록을 확인하세요."))?;
    let output = files::absolute(output)?;
    check_output(&output, root.path(), &binding)?;
    root.recheck()?;
    collector.report.output_created = true;
    finish(&mut collector.report, options)?;
    let raw = serde_json::to_vec(&collector.report).map_err(|_| internal())?;
    diagnostic_io::write_export(&output, &raw)?;
    Ok(collector.report)
}
fn collect(instance: &Path, options: &Options) -> Result<(Collector, Control, Option<Binding>)> {
    let mut c = Collector::new(options)?;
    let root = Control::open(&files::absolute(instance)?)?;
    c.report.operation_busy = match root.lock() {
        Ok(_guard) => Some(false),
        Err(e) if e.code == "INSTANCE_BUSY" => Some(true),
        Err(_) => {
            c.incomplete("control", "LOCK_OBSERVATION_UNAVAILABLE");
            None
        }
    };
    let binding_raw = c.read("registration", &root.path().join("binding.json"), 262_144);
    let binding = binding_raw
        .as_deref()
        .and_then(|raw| json::decode::<Binding>(raw).ok())
        .filter(valid_binding);
    // Keep enough budget to revalidate the registration before an export even
    // when an oversized report exhausts the ordinary collection allowance.
    let reserved = binding_raw
        .as_ref()
        .map_or(0, |raw| raw.len() as u64)
        .min(c.remaining);
    c.remaining -= reserved;
    let checkpoint = c.journal("initialization.journal", &root.path().join("journal"));
    let state = checkpoint
        .as_ref()
        .and_then(|(_, raw)| json::decode::<State>(raw).ok());
    let matched_state = match (&binding, &binding_raw, state) {
        (Some(binding), Some(raw), Some(state)) if valid_state(binding, raw, &state) => Some(state),
        _ => {
            c.incomplete("registration.binding", "REGISTRATION_OR_JOURNAL_INVALID");
            None
        }
    };
    if let Some(state) = matched_state.as_ref() {
        c.report.initialization_record = match state.phase {
            Phase::Registered => "NOT_STARTED",
            Phase::Initialized => "INITIALIZED",
            Phase::InitIntent => "INIT_INTENT",
            Phase::Unknown => "UNKNOWN",
        };
        c.report.initialization_reason = recorded_reason(&state.reason);
        if let Some(attempt) = &state.attempt {
            c.events(
                "initialization.report",
                &root
                    .path()
                    .join("operations")
                    .join(&attempt.id)
                    .join("report.jsonl"),
                &attempt.command,
                &attempt.id,
            );
        } else {
            c.source(
                "initialization.report",
                "NOT_APPLICABLE",
                "NO_RECORDED_ATTEMPT",
                None,
            );
        }
        let service_path = root.path().join("service-journal");
        if matches!(fs::symlink_metadata(&service_path), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
        {
            c.report.service_record = "NO_RECORDED_ATTEMPT";
            c.source(
                "service.journal",
                "NOT_APPLICABLE",
                "NO_RECORDED_ATTEMPT",
                None,
            );
        } else if let Some((store, raw)) = c.journal("service.journal", &service_path) {
            match service::diagnostic_record(&root, state, &raw) {
                Ok((attempt, phase)) => {
                    c.report.service_record = phase;
                    let dir = root.path().join("operations").join(&attempt);
                    c.events("service.report", &dir.join("report.jsonl"), "run", &attempt);
                    c.raw_log("service.stdout", &dir.join("service.stdout.json"));
                    c.raw_log("service.stderr", &dir.join("service.stderr.private"));
                }
                Err(_) => c.incomplete("service.journal.binding", "SERVICE_RECORD_INVALID"),
            }
            c.recheck_journal("service.journal.recheck", &store);
        }
    }
    if let Some((store, _)) = &checkpoint {
        c.recheck_journal("initialization.journal.recheck", store);
    }
    c.remaining += reserved;
    let mut export_binding = if matched_state.is_some() {
        binding
    } else {
        None
    };
    if let Some(raw) = &binding_raw {
        let current = c.read(
            "registration.recheck",
            &root.path().join("binding.json"),
            262_144,
        );
        if current.as_ref() != Some(raw) {
            c.incomplete(
                "registration.binding.recheck",
                "REGISTRATION_CHANGED_UNAVAILABLE_OR_LIMIT",
            );
            export_binding = None;
        }
    }
    root.recheck()?;
    if Instant::now() >= c.deadline {
        c.incomplete("collection", "TIME_BUDGET_EXCEEDED");
    }
    Ok((c, root, export_binding))
}
fn recorded_reason(reason: &str) -> &'static str {
    match reason {
        "REGISTERED_COLD_CHECKED" => "REGISTERED_COLD_CHECKED",
        "INITIALIZATION_VERIFIED" => "INITIALIZATION_VERIFIED",
        "INITIALIZATION_ATTEMPT_UNRESOLVED" => "INITIALIZATION_ATTEMPT_UNRESOLVED",
        "ENGINE_INITIALIZATION_EXIT_FAILED" => "ENGINE_INITIALIZATION_EXIT_FAILED",
        "INSTANCE_INITIALIZATION_UNKNOWN" => "INSTANCE_INITIALIZATION_UNKNOWN",
        "INSTANCE_INPUT_CHANGED" => "INSTANCE_INPUT_CHANGED",
        "ENGINE_INIT_RESULT_INVALID" => "ENGINE_INIT_RESULT_INVALID",
        _ => "DETAIL_WITHHELD",
    }
}
fn valid_binding(b: &Binding) -> bool {
    b.schema_version == 1
        && b.backend == "rocksdb"
        && b.pins.len() <= 128
        && [
            &b.package,
            &b.archive,
            &b.product,
            &b.native,
            &b.lock,
            &b.data_directory,
        ]
        .into_iter()
        .chain(b.public_key.iter())
        .chain(b.pins.iter().map(|p| &p.path))
        .all(|p| p.is_absolute() && files::absolute(p).is_ok_and(|normalized| normalized == *p))
}
fn valid_state(b: &Binding, raw: &[u8], state: &State) -> bool {
    state.schema_version == 1
        && state.instance_id == b.instance_id
        && state.binding_sha256 == files::digest(raw)
        && (state.phase == Phase::Registered) == state.attempt.is_none()
        && (state.phase == Phase::Initialized) == state.genesis_hash.is_some()
        && state.attempt.as_ref().is_none_or(|a| {
            valid_attempt(&a.id) && matches!(a.command.as_str(), "init" | "resume-init")
        })
}
fn check_output(output: &Path, control: &Path, b: &Binding) -> Result<()> {
    for protected in [
        control,
        b.package.as_path(),
        b.archive.as_path(),
        b.product.as_path(),
        b.native.as_path(),
        b.lock.as_path(),
        b.data_directory.as_path(),
    ]
    .into_iter()
    .chain(b.public_key.iter().map(PathBuf::as_path))
    .chain(b.pins.iter().map(|p| p.path.as_path()))
    {
        if paths::overlaps(output, protected)? {
            return Err(error(
                "DIAGNOSTIC_OUTPUT_OVERLAP",
                "진단 출력은 인스턴스·데이터·패키지·설정·신뢰 입력과 분리한 새 경로를 사용하세요.",
            ));
        }
    }
    Ok(())
}
fn finish(report: &mut Report, options: &Options) -> Result<()> {
    if report.events.len() > options.tail {
        let count = report.events.len() - options.tail;
        report.events.drain(..count);
        report.events_omitted += count;
        report.partial = true;
        report.sources.push(Source {
            name: "events",
            result: "TRUNCATED",
            reason: "TAIL_LIMIT",
            size_bytes: None,
        });
    }
    report
        .next_actions
        .push("실시간 상태는 bxdl status --instance <dir>로 별도 확인하세요.");
    if matches!(
        report.initialization_record,
        "INIT_INTENT" | "UNKNOWN" | "UNAVAILABLE"
    ) || matches!(
        report.service_record,
        "START_PREPARED" | "GATE_REFUSED" | "UNAVAILABLE"
    ) {
        report
            .next_actions
            .push("불명 시도의 기록과 데이터를 보존하세요. 자동 초기화·재시작·삭제하지 마세요.");
    }
    if report.partial {
        report.next_actions.push("sources의 누락 이유를 확인하세요. 수집 중 변경은 재조회하고 제한 초과는 수집 범위를 조정하세요.");
    }
    let mut output_clipped = false;
    loop {
        let size = serde_json::to_vec(report).map_err(|_| internal())?.len() as u64;
        if size <= options.max_bytes {
            break;
        }
        if report.events.is_empty() {
            return Err(error(
                "DIAGNOSTIC_OUTPUT_LIMIT",
                "요청한 크기에 진단 결과를 담을 수 없습니다. --max-bytes를 늘리세요.",
            ));
        }
        if !output_clipped {
            report.sources.push(Source {
                name: "events",
                result: "TRUNCATED",
                reason: "OUTPUT_BYTE_LIMIT",
                size_bytes: None,
            });
            output_clipped = true;
        }
        report.events.remove(0);
        report.events_omitted += 1;
        report.partial = true;
    }
    Ok(())
}
fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn internal() -> BxdlError {
    error(
        "DIAGNOSTIC_SERIALIZATION_FAILED",
        "진단 결과를 구성하지 못했습니다.",
    )
}

#[cfg(test)]
#[path = "diagnostics/tests.rs"]
mod tests;
