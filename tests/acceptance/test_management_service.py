"""Offline service-harness admission guards; never invokes CLI, Java or launchd."""
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import run_management_service as runner
import support as h


class ServiceHarnessBoundary(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()

    def test_partial_diagnostic_summary_preserves_outcome_and_reason_code(self):
        logs = {"outcome": "INCOMPLETE", "reasonCode": "LOGS_PARTIAL", "code": "WRONG_FIELD"}
        diagnose = {"outcome": "INCOMPLETE", "reasonCode": "DIAGNOSTIC_PARTIAL", "code": "WRONG_FIELD"}
        self.assertEqual(runner.diagnostic_summary(logs, diagnose, "a" * 64), {
            "logsOutcome": "INCOMPLETE", "logsCode": "LOGS_PARTIAL",
            "diagnoseOutcome": "INCOMPLETE", "diagnoseCode": "DIAGNOSTIC_PARTIAL",
            "reportSha256": "a" * 64,
        })

    def test_stable_entry_is_not_resolved_to_payload(self):
        first, second, entry = self.root / "first", self.root / "second", self.root / "bxdl"
        first.write_text("first")
        second.write_text("second")
        entry.symlink_to(first)
        selected = runner.absolute_entry(entry)
        self.assertEqual(selected, entry)
        entry.unlink()
        entry.symlink_to(second)
        self.assertEqual(selected.read_text(), "second")

    def test_no_run_refuses_before_files_or_commands(self):
        output = self.root / "not-created"
        with patch.object(h, "command") as command, patch.object(h, "observe_listeners") as listeners:
            with self.assertRaisesRegex(h.Failure, "EXPLICIT_RUN_REQUIRED"):
                runner.execute(SimpleNamespace(run=False, output=output))
        command.assert_not_called()
        listeners.assert_not_called()
        self.assertFalse(output.exists())

    def test_existing_output_is_preserved_without_commands(self):
        output = self.root / "existing"
        output.mkdir(mode=0o700)
        sentinel = output / "result.json"
        sentinel.write_text("previous result")
        args = SimpleNamespace(run=True, fixture=self.root / "prior", supply=None, inputs=None,
                               instance_id="new-instance", http_port=29680, p2p_port=30680, output=output)
        with patch.object(runner.platform, "system", return_value="Darwin"), \
                patch.object(runner.platform, "machine", return_value="arm64"), \
                patch.object(h, "command") as command, patch.object(h, "observe_listeners") as listeners:
            with self.assertRaisesRegex(h.Failure, "FRESH_OUTPUT_REQUIRED"):
                runner.execute(args)
        command.assert_not_called()
        listeners.assert_not_called()
        self.assertEqual(sentinel.read_text(), "previous result")

    def test_input_remapping_refuses_nested_or_unexpected_reference(self):
        original, copied = self.root / "original", self.root / "copied"
        value = {"secrets": [str(original / "validator.keystore")]}
        self.assertEqual(runner.remap_inputs(value, original, copied),
                         {"secrets": [str(copied / "validator.keystore")]})
        for bad in [original / "nested/validator.keystore", original / "unlisted.key", original / "../escaped.key"]:
            with self.subTest(path=bad):
                with self.assertRaisesRegex(h.Failure, "FIXTURE_REFERENCE_OUTSIDE_INPUTS"):
                    runner.remap_inputs(str(bad), original, copied)

    def test_snapshot_refuses_symlink_instead_of_claiming_unchanged_data(self):
        folder = self.root / "data"
        folder.mkdir()
        target = self.root / "target"
        target.write_text("data")
        (folder / "link").symlink_to(target)
        with self.assertRaisesRegex(h.Failure, "SNAPSHOT_SYMLINK"):
            runner.snapshot(folder)

    def test_supply_pin_mismatch_is_rejected_before_copy_or_command(self):
        fixture = self.root / "fixture"
        supply, inputs = fixture / "supply", fixture / "operational inputs"
        supply.mkdir(parents=True)
        inputs.mkdir()
        for name in runner.SUPPLY:
            (supply / name).write_text("WRONG_PIN_OFFLINE_STUB")
        for name in runner.INPUT_NAMES:
            (inputs / name).write_text("OFFLINE_STUB_NOT_A_CREDENTIAL")
        output = self.root / "new-output"
        output.mkdir()
        args = SimpleNamespace(fixture=fixture, supply=None, inputs=None)
        with patch.object(h, "command") as command:
            with self.assertRaisesRegex(h.Failure, "RC3_SUPPLY_PIN_ARCHIVE"):
                runner.prepare(args, output)
        command.assert_not_called()
        self.assertEqual(list(output.iterdir()), [])
        self.assertEqual((supply / "package.tar.gz").read_text(), "WRONG_PIN_OFFLINE_STUB")


if __name__ == "__main__":
    unittest.main()
