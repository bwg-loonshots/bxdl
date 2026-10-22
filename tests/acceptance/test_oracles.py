"""Offline regressions for false-positive acceptance and uncertain init cleanup."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import run_acceptance as runner
import support as h


class AcceptanceOracles(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.run = runner.Acceptance.__new__(runner.Acceptance)
        self.run.e = h.Evidence(Path(self.temporary.name))
        self.run.results = {}
        self.run.running = set()
        self.run.unresolved_mutations = {}
        self.run.nodes = [{"label": "n1", "port": 1}]
        self.run.fixture = {"nodes": [{"httpPort": 28080 + i, "p2pPort": 29080 + i} for i in range(4)]}
        listener = patch.object(h, "observe_listeners", return_value={"outcome": "PASS", "scope": "TCP_LISTEN", "listeners": []})
        self.listener = listener.start()
        self.addCleanup(listener.stop)
        self.tx = "0x" + "1" * 64
        self.block_hash = "0x" + "2" * 64
        self.root = "0x" + "3" * 64
        self.receipt = {
            "transactionHash": self.tx, "status": "0x1", "type": "0x2",
            "blockNumber": "0x4", "blockHash": self.block_hash,
        }
        self.rpc_block = {
            "number": "0x4", "hash": self.block_hash, "stateRoot": self.root,
            "transactions": [self.tx],
        }
        self.monitor_block = {
            "number": 4, "blockHash": self.block_hash, "stateRoot": self.root,
        }
        self.expected = {
            "height": 4, "transactionHash": self.tx,
            "blockHash": self.block_hash, "stateRoot": self.root,
        }
        self.run.receipt = lambda node, tx: copy.deepcopy(self.receipt)
        self.run.consensus = lambda node: {"finalizedBlockNumber": 4}

    def finality(self):
        with patch.object(runner, "rpc", return_value=self.rpc_block), \
                patch.object(runner, "http", return_value=self.monitor_block):
            return self.run.finality(self.run.nodes, self.tx, "test")

    def test_matching_receipt_and_both_block_views_are_required(self):
        result = self.finality()
        self.assertTrue(result["receiptIdentityAndBlockMembershipVerified"])
        proof = json.loads((self.run.e.logs / "test-transaction-proof.json").read_text())
        self.assertEqual(proof["receipts"][0]["transactionHash"], self.tx)

    def test_same_successful_receipt_for_another_transaction_is_rejected(self):
        self.receipt["transactionHash"] = "0x" + "4" * 64
        with self.assertRaisesRegex(h.Failure, "RECEIPT_IDENTITY"):
            self.finality()

    def test_agreeing_hashes_do_not_hide_a_wrong_rpc_block_height(self):
        self.rpc_block["number"] = "0x3"
        with self.assertRaisesRegex(h.Failure, "BLOCK_DIVERGENCE"):
            self.finality()

    def test_receipt_without_transaction_in_the_block_is_rejected(self):
        self.rpc_block["transactions"] = []
        with self.assertRaisesRegex(h.Failure, "BLOCK_DIVERGENCE"):
            self.finality()

    def test_monitor_height_must_match_the_requested_height(self):
        self.monitor_block["number"] = 3
        with self.assertRaisesRegex(h.Failure, "CANONICAL_DIVERGENCE"):
            self.finality()

    def test_snapshot_can_lack_old_receipt_when_header_and_state_match(self):
        self.receipt = None
        with patch.object(runner, "http", return_value=self.monitor_block):
            result = self.run.canonical_at(self.run.nodes, self.expected, "snapshot")
        self.assertEqual(result["historicalReceiptPresent"], [False])

    def test_snapshot_does_not_accept_an_incorrect_present_receipt(self):
        self.receipt["status"] = "0x0"
        with patch.object(runner, "http", return_value=self.monitor_block), \
                self.assertRaisesRegex(h.Failure, "RECEIPT_IDENTITY"):
            self.run.canonical_at(self.run.nodes, self.expected, "snapshot")

    def test_timeout_preserves_evidence_without_submitting_or_starting(self):
        with self.assertRaisesRegex(h.Failure, "FINALITY_TIMEOUT"):
            self.run.finality(self.run.nodes, self.tx, "expired", seconds=0)
        self.assertEqual(json.loads((self.run.e.logs / "expired-timeout.json").read_text()), [])
        self.assertEqual(self.run.e.children, [])

    def test_uncertain_init_is_not_reported_as_successful_cleanup(self):
        self.run.unresolved_mutations["n1"] = {
            "node": "n1", "phase": "INIT", "evidenceStage": "n1-init",
        }
        self.run.e.children.append({"stage": "n1-init", "pid": 12345})
        self.run.stop = lambda *args: self.fail("init uncertainty must not trigger a service stop")
        self.assertFalse(self.run.cleanup())
        self.assertEqual(self.run.results["cleanup"]["remaining"], ["n1"])
        self.assertEqual(self.run.results["cleanup"]["errors"][0]["childPids"], [12345])

    def test_clean_bookkeeping_requires_independent_listener_pass(self):
        self.assertTrue(self.run.cleanup())
        self.assertEqual(self.run.results["cleanup"], {"remaining": [], "errors": []})
        self.listener.assert_called_once_with(list(range(28080, 28084)) + list(range(29080, 29084)))
        self.assertEqual(self.run.results["cleanupListeners"]["outcome"], "PASS")

    def test_listener_found_or_unknown_never_means_clean(self):
        for outcome in ("FAIL", "UNKNOWN"):
            with self.subTest(outcome=outcome):
                self.listener.return_value = {"outcome": outcome, "scope": "TCP_LISTEN", "listeners": []}
                self.assertFalse(self.run.cleanup())
                self.assertEqual(self.run.results["cleanup"], {"remaining": [], "errors": []})
                self.assertEqual(self.run.results["cleanupListeners"]["outcome"], outcome)

    def test_listener_exception_preserves_unknown_without_repair(self):
        self.listener.side_effect = OSError("unavailable")
        self.assertFalse(self.run.cleanup())
        self.assertEqual(self.run.results["cleanupListeners"]["outcome"], "UNKNOWN")
        self.assertEqual(self.listener.call_count, 1)

    def test_listener_pass_cannot_hide_unverified_service_stop(self):
        self.run.running.add("n1")
        self.run.stop = lambda *args: (_ for _ in ()).throw(h.Failure("STOP_UNKNOWN"))
        self.assertFalse(self.run.cleanup())
        self.assertEqual(self.run.results["cleanup"]["remaining"], ["n1"])
        self.assertEqual(self.run.results["cleanup"]["errors"][0]["error"], "STOP_UNKNOWN")
        self.assertEqual(self.run.results["cleanupListeners"]["outcome"], "PASS")


if __name__ == "__main__":
    unittest.main()
