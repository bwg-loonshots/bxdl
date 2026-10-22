"""Read-only input boundary for the pinned, test-only candidate acceptance harness.

No node is started and no credential contents are read. Returned pins contain
hashes and relative public-input names, never fixture documents or secret paths.
Signed transaction decoding remains the offline preparation tool's evidence.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat

import support as h
from candidates import candidate_for

ENGINE_SOURCE = "540a89f0814c01d2a4ec67da1087af35195882ca"
JAR_SHA = "f1ea80e25052221ed8a6e3108e020cb1b776833f4f86a29aae0885ad41703f45"
JAVA_SHA = "9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c"
TEST_REVISION = "fecf7a622b06c867fac14eed74f26a66cb0a62c8"
LEGACY_RESULT_SHA = "cf05be22ccf4895a9c783c4efd82868a0c7b78310ebd19f0a9eef1d8f10baf51"
LEGACY_LOCK_SHA = "e24144b0a9115da01b3259f4bde913505475c4fffc6c7e17997d7dd256596c47"
LEGACY_PUBLIC = {
    "fixture-summary.json": "23ce8bc8917ce83efa8723f08682fb57d8665c6db39af877996cf88c47f0488e",
    "chain.json": "bcf529f714586c2030add806cc179a4a4746cf8843cd545144687b036eb8b5aa",
    "network.json": "daf4d4ab5301598110c38d0682cf37b1b55f1b37c8320aa4d2ddaf837f76776c",
    "generated/devnet.json": "c9e366443f726f1c516760f24e324089291c6785973e4840544f7f1717b05d17",
    "products/n1.json": "73b9ffc0b1c699f87fbfb95aa6139527a1eedad343fe4e6953baff91aae34501",
    "products/n2.json": "45f54e879697d7d6531c3f34b8d5147db2ff741b009d37fa63a4b607b239f460",
    "products/n3.json": "d1f02b1d8ee698b975e942022b3e36ae0c8140215a3c5cb2c980fd2453b5d07d",
    "products/n4.json": "70fdfaa9976ee958201f7a64bcef240bbd8eb01c6ca4a1ad0aadc24f6bf03e8e",
}
LEGACY_TRANSACTIONS = {
    "transactions/tx-0.json": "58703dc6f779d6eabb8a2669ded52ec2ec6f46d184953df788f7391daa978336",
    "transactions/tx-1.json": "55b170b76cec366d98cc96bb51c3922554714f08ccbba643f131405f03dfe895",
    "transactions/tx-2.json": "04db41d565b3af03e3f0fcfd4f89f80221308dc45f74873bf25f47f5fdf6e89f",
    "transactions/tx-3.json": "6a9036eac084273a4e40a14f6e458c869fcf8221218138d30743236afa95f33c",
    "transactions/tx-4.json": "4ef98f49c99d5bf3496354fa02cfbf599582470f8e098caba8a4a9d3989a0a08",
    "transactions/tx-5.json": "d2920452cf1cb2c716d4f2fa9efad0a3d96d179a2e2e828718ea41e93283566b",
    "transactions/tx-6.json": "4c30b92c6d8c3d0031edbd43fc2cdcdd0c712126fceccbd4ca3cf704533bd71f",
    "transactions/tx-7.json": "6d8f978b5f701c2047c03b1898eed303eb609637afd2fc24cd7e52402c444e0e",
}
SECRET_FILES = {
    "validatorKeystore": "validator.keystore",
    "validatorPasswordFile": "validator.password",
    "tlsKeyStore": "peer-identity.p12",
    "tlsKeyPasswordFile": "peer-identity.password",
    "tlsTrustStore": "peer-trust.p12",
    "tlsTrustPasswordFile": "peer-trust.password",
}
PUBLIC_LIMIT = 1024 * 1024
TRANSACTION_LIMIT = 16 * 1024


def _require(condition, code):
    h.require(condition, "FIXTURE_GUARD_" + code)


def _digest(data):
    return hashlib.sha256(data).hexdigest()


def _set_digest(values):
    return _digest(json.dumps(values, sort_keys=True, separators=(",", ":")).encode())


def _regular(path, limit, root=None, private=False):
    metadata = path.lstat()
    _require(stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1, "REGULAR_FILE")
    _require(0 < metadata.st_size <= limit, "FILE_SIZE")
    if root is not None:
        # Reject parent symlinks as well as a symlink in the final component.
        _require(path.resolve(strict=True) == path and path.is_relative_to(root), "FILE_BOUNDARY")
    if private:
        _require(stat.S_IMODE(metadata.st_mode) == 0o600, "CREDENTIAL_MODE")
    return metadata


def _read_json(path, limit=PUBLIC_LIMIT, root=None):
    before = _regular(path, limit, root)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        opened = os.fstat(stream.fileno())
        _require((opened.st_dev, opened.st_ino) == (before.st_dev, before.st_ino), "INPUT_CHANGED")
        raw = stream.read(limit + 1)
    after = _regular(path, limit, root)
    _require(len(raw) <= limit and
             (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) ==
             (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns), "INPUT_CHANGED")
    value = json.loads(raw)
    _require(isinstance(value, dict), "JSON_OBJECT")
    return value, _digest(raw)


def _reference(value, expected, root, private=False):
    _require(isinstance(value, str) and Path(value).is_absolute(), "REFERENCE_PATH")
    path = Path(value)
    _require(path == expected, "REFERENCE_TARGET")
    limit = 16 * 1024 if expected.name.endswith(".password") else 4 * PUBLIC_LIMIT
    _regular(path, limit, root, private)


def _hex(value, width):
    return isinstance(value, str) and re.fullmatch("0x[0-9a-f]{" + str(width) + "}", value) is not None


def _port(value):
    return type(value) is int and 1024 <= value <= 65535


def _validate_inputs(args):
    candidate = candidate_for(args)
    candidate_name = getattr(args, "candidate", "rc1")
    root = Path(args.fixture).expanduser().resolve(strict=True)
    _require(root.is_dir(), "ROOT")
    public = {}

    def public_json(relative):
        value, digest = _read_json(root / relative, root=root)
        public[relative] = digest
        return value

    summary = public_json("fixture-summary.json")
    _require(summary.get("schemaVersion") == 1
             and summary.get("purpose") == "EPHEMERAL_LOCAL_BXDL_PACKAGE_ACCEPTANCE_ONLY"
             and summary.get("status") == "PREPARED_NOT_EXECUTED"
             and summary.get("engineSource") == candidate["source"]
             and summary.get("engineJarSha256") == candidate["jarSha256"]
             and summary.get("javaSha256") == JAVA_SHA, "PREPARATION_IDENTITY")
    _require(summary.get("candidate", "rc1") == candidate_name, "CANDIDATE_IDENTITY")
    if candidate_name in ("rc2", "rc3"):
        _require(summary.get("engineJarSizeBytes") == candidate["jarSizeBytes"], "JAR_SIZE")
    chain = public_json("chain.json")
    network = public_json("network.json")
    manifest = public_json("generated/devnet.json")
    _require(public["chain.json"] == summary.get("chainSha256")
             and public["network.json"] == summary.get("networkSha256"), "PUBLIC_HASH")
    _require(chain.get("nigo.protocol.chain-id") == "11578"
             and chain.get("nigo.protocol.consensus.profile-id") == "DEV_QBFT"
             and chain.get("nigo.protocol.consensus.protocol") == "QBFT", "CHAIN_IDENTITY")
    _require(manifest.get("chainId") == 11578 and manifest.get("profileId") == "DEV_QBFT"
             and manifest.get("transportSecurityScheme") == "MTLS", "MANIFEST_IDENTITY")
    nodes = summary.get("nodes")
    generated = manifest.get("nodes")
    peers = network.get("nodes")
    _require(all(isinstance(items, list) and len(items) == 4
                 for items in (nodes, generated, peers)), "FOUR_VALIDATORS")
    _require(network.get("schemaVersion") == 1 and network.get("kind") == "BXDL_QBFT_NETWORK",
             "NETWORK_IDENTITY")
    seen = {name: set() for name in ("nodeId", "validatorId", "credentialFingerprint")}
    ports = set()
    for index, (node, metadata, peer) in enumerate(zip(nodes, generated, peers), 1):
        directory = root / "generated" / f"validator-{index}"
        _require(node.get("index") == metadata.get("index") == index
                 and metadata.get("role") == "VALIDATOR", "NODE_INDEX")
        _require(node.get("product") == f"products/n{index}.json"
                 and metadata.get("directory") == str(directory)
                 and metadata.get("config") == str(directory / "application.yml"), "NODE_PATH")
        for field, width in (("nodeId", 64), ("validatorId", 40), ("credentialFingerprint", 64)):
            value = node.get(field)
            _require(_hex(value, width) and value == metadata.get(field)
                     and value not in seen[field], "NODE_IDENTITY")
            seen[field].add(value)
        for field in ("httpPort", "p2pPort"):
            value = node.get(field)
            _require(_port(value) and value == metadata.get(field) and value not in ports, "PORT")
            ports.add(value)
        _require(peer == {
            "nodeId": node["nodeId"], "validatorId": node["validatorId"],
            "host": "127.0.0.1", "port": node["p2pPort"], "transactionPermission": "INGRESS",
            "credentialFingerprints": [node["credentialFingerprint"]],
            "revokedCredentialFingerprints": [],
        }, "LOOPBACK_PEER")
        product = public_json(f"products/n{index}.json")
        _require(product.get("schemaVersion") == 1 and product.get("role") == "validator"
                 and product.get("nodeId") == node["nodeId"], "PRODUCT_IDENTITY")
        _require(product.get("http") == {"address": "127.0.0.1", "port": node["httpPort"]}
                 and product.get("p2p") == {"address": "127.0.0.1", "port": node["p2pPort"]},
                 "LOOPBACK_PRODUCT")
        _reference(product.get("chainDescription"), root / "chain.json", root)
        secrets = product.get("secrets", {})
        _require(isinstance(secrets, dict) and set(secrets) == set(SECRET_FILES), "CREDENTIAL_FIELDS")
        for field, filename in SECRET_FILES.items():
            _reference(secrets[field], directory / filename, root, private=True)
    validators = {key: value for key, value in chain.items()
                  if key.startswith("nigo.protocol.consensus.qbft.validators[")}
    _require(validators == {
        f"nigo.protocol.consensus.qbft.validators[{index}]": value
        for index, value in enumerate(sorted(seen["validatorId"]))
    }, "CHAIN_VALIDATORS")
    transaction_files = {}
    expected = summary.get("transactions")
    _require(isinstance(expected, list) and len(expected) == candidate["transactionCount"], "TRANSACTION_COUNT")
    transaction_hashes = set()
    for nonce in range(candidate["transactionCount"]):
        relative = f"transactions/tx-{nonce}.json"
        tx, digest = _read_json(root / relative, TRANSACTION_LIMIT, root)
        _require(tx.get("schemaVersion") == 1 and type(tx.get("nonce")) is int
                 and tx["nonce"] == nonce and tx.get("chainId") == 11578
                 and tx.get("type") == 2 and _hex(tx.get("hash"), 64), "TRANSACTION_METADATA")
        raw = tx.get("rawTransaction")
        _require(isinstance(raw, str) and re.fullmatch(r"0x02(?:[0-9a-fA-F]{2})+", raw) is not None,
                 "TRANSACTION_HEX")
        _require(expected[nonce] == {"nonce": nonce, "hash": tx["hash"], "file": relative}
                 and tx["hash"] not in transaction_hashes, "TRANSACTION_IDENTITY")
        transaction_hashes.add(tx["hash"])
        transaction_files[relative] = digest
    lock, lock_digest = _read_json(Path(args.lock).expanduser().resolve(strict=True))
    _require(lock.get("jarSha256") == candidate["jarSha256"]
             and lock.get("javaSha256") == JAVA_SHA
             and lock.get("jarSizeBytes") == candidate["jarSizeBytes"]
             and lock.get("expected", {}).get("source", {}).get("commit") == candidate["source"], "LOCK_ARTIFACT")
    revision = getattr(args, "revision", TEST_REVISION)
    _require(isinstance(revision, str) and re.fullmatch(r"[0-9a-f]{40}", revision) is not None,
             "REVISION")
    return {
        "schemaVersion": 1, "scope": "PINNED_TEST_CANDIDATE_FIXTURE_ONLY",
        "candidate": candidate_name,
        "publicInputs": public, "transactionFiles": transaction_files,
        "transactionSetSha256": _set_digest(transaction_files),
        "engineLockSha256": lock_digest, "revision": revision,
        "revisionDefaultedForFixedDiagnostic": not hasattr(args, "revision"),
        "transactionValidation": "METADATA_AND_HEX_ONLY_CRYPTOGRAPHIC_DECODE_BY_OFFLINE_PREPARATION",
        "credentialValidation": "EXPECTED_LOCAL_PATH_REGULAR_SIZE_MODE_ONLY_CONTENT_NOT_READ",
    }


def validate_inputs(args):
    """Validate fixture/lock and return safe, serializable input pins."""
    try:
        return _validate_inputs(args)
    except h.Failure:
        raise
    except Exception:
        raise h.Failure("FIXTURE_GUARD_INVALID_INPUT") from None


def assert_inputs_unchanged(args, pins):
    """Recheck immediately before mutations/submissions and after a run."""
    _require(validate_inputs(args) == pins, "INPUT_SET_CHANGED")


def validate_legacy_continuation(args, old, pins):
    """Allow only the frozen B receipt-expectation failure with fixture A."""
    try:
        _require(getattr(args, "candidate", "rc1") == "rc1", "LEGACY_RC1_ONLY")
        prior = Path(args.continue_from).expanduser().resolve(strict=True)
        observed, digest = _read_json(prior / "result.json", 2 * PUBLIC_LIMIT, prior)
        _require(digest == LEGACY_RESULT_SHA and observed == old, "LEGACY_RESULT")
        _require(old.get("outcome") == "INCOMPLETE"
                 and old.get("reason") == "FINALITY_TIMEOUT_same-db-catchup"
                 and old["results"]["cleanup"] == {"remaining": [], "errors": []},
                 "LEGACY_OUTCOME")
        _require(all(pins["publicInputs"].get(name) == digest
                     for name, digest in LEGACY_PUBLIC.items())
                 and pins["transactionFiles"] == LEGACY_TRANSACTIONS
                 and pins["transactionSetSha256"] == _set_digest(LEGACY_TRANSACTIONS)
                 and pins["engineLockSha256"] == LEGACY_LOCK_SHA
                 and pins["revision"] == old["results"]["package"]["revision"] == TEST_REVISION,
                 "LEGACY_INPUT_SET")
        assert_inputs_unchanged(args, pins)
    except h.Failure:
        raise
    except Exception:
        raise h.Failure("FIXTURE_GUARD_INVALID_LEGACY_INPUT") from None
