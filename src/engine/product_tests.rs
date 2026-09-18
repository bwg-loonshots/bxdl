use super::*;

const Q: &str = "nigo.protocol.consensus.qbft.node.";
fn qbft_cold() -> Value {
    let mut result = cold();
    result["nodeIdentity"] = json!(format!("0x{}:0x{}", "a".repeat(64), "b".repeat(40)));
    result["checks"][1]["status"] = json!("PASS");
    result
}
fn paired(cold: &Value) -> (Fixture, PathBuf) {
    let fixture = Fixture::new(&identity(), cold, 3, "");
    fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o700)).unwrap();
    let marker = fixture.root.join("invoked");
    let mut fixture = fixture;
    let script = fs::read_to_string(&fixture.options.java).unwrap();
    fixture.replace_java(&script.replacen(
        "#!/bin/sh\n",
        &format!("#!/bin/sh\nprintf 'called' >> '{}'\n", marker.display()),
        1,
    ));
    let product_dir = fixture.root.join("product");
    fs::create_dir(&product_dir).unwrap();
    let instance = product_dir.join("instance.json");
    let mut p: Value = serde_json::from_str(include_str!(
        "../../config/examples/instance.development.json"
    ))
    .unwrap();
    p["nodeId"] = json!(format!("0x{}", "a".repeat(64)));
    p["chainDescription"] = json!("../chain.json");
    p["storage"]["dataDirectory"] = json!("../data");
    let chain = fixture.root.join("chain.json");
    put_json(
        &chain,
        &json!({"nigo.protocol.chain-id":"11578", "nigo.protocol.consensus.protocol":"QBFT", "nigo.protocol.consensus.profile-id":"TEST_QBFT"}),
    );
    let mut node = json!({"server.address":"127.0.0.1","server.port":"18080"});
    for (key, value) in [
        ("node-id", format!("0x{}", "a".repeat(64))),
        ("validator-id", format!("0x{}", "b".repeat(40))),
        ("role", "VALIDATOR".into()),
        ("transport-security-scheme", "MTLS".into()),
        ("listen-host", "192.0.2.10".into()),
        ("listen-port", "19090".into()),
    ] {
        node[format!("{Q}{key}")] = json!(value);
    }
    for (i, (product, native)) in [
        ("validatorKeystore", "keystore-path"),
        ("validatorPasswordFile", "keystore-password-file"),
        ("tlsKeyStore", "mtls-key-store-path"),
        ("tlsKeyPasswordFile", "mtls-key-store-password-file"),
        ("tlsTrustStore", "mtls-trust-store-path"),
        ("tlsTrustPasswordFile", "mtls-trust-store-password-file"),
    ]
    .iter()
    .enumerate()
    {
        let name = format!("{i}.private");
        put(&fixture.root.join(&name), CANARY.as_bytes(), 0o600);
        p["secrets"][product] = json!(format!("../{name}"));
        node[format!("{Q}{native}")] = json!(name);
    }
    put_json(&instance, &p);
    put_json(
        &fixture.node,
        &json!({"chainFile":"chain.json","dataDirectory":"data","backend":"rocksdb","node":node}),
    );
    (fixture, instance)
}
fn edit(path: &Path, change: impl FnOnce(&mut Value)) {
    let mut value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    change(&mut value);
    put_json(path, &value);
}
fn args(f: &Fixture, instance: &Path) -> Vec<String> {
    vec![
        "preflight".into(),
        "--config".into(),
        instance.to_str().unwrap().into(),
        "--engine-config".into(),
        f.node.to_str().unwrap().into(),
        "--jar".into(),
        f.options.jar.to_str().unwrap().into(),
        "--java".into(),
        f.options.java.to_str().unwrap().into(),
        "--lock".into(),
        f.options.lock.to_str().unwrap().into(),
        "--allow-development".into(),
        "--json".into(),
    ]
}

