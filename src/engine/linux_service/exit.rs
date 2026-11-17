//! Durable systemd exit witness. A successful hook records OS termination only;
//! the controller still requires engine STOPPED, empty cgroup and released DB
//! locks before declaring a clean stop. This survives systemd unit GC.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    attempt: String,
    invocation_id: String,
    engine_pid: u32,
    engine_start_ticks: u64,
    control_group: String,
    service_result: String,
    exit_code: String,
    exit_status: String,
}
fn from_environment(r: &Run) -> Result<Receipt> {
    let get = |key| std::env::var(key).map_err(|_| unknown());
    let receipt = Receipt {
        schema_version: 1,
        attempt: r.attempt.clone(),
        invocation_id: get("INVOCATION_ID")?,
        engine_pid: r.pid.ok_or_else(unknown)?,
        engine_start_ticks: r.start_ticks.ok_or_else(unknown)?,
        control_group: r.control_group.clone().ok_or_else(unknown)?,
        service_result: get("SERVICE_RESULT")?,
        exit_code: get("EXIT_CODE")?,
        exit_status: get("EXIT_STATUS")?,
    };
    matches_attempt(&receipt, r)?;
    // Retain a bounded fixed vocabulary; never persist arbitrary environment
    // strings in a privileged receipt/public diagnostic.
    if !matches!(
        receipt.service_result.as_str(),
        "success"
            | "protocol"
            | "timeout"
            | "exit-code"
            | "signal"
            | "core-dump"
            | "watchdog"
            | "start-limit-hit"
            | "resources"
            | "oom-kill"
            | "exec-condition"
    ) || !matches!(receipt.exit_code.as_str(), "exited" | "killed" | "dumped")
        || receipt.exit_status.is_empty()
        || receipt.exit_status.len() > 16
        || !receipt
            .exit_status
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err(unknown());
    }
    Ok(receipt)
}
fn matches_attempt(receipt: &Receipt, r: &Run) -> Result<()> {
    if receipt.schema_version != 1
        || receipt.attempt != r.attempt
        || Some(receipt.invocation_id.as_str()) != r.invocation_id.as_deref()
        || Some(receipt.engine_pid) != r.pid
        || Some(receipt.engine_start_ticks) != r.start_ticks
        || Some(receipt.control_group.as_str()) != r.control_group.as_deref()
        || !matches!(
            r.phase,
            RunPhase::Consumed | RunPhase::Refused | RunPhase::StoppedVerified
        )
    {
        return Err(unknown());
    }
    Ok(())
}
fn successful(receipt: &Receipt) -> bool {
    receipt.service_result == "success"
        && matches!(
            (receipt.exit_code.as_str(), receipt.exit_status.as_str()),
            ("exited", "0" | "143") | ("killed", "TERM" | "15")
        )
}
fn path(b: &Binding, r: &Run) -> PathBuf {
    b.control
        .join("attempts")
        .join(&r.attempt)
        .join("systemd-exit.json")
}

/// Only the current root ExecStopPost ControlPID in the exact loaded unit may
/// create this receipt. Spoofed environment variables/direct CLI invocation do
/// not confer authority. A previous unit invocation cannot overwrite history.
pub fn record_exit(control: &Path, attempt: &str) -> Result<()> {
    boundary::root()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let guard = loop {
        match boundary::operation_lock(control) {
            Ok(guard) => break guard,
            Err(error) if error.code == "LINUX_SERVICE_BUSY" && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(error) => return Err(error),
        }
    };
    let (b, _, state) = load(control)?;
    let r = state.run.as_ref().ok_or_else(unknown)?;
    if r.attempt != attempt {
        return Err(error("LINUX_SERVICE_ATTEMPT_CHANGED"));
    }
    let receipt = from_environment(r)?;
    let program = control.join("attempts").join(attempt).join("bxdl-worker");
    if std::env::current_exe().map_err(|_| unknown())? != program
        || boundary::root_read(&program, MAX_WORKER)?
            != boundary::root_read(&control.join("worker"), MAX_WORKER)?
    {
        return Err(unknown());
    }
    let pid = std::process::id();
    let cgroup = boundary::cgroup(pid)?;
    if cgroup != receipt.control_group {
        return Err(unknown());
    }
    unit(&b, r)?.inspect_exit_hook(&receipt.invocation_id, pid, Duration::from_secs(5))?;
    let raw = encoded(&receipt)?;
    let destination = path(&b, r);
    if fs::symlink_metadata(&destination).is_ok() {
        if boundary::root_read(&destination, 4096)? != raw {
            return Err(unknown());
        }
    } else {
        boundary::write_new(&destination, &raw, 0, 0o600)?;
    }
    drop(guard);
    Ok(())
}

pub(super) fn verify(b: &Binding, r: &Run) -> Result<()> {
    let receipt: Receipt =
        json::decode(&boundary::root_read(&path(b, r), 4096)?).map_err(|_| unknown())?;
    matches_attempt(&receipt, r)?;
    if !successful(&receipt) {
        return Err(unknown());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Run, Receipt) {
        let run = Run {
            attempt: "lnx-test".into(),
            phase: RunPhase::Consumed,
            pid: Some(415),
            invocation_id: Some("1".repeat(32)),
            control_group: Some("/system.slice/bxdl-test.service".into()),
            start_ticks: Some(90),
            reason: String::new(),
        };
        let receipt = Receipt {
            schema_version: 1,
            attempt: run.attempt.clone(),
            invocation_id: run.invocation_id.clone().unwrap(),
            engine_pid: 415,
            engine_start_ticks: 90,
            control_group: run.control_group.clone().unwrap(),
            service_result: "success".into(),
            exit_code: "exited".into(),
            exit_status: "143".into(),
        };
        (run, receipt)
    }
    #[test]
    fn receipt_binds_every_attempt_process_and_manager_identity() {
        let (run, receipt) = fixture();
        assert!(matches_attempt(&receipt, &run).is_ok());
        for changed in ["attempt", "invocation", "pid", "ticks", "cgroup"] {
            let mut bad = receipt.clone();
            match changed {
                "attempt" => bad.attempt = "lnx-other".into(),
                "invocation" => bad.invocation_id = "2".repeat(32),
                "pid" => bad.engine_pid += 1,
                "ticks" => bad.engine_start_ticks += 1,
                _ => bad.control_group.push('x'),
            }
            assert!(matches_attempt(&bad, &run).is_err());
        }
        let mut pending = run;
        pending.phase = RunPhase::Prepared;
        assert!(matches_attempt(&receipt, &pending).is_err());
    }
    #[test]
    fn success_never_excuses_kill_oom_or_timeout() {
        let (_, mut receipt) = fixture();
        assert!(successful(&receipt));
        receipt.exit_code = "killed".into();
        receipt.exit_status = "TERM".into();
        assert!(successful(&receipt));
        receipt.exit_status = "KILL".into();
        assert!(!successful(&receipt));
        receipt.exit_code = "exited".into();
        receipt.exit_status = "0".into();
        for reason in ["timeout", "oom-kill", "signal", "exit-code", "watchdog"] {
            receipt.service_result = reason.into();
            assert!(!successful(&receipt));
        }
    }
    #[test]
    fn arbitrary_receipt_fields_are_rejected() {
        assert!(json::decode::<Receipt>(br#"{"shell":"/bin/sh"}"#).is_err());
    }
}
