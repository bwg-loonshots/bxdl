#!/usr/bin/env python3
"""Fresh RC.4 Linux lifecycle inputs; no install, node initialization or service.

Only the pinned RC.4 development generator/keytool are executed as the ordinary
operator. Four validator identities are generated locally; one is selected for a
manual, loopback, peerless lifecycle test. The full chain membership is preserved.
No wallet, transactions, network consensus or production credentials are used.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import sys
import zipfile

sys.dont_write_bytecode = True

import linux_build_contract as contract
from linux_build_contract import Failure, require

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("bxdl_linux_input_process", Path(__file__).with_name("build-linux-candidate.py"))
builder = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(builder)
RUNTIME_LOCK = "packaging/runtime-lock.temurin-21-linux-amd64.json"
ENGINE_LOCK = "packaging/engine-lock.rc4-temurin-linux-development.json"
RUNTIME_LOCK_SHA = "23eec871655dea8af8f79cb69f21c4add762226a9314df003edce1233e898f47"
ENGINE_LOCK_SHA = "9817d950f19832c725bfd373829023d44a437ad93e9f6291feef6c3185e94675"
HELPERS = {
    "BOOT-INF/classes/org/nigo/node/devnet/qbft/QbftDevnetGenerator.class": "c02940619cc4d51eb611fd912dbdcf619511177337905476c1a39330351a3de6",
    "org/springframework/boot/loader/launch/PropertiesLauncher.class": "4116ec36a9dc21b8d360d6fad6d10eb176cb50e62825fe9d944865392e0270ac",
}
SECRET_FIELDS = {
    "validatorKeystore": "validator.keystore", "validatorPasswordFile": "validator.password",
    "tlsKeyStore": "peer-identity.p12", "tlsKeyPasswordFile": "peer-identity.password",
    "tlsTrustStore": "peer-trust.p12", "tlsTrustPasswordFile": "peer-trust.password",
}
PREFIX = "nigo.protocol.consensus.qbft.node."
CONTROL_PARENT = Path("/var/lib/bxdl/instances")
PURPOSE = "FRESH_LINUX_RC4_MANUAL_LOCAL_LIFECYCLE_INPUTS"


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode()


def save(path, raw):
    builder.write_bytes(path, raw)
    path.chmod(0o600)


def document(path, value):
    save(path, encoded(value))


def bounded_json(path):
    return contract._json(contract._read(path, 256 * 1024))


def private_file(path):
    value = contract.fingerprint(path)
    m = path.lstat()
    require(m.st_uid == os.geteuid() and value["mode"] == 0o600
            and 0 < value["size"] <= 1024 * 1024, "TEST_INPUT_PRIVATE_FILE_INVALID")
    return value


def new_output(path):
    path = contract._absolute(path)
    require(not os.path.lexists(path), "TEST_INPUT_OUTPUT_EXISTS")
    return path


def pin_supply(runtime, jar):
    """Verify the complete selected runtime, not only its launcher executable."""
    runtime, jar = contract._absolute(runtime), contract._absolute(jar)
    contract._directory(runtime)
    require(contract.digest(ROOT / RUNTIME_LOCK) == RUNTIME_LOCK_SHA
            and contract.digest(ROOT / ENGINE_LOCK) == ENGINE_LOCK_SHA, "TEST_INPUT_LOCK_MISMATCH")
    lock, engine = bounded_json(ROOT / RUNTIME_LOCK), bounded_json(ROOT / ENGINE_LOCK)
    expected = {item["path"]: {k: item[k] for k in ("sha256", "size", "mode")} for item in lock["files"]}
    observed = {}
    expected_dirs = set()
    for name in expected:
        for parent in Path(name).parents:
            if parent != Path("."):
                expected_dirs.add(parent.as_posix())
    observed_dirs = set()
    for base, dirs, files in os.walk(runtime, followlinks=False):
        for name in dirs:
            path = Path(base) / name
            contract._directory(path)
            observed_dirs.add(path.relative_to(runtime).as_posix())
        for name in files:
            path = Path(base) / name
            relative = path.relative_to(runtime).as_posix()
            require(relative in expected, "TEST_INPUT_RUNTIME_MEMBERSHIP")
            observed[relative] = contract.fingerprint(path)
    require(observed == expected and observed_dirs == expected_dirs, "TEST_INPUT_RUNTIME_MISMATCH")
    jar_pin = contract.fingerprint(jar)
    require(jar_pin["sha256"] == engine["jarSha256"] and jar_pin["size"] == engine["jarSizeBytes"]
            and observed["bin/java"]["sha256"] == engine["javaSha256"], "TEST_INPUT_ENGINE_MISMATCH")
    with zipfile.ZipFile(jar) as archive:
        for name, expected_sha in HELPERS.items():
            matches = [item for item in archive.infolist() if item.filename == name]
            require(len(matches) == 1 and not matches[0].is_dir() and 0 < matches[0].file_size <= 1024 * 1024,
                    "TEST_INPUT_HELPER_INVALID")
            require(hashlib.sha256(archive.read(matches[0])).hexdigest() == expected_sha,
                    "TEST_INPUT_HELPER_MISMATCH")
    require(contract.fingerprint(jar) == jar_pin, "TEST_INPUT_SUPPLY_CHANGED")
    return {"runtimeLockSha256": RUNTIME_LOCK_SHA, "engineLockSha256": ENGINE_LOCK_SHA,
            "runtimeFiles": len(observed), "engineSource": engine["expected"]["source"]["commit"],
            "jarSha256": jar_pin["sha256"], "jarSizeBytes": jar_pin["size"],
            "javaSha256": observed["bin/java"]["sha256"], "keytoolSha256": observed["bin/keytool"]["sha256"]}


def checked_manifest(generated, http_port, p2p_port):
    manifest = bounded_json(generated / "devnet.json")
    require(manifest.get("chainId") == 11578 and manifest.get("profileId") == "DEV_QBFT"
            and manifest.get("transportSecurityScheme") == "MTLS", "TEST_INPUT_GENERATOR_IDENTITY")
    nodes = manifest.get("nodes")
    require(isinstance(nodes, list) and len(nodes) == 4, "TEST_INPUT_GENERATOR_MEMBERSHIP")
    seen = {"nodeId": set(), "validatorId": set(), "credentialFingerprint": set()}
    for index, node in enumerate(nodes, 1):
        require(isinstance(node, dict), "TEST_INPUT_GENERATOR_NODE")
        directory = generated / ("validator-" + str(index))
        contract._directory(directory)
        require(node.get("index") == index and node.get("role") == "VALIDATOR"
                and node.get("directory") == str(directory) and node.get("config") == str(directory / "application.yml")
                and node.get("httpPort") == http_port + index - 1 and node.get("p2pPort") == p2p_port + index - 1,
                "TEST_INPUT_GENERATOR_NODE")
        for key, width in (("nodeId", 64), ("validatorId", 40), ("credentialFingerprint", 64)):
            value = node.get(key)
            require(isinstance(value, str) and re.fullmatch("0x[0-9a-f]{" + str(width) + "}", value)
                    and value not in seen[key], "TEST_INPUT_GENERATOR_PUBLIC_ID")
            seen[key].add(value)
        for name in SECRET_FIELDS.values():
            private_file(directory / name)
    return nodes


def chain_properties(nodes):
    # Same explicit DEV_QBFT chain whitelist used by RC.4 preparation. This
    # public inert address requires neither a private wallet nor signed txs.
    chain = {
        "nigo.protocol.chain-id": "11578", "nigo.protocol.consensus.profile-id": "DEV_QBFT",
        "nigo.protocol.consensus.protocol": "QBFT", "nigo.protocol.consensus.max-transactions-per-block": "100",
        "nigo.protocol.consensus.qbft.block-period-ms": "1000", "nigo.protocol.consensus.qbft.empty-block-period-ms": "0",
        "nigo.protocol.consensus.qbft.request-timeout-ms": "5000", "nigo.protocol.consensus.qbft.max-request-timeout-ms": "20000",
        "nigo.protocol.consensus.qbft.max-future-block-time-ms": "1000",
        "nigo.protocol.native-assets.registration-mode": "ALLOWLISTED",
        "nigo.protocol.devnet.genesis-funding[0].address": "0x0000000000000000000000000000000000000042",
        "nigo.protocol.devnet.genesis-funding[0].amount": "1000000000000000000000",
        "nigo.protocol.fee-profile.revisions[0].activation-height": "0", "nigo.protocol.fee-profile.revisions[0].mode": "ZERO",
        "nigo.protocol.fee-profile.revisions[0].policy-id": "ZERO", "nigo.protocol.fee-profile.revisions[0].policy-revision": "1",
        "nigo.protocol.fee-profile.revisions[0].gas-price": "0", "nigo.protocol.fee-profile.revisions[0].settlement-mode": "NONE",
    }
    for index, validator in enumerate(sorted(node["validatorId"] for node in nodes)):
        chain[f"nigo.protocol.consensus.qbft.validators[{index}]"] = validator
    return chain


def configs(inputs, instance, node, http_port, p2p_port):
    product = {
        "schemaVersion": 1, "instanceId": instance, "role": "validator", "nodeId": node["nodeId"],
        "chainDescription": str(inputs / "chain.json"),
        "storage": {"backend": "rocksdb", "dataDirectory": str(CONTROL_PARENT / instance / "data")},
        "secrets": {key: str(inputs / value) for key, value in SECRET_FIELDS.items()},
        "http": {"address": "127.0.0.1", "port": http_port}, "p2p": {"address": "127.0.0.1", "port": p2p_port},
    }
    native_node = {
        "server.address": "127.0.0.1", "server.port": http_port,
        "nigo.monitor.console.enabled": True, "nigo.monitor.console.gc-control-enabled": False,
        "nigo.storage.gc.automatic.enabled": False, "nigo.protocol.consensus.auto-start": False,
        "nigo.transaction-ingress.permission": "INGRESS", "nigo.protocol.consensus.qbft.sync.enabled": False,
        "nigo.protocol.consensus.qbft.sync.trust-mode": "GENESIS_VERIFIED", "nigo.protocol.consensus.qbft.sync.archive.enabled": False,
    }
    local = {
        "role": "VALIDATOR", "node-id": node["nodeId"], "validator-id": node["validatorId"],
        "listen-host": "127.0.0.1", "listen-port": p2p_port, "transport-security-scheme": "MTLS",
        "mtls-key-store-type": "PKCS12", "mtls-trust-store-type": "PKCS12",
    }
    references = {
        "keystore-path": "validatorKeystore", "keystore-password-file": "validatorPasswordFile",
        "mtls-key-store-path": "tlsKeyStore", "mtls-key-store-password-file": "tlsKeyPasswordFile",
        "mtls-trust-store-path": "tlsTrustStore", "mtls-trust-store-password-file": "tlsTrustPasswordFile",
    }
    local.update({key: product["secrets"][field] for key, field in references.items()})
    native_node.update({PREFIX + key: value for key, value in local.items()})
    native = {"chainFile": product["chainDescription"], "dataDirectory": product["storage"]["dataDirectory"],
              "backend": "rocksdb", "node": native_node}
    return product, native


def prepare(args):
    host = builder.native_host()
    require(re.fullmatch(r"[a-z][a-z0-9-]{0,31}", args.instance) is not None, "TEST_INPUT_INSTANCE_INVALID")
    require(all(type(port) is int and 1024 <= port <= 65532 for port in (args.http_port, args.p2p_port))
            and not set(range(args.http_port, args.http_port + 4)) & set(range(args.p2p_port, args.p2p_port + 4)),
            "TEST_INPUT_PORTS_INVALID")
    control = CONTROL_PARENT / args.instance
    require(not os.path.lexists(control), "TEST_INPUT_INSTANCE_EXISTS")
    runtime, jar = contract._absolute(args.runtime), contract._absolute(args.jar)
    root = new_output(args.output)
    require(not root.is_relative_to(runtime) and root != jar
            and not runtime.is_relative_to(root) and not jar.is_relative_to(root), "TEST_INPUT_OUTPUT_OVERLAP")
    supply = pin_supply(runtime, jar)
    root.mkdir(mode=0o700)
    for name in ("inputs", "evidence", "tool-cwd", "tool-home", "tmp", "public"):
        (root / name).mkdir(mode=0o700)
    env = {"LANG": "C", "LC_ALL": "C", "PATH": "/usr/bin:/bin", "HOME": str(root / "tool-home"), "TMPDIR": str(root / "tmp")}
    generated = root / "generated"
    builder.run([
        runtime / "bin/java", "-Djava.io.tmpdir=" + str(root / "tmp"), "-Dloader.home=" + str(root / "tool-cwd"),
        "-Dloader.main=org.nigo.node.devnet.qbft.QbftDevnetGenerator", "-cp", jar,
        "org.springframework.boot.loader.launch.PropertiesLauncher", "--output=" + str(generated),
        "--validators=4", "--observers=0", "--transport-security=MTLS", "--chain-id=11578", "--profile-id=DEV_QBFT",
        "--http-base-port=" + str(args.http_port), "--p2p-base-port=" + str(args.p2p_port),
        "--request-timeout-ms=5000", "--max-request-timeout-ms=20000",
    ], root / "tool-cwd", env, "generate", evidence=root / "evidence", timeout=180)
    nodes = checked_manifest(generated, args.http_port, args.p2p_port)
    for index, node in enumerate(nodes, 1):
        require(isinstance(node, dict), "TEST_INPUT_GENERATOR_NODE")
        directory = generated / ("validator-" + str(index))
        leaf = root / "public" / f"leaf-n{index}.der"
        builder.run([
            runtime / "bin/keytool", "-J-Djava.io.tmpdir=" + str(root / "tmp"), "-exportcert", "-alias", "peer", "-storetype", "PKCS12",
            "-keystore", directory / "peer-identity.p12", "-storepass:file", directory / "peer-identity.password", "-file", leaf,
        ], root / "tool-cwd", env, f"export-leaf-n{index}", evidence=root / "evidence", timeout=60)
        leaf.chmod(0o600)
        private_file(leaf)
        expected = "0x" + hashlib.sha256(b"NIGO_TRANSPORT_CREDENTIAL/v1\x00MTLS\x00" + contract._read(leaf, 1024 * 1024)).hexdigest()
        require(expected == node["credentialFingerprint"], "TEST_INPUT_CERTIFICATE_MISMATCH")
    inputs = root / "inputs"
    source = generated / "validator-1"
    for name in SECRET_FIELDS.values():
        before = private_file(source / name)
        save(inputs / name, contract._read(source / name, 1024 * 1024))
        require(private_file(source / name) == before == private_file(inputs / name), "TEST_INPUT_CREDENTIAL_CHANGED")
    product, native = configs(inputs, args.instance, nodes[0], args.http_port, args.p2p_port)
    document(inputs / "chain.json", chain_properties(nodes))
    document(inputs / "instance.json", product)
    document(inputs / "node.json", native)
    require(pin_supply(runtime, jar) == supply, "TEST_INPUT_SUPPLY_CHANGED")
    require(not os.path.lexists(control), "TEST_INPUT_INSTANCE_EXISTS")
    receipt = {
        "schemaVersion": 1, "kind": "bxdl-linux-test-inputs", "purpose": PURPOSE,
        "outcome": "PREPARED", "candidate": "rc4", "host": host, "supply": supply,
        "instance": args.instance, "httpPort": args.http_port, "p2pPort": args.p2p_port,
        "inputs": {path.name: private_file(path) for path in sorted(inputs.iterdir())},
        "chainValidators": 4, "selectedValidator": 1, "newLocalCredentials": True,
        "certificateExport": "EXPORTED_PIN_MATCHED", "localReadinessTarget": "MANUAL_START_INITIALIZED",
        "systemInstall": "NOT_RUN", "initialization": "NOT_RUN", "systemd": "NOT_RUN",
        "nodeDataCreated": False, "nodeStarted": False, "networkAndQuorum": "NOT_TESTED",
        "runtimeAcceptance": "NOT_CHECKED", "executorProfile": "NOT_ADMITTED",
        "privateEvidence": "KEEP_LOCAL_DO_NOT_RETURN_GENERATED_OR_INPUTS_OR_EVIDENCE",
    }
    document(root / "receipt.json", receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runtime", "jar", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--instance", required=True)
    parser.add_argument("--http-port", type=int, required=True)
    parser.add_argument("--p2p-port", type=int, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    try:
        receipt = prepare(args)
    except (Failure, OSError, ValueError, KeyError, TypeError, zipfile.BadZipFile) as error:
        code = str(error) if isinstance(error, Failure) else "TEST_INPUT_PREPARATION_FAILED"
        print(json.dumps({"outcome": "FAILED", "reasonCode": code, "nodeStarted": False}), file=sys.stderr)
        return 1
    print(json.dumps({"outcome": receipt["outcome"], "reasonCode": "LINUX_TEST_INPUTS_PREPARED",
                      "instance": receipt["instance"], "localReadinessTarget": receipt["localReadinessTarget"],
                      "initialization": "NOT_RUN", "systemd": "NOT_RUN", "networkAndQuorum": "NOT_TESTED"}, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
