#!/usr/bin/env python3
"""Explicit, test-owned macOS BXDL package acceptance; never a customer installer."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import copy
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import socket
import subprocess
import time
import urllib.request

import support as h
import fixture_guard as fg
from candidates import CANDIDATES, candidate_for

JAR_SHA = "f1ea80e25052221ed8a6e3108e020cb1b776833f4f86a29aae0885ad41703f45"
JAVA_SHA = "9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c"
MENU = "선택 [save] > "
HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def http(port, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=data,
                                 headers={"Content-Type": "application/json"})
    with HTTP.open(req, timeout=5) as response:
        raw = response.read(2 * 1024 * 1024 + 1)
    h.require(len(raw) <= 2 * 1024 * 1024, "HTTP_RESPONSE_LIMIT")
    return json.loads(raw)


def rpc(port, method, params):
    response = http(port, "/eth-rpc",
                    {"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    h.require("error" not in response, "RPC_ERROR_" + method)
    return response.get("result")


class Acceptance:
    def __init__(self, args):
        for name, value in vars(args).items():
            if isinstance(value, Path):
                expanded = value.expanduser()
                normalized = (expanded.parent.resolve(strict=True) / expanded.name
                              if name == "output" else expanded.resolve(strict=True))
                setattr(args, name, normalized)
        self.a = args
        self.candidate = candidate_for(args)
        self.candidate_name = getattr(args, "candidate", "rc1")
        self.scenario = getattr(args, "scenario", "full")
        h.require(self.scenario == "full" or self.candidate_name == "rc2", "RC2_RECOVERY_CASE_ONLY")
        self.input_pins = fg.validate_inputs(args)
        self.root = args.output
        self.root.mkdir(mode=0o700)
        self.e = h.Evidence(self.root)
        self.cli = self.root / "bxdl"
        h.copy_file(args.cli, self.cli, 0o755)
        self.archive = self.root / "candidate.tar.gz"
        self.public = self.root / "test-package-public.pem"
        self.lock = self.root / "engine-lock.json"
        h.copy_file(args.lock, self.lock, 0o600)
        self.nodes = []
        self.running = set()
        self.unresolved_mutations = {}
        self.round = 0
        self.submissions = {}
        self.recovery_resubmit_nonce = None
        self.results = {"fixtureInputPins": self.input_pins,
                        "candidate": self.candidate_name, "scenario": self.scenario,
                        "submissionAttempts": []}
        Acceptance.freeze_harness(self)
        self.fixture = h.document(args.fixture / "fixture-summary.json")
        self.manifest = h.document(args.fixture / "generated/devnet.json")
        self.transactions = [h.document(args.fixture / f"transactions/tx-{i}.json")
                             for i in range(self.candidate["transactionCount"])]
        for tx in self.transactions:
            self.e.redactions.append(tx["rawTransaction"].encode())

    def freeze_harness(self):
        source = Path(__file__).resolve().parent
        names = ("run_acceptance.py", "support.py", "fixture_guard.py",
                 "prepare_fixture.py", "candidates.py")
        self.harness_pins = {name: h.sha(source / name) for name in names}
        snapshot = self.e.logs / "harness-source"
        snapshot.mkdir(mode=0o700)
        for name, digest in self.harness_pins.items():
            h.copy_file(source / name, snapshot / name, 0o600)
            h.require(h.sha(snapshot / name) == digest, "HARNESS_SNAPSHOT_CHANGED")
        self.results["harnessSources"] = self.harness_pins

    def harness_unchanged(self):
        source = Path(__file__).resolve().parent
        return all(h.sha(source / name) == digest for name, digest in self.harness_pins.items())

    def check_free_ports(self):
        reservations = []
        try:
            for node in self.fixture["nodes"]:
                for field in ("httpPort", "p2pPort"):
                    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                    reservations.append(sock)
                    sock.bind(("127.0.0.1", node[field]))
        finally:
            for sock in reservations:
                sock.close()

    def start_all(self, nodes, label):
        # Wait for every launch attempt before cleanup, even if one launch fails.
        with ThreadPoolExecutor(max_workers=len(nodes)) as executor:
            tasks = [executor.submit(self.start, node, node["label"] + "-" + label)
                     for node in nodes]
            failures = []
            for task in tasks:
                try:
                    task.result()
                except Exception as error:
                    failures.append(error)
            if failures:
                raise failures[0]

    def command(self, node, args, label, expected=(0,), timeout=700):
        cli = self.cli if node is None else node["cli"]
        return h.command(cli, args, self.e, label, expected, timeout)

    def crypto(self, args, label):
        result = subprocess.run([str(a) for a in args], stdin=subprocess.DEVNULL,
                                capture_output=True, timeout=60)
        self.e.capture(label + ".log", result.stdout + result.stderr)
        h.require(result.returncode == 0, "CRYPTO_" + label)

    def package(self):
        payload = self.root / "payload"
        payload.mkdir(mode=0o700)
        for name in ("engine", "runtime", "docs", "licenses"):
            h.copy_tree(self.a.payload_source / name, payload / name)
        (payload / "bin").mkdir(mode=0o700)
        h.copy_file(self.cli, payload / "bin/bxdl", 0o755)
        lock = h.document(self.lock)
        h.require(lock["jarSha256"] == h.sha(payload / "engine/nigo-node.jar") == self.candidate["jarSha256"]
                  and (payload / "engine/nigo-node.jar").stat().st_size
                  == self.candidate["jarSizeBytes"], "ENGINE_PIN")
        h.require(lock["javaSha256"] == h.sha(payload / "runtime/bin/java") == JAVA_SHA,
                  "JAVA_PIN")
        runtime = h.document(payload / "docs/runtime-provenance.json")
        paths = {str(p.relative_to(payload / "runtime"))
                 for p in (payload / "runtime").rglob("*") if not p.is_dir()}
        h.require(paths == {f["path"] for f in runtime["files"]}, "RUNTIME_INVENTORY")
        for item in runtime["files"]:
            path = payload / "runtime" / item["path"]
            st = h.regular(path)
            h.require(st.st_size == item["size"] and st.st_mode & 0o777 == item["mode"]
                      and h.sha(path) == item["sha256"], "RUNTIME_FILE")
        spec = h.document(self.a.spec)
        spec["product"]["revision"] = self.a.revision
        spec["files"] = []
        specpath = self.root / "spec.json"
        h.write_json(specpath, spec)
        private = self.root / "test-package-private.pem"
        self.crypto([self.a.openssl, "genpkey", "-algorithm", "ED25519", "-out", private],
                    "package-test-key")
        self.crypto([self.a.openssl, "pkey", "-in", private, "-pubout", "-out", self.public],
                    "package-test-public")
        built = self.command(None, ["package", "build", "--root", payload, "--spec", specpath,
                                    "--output", self.archive, "--signing-key", private],
                             "package-build")
        h.require(built["outcome"] == "SUCCEEDED", "PACKAGE_BUILD")
        verified = self.command(None, ["package", "verify", self.archive, "--public-key", self.public],
                                "package-verify")
        h.require(verified["data"]["authenticity"] == "verified-external-ed25519", "PACKAGE_SIGNATURE")
        self.results["package"] = {"archiveSha256": h.sha(self.archive),
                                  "manifestSha256": verified["data"]["manifestSha256"],
                                  "cliSha256": h.sha(self.cli),
                                  "candidate": self.candidate_name,
                                  "engineSource": self.candidate["source"],
                                  "engineJarSha256": self.candidate["jarSha256"],
                                  "engineJarSizeBytes": self.candidate["jarSizeBytes"],
                                  "javaSha256": JAVA_SHA, "runtimeFilesVerified": len(runtime["files"]),
                                  "revision": self.a.revision, "testOnlySigningKey": True}
        self.e.note("SIGNED_PACKAGE_FROZEN")

    def prepare_node(self, index, bad=False):
        fg.assert_inputs_unchanged(self.a, self.input_pins)
        label = "wrong-pin" if bad else f"n{index}"
        base = self.root / label
        base.mkdir(mode=0o700)
        product = copy.deepcopy(h.document(self.a.fixture / f"products/n{index}.json"))
        product["instanceId"] = "qbft-" + label + "-" + self.root.name[-8:].replace("_", "-")
        product["storage"]["dataDirectory"] = str(base / "data")
        productpath = base / "instance.json"
        h.write_json(productpath, product)
        network = self.a.fixture / "network.json"
        if bad:
            values = copy.deepcopy(h.document(network))
            for i, peer in enumerate(values["nodes"]):
                if peer["nodeId"] != product["nodeId"]:
                    peer["credentialFingerprints"] = [
                        "0x" + hashlib.sha256(f"wrong-expected-peer-{i}".encode()).hexdigest()]
            network = base / "wrong-network.json"
            h.write_json(network, values)
        native = base / "node.json"
        terminal = h.Terminal(self.cli, ["setup", "--network", "--workspace", base / "network-session",
                                        "--from", productpath, "--network-file", network,
                                        "--output", native], self.root, self.e,
                              label + "-network", 180)
        try:
            terminal.answer("TLS keystore 형식", "PKCS12")
            terminal.answer("TLS truststore 형식", "PKCS12")
            terminal.answer("네트워크 시작 방식", "new-network" if bad else "join-network")
            terminal.answer("엔진 실행 후 합의 참여 시작", "yes")
            terminal.answer("블록 아카이브 사용", "yes")
            terminal.expect(MENU)
            terminal.send("export")
            terminal.expect("[y/N] > ")
            terminal.send("yes")
            terminal.finish("SETUP_NETWORK_WRITTEN")
        finally:
            terminal.close()
        package = base / "package"
        installed = self.command(None, ["install", self.archive, "--destination", package,
                                       "--public-key", self.public], label + "-install")
        h.require(installed["outcome"] == "SUCCEEDED", "INSTALL_" + label)
        cli = package / "bin/bxdl"
        h.require(h.sha(cli) == h.sha(self.cli), "INSTALLED_CLI_" + label)
        node = {"label": label, "index": index, "base": base, "cli": cli,
                "control": base / "control", "port": product["http"]["port"],
                "p2pPort": product["p2p"]["port"], "product": productpath, "native": native}
        self.nodes.append(node)
        registered = self.command(node, ["instance", "register", "--instance", node["control"],
                                        "--package", package, "--archive", self.archive,
                                        "--public-key", self.public, "--config", productpath,
                                        "--engine-config", native, "--lock", self.lock,
                                        "--allow-development", "--timeout-seconds", "120"],
                                  label + "-register")
        h.require(registered["data"]["initialization"] == "NOT_STARTED", "REGISTER_" + label)
        h.require(not (base / "data").exists(), "REGISTER_CREATED_DATA")
        self.unresolved_mutations[label] = {"node": label, "phase": "INIT",
                                           "evidenceStage": label + "-init"}
        initialized = self.command(node, ["init", "--instance", node["control"],
                                         "--confirm-initialize", "--timeout-seconds", "180"],
                                   label + "-init")
        h.require(initialized["outcome"] == "SUCCEEDED", "INIT_" + label)
        identity = h.document(base / "data/engine-instance.json")
        node["genesisHash"] = identity["genesisHash"]
        del self.unresolved_mutations[label]
        self.e.note("PREPARED_" + label)
        return node

    def start(self, node, label):
        fg.assert_inputs_unchanged(self.a, self.input_pins)
        self.running.add(node["label"])
        original = self.command(node, ["start", "--instance", node["control"],
                                      "--timeout-seconds", "180"], label, (0, 4, 5, 6))
        attempt = original.get("data", {}).get("attemptId")
        h.require(attempt, "START_NO_ATTEMPT_" + label)
        if original.get("data", {}).get("runtimeReadiness") != "READY":
            observed = self.command(node, ["status", "--instance", node["control"], "--wait",
                                          "--timeout-seconds", "180"], label + "-observe",
                                    (0, 4, 5, 6))
        else:
            observed = original
        h.require(observed.get("data", {}).get("attemptId") == attempt, "ATTEMPT_CHANGED")
        h.running(observed, label)
        h.require(observed["data"]["runtimeReadiness"] == "READY", "LOCAL_NOT_READY_" + label)
        node["lastAttempt"] = attempt
        node["nodeInstanceId"] = observed["data"]["nodeInstanceId"]
        h.require(h.document(node["base"] / "data/engine-instance.json")["genesisHash"]
                  == node["genesisHash"], "GENESIS_CHANGED")
        self.e.note("READY_" + label)

    def stop(self, node, label):
        result = self.command(node, ["stop", "--instance", node["control"],
                                    "--timeout-seconds", "180"], label, (0, 4, 5, 6))
        h.stopped(result, label)
        self.running.discard(node["label"])
        self.e.note("STOPPED_" + label)

    def submit(self, nonce, port, explicit_resubmission=False):
        fg.assert_inputs_unchanged(self.a, self.input_pins)
        tx = self.transactions[nonce]
        expected = tx["hash"]
        previous = self.submissions.get(nonce, 0)
        h.require((previous == 0 and not explicit_resubmission)
                  or (previous == 1 and explicit_resubmission
                      and self.recovery_resubmit_nonce == nonce), "SUBMISSION_NOT_AUTHORIZED")
        attempt = previous + 1
        # Record a dispatch before I/O. An unknown response never becomes a new
        # automatic attempt. Only the separate same-DB scenario authorizes #2.
        self.submissions[nonce] = attempt
        response = {"expectedHash": expected, "nonce": nonce, "attempt": attempt,
                    "port": port, "unixTimeNs": time.time_ns(),
                    "explicitSameDbRestartResubmission": explicit_resubmission}
        try:
            received = rpc(port, "eth_sendRawTransaction", [tx["rawTransaction"]])
            h.require(isinstance(received, str) and received.lower() == expected.lower(),
                      "SUBMIT_HASH_MISMATCH")
            response["submission"] = "ACKNOWLEDGED"
        except (OSError, TimeoutError):
            response["submission"] = "UNKNOWN_OBSERVE_ORIGINAL_HASH"
        except h.Failure as error:
            response["submission"] = "RPC_ERROR_OBSERVE_ORIGINAL_HASH"
            if not explicit_resubmission or str(error) != "RPC_ERROR_eth_sendRawTransaction":
                raise
        finally:
            h.write_json(self.e.logs / f"tx-{nonce}-submission-{attempt}.json", response)
            self.results["submissionAttempts"].append(response)
        return expected

    def receipt(self, node, txhash):
        return rpc(node["port"], "eth_getTransactionReceipt", [txhash])

    def consensus(self, node):
        return http(node["port"], "/monitor/api/consensus")

    @staticmethod
    def check_receipt(receipt, txhash, height, block_hash, label):
        h.require(receipt["status"] == "0x1" and receipt["type"] == "0x2"
                  and receipt["transactionHash"].lower() == txhash.lower()
                  and int(receipt["blockNumber"], 16) == height
                  and receipt["blockHash"] == block_hash,
                  "RECEIPT_IDENTITY_" + label)

    def diagnostic_sample(self, nodes, txhash):
        sample = {"unixTimeNs": time.time_ns(), "nodes": []}
        for node in nodes:
            item = {"label": node["label"]}
            for field, reader, keys in (
                ("consensus", lambda: self.consensus(node),
                 ("latestBlockNumber", "finalizedBlockNumber", "peerCount", "viewHeight",
                  "viewRound", "txPoolSize")),
                ("progress", lambda: http(node["port"], "/monitor/api/consensus/progress"),
                 ("observedAtEpochMillis", "status", "pendingCount", "height", "round",
                  "step", "prepareVotes", "commitVotes", "roundChangeVotes", "quorumSize",
                  "currentViewProposalCount", "unfinalizedProposal", "executionStatus")),
                ("receipt", lambda: self.receipt(node, txhash),
                 ("transactionHash", "status", "type", "blockNumber", "blockHash")),
            ):
                try:
                    value = reader()
                    item[field] = None if value is None else {key: value.get(key) for key in keys}
                except Exception as error:
                    item[field] = {"observationError": type(error).__name__}
            sample["nodes"].append(item)
        return sample

    def finality(self, nodes, txhash, label, seconds=90):
        deadline = time.monotonic() + seconds
        observations = []
        while time.monotonic() < deadline:
            receipts = [self.receipt(n, txhash) for n in nodes]
            if all(r is not None for r in receipts):
                h.require(all(r["status"] == "0x1" for r in receipts), "TX_FAILED_" + label)
                height = receipts[0]["blockNumber"]
                if not all(int(self.consensus(n)["finalizedBlockNumber"]) >= int(height, 16)
                           for n in nodes):
                    observations.append(self.diagnostic_sample(nodes, txhash))
                    observations = observations[-90:]
                    time.sleep(1)
                    continue
                blocks = [rpc(n["port"], "eth_getBlockByNumber", [height, False]) for n in nodes]
                if all(b is not None for b in blocks):
                    block = blocks[0]
                    for receipt in receipts:
                        self.check_receipt(receipt, txhash, int(height, 16), block["hash"], label)
                    h.require(all(int(b["number"], 16) == int(height, 16)
                                  and txhash.lower() in [v.lower() for v in b["transactions"]]
                                  and b["hash"] == block["hash"] and b["stateRoot"] == block["stateRoot"]
                                  for b in blocks), "BLOCK_DIVERGENCE_" + label)
                    canonical = [http(n["port"], "/monitor/api/blocks/" + str(int(height, 16)))
                                 for n in nodes]
                    h.require(all(int(b["number"]) == int(height, 16)
                                  and b["blockHash"] == block["hash"] and b["stateRoot"] == block["stateRoot"]
                                  for b in canonical), "CANONICAL_DIVERGENCE_" + label)
                    last = {"height": int(height, 16), "blockHash": block["hash"],
                            "stateRoot": block["stateRoot"], "transactionHash": txhash,
                            "nodes": [n["label"] for n in nodes],
                            "receiptIdentityAndBlockMembershipVerified": True}
                    h.write_json(self.e.logs / (label + "-transaction-proof.json"),
                                 {"transactionHash": txhash, "receipts": receipts,
                                  "rpcBlocks": blocks, "monitorBlocks": canonical})
                    h.write_json(self.e.logs / (label + "-finality.json"), last)
                    h.write_json(self.e.logs / (label + "-observations.json"), observations)
                    self.results[label] = last
                    self.e.note("FINALITY_" + label, height=last["height"])
                    return last
            # Bounded, private observations distinguish pending-transaction and
            # consensus-round recovery. They do not alter the node or retry a tx.
            observations.append(self.diagnostic_sample(nodes, txhash))
            observations = observations[-90:]
            time.sleep(1)
        h.write_json(self.e.logs / (label + "-timeout.json"), observations)
        raise h.Failure("FINALITY_TIMEOUT_" + label)

    def canonical_at(self, nodes, expected, label, seconds=90):
        """Snapshot catch-up preserves headers/state, not historical receipts."""
        height = expected["height"]
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if all(int(self.consensus(n)["finalizedBlockNumber"]) >= height for n in nodes):
                blocks = [http(n["port"], "/monitor/api/blocks/" + str(height)) for n in nodes]
                h.require(all(int(b["number"]) == height
                              and b["blockHash"] == expected["blockHash"]
                              and b["stateRoot"] == expected["stateRoot"] for b in blocks),
                          "CATCHUP_CANONICAL_DIVERGENCE")
                receipts = [self.receipt(n, expected["transactionHash"]) for n in nodes]
                for receipt in receipts:
                    if receipt is not None:
                        self.check_receipt(receipt, expected["transactionHash"], height,
                                           expected["blockHash"], label)
                result = {**expected, "nodes": [n["label"] for n in nodes],
                          "historicalReceiptPresent": [r is not None for r in receipts],
                          "historicalReceiptRequiredByThisCase": False,
                          "contract": "SNAPSHOT_INSTALLS_HEADER_AND_STATE_WITHOUT_HISTORICAL_BODY"}
                h.write_json(self.e.logs / (label + "-canonical.json"), result)
                self.results[label] = result
                self.e.note("CANONICAL_CATCHUP_" + label, height=height)
                return result
            time.sleep(0.5)
        raise h.Failure("CANONICAL_CATCHUP_TIMEOUT_" + label)

    def run(self):
        self.check_free_ports()
        self.package()
        good = [self.prepare_node(i) for i in range(1, 5)]
        h.require(len({n["genesisHash"] for n in good}) == 1, "GENESIS_DIVERGENCE")
        self.start_all(good, "first-start")
        tx0 = self.submit(0, good[0]["port"])
        self.finality(good, tx0, "four-validator")
        if self.scenario == "same-db-recovery":
            self.same_db_recovery(good, 1, 2)
            return
        first_attempt = good[3]["lastAttempt"]
        first_instance = good[3]["nodeInstanceId"]
        self.stop(good[3], "n4-before-catchup-stop")
        tx1 = self.submit(1, good[0]["port"])
        behind = self.finality(good[:3], tx1, "three-validator-while-n4-stopped")
        self.start(good[3], "n4-same-db-restart")
        h.require(good[3]["lastAttempt"] != first_attempt
                  and good[3]["nodeInstanceId"] != first_instance, "RESTART_IDENTITY_REUSED")
        caught = self.canonical_at(good, behind, "same-db-catchup")
        h.require(caught["blockHash"] == behind["blockHash"], "CATCHUP_DIVERGENCE")
        tx2 = self.submit(2, good[0]["port"])
        self.finality(good, tx2, "after-rejoin")
        self.two_of_four(good)
        if self.candidate_name == "rc2":
            self.same_db_recovery(good, 5, 6)
            self.wrong_pin(good, nonce=7)
        else:
            self.wrong_pin(good)

    def continue_existing(self):
        """Continue only the preserved, fully stopped receipt-expectation run."""
        prior = self.a.continue_from
        old = h.document(prior / "result.json")
        fg.validate_legacy_continuation(self.a, old, self.input_pins)
        h.require(old["outcome"] == "INCOMPLETE"
                  and old["reason"] == "FINALITY_TIMEOUT_same-db-catchup"
                  and old["results"]["cleanup"] == {"remaining": [], "errors": []},
                  "PRIOR_RUN_NOT_STOPPED_EXPECTED_CASE")
        self.archive = prior / "candidate.tar.gz"
        self.public = prior / "test-package-public.pem"
        self.results = copy.deepcopy(old["results"])
        self.results["fixtureInputPins"] = self.input_pins
        self.results["submissionAttempts"] = []
        self.results["harnessSources"] = self.harness_pins
        self.results.pop("cleanup", None)
        h.require(h.sha(self.archive) == self.results["package"]["archiveSha256"]
                  and h.sha(self.cli) == self.results["package"]["cliSha256"],
                  "CONTINUATION_PACKAGE_CHANGED")
        self.command(None, ["package", "verify", self.archive, "--public-key", self.public],
                     "continuation-package-verify")
        self.results["continuation"] = {
            "priorResultSha256": h.sha(prior / "result.json"),
            "priorOutcome": old["outcome"], "priorReason": old["reason"],
            "reason": "RC1_DOCUMENTED_HEADER_ONLY_SNAPSHOT_CONTRACT",
            "sameArchiveAndControlAndData": True,
        }
        good = []
        for index in range(1, 5):
            base = prior / f"n{index}"
            product = h.document(base / "instance.json")
            node = {"label": f"n{index}", "index": index, "base": base,
                    "cli": base / "package/bin/bxdl", "control": base / "control",
                    "port": product["http"]["port"], "p2pPort": product["p2p"]["port"],
                    "product": base / "instance.json", "native": base / "node.json",
                    "genesisHash": h.document(base / "data/engine-instance.json")["genesisHash"]}
            h.stopped(self.command(node, ["status", "--instance", node["control"]],
                                   f"n{index}-continuation-before-status"), "CONTINUATION")
            self.nodes.append(node)
            good.append(node)
        with ThreadPoolExecutor(max_workers=4) as executor:
            tasks = [executor.submit(self.start, n, n["label"] + "-continuation-start") for n in good]
            errors = []
            for task in tasks:
                try:
                    task.result()
                except Exception as error:
                    errors.append(error)
            if errors:
                raise errors[0]
        self.canonical_at(good, old["results"]["three-validator-while-n4-stopped"],
                          "same-db-catchup")
        tx2 = self.submit(2, good[0]["port"])
        self.finality(good, tx2, "after-rejoin")
        self.two_of_four(good)
        self.wrong_pin(good)

    def observe_quorum_loss(self, good, nonce, label, result_key):
        # Call only after n3/n4 have been normally stopped. Empty blocks are off.
        stable_deadline = time.monotonic() + 30
        while True:
            snapshots = [self.consensus(n) for n in good[:2]]
            baseline = [int(s["finalizedBlockNumber"]) for s in snapshots]
            if len(set(baseline)) == 1 and all(int(s["peerCount"]) == 1 for s in snapshots):
                break
            h.require(time.monotonic() < stable_deadline, "NO_QUORUM_BASELINE_TIMEOUT")
            time.sleep(0.5)
        blocks = [http(n["port"], "/monitor/api/blocks/" + str(baseline[0])) for n in good[:2]]
        h.require(len({(b["blockHash"], b["stateRoot"]) for b in blocks}) == 1,
                  "NO_QUORUM_BASELINE_DIVERGENCE")
        txhash = self.submit(nonce, good[0]["port"])
        observations = []
        held_from = time.monotonic()
        deadline = held_from + 45
        initial_rounds = None
        round_changed = False
        completed = False
        try:
            while time.monotonic() < deadline:
                sample = self.diagnostic_sample(good[:2], txhash)
                observations.append(sample)
                snapshots = [item["consensus"] for item in sample["nodes"]]
                receipts = [item["receipt"] for item in sample["nodes"]]
                h.require(all("observationError" not in item["consensus"]
                              and "observationError" not in item["progress"]
                              and (item["receipt"] is None or "observationError" not in item["receipt"])
                              for item in sample["nodes"]), "NO_QUORUM_OBSERVATION_UNAVAILABLE")
                heads = [int(s["latestBlockNumber"]) for s in snapshots]
                finalized = [int(s["finalizedBlockNumber"]) for s in snapshots]
                h.require(heads == baseline and finalized == baseline and all(r is None for r in receipts),
                          "TWO_OF_FOUR_FINALIZED")
                rounds = [(s.get("viewHeight"), s.get("viewRound")) for s in snapshots]
                if all(r[0] is not None and r[1] is not None for r in rounds):
                    if initial_rounds is None:
                        initial_rounds = rounds
                    elif all(r[0] == old[0] and int(r[1]) > int(old[1])
                             for r, old in zip(rounds, initial_rounds)):
                        round_changed = True
                time.sleep(0.5)
            h.require(round_changed, "NO_QUORUM_ROUND_ADVANCE_NOT_OBSERVED")
            completed = True
        finally:
            h.write_json(self.e.logs / (label + ".json"),
                         {"seconds": 45, "elapsedSeconds": time.monotonic() - held_from,
                          "transactionHash": txhash, "nonce": nonce, "baseline": baseline,
                          "completed": completed, "observations": observations})
            self.results[result_key] = {"seconds": 45, "completed": completed,
                                       "transactionHash": txhash, "nonce": nonce,
                                       "newValidTransactionSubmitted": True,
                                       "newFinality": False if completed else "NOT_CONFIRMED",
                                       "roundAdvanced": round_changed, "samples": len(observations)}
        return txhash, observations[-1]

    def two_of_four(self, good):
        self.stop(good[2], "n3-no-quorum-stop")
        self.stop(good[3], "n4-no-quorum-stop")
        tx3, _ = self.observe_quorum_loss(good, 3, "two-of-four", "twoOfFour")
        self.start(good[3], "n4-quorum-restore")
        restored = self.finality([good[0], good[1], good[3]], tx3, "restored-quorum")
        self.start(good[2], "n3-quorum-restore")
        self.canonical_at(good, restored, "four-restored")
        tx4 = self.submit(4, good[0]["port"])
        self.finality(good, tx4, "after-four-restored")

    def same_db_recovery(self, good, pending_nonce, followup_nonce):
        h.require(self.candidate_name == "rc2", "RC2_RECOVERY_CASE_ONLY")
        # A new, independent skew period follows a fully confirmed 4-node tx.
        # There is no carry-over timeout adoption and no data/reset/re-register.
        original = {node["label"]: {"attemptId": node["lastAttempt"],
                                    "nodeInstanceId": node["nodeInstanceId"]} for node in good}
        before = self.diagnostic_sample(good, self.transactions[pending_nonce]["hash"])
        h.write_json(self.e.logs / "restart-recovery-before-skew.json", before)
        self.stop(good[2], "n3-restart-skew-stop")
        self.stop(good[3], "n4-restart-skew-stop")
        txhash, after = self.observe_quorum_loss(good, pending_nonce,
                                                "restart-two-of-four", "restartTwoOfFour")
        stopped_views = [(item["consensus"].get("viewHeight"),
                          item["consensus"].get("viewRound")) for item in before["nodes"][2:]]
        active_views = [(item["consensus"].get("viewHeight"),
                         item["consensus"].get("viewRound")) for item in after["nodes"]]
        h.require(all(height is not None and round_number is not None
                      for height, round_number in stopped_views + active_views)
                  and len({height for height, _ in stopped_views + active_views}) == 1
                  and min(int(value) for _, value in active_views)
                  > max(int(value) for _, value in stopped_views), "RESTART_ROUND_SKEW_NOT_CONFIRMED")
        self.results["restartRoundSkew"] = {"stoppedViews": stopped_views,
                                             "activeViews": active_views, "confirmed": True}
        for node in good[:2]:
            self.stop(node, node["label"] + "-before-all-restart-stop")
        preserved = {}
        for node in good:
            h.stopped(self.command(node, ["status", "--instance", node["control"]],
                                   node["label"] + "-before-all-restart-status"), "ALL_STOPPED")
            data = node["base"] / "data"
            metadata = data.stat()
            preserved[node["label"]] = {"dataDirectory": str(data),
                                        "device": metadata.st_dev, "inode": metadata.st_ino,
                                        "engineIdentitySha256": h.sha(data / "engine-instance.json"),
                                        "productSha256": h.sha(node["product"]),
                                        "nativeSha256": h.sha(node["native"])}
        h.write_json(self.e.logs / "restart-data-preservation-before.json", preserved)
        self.start_all(good, "same-db-skew-restart")
        for node in good:
            previous = original[node["label"]]
            h.require(node["lastAttempt"] != previous["attemptId"]
                      and node["nodeInstanceId"] != previous["nodeInstanceId"], "RESTART_IDENTITY_REUSED")
            data = node["base"] / "data"
            metadata = data.stat()
            pin = preserved[node["label"]]
            h.require((metadata.st_dev, metadata.st_ino) == (pin["device"], pin["inode"])
                      and h.sha(data / "engine-instance.json") == pin["engineIdentitySha256"]
                      and h.sha(node["product"]) == pin["productSha256"]
                      and h.sha(node["native"]) == pin["nativeSha256"], "RESTART_DATA_OR_CONFIG_REPLACED")
        h.write_json(self.e.logs / "restart-before-explicit-resubmission.json",
                     self.diagnostic_sample(good, txhash))
        self.recovery_resubmit_nonce = pending_nonce
        try:
            repeated = self.submit(pending_nonce, good[0]["port"], explicit_resubmission=True)
        finally:
            self.recovery_resubmit_nonce = None
        h.require(repeated == txhash, "RESUBMITTED_HASH_CHANGED")
        self.finality(good, txhash, "same-db-restart-recovered")
        followup = self.submit(followup_nonce, good[0]["port"])
        self.finality(good, followup, "same-db-restart-followup")
        self.results["sameDbRestartRecovery"] = {
            "outcome": "PASS", "sameFourDataDirectories": True,
            "dataIdentityAndConfigsUnchanged": True, "newLaunchAttempts": True,
            "pendingNonce": pending_nonce, "followupNonce": followup_nonce,
            "pendingTransactionHash": txhash, "explicitResubmissionCount": 1,
            "totalSubmissionAttemptsForPendingHash": self.submissions[pending_nonce],
            "databaseOrRoundReset": False, "engineBytesReplaced": False,
            "rc1ToRc2UpgradeTested": False,
        }

    def wrong_pin(self, good, nonce=5):
        # Smallest node ID initiates all three edges, making wrong expected
        # server pins an explicit outbound WRONG_PIN rather than an inbound
        # manifest-authorization failure.
        original = min(good, key=lambda n: h.document(n["product"])["nodeId"])
        healthy = [n for n in good if n is not original]
        self.stop(original, original["label"] + "-before-wrong-pin")
        bad = self.prepare_node(original["index"], bad=True)
        self.running.add(bad["label"])
        observed = self.command(bad, ["start", "--instance", bad["control"],
                                     "--timeout-seconds", "60"], "wrong-pin-start", (0, 4, 5, 6))
        h.require(observed.get("data", {}).get("attemptId"), "WRONG_PIN_NO_START_ATTEMPT")
        deadline = time.monotonic() + 45
        while True:
            try:
                snapshot = self.consensus(bad)
            except (OSError, TimeoutError):
                h.require(time.monotonic() < deadline, "WRONG_PIN_OBSERVATION_UNAVAILABLE")
                time.sleep(0.5)
                continue
            audit = (snapshot.get("peerSecurity") or {}).get("audit") or {}
            reasons = audit.get("reasonCounts") or []
            count = sum(int(r["count"]) for r in reasons if r.get("reason") == "WRONG_PIN")
            matching_events = [event for event in (audit.get("recentEvents") or [])
                               if event.get("reason") == "WRONG_PIN"
                               and event.get("direction") == "OUTBOUND"
                               and event.get("stage") == "CREDENTIAL_PIN"]
            if count > 0 and matching_events and snapshot.get("peerCount") == 0:
                break
            h.require(time.monotonic() < deadline, "WRONG_PIN_NOT_OBSERVED")
            time.sleep(0.5)
        h.write_json(self.e.logs / "wrong-pin-consensus.json", snapshot)
        tx5 = self.submit(nonce, healthy[0]["port"])
        self.finality(healthy, tx5, "healthy-quorum-during-wrong-pin")
        self.results["wrongPin"] = {"outboundWrongPinCount": count,
                                    "faultNodePeerCount": int(snapshot["peerCount"]),
                                    "healthyValidatorCount": 3,
                                    "faultNodeSyncEnabled": False,
                                    "duplicateSignerRunning": False}
        self.stop(bad, "wrong-pin-stop")

    def cleanup(self):
        errors = []
        for node in reversed(self.nodes):
            if node["label"] not in self.running:
                continue
            try:
                self.stop(node, node["label"] + "-final-stop")
            except Exception as error:
                errors.append({"node": node["label"], "error": str(error)
                               if isinstance(error, h.Failure) else type(error).__name__})
        for pending in self.unresolved_mutations.values():
            children = [entry["pid"] for entry in self.e.children
                        if entry["stage"] == pending["evidenceStage"]]
            errors.append({**pending, "error": "MUTATION_OUTCOME_UNRESOLVED",
                           "childPids": children})
        remaining = sorted(self.running | set(self.unresolved_mutations))
        self.results["cleanup"] = {"remaining": remaining, "errors": errors}
        return not errors and not remaining


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--run", action="store_true")
    p.add_argument("--candidate", choices=tuple(CANDIDATES), required=True)
    p.add_argument("--scenario", choices=("full", "same-db-recovery"), default="full")
    for name in ("cli", "fixture", "payload-source", "lock", "spec", "openssl", "output"):
        p.add_argument("--" + name, type=Path, required=True)
    p.add_argument("--revision", required=True)
    p.add_argument("--continue-from", type=Path)
    a = p.parse_args()
    if not a.run:
        print("No execution. Add --run for isolated test-owned package acceptance.")
        return 0
    os.umask(0o077)
    h.require(platform.system() == "Darwin" and platform.machine() == "arm64", "MAC_ARM64_REQUIRED")
    h.require(re.fullmatch(r"[0-9a-f]{40}", a.revision) is not None, "FULL_REVISION_REQUIRED")
    h.require(a.output.is_absolute() and not a.output.exists(), "NEW_ABSOLUTE_OUTPUT_REQUIRED")
    h.require(not a.continue_from or (a.candidate == "rc1" and a.scenario == "full"),
              "LEGACY_CONTINUATION_RC1_FULL_ONLY")
    acceptance = Acceptance(a)
    error = None
    try:
        if a.continue_from:
            acceptance.continue_existing()
        else:
            acceptance.run()
    except (Exception, KeyboardInterrupt) as exc:
        error = str(exc) if isinstance(exc, h.Failure) else "HARNESS_" + type(exc).__name__
    finally:
        cleaned = acceptance.cleanup()
    try:
        fg.assert_inputs_unchanged(acceptance.a, acceptance.input_pins)
    except h.Failure as changed:
        error = error or str(changed)
        acceptance.results["inputGuardFinalError"] = str(changed)
    preserved_harness = acceptance.harness_unchanged()
    acceptance.results["harnessSourcesUnchanged"] = preserved_harness
    if not preserved_harness:
        error = error or "HARNESS_SOURCE_CHANGED_DURING_RUN"
    result = {"outcome": "PASS" if error is None and cleaned else "INCOMPLETE",
              "reason": error, "results": acceptance.results,
              "scope": "LOCAL_MAC_FOUR_VALIDATOR_PACKAGE_ACCEPTANCE", "steps": acceptance.e.steps,
              "childProcesses": acceptance.e.children}
    h.write_json(a.output / "result.json", result)
    print(result["outcome"], error or "", str(a.output), flush=True)
    return 0 if result["outcome"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
