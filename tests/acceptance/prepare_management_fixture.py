#!/usr/bin/env python3
"""Prepare RC.4 manual single-node inputs without executing CLI, JVM or nodes.

Copies n1 from a validated fresh four-validator fixture. The approved chain and
credentials remain byte-identical; only product paths/ports and manual peerless
local options change. This is test preparation, not genesis or PKI generation.
"""
import argparse
import copy
import os
from pathlib import Path
import re
import stat
from types import SimpleNamespace

import fixture_guard as fg
import runtime_profiles as rp
import support as h

INPUT_NAMES = {"instance.json", "node.json", "chain.json", *fg.SECRET_FILES.values()}
SUPPLY_NAMES = {"package.tar.gz", "public-key.pem", "engine-lock.json"}
RECORD = "service-fixture.json"
PURPOSE = "RC4_MANUAL_PEERLESS_SINGLE_NODE_INPUTS_ONLY"
PREFIX = "nigo.protocol.consensus.qbft.node."


def real_directory(path):
    path = Path(path).expanduser()
    h.require(path.is_absolute() and path.resolve(strict=True) == path
              and stat.S_ISDIR(path.lstat().st_mode), "SERVICE_FIXTURE_DIRECTORY")
    return path


def source_files(fixture):
    return ["fixture-summary.json", "chain.json", "network.json", "products/n1.json",
            *("generated/validator-1/" + name for name in sorted(fg.SECRET_FILES.values()))]


def file_pins(root, names):
    pins = {}
    for name in names:
        path = root / name
        metadata = h.regular(path)
        h.require(0 < metadata.st_size, "SERVICE_FIXTURE_EMPTY_INPUT")
        pins[name] = h.sha(path)
    return pins


def manual_native(product, local):
    """Approved local fields from src/config/network.rs::load, with no peers.

    The network renderer requires every chain validator in its input and emits
    all peers. This explicit test transform preserves the four-member chain and
    copies only its n1 local fields, with consensus/sync/archive disabled.
    """
    h.require(product["nodeId"] == local["nodeId"], "SERVICE_FIXTURE_LOCAL_IDENTITY")
    node = {
        "server.address": product["http"]["address"], "server.port": product["http"]["port"],
        "nigo.monitor.console.enabled": True, "nigo.monitor.console.gc-control-enabled": False,
        "nigo.storage.gc.automatic.enabled": False,
        "nigo.protocol.consensus.auto-start": False,
        "nigo.transaction-ingress.permission": local["transactionPermission"],
        "nigo.protocol.consensus.qbft.sync.enabled": False,
        "nigo.protocol.consensus.qbft.sync.trust-mode": "GENESIS_VERIFIED",
        "nigo.protocol.consensus.qbft.sync.archive.enabled": False,
    }
    values = {
        "role": "VALIDATOR", "node-id": local["nodeId"], "validator-id": local["validatorId"],
        "listen-host": product["p2p"]["address"], "listen-port": product["p2p"]["port"],
        "transport-security-scheme": "MTLS", "mtls-key-store-type": "PKCS12",
        "mtls-trust-store-type": "PKCS12",
    }
    fields = {
        "keystore-path": "validatorKeystore", "keystore-password-file": "validatorPasswordFile",
        "mtls-key-store-path": "tlsKeyStore", "mtls-key-store-password-file": "tlsKeyPasswordFile",
        "mtls-trust-store-path": "tlsTrustStore", "mtls-trust-store-password-file": "tlsTrustPasswordFile",
    }
    values.update({key: product["secrets"][field] for key, field in fields.items()})
    node.update({PREFIX + key: value for key, value in values.items()})
    return {"chainFile": product["chainDescription"],
            "dataDirectory": product["storage"]["dataDirectory"], "backend": "rocksdb", "node": node}


