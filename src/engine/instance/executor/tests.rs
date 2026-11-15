use super::*;
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, time::Instant};

fn summary() -> Value {
    json!({"instanceId":"node-a","initialization":"INITIALIZED","operationBusy":false,"developmentOnly":true,"serviceRegistration":"NOT_REGISTERED","runtimeReadiness":"NOT_CHECKED","reason":"INITIALIZATION_CONFIRMED","attemptId":"attempt-a","genesisHash":format!("0x{}", "a".repeat(64))})
}
fn envelope(command: &str, outcome: &str, reason: &str, data: Option<Value>) -> Vec<u8> {
    let mut v = json!({"schemaVersion":1,"command":command,"outcome":outcome,"reasonCode":reason,"message":"공개 결과입니다.","observedAt":"2026-10-23T00:00:00Z"});
    if let Some(data) = data {
        v["data"] = data;
    }
    serde_json::to_vec(&v).unwrap()
}
fn service_data() -> Value {
    json!({"instanceId":"node-a","outcome":"INCOMPLETE","serviceState":"STARTING","engineState":"STARTING","runtimeReadiness":"NOT_READY","reason":"SERVICE_START_WAIT_EXPIRED","operationBusy":true,"developmentOnly":true,"loginAutoStart":false,"automaticRestart":false,"serviceProfile":"MACOS_USER_LAUNCH_AGENT","globalConsensus":"NOT_CHECKED","attemptId":"attempt-a","pid":123})
}
fn start() -> Request {
    Request::Service {
        action: ServiceAction::Start,
        wait: false,
        timeout: Duration::from_secs(1),
    }
}
fn parse(request: &Request, exit: i32, bytes: &[u8]) -> Result<Response> {
    reply::parse(&profiles::RC3, request, "node-a", exit, bytes, b"")
}

fn profile_report(profile: &profiles::Profile) -> artifact::Report {
    let mut manifest: artifact::Manifest =
        serde_json::from_str(if profile.archive == profiles::RC3.archive {
            include_str!("../../../../packaging/package-spec.rc3-temurin.example.json")
        } else {
            include_str!("../../../../packaging/package-spec.rc4-temurin.example.json")
        })
        .unwrap();
    manifest.product.revision = profile.product_revision.into();
    // This fixture exercises profile selection after verification, not the
    // archive parser. Full archive/inventory authentication is tested separately.
    manifest.files = vec![
        artifact::FileEntry {
            path: "bin/bxdl".into(),
            size: profile.cli_size,
            sha256: profile.cli.into(),
            mode: 0o755,
        },
        artifact::FileEntry {
            path: "engine/nigo-node.jar".into(),
            size: profile.jar_size,
            sha256: profile.jar.into(),
            mode: 0o644,
        },
    ];
    artifact::Report {
        manifest,
        archive_sha256: profile.archive.into(),
        manifest_sha256: profile.manifest.into(),
        authenticity: "verified-external-ed25519".into(),
        files_verified: profile.files,
        bytes_verified: profile.bytes,
    }
}

