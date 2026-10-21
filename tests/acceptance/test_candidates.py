"""Offline boundary and one-shot recovery regressions; no Java, node, or RPC I/O."""
import copy
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from candidates import CANDIDATES, JAVA_SHA256
import fixture_guard as fg
import run_acceptance as runner
import support as h


class CandidateBoundary(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()

    def write(self, relative, value):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value))
        path.chmod(0o600)
        return path

    def fixture(self, candidate, legacy=False):
        identity = CANDIDATES[candidate]
        nodes, generated, peers = [], [], []
        for index in range(1, 5):
            node = {"index": index, "nodeId": "0x" + str(index) * 64,
                    "validatorId": "0x" + str(index) * 40,
                    "credentialFingerprint": "0x" + str(index + 4) * 64,
                    "httpPort": 28000 + index, "p2pPort": 29000 + index,
                    "product": f"products/n{index}.json"}
            nodes.append(node)
            directory = self.root / "generated" / f"validator-{index}"
            directory.mkdir(parents=True)
            generated.append({**node, "role": "VALIDATOR", "directory": str(directory),
                              "config": str(directory / "application.yml")})
            peers.append({"nodeId": node["nodeId"], "validatorId": node["validatorId"],
                          "host": "127.0.0.1", "port": node["p2pPort"],
                          "transactionPermission": "INGRESS",
                          "credentialFingerprints": [node["credentialFingerprint"]],
                          "revokedCredentialFingerprints": []})
            secrets = {}
            for field, filename in fg.SECRET_FILES.items():
                path = directory / filename
                path.write_text("OFFLINE_STUB_NOT_A_CREDENTIAL")
                path.chmod(0o600)
                secrets[field] = str(path)
            self.write(f"products/n{index}.json", {
                "schemaVersion": 1, "role": "validator", "nodeId": node["nodeId"],
                "http": {"address": "127.0.0.1", "port": node["httpPort"]},
                "p2p": {"address": "127.0.0.1", "port": node["p2pPort"]},
                "chainDescription": str(self.root / "chain.json"), "secrets": secrets,
            })
        chain = {"nigo.protocol.chain-id": "11578",
                 "nigo.protocol.consensus.profile-id": "DEV_QBFT",
                 "nigo.protocol.consensus.protocol": "QBFT"}
        for index, node in enumerate(nodes):
            chain[f"nigo.protocol.consensus.qbft.validators[{index}]"] = node["validatorId"]
        self.write("chain.json", chain)
        self.write("network.json", {"schemaVersion": 1, "kind": "BXDL_QBFT_NETWORK", "nodes": peers})
        self.write("generated/devnet.json", {"chainId": 11578, "profileId": "DEV_QBFT",
                                            "transportSecurityScheme": "MTLS", "nodes": generated})
        transactions = []
        for nonce in range(identity["transactionCount"]):
            relative = f"transactions/tx-{nonce}.json"
            txhash = "0x" + f"{nonce + 1:064x}"
            self.write(relative, {"schemaVersion": 1, "nonce": nonce, "chainId": 11578,
                                  "type": 2, "hash": txhash, "rawTransaction": "0x02abcd"})
            transactions.append({"nonce": nonce, "hash": txhash, "file": relative})
        summary = {"schemaVersion": 1, "status": "PREPARED_NOT_EXECUTED",
                   "purpose": "EPHEMERAL_LOCAL_BXDL_PACKAGE_ACCEPTANCE_ONLY",
                   "engineSource": identity["source"], "engineJarSha256": identity["jarSha256"],
                   "javaSha256": JAVA_SHA256, "nodes": nodes, "transactions": transactions,
                   "chainSha256": h.sha(self.root / "chain.json"),
                   "networkSha256": h.sha(self.root / "network.json")}
        if not legacy:
            summary.update(candidate=candidate, engineJarSizeBytes=identity["jarSizeBytes"])
        self.write("fixture-summary.json", summary)
        self.write("lock.json", {"jarSha256": identity["jarSha256"],
                                 "jarSizeBytes": identity["jarSizeBytes"], "javaSha256": JAVA_SHA256,
                                 "expected": {"source": {"commit": identity["source"]}}})
        args = SimpleNamespace(fixture=self.root, lock=self.root / "lock.json", revision="1" * 40)
        if not legacy:
            args.candidate = candidate
        return args

    def test_rc2_validates_all_twelve_transactions_and_candidate_pin(self):
        args = self.fixture("rc2")
        pins = fg.validate_inputs(args)
        self.assertEqual(pins["candidate"], "rc2")
        self.assertEqual(len(pins["transactionFiles"]), 12)
        fg.assert_inputs_unchanged(args, pins)

    def test_original_rc1_eight_transaction_fixture_still_supported(self):
        args = self.fixture("rc1", legacy=True)
        pins = fg.validate_inputs(args)
        self.assertEqual(pins["candidate"], "rc1")
        self.assertEqual(len(pins["transactionFiles"]), 8)

    def test_rc1_fixture_cannot_be_relabelled_as_rc2(self):
        args = self.fixture("rc1")
        args.candidate = "rc2"
        with self.assertRaisesRegex(h.Failure, "PREPARATION_IDENTITY"):
            fg.validate_inputs(args)

    def test_rc2_without_explicit_candidate_is_not_inferred(self):
        args = self.fixture("rc2")
        del args.candidate
        with self.assertRaisesRegex(h.Failure, "PREPARATION_IDENTITY"):
            fg.validate_inputs(args)

    def test_rc1_lock_cannot_be_mixed_with_rc2_fixture(self):
        args = self.fixture("rc2")
        lock = h.document(args.lock)
        lock["jarSha256"] = CANDIDATES["rc1"]["jarSha256"]
        self.write("lock.json", lock)
        with self.assertRaisesRegex(h.Failure, "LOCK_ARTIFACT"):
            fg.validate_inputs(args)

    def test_same_jar_but_wrong_source_or_size_is_rejected(self):
        args = self.fixture("rc2")
        original = h.document(args.lock)
        for changed in ("source", "size"):
            with self.subTest(changed=changed):
                lock = copy.deepcopy(original)
                if changed == "source":
                    lock["expected"]["source"]["commit"] = "0" * 40
                else:
                    lock["jarSizeBytes"] += 1
                self.write("lock.json", lock)
                with self.assertRaisesRegex(h.Failure, "LOCK_ARTIFACT"):
                    fg.validate_inputs(args)

    def test_unused_reserved_transaction_is_also_required(self):
        args = self.fixture("rc2")
        (self.root / "transactions/tx-11.json").unlink()
        with self.assertRaisesRegex(h.Failure, "INVALID_INPUT"):
            fg.validate_inputs(args)

    def test_post_validation_transaction_change_is_detected(self):
        args = self.fixture("rc2")
        pins = fg.validate_inputs(args)
        changed = h.document(self.root / "transactions/tx-11.json")
        changed["rawTransaction"] = "0x02ef12"
        self.write("transactions/tx-11.json", changed)
        with self.assertRaisesRegex(h.Failure, "INPUT_SET_CHANGED"):
            fg.assert_inputs_unchanged(args, pins)

    def test_rc2_never_enters_legacy_continuation(self):
        with self.assertRaisesRegex(h.Failure, "LEGACY_RC1_ONLY"):
            fg.validate_legacy_continuation(SimpleNamespace(candidate="rc2"), {}, {})


