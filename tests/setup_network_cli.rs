#![cfg(unix)]

use base64::{Engine as _, engine::general_purpose::STANDARD};
use bxdl::cli;
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Cursor},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

const CANARY: &str = "NETWORK-PRIVATE-CANARY-756103";
// Public test certificate, generated once with OpenSSL. No private key is kept.
// This command computes a pin only; the certificate's trust/dates are not accepted.
const CERTIFICATE_BASE64: &str = "MIICHTCCAcQCCQC1BLoONMqcvjAKBggqhkjOPQQDAjAdMRswGQYDVQQDDBJCWERMIENMSSBUZXN0IE9ubHkwHhcNMjYxMDE5MTM0MzE5WhcNMjYxMDIxMTM0MzE5WjAdMRswGQYDVQQDDBJCWERMIENMSSBUZXN0IE9ubHkwggFLMIIBAwYHKoZIzj0CATCB9wIBATAsBgcqhkjOPQEBAiEA/////wAAAAEAAAAAAAAAAAAAAAD///////////////8wWwQg/////wAAAAEAAAAAAAAAAAAAAAD///////////////wEIFrGNdiqOpPns+u9VXaYhrxlHQawzFOw9jvOPD4n0mBLAxUAxJ02CIbnBJNqZnjhE50mt4GffpAEQQRrF9Hy4SxCR/i85uVjpEDydwN9gS3rM6D0oTlF2JjClk/jQuL+Gn+bjufrSnwPnhYrzjNXazFezsu2QGg3v1H1AiEA/////wAAAAD//////////7zm+q2nF56E87nKwvxjJVECAQEDQgAEXyfqyNEeMCP9KpbuanmVj9MtDWoWFzOaxZlabSdbGgeJ0J/G0KfOHpLCXmd2nFeJ88HCI+pmOr+NrQoh+b2OozAKBggqhkjOPQQDAgNHADBEAiBcHwkZu3PoMUz/9PSD3TPO8jZuZPJlWg+Epkdw1Dp3tgIgFqG4/gho1y5eM0/UeuTc/Cz42MqTYdXoaqoCzkP/0jE=";
// Independently calculated from the frozen DER with Python hashlib and the
// NIGO_TRANSPORT_CREDENTIAL/v1 + NUL + MTLS + NUL domain prefix.
const EXPECTED_PIN: &str = "0xb94b6a0ff99d1641cdeda631a6bc08b3f0bf62ac9983582e67e17d0c3b345563";

struct NoInput;
impl io::Read for NoInput {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("rejected or machine command must not consume input")
    }
}
impl io::BufRead for NoInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        panic!("rejected or machine command must not consume input")
    }
    fn consume(&mut self, _: usize) {
        panic!("rejected or machine command must not consume input")
    }
}