#[test]
fn profiles_select_only_exact_authenticated_runtime_combinations() {
    for profile in [&profiles::RC3, &profiles::RC4] {
        let report = profile_report(profile);
        assert!(supports_package(&report));
        assert!(std::ptr::eq(
            profiles::for_archive(&report.archive_sha256).unwrap(),
            profile
        ));
        for change in 0..17 {
            let mut altered = report.clone();
            match change {
                0 => altered.archive_sha256 = "a".repeat(64),
                1 => altered.manifest_sha256 = "a".repeat(64),
                2 => altered.manifest.files[0].size += 1,
                3 => altered.manifest.files[0].sha256 = "a".repeat(64),
                4 => altered.authenticity = "unsigned-development".into(),
                5 => altered
                    .manifest
                    .files
                    .push(altered.manifest.files[0].clone()),
                6 => altered.manifest.files[0].mode = 0o644,
                7 => altered.manifest.engine.jar_sha256 = "a".repeat(64),
                8 => altered.manifest.runtime.java_sha256 = "a".repeat(64),
                9 => altered.manifest.engine.revision = "a".repeat(40),
                10 => altered.manifest.product.revision = "a".repeat(40),
                11 => altered.manifest.engine.contract_revision = "a".repeat(64),
                12 => altered.manifest.files[1].sha256 = "a".repeat(64),
                13 => altered.manifest.files[1].size += 1,
                14 => altered.files_verified += 1,
                15 => altered.bytes_verified += 1,
                _ => altered
                    .manifest
                    .files
                    .push(altered.manifest.files[1].clone()),
            }
            assert!(
                !supports_package(&altered),
                "{} change {change}",
                profile.archive
            );
        }
    }
    // A known archive cannot borrow another profile's manifest/executor/JAR.
    for (profile, other) in [
        (&profiles::RC3, &profiles::RC4),
        (&profiles::RC4, &profiles::RC3),
    ] {
        let mut mixed = profile_report(other);
        mixed.archive_sha256 = profile.archive.into();
        assert!(!supports_package(&mixed));
        assert!(!profile.matches_package(&profile_report(other)));
    }
    for unknown in ["", "a", &"0".repeat(64), "nigo-engine-v0.1.0-rc.4"] {
        assert!(profiles::for_archive(unknown).is_none());
    }
}

#[test]
fn rc4_profile_matches_frozen_assembly_evidence_and_rc3_remains_unchanged() {
    let evidence: Value = serde_json::from_str(include_str!(
        "../../../../results/2026-11-15-rc4-runtime-assembly.json"
    ))
    .unwrap();
    let package = &evidence["package"];
    let profile = &profiles::RC4;
    assert_eq!(package["archiveSha256"], profile.archive);
    assert_eq!(package["manifestSha256"], profile.manifest);
    assert_eq!(package["executorSha256"], profile.cli);
    assert_eq!(package["executorSizeBytes"], profile.cli_size);
    assert_eq!(package["engineJarSha256"], profile.jar);
    assert_eq!(package["engineJarSizeBytes"], profile.jar_size);
    assert_eq!(package["javaSha256"], profile.java);
    assert_eq!(package["filesVerified"], profile.files);
    assert_eq!(package["bytesVerified"], profile.bytes);
    assert_eq!(
        evidence["selection"]["productRevision"],
        profile.product_revision
    );
    let previous: Value = serde_json::from_str(include_str!(
        "../../../../results/2026-10-22-macos-rc3-package.json"
    ))
    .unwrap();
    let package = &previous["package"];
    assert_eq!(package["archiveSha256"], profiles::RC3.archive);
    assert_eq!(package["manifestSha256"], profiles::RC3.manifest);
    assert_eq!(package["cliSha256"], profiles::RC3.cli);
    assert_eq!(previous["product"]["cliSizeBytes"], profiles::RC3.cli_size);
    assert_eq!(package["engineJarSha256"], profiles::RC3.jar);
    assert_eq!(package["engineJarSizeBytes"], profiles::RC3.jar_size);
    assert_eq!(package["javaSha256"], profiles::RC3.java);
    assert_eq!(package["filesVerified"], profiles::RC3.files);
    assert_eq!(package["bytesVerified"], profiles::RC3.bytes);
    assert_eq!(package["revision"], profiles::RC3.product_revision);
    assert_eq!(PROFILE, "BXDL_RC3_EXECUTOR_V1");
    assert_eq!(RC4_PROFILE, "BXDL_RC4_EXECUTOR_V1");
}