class SubmissionBoundary(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.run = runner.Acceptance.__new__(runner.Acceptance)
        self.run.a = SimpleNamespace()
        self.run.input_pins = {}
        self.run.e = h.Evidence(self.root)
        self.run.results = {"submissionAttempts": []}
        self.run.submissions = {}
        self.run.recovery_resubmit_nonce = None
        self.tx = {"hash": "0x" + "a" * 64, "rawTransaction": "0x02abcd"}
        self.run.transactions = [self.tx]
        guard = patch.object(fg, "assert_inputs_unchanged")
        guard.start()
        self.addCleanup(guard.stop)

    def test_duplicate_submit_is_rejected_before_any_second_rpc(self):
        with patch.object(runner, "rpc", return_value=self.tx["hash"]) as rpc:
            self.run.submit(0, 28000)
            with self.assertRaisesRegex(h.Failure, "SUBMISSION_NOT_AUTHORIZED"):
                self.run.submit(0, 28000)
        self.assertEqual(rpc.call_count, 1)

    def test_lost_response_keeps_one_attempt_and_original_hash(self):
        with patch.object(runner, "rpc", side_effect=TimeoutError) as rpc:
            self.assertEqual(self.run.submit(0, 28000), self.tx["hash"])
            with self.assertRaisesRegex(h.Failure, "SUBMISSION_NOT_AUTHORIZED"):
                self.run.submit(0, 28000)
        self.assertEqual(rpc.call_count, 1)
        self.assertEqual(self.run.results["submissionAttempts"][0]["submission"],
                         "UNKNOWN_OBSERVE_ORIGINAL_HASH")
        self.assertTrue((self.run.e.logs / "tx-0-submission-1.json").exists())

    def test_explicit_restart_allows_only_one_second_dispatch_of_same_bytes(self):
        with patch.object(runner, "rpc", return_value=self.tx["hash"]) as rpc:
            self.run.submit(0, 28000)
            self.run.recovery_resubmit_nonce = 0
            self.run.submit(0, 28000, explicit_resubmission=True)
            with self.assertRaisesRegex(h.Failure, "SUBMISSION_NOT_AUTHORIZED"):
                self.run.submit(0, 28000, explicit_resubmission=True)
        self.assertEqual(rpc.call_count, 2)
        self.assertEqual(rpc.call_args_list[0], rpc.call_args_list[1])
        self.assertEqual(self.run.submissions, {0: 2})
        self.assertTrue((self.run.e.logs / "tx-0-submission-2.json").exists())

    def test_explicit_flag_alone_does_not_authorize_resubmission(self):
        with patch.object(runner, "rpc", return_value=self.tx["hash"]) as rpc:
            self.run.submit(0, 28000)
            with self.assertRaisesRegex(h.Failure, "SUBMISSION_NOT_AUTHORIZED"):
                self.run.submit(0, 28000, explicit_resubmission=True)
        self.assertEqual(rpc.call_count, 1)

    def test_resubmission_rpc_error_does_not_trigger_a_third_attempt(self):
        with patch.object(runner, "rpc", side_effect=[self.tx["hash"],
                h.Failure("RPC_ERROR_eth_sendRawTransaction")]) as rpc:
            self.run.submit(0, 28000)
            self.run.recovery_resubmit_nonce = 0
            self.assertEqual(self.run.submit(0, 28000, explicit_resubmission=True), self.tx["hash"])
        self.assertEqual(rpc.call_count, 2)
        self.assertEqual(self.run.results["submissionAttempts"][1]["submission"],
                         "RPC_ERROR_OBSERVE_ORIGINAL_HASH")


class SameDbRecoveryBoundary(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.run = runner.Acceptance.__new__(runner.Acceptance)
        self.run.candidate_name = "rc2"
        self.run.e = h.Evidence(self.root)
        self.run.results = {}
        self.run.submissions = {1: 1}
        self.run.recovery_resubmit_nonce = None
        self.run.transactions = [{"hash": str(n)} for n in range(3)]
        self.nodes = []
        for index in range(1, 5):
            base = self.root / f"n{index}"
            data = base / "data"
            data.mkdir(parents=True)
            (data / "engine-instance.json").write_text('"fixed-engine-identity"')
            (base / "product.json").write_text('"fixed-product"')
            (base / "native.json").write_text('"fixed-native"')
            self.nodes.append({"label": f"n{index}", "base": base, "control": base / "control",
                               "port": index, "lastAttempt": "old-attempt", "nodeInstanceId": "old-process",
                               "product": base / "product.json", "native": base / "native.json"})
        self.run.diagnostic_sample = Mock(return_value=self.sample(4, 0))
        self.run.observe_quorum_loss = Mock(return_value=("1", self.sample(2, 2)))
        self.run.stop = Mock()
        self.run.command = Mock(return_value={"outcome": "SUCCEEDED", "data": {
            "reason": "SERVICE_STOPPED_VERIFIED", "engineState": "STOPPED", "operationBusy": False}})
        self.run.start_all = Mock(side_effect=self.restart)
        self.run.submit = Mock(side_effect=self.submit)
        self.run.finality = Mock()

    @staticmethod
    def sample(count, view_round):
        return {"nodes": [{"consensus": {"viewHeight": 2, "viewRound": view_round}}
                          for _ in range(count)]}

    def restart(self, nodes, label):
        for node in nodes:
            node.update(lastAttempt="new-attempt", nodeInstanceId="new-process")

    def submit(self, nonce, port, explicit_resubmission=False):
        if explicit_resubmission:
            self.assertEqual(self.run.recovery_resubmit_nonce, nonce)
            self.run.submissions[nonce] += 1
        return str(nonce)

    def test_restart_requires_four_stops_new_processes_same_data_and_two_finalities(self):
        self.run.same_db_recovery(self.nodes, 1, 2)
        self.assertEqual(self.run.command.call_count, 4)
        self.assertEqual(self.run.stop.call_count, 4)
        self.assertEqual(self.run.submit.call_args_list[0].kwargs, {"explicit_resubmission": True})
        self.assertEqual([call.args[1] for call in self.run.finality.call_args_list], ["1", "2"])
        self.assertEqual(self.run.results["sameDbRestartRecovery"]["totalSubmissionAttemptsForPendingHash"], 2)
        self.assertFalse(self.run.results["sameDbRestartRecovery"]["rc1ToRc2UpgradeTested"])
        self.assertIsNone(self.run.recovery_resubmit_nonce)

    def test_no_round_skew_prevents_restart_and_resubmission(self):
        self.run.observe_quorum_loss.return_value = ("1", self.sample(2, 0))
        with self.assertRaisesRegex(h.Failure, "RESTART_ROUND_SKEW_NOT_CONFIRMED"):
            self.run.same_db_recovery(self.nodes, 1, 2)
        self.run.start_all.assert_not_called()
        self.run.submit.assert_not_called()

    def test_unverified_stop_prevents_restart_and_resubmission(self):
        self.run.command.return_value["data"]["operationBusy"] = True
        with self.assertRaisesRegex(h.Failure, "ALL_STOPPED_NOT_STOPPED"):
            self.run.same_db_recovery(self.nodes, 1, 2)
        self.run.start_all.assert_not_called()
        self.run.submit.assert_not_called()

    def test_changed_engine_journal_prevents_any_resubmission(self):
        def changed(nodes, label):
            self.restart(nodes, label)
            (nodes[0]["base"] / "data/engine-instance.json").write_text('"replaced-identity"')
        self.run.start_all.side_effect = changed
        with self.assertRaisesRegex(h.Failure, "RESTART_DATA_OR_CONFIG_REPLACED"):
            self.run.same_db_recovery(self.nodes, 1, 2)
        self.run.submit.assert_not_called()

    def test_failed_pending_recovery_never_submits_followup(self):
        self.run.finality.side_effect = h.Failure("FINALITY_TIMEOUT_same-db-restart-recovered")
        with self.assertRaisesRegex(h.Failure, "FINALITY_TIMEOUT"):
            self.run.same_db_recovery(self.nodes, 1, 2)
        self.assertEqual(self.run.submit.call_count, 1)
        self.assertNotIn("sameDbRestartRecovery", self.run.results)
        self.assertIsNone(self.run.recovery_resubmit_nonce)


if __name__ == "__main__":
    unittest.main()