fn invoke(
    args: &[String],
    input: &mut dyn io::BufRead,
    interactive: bool,
) -> (i32, String, String) {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let exit = cli::run_with_input(args, input, interactive, &mut stdout, &mut stderr);
    (
        exit,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

fn machine(args: &[String], expected_exit: i32) -> Value {
    let mut args = args.to_vec();
    args.push("--json".into());
    let (exit, stdout, stderr) = invoke(&args, &mut NoInput, false);
    assert_eq!(exit, expected_exit, "{stdout}\n{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    let result: Value = serde_json::from_str(&stdout).expect("exactly one JSON document");
    assert_eq!(result["schemaVersion"], 1);
    assert!(!result["observedAt"].as_str().unwrap().is_empty());
    assert!(!stdout.contains(CANARY));
    result
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|value| (*value).into()).collect()
}

fn write(path: &Path, contents: impl AsRef<[u8]>, mode: u32) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn network_argument_and_terminal_rejections_do_not_read_input_or_create_paths() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let workspace = root.join("must not create session");
    let output = root.join("must not create native.json");
    let base = strings(&[
        "setup",
        "--network",
        "--workspace",
        workspace.to_str().unwrap(),
        "--from",
        CANARY,
        "--network-file",
        CANARY,
        "--output",
        output.to_str().unwrap(),
    ]);
    let (exit, stdout, stderr) = invoke(&base, &mut NoInput, false);
    assert_eq!(exit, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("SETUP_INTERACTIVE_TERMINAL_REQUIRED"));
    assert!(!stderr.contains(CANARY));
    assert_eq!(machine(&base, 2)["reasonCode"], "INVALID_ARGUMENTS");

    let mut invalid = Vec::new();
    for flag in ["--non-interactive", "--install", "--resume", "--unknown"] {
        let mut args = base.clone();
        args.push(flag.into());
        invalid.push(args);
    }
    invalid.extend([
        strings(&[
            "setup",
            "--network",
            "--from",
            CANARY,
            "--network-file",
            CANARY,
        ]),
        strings(&[
            "setup",
            "--network",
            "--workspace",
            workspace.to_str().unwrap(),
            "--from",
            CANARY,
        ]),
        strings(&[
            "setup",
            "--network",
            "--workspace",
            workspace.to_str().unwrap(),
            "--network-file",
            CANARY,
        ]),
        strings(&[
            "setup",
            "--workspace",
            workspace.to_str().unwrap(),
            "--network-file",
            CANARY,
        ]),
        strings(&[
            "setup",
            "--network",
            "--workspace",
            workspace.to_str().unwrap(),
            "--resume",
            "--network-file",
            CANARY,
        ]),
        strings(&[
            "setup",
            "--network",
            "--workspace",
            workspace.to_str().unwrap(),
            "--resume",
            "--from",
            CANARY,
        ]),
    ]);
    for args in invalid {
        let (exit, stdout, stderr) = invoke(&args, &mut NoInput, true);
        assert_eq!(exit, 2, "{stderr}");
        assert!(stdout.is_empty());
        assert!(stderr.contains("INVALID_ARGUMENTS"));
        assert!(!stderr.contains(CANARY));
        assert_eq!(machine(&args, 2)["reasonCode"], "INVALID_ARGUMENTS");
    }
    assert!(!workspace.exists());
    assert!(!output.exists());
    assert_eq!(fs::read_dir(root).unwrap().count(), 0);
}

#[test]
fn fingerprint_cli_matches_domain_separated_pin_for_pem_and_der_without_source_metadata() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let der = STANDARD.decode(CERTIFICATE_BASE64).unwrap();
    let pem =
        format!("-----BEGIN CERTIFICATE-----\n{CERTIFICATE_BASE64}\n-----END CERTIFICATE-----\n");
    for (encoding, contents) in [("DER", der.as_slice()), ("PEM", pem.as_bytes())] {
        let path = root.join(format!("{CANARY}.{encoding}"));
        write(&path, contents, 0o600);
        let args = strings(&[
            "config",
            "fingerprint",
            "--certificate",
            path.to_str().unwrap(),
        ]);
        let result = machine(&args, 0);
        assert_eq!(result["command"], "config fingerprint");
        assert_eq!(result["reasonCode"], "TRANSPORT_FINGERPRINT_COMPUTED");
        assert_eq!(result["data"]["credentialFingerprint"], EXPECTED_PIN);
        assert_eq!(result["data"]["encoding"], encoding);
        assert_eq!(result["data"]["certificateValidation"], "NOT_CHECKED");
        assert!(!result.to_string().contains(root.to_str().unwrap()));
        assert!(!result.to_string().contains("BXDL CLI Test Only"));
        assert_eq!(fs::read(&path).unwrap(), contents);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert_eq!(machine(&args, 3)["reasonCode"], "NETWORK_INPUT_INVALID");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o666
        );
    }
    assert_eq!(fs::read_dir(root).unwrap().count(), 2);
}

