use super::*;
#[path = "product_tests.rs"]
mod product_tests;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    process::Command,
    time::Instant,
};

const CANARY: &str = "PRIVATE_ENGINE_OUTPUT_CANARY";

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    options: Options,
    node: PathBuf,
}
impl Fixture {
    fn new(info: &Value, cold: &Value, cold_exit: i32, extra: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let jar = root.join("candidate with spaces.jar");
        put(&jar, b"not a real engine; pinned unit-test fixture", 0o600);
        let java = root.join("java");
        let script = format!(
            "#!/bin/sh\n{extra}\nfor argument in \"$@\"; do\ncase \"$argument\" in\nengine-info) printf '%s\\n' '{}'; exit 0;;\npreflight) printf '%s\\n' '{}'; exit {cold_exit};;\nesac\ndone\nexit 77\n",
            info, cold
        );
        put(&java, script.as_bytes(), 0o700);
        let lock = root.join("engine.lock.json");
        let value = json!({"schemaVersion":1,"jarSha256":files::digest(&fs::read(&jar).unwrap()),
            "jarSizeBytes":fs::metadata(&jar).unwrap().len(),"javaSha256":files::digest(script.as_bytes()),"expected":identity()});
        put_json(&lock, &value);
        let chain = root.join("chain.json");
        put_json(
            &chain,
            &json!({"nigo.protocol.chain-id":"11578","nigo.protocol.consensus.protocol":"INSTANT","nigo.protocol.consensus.profile-id":"DEV_INSTANT"}),
        );
        let node = root.join("node.json");
        put_json(
            &node,
            &json!({"chainFile":"chain.json","dataDirectory":"data","backend":"rocksdb","node":{"server.address":"127.0.0.1","server.port":"18080"}}),
        );
        Self {
            _temp: temp,
            root,
            options: Options {
                jar,
                java,
                lock,
                // Response/schema tests are not startup latency benchmarks.
                // The owned-process timeout has a separate 100ms regression.
                timeout: Duration::from_secs(10),
            },
            node,
        }
    }
    fn standard() -> Self {
        Self::new(&identity(), &cold(), 3, "")
    }
    fn replace_java(&mut self, script: &str) {
        put(&self.options.java, script.as_bytes(), 0o700);
        self.edit_lock(|v| v["javaSha256"] = json!(files::digest(script.as_bytes())));
    }
    fn edit_lock(&self, mutate: impl FnOnce(&mut Value)) {
        let mut value: Value =
            serde_json::from_slice(&fs::read(&self.options.lock).unwrap()).unwrap();
        mutate(&mut value);
        put_json(&self.options.lock, &value);
    }
}
fn put(path: &Path, raw: &[u8], mode: u32) {
    fs::write(path, raw).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn put_json(path: &Path, value: &Value) {
    put(path, &serde_json::to_vec(value).unwrap(), 0o600);
}
fn identity() -> Value {
    json!({"buildInfoStatus":"AVAILABLE","engine":"NIGO","version":"0.0.1-SNAPSHOT",
        "source":{"commit":"a".repeat(40),"dirty":true},"java":{"requiredMajor":21},
        "console":{"node":"v24.21.0","npm":"11.19.0","sourceFingerprint":"b".repeat(64)},
        "contract":{"status":"PROPOSED","revision":"a".repeat(40),"documentationSha256":"1".repeat(64),
            "definitionSha256":"2".repeat(64),"runtimeDocumentationSha256":"3".repeat(64),"runtimeFixtureSha256":"4".repeat(64),"fingerprint":"5".repeat(64)},
        "distribution":{"channel":"development","officialRelease":false}})
}
fn cold() -> Value {
    json!({"command":"preflight","status":"INCOMPLETE","reason":"RUNTIME_CHECKS_REQUIRED","contractStatus":"PROPOSED",
        "backend":"rocksdb","chainFingerprint":"a".repeat(64),"nodeIdentity":"INSTANT",
        "checks":[{"check":"CONFIGURATION","status":"PASS"},{"check":"KEY_MATERIAL","status":"NOT_APPLICABLE"},
            {"check":"DATABASE_AND_WAL","status":"NOT_CHECKED","reason":"REQUIRES_EXCLUSIVE_OPEN"},
            {"check":"PORTS_AND_PEERS","status":"NOT_CHECKED","reason":"NETWORK_NOT_ACCESSED"},
            {"check":"NATIVE_RUNTIME","status":"NOT_CHECKED","reason":"REQUIRES_RUNTIME_LOAD"}]})
}
fn secret_free(error: &BxdlError) {
    assert!(!error.code.contains(CANARY));
    assert!(!error.message.contains(CANARY));
    assert!(!error.message.contains("/private/"));
}

#[test]
fn development_identity_and_cold_incomplete_preserve_contract_without_database_creation() {
    let fixture = Fixture::new(
        &identity(),
        &cold(),
        3,
        &format!("printf '%s\\n' '{CANARY}' >&2"),
    );
    let config_before = fs::read(&fixture.node).unwrap();
    let chain_before = fs::read(fixture.root.join("chain.json")).unwrap();
    let inspected = inspect(&fixture.options).unwrap();
    assert_eq!(inspected.outcome, "INSPECTED_DEVELOPMENT");
    assert!(inspected.development_only);
    let report = preflight(&fixture.options, &fixture.node).unwrap();
    assert_eq!(report.outcome, "INCOMPLETE");
    assert_eq!(report.engine_exit_code, 3);
    assert_eq!(report.config_sha256, Some(files::digest(&config_before)));
    assert_eq!(report.chain_file_sha256, Some(files::digest(&chain_before)));
    assert!(!serde_json::to_string(&report).unwrap().contains(CANARY));
    assert!(!fixture.root.join("data").exists());
    assert_eq!(fs::read(&fixture.node).unwrap(), config_before);
    assert_eq!(
        fs::read(fixture.root.join("chain.json")).unwrap(),
        chain_before
    );
}

#[test]
fn mismatched_jar_java_and_identity_are_rejected_before_cold_execution() {
    let fixture = Fixture::standard();
    fixture.edit_lock(|v| v["jarSha256"] = json!("0".repeat(64)));
    assert_eq!(
        inspect(&fixture.options).unwrap_err().code,
        "ENGINE_PIN_MISMATCH"
    );
    fixture.edit_lock(|v| {
        v["jarSha256"] = json!(files::digest(&fs::read(&fixture.options.jar).unwrap()));
        v["javaSha256"] = json!("0".repeat(64));
    });
    assert_eq!(
        inspect(&fixture.options).unwrap_err().code,
        "ENGINE_PIN_MISMATCH"
    );
    let mut wrong = identity();
    wrong["source"]["dirty"] = json!(false);
    let fixture = Fixture::new(&wrong, &cold(), 3, "");
    assert_eq!(
        preflight(&fixture.options, &fixture.node).unwrap_err().code,
        "ENGINE_IDENTITY_MISMATCH"
    );
}

#[test]
fn strict_lock_rejects_unknown_duplicate_missing_fields_and_release_claims() {
    for mutated in [
        {
            let mut v = identity();
            v["distribution"]["officialRelease"] = json!(true);
            v
        },
        {
            let mut v = identity();
            v["buildInfoStatus"] = json!("MISSING");
            v
        },
        {
            let mut v = identity();
            v["contract"]["revision"] = json!("b".repeat(40));
            v
        },
    ] {
        let fixture = Fixture::standard();
        fixture.edit_lock(|v| v["expected"] = mutated);
        assert_eq!(
            inspect(&fixture.options).unwrap_err().code,
            "ENGINE_LOCK_INVALID"
        );
    }
    let fixture = Fixture::standard();
    for bytes in [
        "{\"schemaVersion\":1,\"schemaVersion\":1}".to_string(),
        format!("{{\"{CANARY}\":true}}"),
        "{}".into(),
    ] {
        put(&fixture.options.lock, bytes.as_bytes(), 0o600);
        let error = inspect(&fixture.options).unwrap_err();
        assert_eq!(error.code, "ENGINE_LOCK_INVALID");
        secret_free(&error);
    }
}

#[test]
fn raw_or_forged_child_output_never_escapes_the_typed_allowlist() {
    let mut mutations = Vec::new();
    for field in [
        "reason",
        "nodeIdentity",
        "chainFingerprint",
        "backend",
        "status",
    ] {
        let mut v = cold();
        v[field] = json!(CANARY);
        mutations.push(v);
    }
    let mut v = cold();
    v["checks"][0]["reason"] = json!(CANARY);
    mutations.push(v);
    let mut v = cold();
    v["checks"][0]["extra"] = json!(CANARY);
    mutations.push(v);
    let mut v = cold();
    v["extra"] = json!(CANARY);
    mutations.push(v);
    let mut v = cold();
    v["checks"][1] = v["checks"][0].clone();
    mutations.push(v);
    let mut v = cold();
    v["checks"][2]["status"] = json!("PASS");
    mutations.push(v);
    for value in mutations {
        let error = consume_cold(3, &serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert_eq!(error.code, "ENGINE_RESPONSE_INVALID");
        secret_free(&error);
    }
    for bytes in [
        format!("{CANARY}\n"),
        format!("{{\"status\":\"{CANARY}\",\"status\":\"INCOMPLETE\"}}"),
        format!("{} {{}}", cold()),
    ] {
        let error = consume_cold(3, bytes.as_bytes()).unwrap_err();
        secret_free(&error);
    }
    let mut info = identity();
    info["leak"] = json!(CANARY);
    let fixture = Fixture::new(&info, &cold(), 3, "");
    let error = inspect(&fixture.options).unwrap_err();
    assert_eq!(error.code, "ENGINE_RESPONSE_INVALID");
    secret_free(&error);
}

#[test]
fn exit_and_status_must_agree_and_engine_failures_are_sanitized() {
    let invalid =
        json!({"status":"INVALID_CONFIGURATION","reason":"INVALID_ARGUMENTS_OR_CONFIGURATION"});
    let io = json!({"status":"FAILED","reason":"PRECONDITION_OR_IO_FAILURE"});
    assert_eq!(
        consume_cold(64, &serde_json::to_vec(&invalid).unwrap())
            .unwrap_err()
            .code,
        "ENGINE_INVALID_CONFIGURATION"
    );
    assert_eq!(
        consume_cold(74, &serde_json::to_vec(&io).unwrap())
            .unwrap_err()
            .code,
        "ENGINE_PRECONDITION_FAILED"
    );
    for (exit, value) in [(0, cold()), (3, invalid.clone()), (74, invalid), (64, io)] {
        assert_eq!(
            consume_cold(exit, &serde_json::to_vec(&value).unwrap())
                .unwrap_err()
                .code,
            "ENGINE_RESPONSE_INVALID"
        );
    }
    assert!(node_identity(&format!(
        "0x{}:0x{}",
        "a".repeat(64),
        "b".repeat(40)
    )));
    assert!(node_identity(&format!("0x{}:OBSERVER", "a".repeat(64))));
    for value in [
        format!("{}:OBSERVER", "a".repeat(64)),
        format!("0x{}:OBSERVER", "A".repeat(64)),
        format!("0x{}:0x{}", "a".repeat(40), "b".repeat(40)),
    ] {
        assert!(!node_identity(&value));
    }
}

#[test]
fn snapshots_rebase_only_native_paths_keep_originals_and_detect_changes() {
    let fixture = Fixture::standard();
    let mut value: Value = serde_json::from_slice(&fs::read(&fixture.node).unwrap()).unwrap();
    value["node"]["nigo.protocol.consensus.qbft.node.keystore-path"] = json!("absent-key");
    put_json(&fixture.node, &value);
    let native = files::NativeInput::load(&fixture.node).unwrap();
    let workspace = files::Workspace::create(Some(&native)).unwrap();
    let path = native.snapshot(&workspace).unwrap();
    let snap: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        snap["dataDirectory"],
        fixture.root.join("data").to_str().unwrap()
    );
    assert_eq!(
        snap["node"]["nigo.protocol.consensus.qbft.node.keystore-path"],
        fixture.root.join("absent-key").to_str().unwrap()
    );
    assert!(Path::new(snap["chainFile"].as_str().unwrap()).starts_with(&workspace.path));
    assert!(!fixture.root.join("absent-key").exists());
    put(&fixture.node, b"{}", 0o600);
    assert_eq!(native.recheck().unwrap_err().code, "ENGINE_INPUT_CHANGED");
}

#[test]
fn symlink_unsafe_permissions_and_data_containment_are_rejected_without_mutation() {
    let fixture = Fixture::standard();
    let alias = fixture.root.join("alias");
    symlink(&fixture.options.jar, &alias).unwrap();
    let options = Options {
        jar: alias,
        java: fixture.options.java.clone(),
        lock: fixture.options.lock.clone(),
        timeout: Duration::from_secs(1),
    };
    assert_eq!(inspect(&options).unwrap_err().code, "ENGINE_PATH_INVALID");
    fs::set_permissions(&fixture.options.lock, fs::Permissions::from_mode(0o666)).unwrap();
    assert_eq!(
        inspect(&fixture.options).unwrap_err().code,
        "ENGINE_INPUT_UNSAFE"
    );
    fs::set_permissions(&fixture.options.lock, fs::Permissions::from_mode(0o600)).unwrap();
    let mut value: Value = serde_json::from_slice(&fs::read(&fixture.node).unwrap()).unwrap();
    value["dataDirectory"] = json!(fixture.root.to_str().unwrap());
    put_json(&fixture.node, &value);
    assert_eq!(
        preflight(&fixture.options, &fixture.node).unwrap_err().code,
        "ENGINE_CONFIG_INVALID"
    );
}

#[test]
fn oversized_stdout_and_stderr_are_bounded_and_secret_free() {
    for target in ["", " >&2"] {
        let mut fixture = Fixture::standard();
        fixture.replace_java(&format!(
            "#!/bin/sh\nwhile :; do printf '%s\\n' '{CANARY}{}'{target}; done\n",
            "x".repeat(1000)
        ));
        let start = Instant::now();
        let error = inspect(&fixture.options).unwrap_err();
        assert_eq!(error.code, "ENGINE_OUTPUT_LIMIT");
        secret_free(&error);
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}

#[test]
fn ambient_java_options_are_absent_in_spawned_engine() {
    const CHILD: &str = "BXDL_ENGINE_ENV_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let check = "[ -z \"${JAVA_TOOL_OPTIONS+x}\" ] && [ -z \"${JDK_JAVA_OPTIONS+x}\" ] && [ -z \"${_JAVA_OPTIONS+x}\" ] && [ -z \"${CLASSPATH+x}\" ] && [ -z \"${DYLD_INSERT_LIBRARIES+x}\" ] || exit 77";
        let fixture = Fixture::new(&identity(), &cold(), 3, check);
        inspect(&fixture.options).unwrap();
        return;
    }
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "engine::tests::ambient_java_options_are_absent_in_spawned_engine",
        ])
        .env(CHILD, "1")
        .env("JAVA_TOOL_OPTIONS", CANARY)
        .env("JDK_JAVA_OPTIONS", CANARY)
        .env("_JAVA_OPTIONS", CANARY)
        .env("CLASSPATH", CANARY)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn output_descriptors_held_by_descendant_do_not_hang_parent_reader() {
    let mut fixture = Fixture::standard();
    let pidfile = fixture.root.join("descriptor-holder-pid");
    // A short-lived test descendant inherits the socket. The adapter must not
    // wait for its inherited descriptors after the direct child exits.
    fixture.replace_java(&format!(
        "#!/bin/sh\n/bin/sleep 5 &\nprintf '%s' \"$!\" > '{}'\nprintf '%s\\n' '{}'\nexit 0\n",
        pidfile.display(),
        identity()
    ));
    let start = Instant::now();
    let result = inspect(&fixture.options);
    let pid = fs::read_to_string(pidfile).unwrap();
    let _ = Command::new("/bin/kill")
        .args(["-KILL", &pid])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    assert_eq!(result.unwrap_err().code, "ENGINE_PROCESS_FAILED");
    assert!(start.elapsed() < Duration::from_secs(3));
}

#[cfg(target_os = "macos")]
#[test]
fn mac_temp_parent_alias_is_rejected_before_workspace_creation() {
    let fixture = Fixture::standard();
    let mut value: Value = serde_json::from_slice(&fs::read(&fixture.node).unwrap()).unwrap();
    value["dataDirectory"] = json!("/private/TMP");
    put_json(&fixture.node, &value);
    let native = files::NativeInput::load(&fixture.node).unwrap();
    assert_eq!(
        files::Workspace::create(Some(&native)).err().unwrap().code,
        "ENGINE_CONFIG_INVALID"
    );
    assert_eq!(
        fs::read(&fixture.node).unwrap(),
        serde_json::to_vec(&value).unwrap()
    );
}