def selected_product(fixture, root, instance_id, http_port, p2p_port):
    h.require(isinstance(instance_id, str) and
              re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}", instance_id), "INSTANCE_ID_INVALID")
    h.require(all(type(port) is int and 1024 <= port <= 65535 for port in (http_port, p2p_port))
              and http_port != p2p_port, "PORTS_INVALID")
    product = copy.deepcopy(h.document(fixture / "products/n1.json"))
    inputs = root / "operational inputs"
    product.update(instanceId=instance_id, chainDescription=str(inputs / "chain.json"))
    product["storage"] = {"backend": "rocksdb", "dataDirectory": str(root / "fresh data")}
    product["secrets"] = {field: str(inputs / name) for field, name in fg.SECRET_FILES.items()}
    product["http"] = {"address": "127.0.0.1", "port": http_port}
    product["p2p"] = {"address": "127.0.0.1", "port": p2p_port}
    locals_ = [node for node in h.document(fixture / "network.json")["nodes"]
               if node["nodeId"] == product["nodeId"]]
    h.require(len(locals_) == 1, "SERVICE_FIXTURE_LOCAL_IDENTITY")
    return product, manual_native(product, locals_[0])


def validate_service_fixture(root, candidate="rc4"):
    """Recheck generated input bytes and fresh source provenance, without writes."""
    h.require(candidate == "rc4", "SERVICE_FIXTURE_CANDIDATE")
    root = real_directory(root)
    record_path = root / RECORD
    metadata = h.regular(record_path)
    h.require(0 < metadata.st_size <= 262144 and stat.S_IMODE(metadata.st_mode) == 0o600,
              "SERVICE_FIXTURE_RECORD")
    record = h.document(record_path)
    h.require(record.get("schemaVersion") == 1 and record.get("candidate") == candidate
              and record.get("purpose") == PURPOSE and record.get("status") == "PREPARED_NOT_EXECUTED"
              and record.get("nodeDataCreated") is False and record.get("nodeStarted") is False
              and record.get("genesisMembershipChanged") is False
              and record.get("newCredentialsIssued") is False, "SERVICE_FIXTURE_PROVENANCE")
    fixture = real_directory(record["sourceFixture"])
    h.require(not root.is_relative_to(fixture) and not fixture.is_relative_to(root), "OUTPUT_OVERLAPS_FIXTURE")
    guard_args = SimpleNamespace(fixture=fixture, candidate=candidate, revision=record["revision"])
    pins = fg.validate_preparation(guard_args)
    h.require(pins == record["sourcePreparationPins"], "SERVICE_FIXTURE_SOURCE_CHANGED")
    original = file_pins(fixture, source_files(fixture))
    h.require(original == record["sourceFiles"], "SERVICE_FIXTURE_SOURCE_CHANGED")
    inputs, supply = real_directory(root / "operational inputs"), real_directory(root / "supply")
    h.require({p.name for p in inputs.iterdir()} == INPUT_NAMES and
              {p.name for p in supply.iterdir()} == SUPPLY_NAMES, "SERVICE_FIXTURE_MEMBERSHIP")
    rp.verify_runtime_supply(supply / "package.tar.gz", supply / "public-key.pem",
                             supply / "engine-lock.json", candidate)
    h.require(file_pins(inputs, INPUT_NAMES) == record["inputs"] and
              file_pins(supply, SUPPLY_NAMES) == record["supply"], "SERVICE_FIXTURE_INPUT_CHANGED")
    for name in INPUT_NAMES:
        h.require(stat.S_IMODE(h.regular(inputs / name).st_mode) == 0o600, "SERVICE_FIXTURE_PRIVATE_INPUT")
    h.require(h.sha(inputs / "chain.json") == original["chain.json"], "SERVICE_FIXTURE_CHAIN_CHANGED")
    for name in fg.SECRET_FILES.values():
        h.require(h.sha(inputs / name) == original["generated/validator-1/" + name],
                  "SERVICE_FIXTURE_CREDENTIAL_CHANGED")
    expected_product, expected_native = selected_product(fixture, root, record["instanceId"],
                                                         record["httpPort"], record["p2pPort"])
    h.require(h.document(inputs / "instance.json") == expected_product and
              h.document(inputs / "node.json") == expected_native, "SERVICE_FIXTURE_TRANSFORM_CHANGED")
    h.require(not os.path.lexists(root / "fresh data"), "SERVICE_FIXTURE_DATA_ALREADY_EXISTS")
    return {"recordSha256": h.sha(record_path), "sourcePreparationPins": pins,
            "inputs": record["inputs"], "supply": record["supply"], "candidate": candidate}


