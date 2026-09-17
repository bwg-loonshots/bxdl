use bxdl::cli;
use serde_json::Value;
use std::io::{self, Write};

fn run(args: &[&str]) -> (Value, i32) {
    let args: Vec<String> = std::iter::once("--json")
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect();
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let exit = cli::run(&args, &mut stdout, &mut stderr);
    assert!(stderr.is_empty());
    let value: Value = serde_json::from_slice(&stdout).expect("exactly one JSON document");
    assert_eq!(value["schemaVersion"], 1);
    assert!(!value["observedAt"].as_str().unwrap().is_empty());
    assert!(!value["reasonCode"].as_str().unwrap().is_empty());
    (value, exit)
}
#[test]
fn version_reports_rust_mac_priority_without_service_claims() {
    let (r, code) = run(&["version"]);
    assert_eq!(code, 0);
    assert_eq!(r["data"]["implementation"], "rust");
    assert_eq!(r["data"]["primaryTarget"], "darwin-arm64");
    for (key, value) in [
        ("bundleInspection", "NOT_PERFORMED"),
        ("engineContractStatus", "NOT_DELIVERED"),
        ("macosServiceAcceptance", "NOT_CHECKED"),
        ("linuxServiceAcceptance", "NOT_CHECKED"),
    ] {
        assert_eq!(r["data"][key], value);
    }
}
#[test]
fn operations_remain_explicitly_unavailable() {
    for command in [
        "install",
        "init",
        "start",
        "stop",
        "status",
        "logs",
        "diagnose",
        "upgrade",
        "uninstall",
    ] {
        let (r, code) = run(&[command, "--instance", "node-a"]);
        assert_eq!(code, 4);
        assert_eq!(r["outcome"], "UNSUPPORTED");
        assert_eq!(r["reasonCode"], "CAPABILITY_NOT_IMPLEMENTED");
    }
}
#[test]
fn errors_do_not_echo_secret_input() {
    let secret = "CANARY-private-secret-token";
    for args in [
        vec![secret],
        vec!["version", secret],
        vec!["package", "verify", "archive", "--password", secret],
        vec!["config", "validate", "--file", "a", "--file", secret],
        vec![
            "package",
            "verify",
            "a",
            "--public-key",
            "k",
            "--allow-unsigned-development",
        ],
        vec![
            "package", "build", "--root", "x", "--spec", "s", "--output", "o",
        ],
    ] {
        let (r, code) = run(&args);
        assert_eq!(code, 2);
        assert!(!r.to_string().contains(secret));
    }
}
#[test]
fn parser_supports_trailing_flags_and_literal_paths() {
    for args in [
        vec![
            "package",
            "verify",
            "/missing-bxdl-test-archive",
            "--allow-unsigned-development",
        ],
        vec![
            "package",
            "verify",
            "--allow-unsigned-development",
            "--",
            "--missing-bxdl-test-archive",
        ],
    ] {
        let (r, code) = run(&args);
        assert_eq!(code, 3);
        assert_eq!(r["command"], "package verify");
    }
    for args in [
        vec!["package", "verify", "a", "--public-key"],
        vec!["package", "verify", "a", "--public-key="],
        vec!["package", "verify", "a", "-x"],
        vec!["package", "verify", "a", "--unknown"],
        vec![
            "package",
            "verify",
            "a",
            "--public-key",
            "k",
            "--public-key",
            "q",
        ],
    ] {
        assert_eq!(run(&args).1, 2);
    }
}
#[test]
fn human_and_machine_help() {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(cli::run(&[], &mut out, &mut err), 0);
    assert!(err.is_empty());
    assert!(
        String::from_utf8(out)
            .unwrap()
            .contains("bxdl package verify")
    );
    assert_eq!(run(&["help"]).0["command"], "help");
}
struct FailWriter;
impl Write for FailWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn output_failure_cannot_report_success() {
    assert_eq!(
        cli::run(
            &["version".into(), "--json".into()],
            &mut FailWriter,
            &mut io::sink()
        ),
        7
    );
}
#[test]
fn binary_handles_non_utf8_without_panicking_or_leaking() {
    use std::os::unix::ffi::OsStringExt;
    let secret = std::ffi::OsString::from_vec(b"PRIVATE-CANARY-\xff".to_vec());
    let r = std::process::Command::new(env!("CARGO_BIN_EXE_bxdl"))
        .arg(secret)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2));
    assert!(r.stderr.is_empty());
    let value: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(value["reasonCode"], "INVALID_ARGUMENTS");
    assert!(!String::from_utf8_lossy(&r.stdout).contains("PRIVATE-CANARY"));
}