#[test]
fn fingerprint_rejects_bad_arguments_and_secret_like_contents_without_echo() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let path = root.join(format!("{CANARY}.pem"));
    write(
        &path,
        format!("-----BEGIN PRIVATE KEY-----\n{CANARY}\n-----END PRIVATE KEY-----\n"),
        0o600,
    );
    let base = strings(&[
        "config",
        "fingerprint",
        "--certificate",
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        machine(&base, 3)["reasonCode"],
        "NETWORK_CERTIFICATE_INVALID"
    );
    for tail in [
        vec!["--certificate", CANARY],
        vec!["--password", CANARY],
        vec![CANARY],
    ] {
        let mut args = base.clone();
        args.extend(strings(&tail));
        assert_eq!(machine(&args, 2)["reasonCode"], "INVALID_ARGUMENTS");
    }
    assert_eq!(
        machine(&strings(&["config", "fingerprint"]), 2)["reasonCode"],
        "INVALID_ARGUMENTS"
    );
    assert!(fs::read_to_string(path).unwrap().contains(CANARY));
}

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    sources: Vec<(PathBuf, Vec<u8>)>,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let secret_dir = root.join(CANARY);
        fs::create_dir(&secret_dir).unwrap();
        fs::set_permissions(&secret_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let node = format!("0x{}", "11".repeat(32));
        let validator = format!("0x{}", "22".repeat(20));
        let mut product: Value = serde_json::from_slice(include_bytes!(
            "../config/examples/instance.development.json"
        ))
        .unwrap();
        product["nodeId"] = node.clone().into();
        product["chainDescription"] = "chain.json".into();
        product["storage"]["dataDirectory"] = "must-not-create-data".into();
        product["p2p"]["address"] = "127.0.0.1".into();
        product["p2p"]["port"] = 19090.into();
        for name in [
            "validatorKeystore",
            "validatorPasswordFile",
            "tlsKeyStore",
            "tlsKeyPasswordFile",
            "tlsTrustStore",
            "tlsTrustPasswordFile",
        ] {
            let path = secret_dir.join(name);
            // Deliberately not real credentials: preparation must use metadata,
            // and leave content validation to the later engine cold check.
            write(&path, CANARY, 0o600);
            product["secrets"][name] = format!("{CANARY}/{name}").into();
        }
        let chain = json!({
            "nigo.protocol.consensus.protocol": "QBFT",
            "nigo.protocol.consensus.qbft.validators[0]": validator,
        });
        let network = json!({
            "schemaVersion": 1,
            "kind": "BXDL_QBFT_NETWORK",
            "nodes": [{
                "nodeId": node,
                "validatorId": validator,
                "host": "127.0.0.1",
                "port": 19090,
                "transactionPermission": "NONE",
                "credentialFingerprints": [EXPECTED_PIN],
            }],
        });
        for (name, document) in [
            ("instance.json", product),
            ("chain.json", chain),
            ("network.json", network),
        ] {
            write(
                &root.join(name),
                serde_json::to_vec_pretty(&document).unwrap(),
                0o600,
            );
        }
        let mut paths: Vec<PathBuf> = fs::read_dir(&secret_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        paths.extend(["instance.json", "chain.json", "network.json"].map(|name| root.join(name)));
        let sources = paths
            .into_iter()
            .map(|path| {
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        Self {
            _temporary: temporary,
            root,
            sources,
        }
    }

    fn arguments(&self, resume: bool) -> Vec<String> {
        let mut args = strings(&[
            "setup",
            "--network",
            "--workspace",
            self.root.join("network session").to_str().unwrap(),
        ]);
        if resume {
            args.push("--resume".into());
        } else {
            args.extend(strings(&[
                "--from",
                self.root.join("instance.json").to_str().unwrap(),
                "--network-file",
                self.root.join("network.json").to_str().unwrap(),
            ]));
        }
        args.extend(strings(&[
            "--output",
            self.root.join("native output.json").to_str().unwrap(),
        ]));
        args
    }

    fn assert_sources_unchanged(&self) {
        for (path, original) in &self.sources {
            assert_eq!(fs::read(path).unwrap(), *original);
        }
        assert!(!self.root.join("must-not-create-data").exists());
    }
}

#[test]
fn interactive_network_cancel_resumes_selected_options_and_exports_private_native_only() {
    let fixture = Fixture::new();
    let (exit, stdout, stderr) = invoke(
        &fixture.arguments(false),
        &mut Cursor::new(format!("{CANARY}\nJKS\n:cancel\n")),
        true,
    );
    assert_eq!(exit, 5, "{stdout}\n{stderr}");
    assert!(stdout.is_empty());
    assert!(stderr.contains("SETUP_NETWORK_PAUSED"));
    assert!(!stderr.contains(CANARY));
    assert!(!fixture.root.join("native output.json").exists());
    fixture.assert_sources_unchanged();

    let (exit, stdout, stderr) = invoke(
        &fixture.arguments(true),
        &mut Cursor::new(b"PKCS12\njoin-network\nno\nyes\nexport\ny\n"),
        true,
    );
    assert_eq!(exit, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("SETUP_NETWORK_WRITTEN"));
    assert!(stdout.contains("엔진 cold 검사·설치·실제 네트워크 검증: 미실행"));
    assert!(stderr.contains("[2/5]"));
    assert!(!stderr.contains("[1/5]"));
    assert!(!format!("{stdout}{stderr}").contains(CANARY));
    let output_path = fixture.root.join("native output.json");
    let native: Value = serde_json::from_slice(&fs::read(&output_path).unwrap()).unwrap();
    let node = &native["node"];
    assert_eq!(
        node["nigo.protocol.consensus.qbft.node.mtls-key-store-type"],
        "JKS"
    );
    assert_eq!(
        node["nigo.protocol.consensus.qbft.node.mtls-trust-store-type"],
        "PKCS12"
    );
    assert_eq!(node["nigo.protocol.consensus.qbft.sync.enabled"], true);
    assert_eq!(node["nigo.protocol.consensus.auto-start"], false);
    assert_eq!(
        node["nigo.protocol.consensus.qbft.sync.archive.enabled"],
        true
    );
    assert_eq!(
        native["dataDirectory"],
        fixture.root.join("must-not-create-data").to_str().unwrap()
    );
    assert_eq!(
        fs::metadata(&output_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let workspace = fixture.root.join("network session");
    assert_eq!(
        fs::metadata(&workspace).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for entry in fs::read_dir(workspace).unwrap() {
        let metadata = entry.unwrap().metadata().unwrap();
        if metadata.is_file() {
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        }
    }
    fixture.assert_sources_unchanged();
}

#[test]
fn invalid_network_document_does_not_echo_contents_or_create_session() {
    let fixture = Fixture::new();
    write(
        &fixture.root.join("network.json"),
        json!({"schemaVersion": 1, "privateKey": CANARY}).to_string(),
        0o600,
    );
    let (exit, stdout, stderr) = invoke(&fixture.arguments(false), &mut NoInput, true);
    assert_eq!(exit, 3);
    assert!(stdout.is_empty());
    assert!(stderr.contains("NETWORK_INPUT_INVALID"));
    assert!(!stderr.contains(CANARY));
    assert!(!fixture.root.join("network session").exists());
    assert!(!fixture.root.join("native output.json").exists());
    assert!(!fixture.root.join("must-not-create-data").exists());
}

#[test]
fn uncertain_native_publication_stays_unknown_in_cli() {
    let fixture = Fixture::new();
    let mut args = fixture.arguments(false);
    let output_index = args.iter().position(|a| a == "--output").unwrap() + 1;
    args[output_index] = fixture
        .root
        .join("missing-parent/native.json")
        .to_str()
        .unwrap()
        .to_owned();
    let (exit, stdout, stderr) = invoke(
        &args,
        &mut Cursor::new(b"\n\n\n\n\nexport\ny\nsave\n"),
        true,
    );
    assert_eq!(exit, 6, "{stdout}\n{stderr}");
    assert!(stdout.is_empty());
    assert!(stderr.contains("SETUP_NETWORK_PUBLICATION_UNCERTAIN"));
    assert!(!stderr.contains("SETUP_NETWORK_SAVED"));
    fixture.assert_sources_unchanged();
}

#[test]
fn network_help_single_field_edit_and_output_back_do_not_publish_or_echo_private_refs() {
    let fixture = Fixture::new();
    let mut arguments = fixture.arguments(false);
    arguments.truncate(arguments.len() - 2); // Exercise the output path prompt.
    let text = ":help\nJKS\n:back\n\nPKCS12\nnew-network\nyes\nno\n:help\nedit\n:help\n5\n:help\nyes\nexport\n:help\n:back\nexport\n\n:help\n\nsave\n";
    let (exit, stdout, stderr) = invoke(&arguments, &mut Cursor::new(text), true);
    assert_eq!(exit, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("SETUP_NETWORK_SAVED"));
    assert!(stderr.contains("아카이브: yes"));
    assert!(!format!("{stdout}{stderr}").contains(CANARY));
    assert!(!fixture.root.join("network session-node.json").exists());
    fixture.assert_sources_unchanged();

    // One edit changed only archiveEnabled; earlier answers survive resume.
    let (exit, stdout, stderr) = invoke(
        &fixture.arguments(true),
        &mut Cursor::new("export\n:help\nyes\n"),
        true,
    );
    assert_eq!(exit, 0, "{stdout}\n{stderr}");
    let value: Value =
        serde_json::from_slice(&fs::read(fixture.root.join("native output.json")).unwrap())
            .unwrap();
    let node = &value["node"];
    assert_eq!(
        node["nigo.protocol.consensus.qbft.node.mtls-key-store-type"],
        "JKS"
    );
    assert_eq!(
        node["nigo.protocol.consensus.qbft.node.mtls-trust-store-type"],
        "PKCS12"
    );
    assert_eq!(node["nigo.protocol.consensus.qbft.sync.enabled"], false);
    assert_eq!(node["nigo.protocol.consensus.auto-start"], true);
    assert_eq!(
        node["nigo.protocol.consensus.qbft.sync.archive.enabled"],
        true
    );
    fixture.assert_sources_unchanged();
}

#[test]
fn network_edit_cancel_and_confirmation_back_leave_native_absent() {
    let fixture = Fixture::new();
    let (exit, _, stderr) = invoke(
        &fixture.arguments(false),
        &mut Cursor::new("\n\n\n\n\nsave\n"),
        true,
    );
    assert_eq!(exit, 0, "{stderr}");
    for text in [
        "edit\n:help\n:back\nexport\n:back\nsave\n",
        "edit\n3\n:help\n:back\nsave\n",
        "edit\n:cancel\n",
        "export\n:help\n",
    ] {
        let (_, stdout, stderr) = invoke(&fixture.arguments(true), &mut Cursor::new(text), true);
        assert!(!format!("{stdout}{stderr}").contains(CANARY));
        assert!(!fixture.root.join("native output.json").exists());
        fixture.assert_sources_unchanged();
    }
}