#[test]
fn product_and_native_with_separate_relative_bases_are_bound_to_exact_inputs() {
    let (f, p) = paired(&qbft_cold());
    let report = preflight_product(&f.options, &p, &f.node).unwrap();
    assert_eq!(
        report.configuration_binding, "MATCHED",
        "{:?}",
        report.product
    );
    assert_eq!(report.outcome, "INCOMPLETE");
    assert_eq!(
        report.product.config_sha256,
        files::digest(&fs::read(&p).unwrap())
    );
    let engine = report.engine.as_ref().unwrap();
    assert_eq!(
        engine.config_sha256,
        Some(files::digest(&fs::read(&f.node).unwrap()))
    );
    assert_eq!(
        engine.chain_file_sha256,
        Some(files::digest(&fs::read(f.root.join("chain.json")).unwrap()))
    );
    assert!(!f.root.join("data").exists());
    let serialized = serde_json::to_string(&report).unwrap();
    assert!(!serialized.contains(CANARY));
    assert!(!serialized.contains("0.private"));
    assert!(!serialized.contains(f.root.to_str().unwrap()));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(crate::cli::run(&args(&f, &p), &mut out, &mut err), 5);
    assert!(err.is_empty());
    let result: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(result["command"], "preflight");
    assert_eq!(result["data"]["configurationBinding"], "MATCHED");
    assert_eq!(result["data"]["engine"]["engineExitCode"], 3);
}

#[test]
fn every_required_native_binding_field_is_explicit_and_checked_before_invocation() {
    let keys = ["server.address".into(), "server.port".into()]
        .into_iter()
        .chain(
            [
                "role",
                "transport-security-scheme",
                "node-id",
                "listen-host",
                "listen-port",
                "keystore-path",
                "keystore-password-file",
                "mtls-key-store-path",
                "mtls-key-store-password-file",
                "mtls-trust-store-path",
                "mtls-trust-store-password-file",
            ]
            .into_iter()
            .map(|s| format!("{Q}{s}")),
        );
    for key in keys {
        for remove in [true, false] {
            let (f, p) = paired(&qbft_cold());
            edit(&f.node, |v| {
                if remove {
                    v["node"].as_object_mut().unwrap().remove(&key);
                } else {
                    v["node"][&key] = json!("different");
                }
            });
            let error = preflight_product(&f.options, &p, &f.node).err().unwrap();
            assert_eq!(
                error.code, "ENGINE_PRODUCT_MISMATCH",
                "{key} remove={remove}"
            );
            secret_free(&error);
            assert!(!f.root.join("invoked").exists());
        }
    }
}

#[test]
fn instant_wrong_chain_data_and_backend_cannot_bind_validator_draft() {
    for case in 0..4 {
        let (f, p) = paired(&qbft_cold());
        match case {
            0 => edit(&f.root.join("chain.json"), |v| {
                v["nigo.protocol.consensus.protocol"] = json!("INSTANT")
            }),
            1 => {
                fs::copy(f.root.join("chain.json"), f.root.join("different.json")).unwrap();
                edit(&f.node, |v| v["chainFile"] = json!("different.json"));
            }
            2 => edit(&f.node, |v| v["dataDirectory"] = json!("different-data")),
            _ => edit(&f.node, |v| v["backend"] = json!("h2")),
        }
        assert_eq!(
            preflight_product(&f.options, &p, &f.node)
                .err()
                .unwrap()
                .code,
            "ENGINE_PRODUCT_MISMATCH"
        );
        assert!(!f.root.join("invoked").exists());
    }
}

#[test]
fn missing_product_material_retains_checks_and_does_not_invoke_engine() {
    let (f, p) = paired(&qbft_cold());
    fs::remove_file(f.root.join("0.private")).unwrap();
    let report = preflight_product(&f.options, &p, &f.node).unwrap();
    assert_eq!(report.outcome, "FAIL");
    assert_eq!(report.configuration_binding, "NOT_CHECKED");
    assert!(report.engine.is_none());
    assert!(report.product.checks.iter().any(|c| c.status == "FAIL"));
    assert!(!f.root.join("invoked").exists());
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(crate::cli::run(&args(&f, &p), &mut out, &mut err), 4);
}