def prepare(args):
    h.require(getattr(args, "candidate", "rc4") == "rc4", "SERVICE_FIXTURE_CANDIDATE")
    fixture = real_directory(args.fixture)
    root = Path(args.output).expanduser()
    h.require(root.is_absolute() and root.parent.resolve(strict=True) == root.parent
              and not os.path.lexists(root), "NEW_ABSOLUTE_OUTPUT_REQUIRED")
    h.require(not root.is_relative_to(fixture) and not fixture.is_relative_to(root), "OUTPUT_OVERLAPS_FIXTURE")
    guard_args = SimpleNamespace(fixture=fixture, candidate="rc4", revision=args.revision)
    guard_pins = fg.validate_preparation(guard_args)
    sources = file_pins(fixture, source_files(fixture))
    rp.verify_runtime_supply(args.archive, args.public_key, args.lock, "rc4")
    source_supply = {"package.tar.gz": Path(args.archive), "public-key.pem": Path(args.public_key),
                     "engine-lock.json": Path(args.lock)}
    supply_pins = {name: h.sha(path) for name, path in source_supply.items()}
    product, native = selected_product(fixture, root, args.instance_id, args.http_port, args.p2p_port)
    root.mkdir(mode=0o700)
    inputs, supply = root / "operational inputs", root / "supply"
    inputs.mkdir(mode=0o700)
    supply.mkdir(mode=0o700)
    for name, source in source_supply.items():
        h.copy_file(source, supply / name, 0o600)
    h.copy_file(fixture / "chain.json", inputs / "chain.json", 0o600)
    for name in fg.SECRET_FILES.values():
        h.copy_file(fixture / "generated/validator-1" / name, inputs / name, 0o600)
    h.write_json(inputs / "instance.json", product)
    h.write_json(inputs / "node.json", native)
    fg.assert_preparation_unchanged(guard_args, guard_pins)
    h.require(file_pins(fixture, source_files(fixture)) == sources and
              all(h.sha(path) == supply_pins[name] for name, path in source_supply.items()),
              "SERVICE_FIXTURE_SOURCE_CHANGED")
    record = {
        "schemaVersion": 1, "candidate": "rc4", "purpose": PURPOSE, "status": "PREPARED_NOT_EXECUTED",
        "sourceFixture": str(fixture), "revision": args.revision, "sourcePreparationPins": guard_pins,
        "sourceFiles": sources, "sourceSelectedFilesUnchanged": True,
        "instanceId": args.instance_id, "httpPort": args.http_port, "p2pPort": args.p2p_port,
        "inputs": file_pins(inputs, INPUT_NAMES), "supply": file_pins(supply, SUPPLY_NAMES),
        "nodeDataCreated": False, "nodeStarted": False, "genesisMembershipChanged": False,
        "newCredentialsIssued": False, "cliInvocations": 0, "jvmInvocations": 0,
        "runtimeAcceptance": "NOT_CHECKED", "localReadinessTarget": "MANUAL_START_INITIALIZED",
        "networkAndQuorum": "NOT_CHECKED", "nativeTransform": "APPROVED_LOCAL_FIELDS_MANUAL_PEERLESS",
    }
    h.write_json(root / RECORD, record)
    validate_service_fixture(root)
    return record


def parser():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("fixture", "archive", "public-key", "lock", "output"):
        p.add_argument("--" + name, type=Path, required=True)
    p.add_argument("--candidate", choices=("rc4",), default="rc4")
    p.add_argument("--revision", required=True, help="explicit full preparation harness revision")
    p.add_argument("--instance-id", required=True)
    p.add_argument("--http-port", type=int, required=True)
    p.add_argument("--p2p-port", type=int, required=True)
    return p


def main(argv=None):
    prepare(parser().parse_args(argv))
    print("RC4_MANUAL_SINGLE_NODE_INPUTS_PREPARED_NOT_EXECUTED")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
