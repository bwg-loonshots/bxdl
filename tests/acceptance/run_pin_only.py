#!/usr/bin/env python3
"""Fresh, isolated wrong-pin acceptance using the already signed BXDL archive.

This is test-only orchestration. It does not modify earlier runs or fixtures,
rebuild NIGO, or use kill/live bootout/data deletion for cleanup.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import copy
import os
from pathlib import Path
import platform
import socket
import time

import run_acceptance as base
import support as h

ARCHIVE_SHA = "9edee7c322cb975f402b9715a2d49386b70e00629c2bef453db9ac3ac90becea"
MANIFEST_SHA = "4f833ea042a8779103d1e7ab70b322eb768a4e2ea7bbbdcb4ccf399b4bdc8b99"
CLI_SHA = "120089de764575709fe5d708a2d502df7dafd0b659ec4086c1176bfb8c0a803d"
HTTP_PORTS = tuple(range(24420, 24424))
P2P_PORTS = tuple(range(25420, 25424))
PREFIX = "nigo.protocol.consensus.qbft.node."
HARNESS_FILES = ("run_pin_only.py", "run_acceptance.py", "support.py",
                 "prepare_fixture.py", "fixture_guard.py")
HARNESS_SOURCE = Path(__file__).resolve().parent
HARNESS_PINS = {name: h.sha(HARNESS_SOURCE / name) for name in HARNESS_FILES}


def check_free_ports():
    reservations = []
    try:
        for port in HTTP_PORTS + P2P_PORTS:
            reservation = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            reservations.append(reservation)
            reservation.bind(("127.0.0.1", port))
    finally:
        for reservation in reservations:
            reservation.close()


class PinAcceptance(base.Acceptance):
    def frozen_package(self):
        old = h.document(self.a.source_run / "result.json")
        h.require(old["results"]["cleanup"] == {"remaining": [], "errors": []},
                  "SOURCE_RUN_NOT_CLEANLY_STOPPED")
        self.archive = self.a.source_run / "candidate.tar.gz"
        self.public = self.a.source_run / "test-package-public.pem"
        h.require(h.sha(self.archive) == ARCHIVE_SHA
                  and h.sha(self.cli) == CLI_SHA, "FROZEN_PACKAGE_IDENTITY")
        verified = self.command(None, ["package", "verify", self.archive,
                                      "--public-key", self.public], "existing-package-verify")
        h.require(verified["outcome"] == "SUCCEEDED"
                  and verified["data"]["authenticity"] == "verified-external-ed25519"
                  and verified["data"]["archiveSha256"] == ARCHIVE_SHA
                  and verified["data"]["manifestSha256"] == MANIFEST_SHA,
                  "FROZEN_PACKAGE_VERIFICATION")
        lock = h.document(self.lock)
        h.require(lock["jarSha256"] == base.JAR_SHA
                  and lock["javaSha256"] == base.JAVA_SHA, "ENGINE_RUNTIME_LOCK")
        self.results["package"] = copy.deepcopy(old["results"]["package"])
        self.results["package"]["reusedUnchangedArchive"] = True
        self.results["package"]["publicKeySha256"] = h.sha(self.public)
        self.results["package"]["engineLockSha256"] = h.sha(self.lock)
        self.results["sourceRun"] = {
            "resultSha256": h.sha(self.a.source_run / "result.json"),
            "outcome": old["outcome"], "reason": old["reason"],
            "earlierFailureSuperseded": False,
            "earlierDataOrControlsReused": False,
        }
        self.e.note("EXISTING_SIGNED_PACKAGE_VERIFIED")

    def check_fresh_fixture(self):
        summary = self.fixture
        nodes = summary["nodes"]
        h.require(summary["status"] == "PREPARED_NOT_EXECUTED"
                  and summary["engineJarSha256"] == base.JAR_SHA
                  and summary["javaSha256"] == base.JAVA_SHA
                  and summary["certificateExport"] == "EXPORTED_PIN_MATCHED"
                  and summary["nodeStarted"] is False
                  and summary["nodeDataCreated"] is False, "FRESH_FIXTURE_SCOPE")
        h.require(sorted(n["httpPort"] for n in nodes) == list(HTTP_PORTS)
                  and sorted(n["p2pPort"] for n in nodes) == list(P2P_PORTS),
                  "ISOLATED_PORTS_REQUIRED")
        h.require(h.sha(self.a.fixture / "chain.json") == summary["chainSha256"]
                  and h.sha(self.a.fixture / "network.json") == summary["networkSha256"],
                  "FIXTURE_PUBLIC_INPUTS_CHANGED")
        old_node_ids, old_validator_ids, old_pins = set(), set(), set()
        for index in range(1, 5):
            previous = self.a.source_run / f"n{index}"
            old_node_ids.add(h.document(previous / "instance.json")["nodeId"])
            native = h.document(previous / "node.json")["node"]
            old_validator_ids.add(native[PREFIX + "validator-id"])
            old_pins.update(value for key, value in native.items()
                            if ".credential-fingerprints[" in key)
        h.require(len(nodes) == 4
                  and not ({n["nodeId"] for n in nodes} & old_node_ids)
                  and not ({n["validatorId"] for n in nodes} & old_validator_ids)
                  and not ({n["credentialFingerprint"] for n in nodes} & old_pins),
                  "FIXTURE_IDENTITIES_NOT_FRESH")
        self.results["fixture"] = {
            "summarySha256": h.sha(self.a.fixture / "fixture-summary.json"),
            "chainSha256": summary["chainSha256"],
            "networkSha256": summary["networkSha256"],
            "freshValidatorKeysAndTlsIdentities": True,
            "certificateExport": summary["certificateExport"],
            "httpPorts": list(HTTP_PORTS), "p2pPorts": list(P2P_PORTS),
        }
        self.e.note("FOUR_FRESH_TEST_IDENTITIES_VERIFIED")

    def freeze_harness(self):
        target = self.e.logs / "harness"
        target.mkdir(mode=0o700)
        for name, digest in HARNESS_PINS.items():
            h.require(h.sha(HARNESS_SOURCE / name) == digest, "HARNESS_CHANGED_BEFORE_RUN")
            h.copy_file(HARNESS_SOURCE / name, target / name, 0o600)
            h.require(h.sha(target / name) == digest, "HARNESS_SNAPSHOT_MISMATCH")
        self.results["harness"] = HARNESS_PINS

    def wrong_pin_audit(self, bad):
        observations = []
        deadline = time.monotonic() + 45
        try:
            while time.monotonic() < deadline:
                try:
                    snapshot = self.consensus(bad)
                except (OSError, TimeoutError):
                    observations.append({"observation": "UNAVAILABLE"})
                    time.sleep(0.5)
                    continue
                security = snapshot.get("peerSecurity") or {}
                audit = security.get("audit") or {}
                count = sum(int(r["count"]) for r in (audit.get("reasonCounts") or [])
                            if r.get("reason") == "WRONG_PIN")
                matching = [event for event in (audit.get("recentEvents") or [])
                            if event.get("reason") == "WRONG_PIN"
                            and event.get("direction") == "OUTBOUND"
                            and event.get("stage") == "CREDENTIAL_PIN"]
                observations.append({"peerCount": snapshot.get("peerCount"),
                                     "wrongPinReasonCount": count,
                                     "matchingOutboundEvents": len(matching)})
                if count > 0 and matching and snapshot.get("peerCount") == 0:
                    h.write_json(self.e.logs / "wrong-pin-consensus.json", snapshot)
                    self.results["wrongPin"] = {
                        "faultNode": bad["label"],
                        "faultNodeSyncEnabled": False,
                        "faultNodePeerCount": 0,
                        "wrongPinReasonCount": count,
                        "matchingOutboundEvents": len(matching),
                        "direction": "OUTBOUND", "stage": "CREDENTIAL_PIN",
                        "reason": "WRONG_PIN",
                        "duplicateSignerRunning": False,
                    }
                    self.e.note("OUTBOUND_WRONG_PIN_REJECTION_VERIFIED")
                    return
                time.sleep(0.5)
            raise h.Failure("WRONG_PIN_NOT_OBSERVED")
        finally:
            h.write_json(self.e.logs / "wrong-pin-observations.json", observations)

    def run_pin_only(self):
        self.freeze_harness()
        self.frozen_package()
        self.check_fresh_fixture()
        check_free_ports()
        bad_metadata = min(self.fixture["nodes"], key=lambda n: n["nodeId"])
        bad_index = bad_metadata["index"]
        good = [self.prepare_node(index) for index in range(1, 5) if index != bad_index]
        h.require(len({node["genesisHash"] for node in good}) == 1, "GENESIS_DIVERGENCE")
        with ThreadPoolExecutor(max_workers=3) as executor:
            tasks = [executor.submit(self.start, node, node["label"] + "-first-start")
                     for node in good]
            failures = []
            for task in tasks:
                try:
                    task.result()
                except Exception as error:
                    failures.append(error)
            if failures:
                raise failures[0]
        bad = self.prepare_node(bad_index, bad=True)
        h.require(bad["genesisHash"] == good[0]["genesisHash"], "FAULT_GENESIS_DIVERGENCE")
        native = h.document(bad["native"])["node"]
        h.require(native["nigo.protocol.consensus.qbft.sync.enabled"] is False,
                  "WRONG_PIN_SYNC_MUST_BE_DISABLED")
        self.start(bad, "wrong-pin-first-start")
        self.wrong_pin_audit(bad)
        txhash = self.submit(0, good[0]["port"])
        self.finality(good, txhash, "healthy-three-during-wrong-pin")
        for node in good:
            snapshot = self.consensus(node)
            security = snapshot.get("peerSecurity") or {}
            sessions = security.get("sessions") or []
            h.require(snapshot.get("validatorCount") == 4
                      and snapshot.get("quorumSize") == 3
                      and snapshot.get("peerCount") == 2
                      and security.get("scheme") == "MTLS"
                      and security.get("authenticatedPeers") == 2
                      and len(sessions) == 2
                      and all(s.get("authenticated") is True
                              and s.get("protocol") == "TLSv1.3" for s in sessions),
                      "HEALTHY_MTLS_QUORUM_" + node["label"])
            h.write_json(self.e.logs / (node["label"] + "-healthy-consensus.json"), snapshot)
        self.results["scope"] = {
            "freshNetwork": True, "freshData": True,
            "healthyValidatorCount": 3, "wrongPinValidatorCount": 1,
            "freshTransactionNonce": 0, "submissionCount": 1,
            "healthyNodesAuthenticatedTls13": True,
            "catchupOrTwoOfFourRecoveryTested": False,
            "priorFailuresSuperseded": False,
        }
        self.e.note("HEALTHY_THREE_FINALITY_AND_MTLS_VERIFIED")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", action="store_true")
    for name in ("cli", "fixture", "lock", "source-run", "output"):
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    if not args.run:
        print("No execution. Add --run for this isolated wrong-pin case.")
        return 0
    os.umask(0o077)
    h.require(platform.system() == "Darwin" and platform.machine() == "arm64",
              "MAC_ARM64_REQUIRED")
    h.require(args.output.is_absolute() and not args.output.exists(),
              "NEW_ABSOLUTE_OUTPUT_REQUIRED")
    acceptance = PinAcceptance(args)
    error = None
    try:
        acceptance.run_pin_only()
    except (Exception, KeyboardInterrupt) as exc:
        error = str(exc) if isinstance(exc, h.Failure) else "HARNESS_" + type(exc).__name__
    finally:
        cleaned = acceptance.cleanup()
    try:
        base.fg.assert_inputs_unchanged(acceptance.a, acceptance.input_pins)
    except h.Failure as changed:
        error = error or str(changed)
    unchanged = all(h.sha(HARNESS_SOURCE / name) == digest
                    for name, digest in HARNESS_PINS.items())
    result = {
        "outcome": "PASS" if error is None and cleaned and unchanged else "INCOMPLETE",
        "reason": error if error is not None else (None if unchanged else "HARNESS_CHANGED"),
        "scope": "FRESH_MAC_WRONG_PIN_AND_HEALTHY_THREE_FINALITY_ONLY",
        "results": acceptance.results, "steps": acceptance.e.steps,
        "childProcesses": acceptance.e.children,
        "harnessFilesUnchanged": unchanged,
    }
    h.write_json(args.output / "result.json", result)
    print(result["outcome"], result["reason"] or "", str(args.output), flush=True)
    return 0 if result["outcome"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