#[test]
fn equivalent_ip_integer_port_and_hex_case_are_accepted_without_changing_inputs() {
    let (f, p) = paired(&qbft_cold());
    edit(&p, |v| {
        v["nodeId"] = json!(format!("0x{}", "A".repeat(64)));
        v["p2p"]["address"] = json!("::1");
    });
    edit(&f.node, |v| {
        v["node"]["server.port"] = json!(18080);
        v["node"][format!("{Q}listen-host")] = json!("0:0:0:0:0:0:0:1");
    });
    assert_eq!(
        preflight_product(&f.options, &p, &f.node)
            .unwrap()
            .configuration_binding,
        "MATCHED"
    );
    edit(&f.node, |v| v["node"]["server.port"] = json!(18080.0));
    assert_eq!(
        preflight_product(&f.options, &p, &f.node)
            .err()
            .unwrap()
            .code,
        "ENGINE_PRODUCT_MISMATCH"
    );
}

#[test]
fn wrong_runtime_backend_identity_observer_or_instant_is_not_a_bound_validator_result() {
    for case in 0..4 {
        let mut cold = qbft_cold();
        match case {
            0 => cold["backend"] = json!("h2"),
            1 => cold["nodeIdentity"] = json!(format!("0x{}:0x{}", "c".repeat(64), "b".repeat(40))),
            2 => cold["nodeIdentity"] = json!(format!("0x{}:OBSERVER", "a".repeat(64))),
            _ => {
                cold["nodeIdentity"] = json!("INSTANT");
                cold["checks"][1]["status"] = json!("NOT_APPLICABLE");
            }
        }
        let (f, p) = paired(&cold);
        assert_eq!(
            preflight_product(&f.options, &p, &f.node)
                .err()
                .unwrap()
                .code,
            "ENGINE_PRODUCT_MISMATCH"
        );
    }
}

#[test]
fn mutation_after_binding_is_rejected_before_cold_invocation() {
    let (mut f, p) = paired(&qbft_cold());
    let script = fs::read_to_string(&f.options.java).unwrap();
    f.replace_java(&script.replacen(
        "engine-info)",
        &format!("engine-info) printf 'changed' > '{}';", p.display()),
        1,
    ));
    assert_eq!(
        preflight_product(&f.options, &p, &f.node)
            .err()
            .unwrap()
            .code,
        "ENGINE_INPUT_CHANGED"
    );
    assert_eq!(fs::read(f.root.join("invoked")).unwrap(), b"called");
}

#[test]
fn partial_engine_options_are_rejected_instead_of_silently_running_local_checks() {
    let (f, p) = paired(&qbft_cold());
    let full = args(&f, &p);
    for key in [
        "--engine-config",
        "--jar",
        "--java",
        "--lock",
        "--allow-development",
    ] {
        let mut partial = full.clone();
        let index = partial.iter().position(|s| s == key).unwrap();
        partial.remove(index);
        if key != "--allow-development" {
            partial.remove(index);
        }
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(crate::cli::run(&partial, &mut out, &mut err), 2);
        assert!(!f.root.join("invoked").exists());
    }
}

#[test]
fn possible_mac_name_collision_is_not_proof_of_product_binding() {
    let (f, p) = paired(&qbft_cold());
    edit(&f.node, |v| v["dataDirectory"] = json!("DATA"));
    assert_eq!(
        preflight_product(&f.options, &p, &f.node)
            .err()
            .unwrap()
            .code,
        "ENGINE_PRODUCT_MISMATCH"
    );
    assert!(!f.root.join("invoked").exists());
    // On filesystems where these two existing names are distinct, conservative
    // Unicode folding used for setup collisions must not equate their bytes.
    let first = f.root.join("credential-ß");
    let second = f.root.join("credential-ss");
    put(&first, b"first", 0o600);
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&second)
    {
        Ok(_) => {
            put(&second, b"second", 0o600);
            edit(&p, |v| {
                v["secrets"]["validatorKeystore"] = json!("../credential-ß")
            });
            edit(&f.node, |v| {
                v["dataDirectory"] = json!("data");
                v["node"][format!("{Q}keystore-path")] = json!("credential-ss");
            });
            assert_eq!(
                preflight_product(&f.options, &p, &f.node)
                    .err()
                    .unwrap()
                    .code,
                "ENGINE_PRODUCT_MISMATCH"
            );
            assert!(!f.root.join("invoked").exists());
        }
        Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists),
    }
}
