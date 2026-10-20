#!/usr/bin/env python3
"""Explicit test-only recovery probe of an unchanged, previously stopped package.

Reuses B's control/data after C's normal cleanup. Never builds, resets data, signs
transactions, kills processes, or changes the three imported acceptance helpers.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import copy
from datetime import datetime, timezone
import json
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
REVISION = "fecf7a622b06c867fac14eed74f26a66cb0a62c8"
SOURCE = Path(__file__).resolve().parent
HELPER_PINS = {
    "run_acceptance.py": "ce79e9dc90bbdc1d7c9b0d187ca088199474182382c86772846a198d6c9365fe",
    "support.py": "07f8ba80d300c3ca97c2d600bce749ed07b72f66b123f460adbf5719f82bea2e",
    "prepare_fixture.py": "edd450b9d74a4e40d3961983f4cb9bdf8d30be684c6716186f1886cbe7c54af7",
    "fixture_guard.py": "5b65ca351c64780dd2bfcf4d3b2dfa9510e07fb259e2a83e46db909cddaefdf4"
}


def utc():
    return datetime.now(timezone.utc).isoformat()


def helpers_unchanged():
    return all(h.sha(SOURCE / name) == digest for name, digest in HELPER_PINS.items())


class RecoveryProbe(base.Acceptance):
    def __init__(self, args):
        super().__init__(args)
        self.started_at = time.monotonic()
        self.samples = 0
        self.next_notice = 0
        self.trace = (self.e.logs / "observations.jsonl").open("x", encoding="utf-8")
        for tx in self.transactions:
            self.e.redactions.append(tx["rawTransaction"].encode())
        self.results["scope"] = {
            "sameArchiveAndControlAndData": True,
            "priorFailuresSuperseded": False,
            "newSignatures": False,
            "automaticResubmission": False,
            "quorumLossHoldSeconds": 45,
            "restoredFinalityBudgetSeconds": 90,
        }

    def frozen_inputs(self):
        old = h.document(self.a.source_run / "result.json")
        last = h.document(self.a.last_run / "result.json")
        for result in (old, last):
            h.require(result["results"]["cleanup"] == {"remaining": [], "errors": []},
                      "PRIOR_CLEANUP_NOT_VERIFIED")
        h.require(last["outcome"] == "INCOMPLETE"
                  and last["reason"] == "FINALITY_TIMEOUT_restored-quorum",
                  "UNEXPECTED_PRIOR_RESULT")
        h.require(helpers_unchanged(), "HELPER_SOURCE_CHANGED")
        self.archive = self.a.source_run / "candidate.tar.gz"
        self.public = self.a.source_run / "test-package-public.pem"
        h.require(h.sha(self.archive) == ARCHIVE_SHA and h.sha(self.cli) == CLI_SHA,
                  "FROZEN_PACKAGE_IDENTITY")
        verified = self.command(None, ["package", "verify", self.archive,
                                      "--public-key", self.public], "package-verify")
        h.require(verified["outcome"] == "SUCCEEDED"
                  and verified["data"]["authenticity"] == "verified-external-ed25519"
                  and verified["data"]["archiveSha256"] == ARCHIVE_SHA
                  and verified["data"]["manifestSha256"] == MANIFEST_SHA,
                  "FROZEN_PACKAGE_VERIFY")
        lock = h.document(self.lock)
        h.require(lock["jarSha256"] == base.JAR_SHA
                  and lock["javaSha256"] == base.JAVA_SHA, "LOCK_IDENTITY")
        h.require(self.fixture["chainSha256"] == h.sha(self.a.fixture / "chain.json")
                  and self.fixture["networkSha256"] == h.sha(self.a.fixture / "network.json"),
                  "PUBLIC_FIXTURE_CHANGED")
        self.results["package"] = copy.deepcopy(old["results"]["package"])
        self.results["package"]["reusedUnchangedArchive"] = True
        self.results["inputs"] = {
            "sourceResultSha256": h.sha(self.a.source_run / "result.json"),
            "lastResultSha256": h.sha(self.a.last_run / "result.json"),
            "fixtureSummarySha256": h.sha(self.a.fixture / "fixture-summary.json"),
            "engineLockSha256": h.sha(self.lock),
            "publicKeySha256": h.sha(self.public),
            "helpers": HELPER_PINS,
            "probeSha256": h.sha(Path(__file__)),
        }
        for name in (*HELPER_PINS, Path(__file__).name):
            h.copy_file(SOURCE / name, self.e.logs / ("source-" + name), 0o600)
        for index in range(1, 5):
            folder = self.a.source_run / f"n{index}"
            product = h.document(folder / "instance.json")
            h.require(product["http"]["port"] == 24319 + index
                      and product["p2p"]["port"] == 25319 + index,
                      "FIXTURE_A_PORT_REQUIRED")
            h.require(product["storage"]["dataDirectory"] == str(folder / "data")
                      and product["chainDescription"] == str(self.a.fixture / "chain.json"),
                      "EXISTING_NODE_PATH_MISMATCH")
            node = {"label": f"n{index}", "index": index, "base": folder,
                    "cli": folder / "package/bin/bxdl", "control": folder / "control",
                    "port": product["http"]["port"], "p2pPort": product["p2p"]["port"],
                    "product": folder / "instance.json", "native": folder / "node.json",
                    "genesisHash": h.document(folder / "data/engine-instance.json")["genesisHash"]}
            h.require(h.sha(node["cli"]) == CLI_SHA
                      and h.sha(folder / "package/engine/nigo-node.jar") == base.JAR_SHA
                      and h.sha(folder / "package/runtime/bin/java") == base.JAVA_SHA,
                      "INSTALLED_PAYLOAD_CHANGED")
            h.stopped(self.command(node, ["status", "--instance", node["control"]],
                                   node["label"] + "-before-status"), "BEFORE_PROBE")
            self.nodes.append(node)
        sockets = []
        try:
            for port in tuple(range(24320, 24324)) + tuple(range(25320, 25324)):
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sockets.append(sock)
                sock.bind(("127.0.0.1", port))
        finally:
            for sock in sockets:
                sock.close()
        self.e.note("FROZEN_INPUTS_AND_ALL_STOPPED_VERIFIED")

    def submit_once(self, nonce, node, label):
        base.fg.assert_inputs_unchanged(self.a, self.input_pins)
        tx = self.transactions[nonce]
        entry = {"at": utc(), "node": node["label"], "port": node["port"],
                 "nonce": nonce, "transactionHash": tx["hash"], "attempts": 1}
        try:
            response = base.http(node["port"], "/eth-rpc", {
                "jsonrpc": "2.0", "id": label, "method": "eth_sendRawTransaction",
                "params": [tx["rawTransaction"]],
            })
            entry["response"] = response
            if response.get("result") is not None:
                h.require(response["result"].lower() == tx["hash"].lower(),
                          "SUBMISSION_HASH_MISMATCH")
                entry["submission"] = "ACKNOWLEDGED"
            else:
                entry["submission"] = "RPC_ERROR_OBSERVE_EXISTING_HASH"
        except (OSError, TimeoutError) as error:
            entry["submission"] = "UNKNOWN_OBSERVE_EXISTING_HASH"
            entry["transportError"] = type(error).__name__
        self.e.capture(label + ".json", json.dumps(entry, indent=2).encode())
        self.results[label] = {key: value for key, value in entry.items() if key != "response"}
        self.e.note("SUBMITTED_ONCE_" + label)
        return tx["hash"]

    def sample(self, nodes, txhash, phase):
        def one(node):
            value = {"node": node["label"], "at": utc()}
            try:
                value["consensus"] = self.consensus(node)
                value["progress"] = base.http(node["port"], "/monitor/api/consensus/progress")
                value["receiptResponse"] = base.http(node["port"], "/eth-rpc", {
                    "jsonrpc": "2.0", "id": self.samples,
                    "method": "eth_getTransactionReceipt", "params": [txhash]})
            except (OSError, TimeoutError, ValueError) as error:
                value["observationError"] = type(error).__name__
            return value
        with ThreadPoolExecutor(max_workers=len(nodes)) as executor:
            values = list(executor.map(one, nodes))
        record = {"at": utc(), "elapsedSeconds": round(time.monotonic() - self.started_at, 3),
                  "phase": phase, "transactionHash": txhash, "nodes": values}
        self.trace.write(self.e.safe((json.dumps(record) + "\n").encode()).decode())
        self.trace.flush()
        self.samples += 1
        if time.monotonic() >= self.next_notice:
            self.next_notice = time.monotonic() + 15
            summary = [{"node": value["node"],
                        "head": value.get("consensus", {}).get("latestBlockNumber"),
                        "round": value.get("progress", {}).get("round"),
                        "pending": value.get("progress", {}).get("pendingCount"),
                        "roundVotes": value.get("progress", {}).get("roundChangeVotes")}
                       for value in values]
            print(phase, json.dumps(summary), flush=True)
        return record

    def await_finality(self, nodes, txhash, label, allow_historical_missing=False, seconds=90):
        deadline = time.monotonic() + seconds
        last = None
        while time.monotonic() < deadline:
            last = self.sample(nodes, txhash, label)
            values = last["nodes"]
            receipts = [value.get("receiptResponse", {}).get("result") for value in values]
            present = [receipt for receipt in receipts if receipt is not None]
            if present and (allow_historical_missing or len(present) == len(nodes)):
                h.require(all(receipt["status"] == "0x1"
                              and receipt["type"] == "0x2"
                              and receipt["transactionHash"].lower() == txhash.lower()
                              for receipt in present),
                          "TRANSACTION_FAILED_" + label)
                height = int(present[0]["blockNumber"], 16)
                if all(int(value.get("consensus", {}).get("finalizedBlockNumber", -1)) >= height
                       for value in values):
                    canonical = [base.http(node["port"], "/monitor/api/blocks/" + str(height))
                                 for node in nodes]
                    rpc_blocks = [base.rpc(node["port"], "eth_getBlockByNumber",
                                           [hex(height), False]) for node in nodes]
                    expected = canonical[0]
                    h.require(all(block["blockHash"] == expected["blockHash"]
                                  and block["stateRoot"] == expected["stateRoot"]
                                  and int(block["number"]) == height
                                  for block in canonical), "CANONICAL_DIVERGENCE_" + label)
                    h.require(all(block is not None
                                  and int(block["number"], 16) == height
                                  and block["hash"] == expected["blockHash"]
                                  and block["stateRoot"] == expected["stateRoot"]
                                  for block in rpc_blocks), "RPC_BLOCK_DIVERGENCE_" + label)
                    if not allow_historical_missing:
                        h.require(all(txhash.lower() in [value.lower()
                                                         for value in block["transactions"]]
                                      for block in rpc_blocks), "TX_BODY_MISSING_" + label)
                    h.require(all(int(receipt["blockNumber"], 16) == height
                                  and receipt["blockHash"] == expected["blockHash"]
                                  for receipt in present), "RECEIPT_DIVERGENCE_" + label)
                    result = {"height": height, "blockHash": expected["blockHash"],
                              "stateRoot": expected["stateRoot"], "transactionHash": txhash,
                              "nodes": [node["label"] for node in nodes],
                              "receiptPresent": [receipt is not None for receipt in receipts],
                              "historicalMissingAllowed": allow_historical_missing}
                    h.write_json(self.e.logs / (label + "-canonical.json"),
                                 {"result": result, "blocks": canonical,
                                  "rpcBlocks": rpc_blocks, "receipts": receipts})
                    self.results[label] = result
                    self.e.note("FINALITY_" + label, height=height)
                    return result
            time.sleep(0.75)
        self.results[label] = {"outcome": "INCOMPLETE", "budgetSeconds": seconds,
                               "transactionHash": txhash, "lastObservation": last}
        raise h.Failure("FINALITY_TIMEOUT_" + label)

    def run_probe(self):
        self.frozen_inputs()
        good = self.nodes
        with ThreadPoolExecutor(max_workers=4) as executor:
            tasks = [executor.submit(self.start, node, node["label"] + "-baseline-start")
                     for node in good]
            errors = []
            for task in tasks:
                try:
                    task.result()
                except Exception as error:
                    errors.append(error)
            if errors:
                raise errors[0]
        self.sample(good, self.transactions[3]["hash"], "baseline-before-resubmission")
        tx3 = self.submit_once(3, good[0], "baseline-tx3-one-resubmission")
        baseline = self.await_finality(good, tx3, "baseline-tx3",
                                       allow_historical_missing=True)
        h.require(baseline["height"] >= 4, "BASELINE_HEIGHT_NOT_ADVANCED")
        self.stop(good[2], "n3-no-quorum-stop")
        self.stop(good[3], "n4-no-quorum-stop")
        deadline = time.monotonic() + 30
        while True:
            before = self.sample(good[:2], self.transactions[4]["hash"], "before-no-quorum")
            if all(value.get("consensus", {}).get("peerCount") == 1
                   for value in before["nodes"]):
                break
            h.require(time.monotonic() < deadline, "PEER_GRAPH_TIMEOUT")
            time.sleep(0.75)
        tx4 = self.submit_once(4, good[0], "two-of-four-tx4-first-submission")
        held_from = time.monotonic()
        first_rounds = None
        last_rounds = None
        count = 0
        while time.monotonic() - held_from < 45:
            current = self.sample(good[:2], tx4, "two-of-four-45-seconds")
            for value in current["nodes"]:
                h.require("observationError" not in value, "NO_QUORUM_OBSERVATION_UNAVAILABLE")
                consensus = value["consensus"]
                h.require(int(consensus["latestBlockNumber"]) == baseline["height"]
                          and int(consensus["finalizedBlockNumber"]) == baseline["height"]
                          and value["receiptResponse"].get("result") is None,
                          "TWO_OF_FOUR_FINALIZED")
            rounds = [value["progress"]["round"] for value in current["nodes"]]
            first_rounds = first_rounds if first_rounds is not None else rounds
            last_rounds = rounds
            count += 1
            time.sleep(0.75)
        advanced = all(end > start for start, end in zip(first_rounds, last_rounds))
        self.results["twoOfFour"] = {
            "elapsedSeconds": round(time.monotonic() - held_from, 3), "samples": count,
            "height": baseline["height"], "newFinality": False, "roundAdvanced": advanced,
            "initialRounds": first_rounds, "lastRounds": last_rounds, "transactionHash": tx4}
        h.require(advanced, "NO_QUORUM_ROUND_ADVANCE_NOT_OBSERVED")
        self.start(good[3], "n4-quorum-restore")
        restored = [good[0], good[1], good[3]]
        self.sample(restored, tx4, "restored-before-explicit-resubmission")
        self.submit_once(4, good[3], "restored-n4-tx4-one-resubmission")
        self.await_finality(restored, tx4, "restored-quorum-explicit-retry")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", action="store_true")
    for name in ("cli", "fixture", "lock", "source-run", "last-run", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    if not args.run:
        print("No execution. Add --run for this isolated recovery probe.")
        return 0
    os.umask(0o077)
    h.require(platform.system() == "Darwin" and platform.machine() == "arm64",
              "MAC_ARM64_REQUIRED")
    h.require(args.output.is_absolute() and not args.output.exists(), "NEW_OUTPUT_REQUIRED")
    h.require(helpers_unchanged(), "HELPER_SOURCE_CHANGED")
    probe = RecoveryProbe(args)
    error = None
    try:
        probe.run_probe()
    except (Exception, KeyboardInterrupt) as failure:
        error = str(failure) if isinstance(failure, h.Failure) else "HARNESS_" + type(failure).__name__
    finally:
        cleaned = probe.cleanup()
        probe.trace.close()
    preserved = (helpers_unchanged() and probe.archive.exists()
                 and h.sha(probe.archive) == ARCHIVE_SHA)
    probe.results["preservedInputs"] = {"helperSourcesAndArchiveUnchanged": preserved}
    try:
        base.fg.assert_inputs_unchanged(probe.a, probe.input_pins)
    except h.Failure as changed:
        error = error or str(changed)
    if not preserved:
        error = error or "FROZEN_INPUTS_CHANGED"
    final_status = []
    if cleaned:
        for node in probe.nodes:
            try:
                observed = probe.command(node, ["status", "--instance", node["control"]],
                                         node["label"] + "-after-status")
                h.stopped(observed, "AFTER_PROBE")
                final_status.append({"node": node["label"], "stopped": True})
            except Exception as failure:
                final_status.append({"node": node["label"], "stopped": False,
                                     "error": str(failure) if isinstance(failure, h.Failure)
                                     else type(failure).__name__})
                error = error or "FINAL_STOP_STATUS_NOT_VERIFIED"
    probe.results["finalStatus"] = final_status
    result = {"outcome": "PASS" if error is None and cleaned else "INCOMPLETE",
              "reason": error, "scope": "LOCAL_MAC_EXISTING_PACKAGE_QUORUM_RECOVERY_PROBE",
              "results": probe.results, "steps": probe.e.steps,
              "childProcesses": probe.e.children, "observationSamples": probe.samples}
    h.write_json(args.output / "result.json", result)
    print(result["outcome"], error or "", str(args.output), flush=True)
    return 0 if result["outcome"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