#[test]
fn arguments_are_fixed_and_paths_are_single_literal_arguments() {
    let request = Request::Diagnose {
        output: PathBuf::from("/private/tmp/a ; $(touch bad).json"),
        tail: 20,
        max_bytes: 8192,
        timeout: Duration::from_secs(3),
    };
    let args = request
        .arguments(Path::new("/private/tmp/control ; $HOME"))
        .unwrap();
    assert_eq!(args[0], "diagnose");
    assert_eq!(args[2], "/private/tmp/control ; $HOME");
    assert_eq!(args.last().unwrap(), "/private/tmp/a ; $(touch bad).json");
    assert_eq!(args.iter().filter(|x| *x == "--json").count(), 1);
    assert!(
        Request::Service {
            action: ServiceAction::Stop,
            wait: true,
            timeout: Duration::from_secs(1)
        }
        .validate()
        .is_err()
    );
    assert!(
        Request::Logs {
            tail: 201,
            max_bytes: 8192,
            timeout: Duration::from_secs(1)
        }
        .validate()
        .is_err()
    );
    assert!(
        Request::Preflight {
            timeout: Duration::ZERO
        }
        .validate()
        .is_err()
    );
}
#[test]
fn initialization_arguments_preserve_exact_confirmation_and_literal_control_path() {
    for (resume, command, confirmation) in [
        (false, "init", "--confirm-initialize"),
        (true, "resume-init", "--confirm-resume"),
    ] {
        let request = Request::Initialize {
            resume,
            timeout: Duration::from_secs(137),
        };
        let args = request
            .arguments(Path::new("/private/tmp/control ; $HOME"))
            .unwrap();
        let expected: Vec<OsString> = [
            command,
            "--instance",
            "/private/tmp/control ; $HOME",
            "--json",
            "--timeout-seconds",
            "137",
            confirmation,
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        assert_eq!(args, expected);
    }
}
#[test]
fn accepted_reply_preserves_incomplete_unknown_and_public_data() {
    let raw = envelope(
        "start",
        "INCOMPLETE",
        "SERVICE_START_WAIT_EXPIRED",
        Some(service_data()),
    );
    let response = parse(&start(), 5, &raw).unwrap();
    assert_eq!(response.exit, 5);
    assert_eq!(response.outcome, "INCOMPLETE");
    let data = response.into_service().unwrap();
    assert_eq!(data.attempt_id.as_deref(), Some("attempt-a"));
    assert_eq!(data.global_consensus, "NOT_CHECKED");
    let raw = envelope(
        "init",
        "UNKNOWN",
        "INSTANCE_INITIALIZATION_UNKNOWN",
        Some({
            let mut x = summary();
            x["initialization"] = "UNKNOWN".into();
            x
        }),
    );
    let request = Request::Initialize {
        resume: false,
        timeout: Duration::from_secs(1),
    };
    let response = parse(&request, 6, &raw).unwrap();
    assert_eq!(response.exit, 6);
    assert_eq!(
        response.into_initialization().err().unwrap().code,
        "INSTANCE_INITIALIZATION_UNKNOWN"
    );
    let failure = envelope("start", "FAILED", "INSTANCE_BUSY", None);
    let response = parse(&start(), 3, &failure).unwrap();
    assert_eq!(response.into_service().err().unwrap().code, "INSTANCE_BUSY");
}
#[test]
fn reply_rejects_forged_identity_missing_fields_and_unknown_private_members() {
    let valid = envelope(
        "start",
        "INCOMPLETE",
        "SERVICE_START_WAIT_EXPIRED",
        Some(service_data()),
    );
    for change in 0..6 {
        let mut value: Value = serde_json::from_slice(&valid).unwrap();
        match change {
            0 => value["command"] = "stop".into(),
            1 => value["data"]["instanceId"] = "different-node".into(),
            2 => {
                value["data"]
                    .as_object_mut()
                    .unwrap()
                    .remove("operationBusy");
            }
            3 => value["data"]["secretPath"] = "PRIVATE-EXECUTOR-CANARY".into(),
            4 => value["extra"] = "PRIVATE-EXECUTOR-CANARY".into(),
            _ => value["data"]["reason"] = "DIFFERENT".into(),
        }
        let error = parse(&start(), 5, &serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert_eq!(error.code, "EXECUTOR_RESPONSE_INVALID");
        assert!(!error.message.contains("CANARY"));
    }
    assert!(parse(&start(), 0, &valid).is_err());
    assert!(
        parse(
            &start(),
            5,
            &envelope("start", "INCOMPLETE", "SOME_REASON", None)
        )
        .is_err()
    );
}
#[test]
fn reply_rejects_multiple_documents_duplicate_keys_stderr_and_deep_payloads() {
    let raw = envelope(
        "instance show",
        "SUCCEEDED",
        "INSTANCE_STATE_READ",
        Some(summary()),
    );
    assert!(parse(&Request::Show, 0, &[raw.clone(), raw.clone()].concat()).is_err());
    let text = String::from_utf8(raw.clone()).unwrap().replacen(
        "\"schemaVersion\":1",
        "\"schemaVersion\":1,\"schemaVersion\":1",
        1,
    );
    assert!(parse(&Request::Show, 0, text.as_bytes()).is_err());
    assert!(
        reply::parse(
            &profiles::RC3,
            &Request::Show,
            "node-a",
            0,
            &raw,
            b"PRIVATE-EXECUTOR-CANARY"
        )
        .is_err()
    );
    let mut value: Value = serde_json::from_slice(&raw).unwrap();
    value["data"]["x"] = json!([[1]]);
    assert!(parse(&Request::Show, 0, &serde_json::to_vec(&value).unwrap()).is_err());
}
#[test]
fn preflight_accepts_the_real_cold_exit_three_contract() {
    let identity: Value = serde_json::from_str(include_str!(
        "../../../../contracts/nigo/engine-rc3-2026-10-22/consumer-evidence/engine-info.json"
    ))
    .unwrap();
    let cold = json!({"command":"preflight","status":"INCOMPLETE","reason":"RUNTIME_CHECKS_REQUIRED","contractStatus":"PROPOSED","backend":"rocksdb","chainFingerprint":"a".repeat(64),"nodeIdentity":format!("0x{}:0x{}","a".repeat(64),"b".repeat(40)),"checks":[]});
    let data = json!({"outcome":"INCOMPLETE","product":{"schemaVersion":1,"instanceId":"node-a","outcome":"INCOMPLETE","checks":[],"configSha256":"a".repeat(64)},"configurationBinding":"MATCHED","engine":{"outcome":"INCOMPLETE","engineExitCode":3,"developmentOnly":true,"identity":identity,"jarSha256":profiles::RC3.jar,"javaSha256":profiles::RC3.java,"configSha256":"a".repeat(64),"chainFileSha256":"a".repeat(64),"preflight":cold}});
    let request = Request::Preflight {
        timeout: Duration::from_secs(1),
    };
    assert!(
        parse(
            &request,
            5,
            &envelope(
                "preflight",
                "INCOMPLETE",
                "ENGINE_RUNTIME_CHECKS_REQUIRED",
                Some(data.clone())
            )
        )
        .is_ok()
    );
    let mut missing = data;
    missing.as_object_mut().unwrap().remove("engine");
    assert!(
        parse(
            &request,
            5,
            &envelope(
                "preflight",
                "INCOMPLETE",
                "ENGINE_RUNTIME_CHECKS_REQUIRED",
                Some(missing)
            )
        )
        .is_err()
    );
}
#[test]
fn dispatch_uncertainty_never_becomes_safe_to_retry_mutation() {
    for request in [
        start(),
        Request::Initialize {
            resume: false,
            timeout: Duration::from_secs(1),
        },
        Request::Diagnose {
            output: "/private/tmp/report.json".into(),
            tail: 1,
            max_bytes: 4096,
            timeout: Duration::from_secs(1),
        },
    ] {
        for code in [
            "EXECUTOR_TIMEOUT",
            "EXECUTOR_CAPTURE_FAILED",
            "EXECUTOR_RESPONSE_INVALID",
            "ENGINE_INPUT_CHANGED",
        ] {
            assert_eq!(
                dispatched_error(&request, BxdlError::new(code, "private")).code,
                "EXECUTOR_OPERATION_UNKNOWN"
            );
        }
        assert_eq!(
            dispatched_error(&request, BxdlError::new("EXECUTOR_SPAWN_FAILED", "public")).code,
            "EXECUTOR_SPAWN_FAILED"
        );
    }
    assert_eq!(
        dispatched_error(&Request::Show, response_invalid()).code,
        "EXECUTOR_RESPONSE_INVALID"
    );
}
fn fixture(script: &str) -> (tempfile::TempDir, files::Input, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().canonicalize().unwrap();
    let executable = cwd.join("executor");
    fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let cli = files::Input::read(&executable, 65536, true).unwrap();
    (temp, cli, cwd)
}
#[test]
fn child_has_no_stdin_and_environment_is_not_forwarded() {
    let (_temp, cli, cwd) =
        fixture("if read answer; then exit 9; fi\nprintf '%s' \"${BXDL_PRIVATE_CANARY-unset}:$1\"");
    let output = process::run(
        &cli,
        &cwd,
        &["literal ; $(none)".into()],
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(output.exit, 0);
    assert_eq!(output.stdout, b"unset:literal ; $(none)");
    assert!(output.stderr.is_empty());
}
#[test]
fn changed_binary_refuses_before_spawn() {
    let (_temp, cli, cwd) = fixture("/usr/bin/touch spawned");
    fs::write(&cli.path, b"#!/bin/sh\n/usr/bin/touch changed\n").unwrap();
    assert!(process::run(&cli, &cwd, &[], Duration::from_secs(1)).is_err());
    assert!(!cwd.join("spawned").exists());
    assert!(!cwd.join("changed").exists());
}
#[test]
fn output_limit_is_bounded_and_no_raw_output_is_returned() {
    let (_temp, cli, cwd) = fixture("/usr/bin/head -c 2200000 /dev/zero");
    let error = process::run(&cli, &cwd, &[], Duration::from_secs(2))
        .err()
        .unwrap();
    assert!(error.code.starts_with("EXECUTOR_"));
    assert!(error.message.len() < 300);
}
#[test]
fn timeout_does_not_kill_or_replay_the_child_and_reaps_after_completion() {
    let (_temp, cli, cwd) = fixture("/bin/sleep 1.0\nprintf complete > completed");
    let begin = Instant::now();
    let error = process::run(&cli, &cwd, &[], Duration::from_millis(30))
        .err()
        .unwrap();
    assert_eq!(error.code, "EXECUTOR_TIMEOUT");
    assert!(begin.elapsed() < Duration::from_millis(800));
    while fs::read(cwd.join("completed")).ok().as_deref() != Some(b"complete")
        && begin.elapsed() < Duration::from_secs(3)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(fs::read(cwd.join("completed")).unwrap(), b"complete");
}
#[test]
fn descendant_retaining_output_cannot_hold_manager_indefinitely() {
    let (_temp, cli, cwd) = fixture("/bin/sleep 1.5 &\nprintf done\nexit 0");
    let begin = Instant::now();
    let error = process::run(&cli, &cwd, &[], Duration::from_secs(2))
        .err()
        .unwrap();
    assert_eq!(error.code, "EXECUTOR_CAPTURE_FAILED");
    assert!(begin.elapsed() < Duration::from_millis(1200));
}

#[test]
fn show_preserves_unobserved_service_and_workflow_cannot_relabel_failure_as_success() {
    let mut data = summary();
    data["serviceRegistration"] = "NOT_OBSERVED".into();
    let response = parse(
        &Request::Show,
        0,
        &envelope(
            "instance show",
            "SUCCEEDED",
            "INSTANCE_STATE_READ",
            Some(data),
        ),
    )
    .unwrap();
    assert_eq!(
        response.into_initialization().unwrap().service_registration,
        "NOT_OBSERVED"
    );
    let request = Request::Initialize {
        resume: false,
        timeout: Duration::from_secs(1),
    };
    let response = parse(
        &request,
        3,
        &envelope(
            "init",
            "FAILED",
            "INSTANCE_INITIALIZATION_STATE",
            Some(summary()),
        ),
    )
    .unwrap();
    assert_eq!(
        response.into_initialization().err().unwrap().code,
        "INSTANCE_INITIALIZATION_STATE"
    );
}

#[test]
fn initialization_uses_engine_prefixed_genesis_contract() {
    let request = Request::Initialize {
        resume: false,
        timeout: Duration::from_secs(1),
    };
    let expected = format!("0x{}", "a".repeat(64));
    let response = parse(
        &request,
        0,
        &envelope("init", "SUCCEEDED", "INSTANCE_INITIALIZED", Some(summary())),
    )
    .unwrap();
    assert_eq!(
        response
            .into_initialization()
            .unwrap()
            .genesis_hash
            .as_deref(),
        Some(expected.as_str())
    );
    for invalid in [
        "a".repeat(64),
        format!("0X{}", "a".repeat(64)),
        format!("0x{}", "a".repeat(63)),
        format!("0x{}", "G".repeat(64)),
    ] {
        let mut data = summary();
        data["genesisHash"] = invalid.into();
        let raw = envelope("init", "SUCCEEDED", "INSTANCE_INITIALIZED", Some(data));
        assert!(parse(&request, 0, &raw).is_err());
    }
    let mut registered = summary();
    registered["initialization"] = "NOT_STARTED".into();
    registered.as_object_mut().unwrap().remove("genesisHash");
    registered.as_object_mut().unwrap().remove("attemptId");
    assert!(
        parse(
            &Request::Show,
            0,
            &envelope(
                "instance show",
                "SUCCEEDED",
                "INSTANCE_STATE_READ",
                Some(registered)
            )
        )
        .is_ok()
    );
}

#[test]
fn accepted_rc3_manual_and_auto_ready_health_keep_nullable_fields() {
    for (mode, reason, running, execution) in [
        (
            "MANUAL_START",
            "MANUAL_START_INITIALIZED",
            false,
            Value::Null,
        ),
        (
            "AUTO_START",
            "VALIDATOR_RUNNING",
            true,
            json!("WAITING_TIMER"),
        ),
    ] {
        let mut data = service_data();
        data["outcome"] = "SUCCEEDED".into();
        data["reason"] = reason.into();
        data["runtimeReadiness"] = "READY".into();
        data["serviceState"] = "RUNNING".into();
        data["engineState"] = "RUNNING".into();
        data["startupStage"] = "ENGINE_EXEC".into();
        data["nodeInstanceId"] = "node-instance-a".into();
        data["health"] = json!({"executionStatus":execution,"lifecycleRunning":running,"observedAtEpochMillis":"1792641176183","readiness":{"reason":reason,"role":"VALIDATOR","startupCompleted":true,"startupMode":mode,"status":"READY","syncStatus":"UNKNOWN"},"running":running});
        for action in [ServiceAction::Start, ServiceAction::Status] {
            let request = Request::Service {
                action,
                wait: false,
                timeout: Duration::from_secs(30),
            };
            let raw = envelope(request.command(), "SUCCEEDED", reason, Some(data.clone()));
            let result = parse(&request, 0, &raw).unwrap().into_service().unwrap();
            assert_eq!(result.health, Some(data["health"].clone()));
            assert_eq!(result.runtime_readiness, "READY");
        }
    }
}

#[test]
fn nullable_response_fields_do_not_allow_null_required_fields_or_duplicate_keys() {
    let mut data = service_data();
    data["health"] = json!({"observedAtEpochMillis":"1792641176183","running":false,"lifecycleRunning":false,"readiness":null,"executionStatus":null});
    let raw = envelope(
        "start",
        "INCOMPLETE",
        "SERVICE_START_WAIT_EXPIRED",
        Some(data.clone()),
    );
    assert!(parse(&start(), 5, &raw).is_ok());
    for key in ["outcome", "operationBusy", "instanceId"] {
        let mut altered = data.clone();
        altered[key] = Value::Null;
        assert!(
            parse(
                &start(),
                5,
                &envelope(
                    "start",
                    "INCOMPLETE",
                    "SERVICE_START_WAIT_EXPIRED",
                    Some(altered)
                )
            )
            .is_err()
        );
    }
    let duplicate = String::from_utf8(raw).unwrap().replace(
        "\"executionStatus\":null",
        "\"executionStatus\":null,\"executionStatus\":null",
    );
    assert!(parse(&start(), 5, duplicate.as_bytes()).is_err());
    data["health"]["privateUnknownField"] = Value::Null;
    assert!(
        parse(
            &start(),
            5,
            &envelope(
                "start",
                "INCOMPLETE",
                "SERVICE_START_WAIT_EXPIRED",
                Some(data)
            )
        )
        .is_err()
    );
}

#[test]
fn diagnostic_nullable_options_remain_typed_and_sanitized() {
    let data = json!({"schemaVersion":1,"partial":false,"mode":"OFFLINE_RECORDED_EVIDENCE","scope":"LATEST_INITIALIZATION_AND_SERVICE_ATTEMPTS","initializationRecord":"INITIALIZED","initializationReason":"INITIALIZATION_VERIFIED","serviceRecord":"GATE_CONSUMED","startupStage":null,"startupFailureReason":null,"runtimeReadiness":"NOT_OBSERVED","globalConsensus":"NOT_CHECKED","operationBusy":null,"bytesRead":0,"eventsOmitted":0,"events":[],"sources":[{"name":"service.stderr","result":"WITHHELD","reason":"RAW_TEXT_EXCLUDED_BY_POLICY","sizeBytes":null}],"nextActions":[],"rawTextPolicy":"ALLOWLIST_EVENTS_ONLY_RAW_TEXT_AND_IDENTIFIERS_WITHHELD","outputCreated":false});
    let request = Request::Logs {
        tail: 50,
        max_bytes: 262144,
        timeout: Duration::from_secs(5),
    };
    let result = parse(
        &request,
        0,
        &envelope("logs", "SUCCEEDED", "LOGS_COLLECTED", Some(data.clone())),
    )
    .unwrap();
    assert_eq!(result.data, Some(data));
}

fn preflight_data(profile: &profiles::Profile) -> Value {
    let identity: Value = serde_json::from_str(profile.expected_json).unwrap();
    let cold = json!({"command":"preflight","status":"INCOMPLETE","reason":"RUNTIME_CHECKS_REQUIRED","contractStatus":"PROPOSED","backend":"rocksdb","chainFingerprint":"a".repeat(64),"nodeIdentity":format!("0x{}:0x{}","a".repeat(64),"b".repeat(40)),"checks":[]});
    json!({"outcome":"INCOMPLETE","product":{"schemaVersion":1,"instanceId":"node-a","outcome":"INCOMPLETE","checks":[],"configSha256":"a".repeat(64)},"configurationBinding":"MATCHED","engine":{"outcome":"INCOMPLETE","engineExitCode":3,"developmentOnly":true,"identity":identity,"jarSha256":profile.jar,"javaSha256":profile.java,"configSha256":"a".repeat(64),"chainFileSha256":"a".repeat(64),"preflight":cold}})
}

fn parse_preflight(profile: &profiles::Profile, data: Value) -> Result<Response> {
    reply::parse(
        profile,
        &Request::Preflight {
            timeout: Duration::from_secs(1),
        },
        "node-a",
        5,
        &envelope(
            "preflight",
            "INCOMPLETE",
            "ENGINE_RUNTIME_CHECKS_REQUIRED",
            Some(data),
        ),
        b"",
    )
}

#[test]
fn preflight_binds_the_complete_engine_identity_to_the_selected_profile() {
    for (profile, other) in [
        (&profiles::RC3, &profiles::RC4),
        (&profiles::RC4, &profiles::RC3),
    ] {
        let data = preflight_data(profile);
        assert!(parse_preflight(profile, data.clone()).is_ok());
        assert!(parse_preflight(other, data.clone()).is_err());
        // Even retaining the correct JAR/source does not admit a modified
        // Console, contract, dirty source, Java requirement, or release channel.
        for path in [
            "/engine/identity/buildInfoStatus",
            "/engine/identity/engine",
            "/engine/identity/version",
            "/engine/identity/source/commit",
            "/engine/identity/source/dirty",
            "/engine/identity/java/requiredMajor",
            "/engine/identity/console/node",
            "/engine/identity/console/npm",
            "/engine/identity/console/sourceFingerprint",
            "/engine/identity/contract/status",
            "/engine/identity/contract/revision",
            "/engine/identity/contract/documentationSha256",
            "/engine/identity/contract/definitionSha256",
            "/engine/identity/contract/runtimeDocumentationSha256",
            "/engine/identity/contract/runtimeFixtureSha256",
            "/engine/identity/contract/fingerprint",
            "/engine/identity/distribution/channel",
            "/engine/identity/distribution/officialRelease",
            "/engine/jarSha256",
            "/engine/javaSha256",
        ] {
            let mut altered = data.clone();
            let item = altered.pointer_mut(path).unwrap();
            *item = match item {
                Value::Bool(value) => json!(!*value),
                Value::Number(_) => json!(22),
                _ => json!("altered"),
            };
            let error = parse_preflight(profile, altered).unwrap_err();
            assert_eq!(error.code, "EXECUTOR_RESPONSE_INVALID", "{path}");
        }
        let mut mixed = data.clone();
        mixed["engine"]["identity"] = preflight_data(other)["engine"]["identity"].clone();
        assert!(parse_preflight(profile, mixed).is_err());
        let mut unknown = data;
        unknown["engine"]["identity"]["console"]["privateUnknown"] = json!("PRIVATE-CANARY");
        let error = parse_preflight(profile, unknown).unwrap_err();
        assert!(!error.message.contains("CANARY"));
    }
}

#[test]
fn both_profiles_preserve_unknown_and_reject_mutating_reply_ambiguity() {
    for profile in [&profiles::RC3, &profiles::RC4] {
        for request in [
            start(),
            Request::Service {
                action: ServiceAction::Stop,
                wait: false,
                timeout: Duration::from_secs(1),
            },
            Request::Initialize {
                resume: false,
                timeout: Duration::from_secs(1),
            },
            Request::Initialize {
                resume: true,
                timeout: Duration::from_secs(1),
            },
            Request::Diagnose {
                output: "/private/tmp/diagnostic.json".into(),
                tail: 1,
                max_bytes: 4096,
                timeout: Duration::from_secs(1),
            },
        ] {
            let unknown = envelope(
                request.command(),
                "UNKNOWN",
                "EXECUTOR_OPERATION_UNKNOWN",
                None,
            );
            let response = reply::parse(profile, &request, "node-a", 6, &unknown, b"").unwrap();
            assert_eq!(response.exit, 6);
            assert_eq!(response.outcome, "UNKNOWN");
            for raw in [b"{".as_slice(), unknown.as_slice()] {
                let error = reply::parse(profile, &request, "node-a", 0, raw, b"")
                    .map_err(|error| dispatched_error(&request, error))
                    .unwrap_err();
                assert_eq!(error.code, "EXECUTOR_OPERATION_UNKNOWN");
            }
            assert_eq!(
                dispatched_error(&request, BxdlError::new("EXECUTOR_SPAWN_FAILED", "public")).code,
                "EXECUTOR_SPAWN_FAILED"
            );
        }
        let raw = envelope(
            "start",
            "INCOMPLETE",
            "SERVICE_START_WAIT_EXPIRED",
            Some(service_data()),
        );
        let response = reply::parse(profile, &start(), "node-a", 5, &raw, b"").unwrap();
        assert_eq!(
            response.into_service().unwrap().runtime_readiness,
            "NOT_READY"
        );
        let error = reply::parse(profile, &Request::Show, "node-a", 0, b"{", b"")
            .map_err(|error| dispatched_error(&Request::Show, error))
            .unwrap_err();
        assert_eq!(error.code, "EXECUTOR_RESPONSE_INVALID");
    }
}
