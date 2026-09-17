use bxdl::cli;
use serde_json::Value;
use std::{
    fs,
    io::Cursor,
    path::Path,
    process::{Command, Output},
};

fn command(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bxdl"))
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}
fn machine(output: &Output, exit: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(exit),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("one JSON document")
}
fn example(root: &Path) {
    fs::write(
        root.join("input.json"),
        include_bytes!("../config/examples/instance.development.json"),
    )
    .unwrap();
}

#[test]
fn noninteractive_import_resume_export_and_conflict_have_honest_results() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    example(&root);
    let output = command(
        &[
            "setup",
            "--workspace",
            "setup with spaces",
            "--from",
            "input.json",
            "--non-interactive",
            "--json",
        ],
        &root,
    );
    let result = machine(&output, 0);
    assert_eq!(result["reasonCode"], "SETUP_DRAFT_SAVED");
    assert_eq!(result["data"]["engineValidation"], "NOT_CHECKED");
    assert_eq!(result["data"]["installation"], "NOT_PERFORMED");
    assert_eq!(result["data"]["draftComplete"], true);
    assert_eq!(result["data"]["preflight"]["outcome"], "FAIL");
    fs::remove_file(root.join("input.json")).unwrap();
    let elsewhere = root.join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    let workspace = root.join("setup with spaces");
    let destination = workspace.join("instance.json");
    let args = [
        "setup",
        "--workspace",
        workspace.to_str().unwrap(),
        "--resume",
        "--non-interactive",
        "--output",
        destination.to_str().unwrap(),
        "--json",
    ];
    assert_eq!(
        machine(&command(&args, &elsewhere), 0)["reasonCode"],
        "SETUP_CONFIG_WRITTEN"
    );
    let bytes = fs::read(&destination).unwrap();
    let config: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        config["storage"]["dataDirectory"],
        root.join("data").to_str().unwrap()
    );
    assert_eq!(
        machine(&command(&args, &elsewhere), 3)["reasonCode"],
        "OUTPUT_EXISTS"
    );
    assert_eq!(fs::read(destination).unwrap(), bytes);
    assert!(!root.join("data").exists());
    assert!(!elsewhere.join("data").exists());
}

#[test]
fn piped_interaction_and_invalid_argument_combinations_have_no_side_effects() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let piped = command(&["setup", "--workspace", "setup"], &root);
    assert_eq!(piped.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&piped.stderr).contains("SETUP_INTERACTIVE_TERMINAL_REQUIRED"));
    for tail in [
        vec!["--json"],
        vec!["--non-interactive", "--json"],
        vec![
            "--resume",
            "--from",
            "PRIVATE-CANARY",
            "--json",
            "--non-interactive",
        ],
        vec![
            "--from",
            "input.json",
            "--password",
            "PRIVATE-CANARY",
            "--json",
            "--non-interactive",
        ],
    ] {
        let mut args = vec!["setup", "--workspace", "setup"];
        args.extend(tail);
        let response = machine(&command(&args, &root), 2);
        assert_eq!(response["reasonCode"], "INVALID_ARGUMENTS");
        assert!(!response.to_string().contains("PRIVATE-CANARY"));
    }
    assert!(!root.join("setup").exists());
}

#[test]
fn interactive_cancel_resumes_and_noninteractive_incomplete_does_not_export() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let workspace = root.join("setup");
    let args: Vec<String> = vec![
        "setup".into(),
        "--workspace".into(),
        workspace.to_str().unwrap().into(),
    ];
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(
        cli::run_with_input(
            &args,
            &mut Cursor::new(b"node-a\n:cancel\n"),
            true,
            &mut out,
            &mut err
        ),
        5
    );
    assert!(out.is_empty());
    assert!(String::from_utf8_lossy(&err).contains("SETUP_PAUSED"));
    let target = root.join("must-not-exist.json");
    let response = machine(
        &command(
            &[
                "setup",
                "--workspace",
                workspace.to_str().unwrap(),
                "--resume",
                "--non-interactive",
                "--output",
                target.to_str().unwrap(),
                "--json",
            ],
            &root,
        ),
        5,
    );
    assert_eq!(response["reasonCode"], "SETUP_DRAFT_INCOMPLETE");
    assert_eq!(response["data"]["nextField"], "nodeId");
    assert!(!target.exists());
}

#[test]
fn full_interactive_form_corrects_error_and_exports_without_engine_effects() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let workspace = root.join("setup");
    let args: Vec<String> = vec![
        "setup".into(),
        "--workspace".into(),
        workspace.to_str().unwrap().into(),
    ];
    let values = [
        "node-a".to_string(),
        "public-node".into(),
        root.join("data").to_str().unwrap().into(),
        "127.0.0.1".into(),
        "99999".into(),
        "18080".into(),
        "127.0.0.1".into(),
        "19090".into(),
        root.join("chain.json").to_str().unwrap().into(),
        root.join("validator.p12").to_str().unwrap().into(),
        root.join("validator.pass").to_str().unwrap().into(),
        root.join("tls.p12").to_str().unwrap().into(),
        root.join("tls.pass").to_str().unwrap().into(),
        root.join("trust.p12").to_str().unwrap().into(),
        root.join("trust.pass").to_str().unwrap().into(),
        "export".into(),
        "".into(),
        "y".into(),
    ]
    .join("\n")
        + "\n";
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(
        cli::run_with_input(&args, &mut Cursor::new(values), true, &mut out, &mut err),
        0
    );
    assert!(String::from_utf8_lossy(&err).contains("PORTS_INVALID"));
    assert!(String::from_utf8_lossy(&out).contains("SETUP_CONFIG_WRITTEN"));
    assert!(workspace.join("instance.json").exists());
    assert!(!root.join("data").exists());
    assert!(!root.join("validator.p12").exists());
    bxdl::config::validate_file(&workspace.join("instance.json")).unwrap();
}

#[test]
fn machine_mode_never_reads_stdin_and_errors_do_not_echo_import_contents() {
    struct NoInput;
    impl std::io::Read for NoInput {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("machine mode read stdin")
        }
    }
    impl std::io::BufRead for NoInput {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            panic!("machine mode read stdin")
        }
        fn consume(&mut self, _: usize) {
            panic!("machine mode read stdin")
        }
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    fs::write(root.join("bad.json"), br#"{"password":"PRIVATE-CANARY"}"#).unwrap();
    let args: Vec<String> = vec![
        "setup".into(),
        "--workspace".into(),
        root.join("setup").to_str().unwrap().into(),
        "--from".into(),
        root.join("bad.json").to_str().unwrap().into(),
        "--non-interactive".into(),
        "--json".into(),
    ];
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(
        cli::run_with_input(&args, &mut NoInput, true, &mut out, &mut err),
        3
    );
    assert!(err.is_empty());
    assert!(!String::from_utf8_lossy(&out).contains("PRIVATE-CANARY"));
    assert!(!root.join("setup").exists());
}
