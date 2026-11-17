"""Offline service-harness admission guards; never invokes CLI, Java or launchd."""
import copy
from pathlib import Path
import stat
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

    def test_complete_diagnostics_require_private_init_collection_and_terminal_event(self):
        response = {"outcome": "SUCCEEDED", "data": {"partial": False,
                    "sources": [{"name": "initialization.report", "result": "COLLECTED",
                                 "reason": "PRIVATE_SNAPSHOT"},
                                {"name": "service.stderr", "result": "WITHHELD",
                                 "reason": "RAW_TEXT_EXCLUDED_BY_POLICY"}],
                    "events": [{"source": "initialization.report", "command": "init",
                                "status": "INITIALIZED"}]}}
        observed = runner.diagnostic_collection(response, True)
        self.assertFalse(observed["partial"])
        self.assertEqual(len(observed["initializationSources"]), 1)
        self.assertEqual(observed["initializationEvents"][0]["status"], "INITIALIZED")
        changes = [
            lambda x: x.update(outcome="INCOMPLETE"),
            lambda x: x["data"].update(partial=True),
            lambda x: x["data"].update(sources=[]),
            lambda x: x["data"]["sources"].append({"name": "initialization.report", "result": "PARTIAL"}),
            lambda x: x["data"]["sources"][0].update(result="UNAVAILABLE"),
            lambda x: x["data"].update(events=[]),
            lambda x: x["data"]["events"][0].update(status="CHECKING"),
        ]
        for index, change in enumerate(changes):
            with self.subTest(change=index):
                changed = copy.deepcopy(response)
                change(changed)
                with self.assertRaises(h.Failure):
                    runner.diagnostic_collection(changed, True)
                # Baseline collection preserves incomplete or missing evidence.
                runner.diagnostic_collection(changed)

    def test_report_observation_does_not_repair_nonprivate_mode_or_change_bytes(self):
        report = self.root / "report.jsonl"
        report.write_bytes(b'{"fixture":"not-engine-evidence"}\n')
        report.chmod(0o644)
        before = report.stat()
        observed = runner.report_observation(report)
        after = report.stat()
        self.assertEqual(observed["mode"], "0644")
        self.assertEqual(observed["sha256"], h.sha(report))
        self.assertEqual((before.st_dev, before.st_ino, before.st_mode, before.st_mtime_ns, before.st_ctime_ns),
                         (after.st_dev, after.st_ino, after.st_mode, after.st_mtime_ns, after.st_ctime_ns))
        self.assertEqual(stat.S_IMODE(after.st_mode), 0o644)
        alias = self.root / "alias"
        alias.symlink_to(report)
        with self.assertRaisesRegex(h.Failure, "INPUT_NOT_REGULAR"):
            runner.report_observation(alias)

    def test_main_records_requested_umask_before_execute_and_restores_on_failure(self):
        args = SimpleNamespace(run=True, caller_umask="022")
        with patch.object(runner, "parser") as parser, \
                patch.object(runner.os, "umask", side_effect=[0o002, 0o077, 0o022]) as mask, \
                patch.object(runner, "execute", side_effect=h.Failure("TEST_FAILURE")) as execute:
            parser.return_value.parse_args.return_value = args
            with self.assertRaisesRegex(h.Failure, "TEST_FAILURE"):
                runner.main([])
        self.assertEqual([call.args[0] for call in mask.call_args_list], [0o077, 0o022, 0o002])
        self.assertEqual(args.caller_umask_observation["inherited"], "002")
        self.assertEqual(args.caller_umask_observation["applied"], "022")
        execute.assert_called_once_with(args)

    def test_main_records_inherited_umask_without_forcing_private_baseline(self):
        args = SimpleNamespace(run=True, caller_umask=None)
        with patch.object(runner, "parser") as parser, \
                patch.object(runner.os, "umask", side_effect=[0o022, 0o077, 0o022]) as mask, \
                patch.object(runner, "execute", return_value=0):
            parser.return_value.parse_args.return_value = args
            self.assertEqual(runner.main([]), 0)
        self.assertEqual([call.args[0] for call in mask.call_args_list], [0o077, 0o022, 0o022])
        self.assertIsNone(args.caller_umask_observation["requested"])
        self.assertEqual(args.caller_umask_observation["applied"], "022")

    def test_main_without_run_does_not_measure_or_change_umask(self):
        args = SimpleNamespace(run=False, caller_umask="077")
        with patch.object(runner, "parser") as parser, \
                patch.object(runner.os, "umask") as mask, \
                patch.object(runner, "execute", side_effect=h.Failure("EXPLICIT_RUN_REQUIRED")):
            parser.return_value.parse_args.return_value = args
            with self.assertRaisesRegex(h.Failure, "EXPLICIT_RUN_REQUIRED"):
                runner.main([])
        mask.assert_not_called()

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


    def test_unknown_candidate_is_rejected_before_files_or_commands(self):
        output = self.root / "not-created"
        with patch.object(h, "command") as command, patch.object(h, "observe_listeners") as listeners, \
                self.assertRaisesRegex(h.Failure, "RUNTIME_PROFILE_NOT_SUPPORTED"):
            runner.execute(SimpleNamespace(run=True, candidate="unknown", output=output))
        command.assert_not_called()
        listeners.assert_not_called()
        self.assertFalse(output.exists())

    def test_rc4_cannot_reuse_legacy_inputs_without_preparation_provenance(self):
        fixture = self.root / "fixture"
        (fixture / "supply").mkdir(parents=True)
        (fixture / "operational inputs").mkdir()
        output = self.root / "output"
        output.mkdir()
        args = SimpleNamespace(candidate="rc4", fixture=fixture, supply=None, inputs=None)
        with patch.object(h, "copy_file") as copy_file, patch.object(h, "command") as command, \
                self.assertRaises((h.Failure, FileNotFoundError)):
            runner.prepare(args, output)
        copy_file.assert_not_called()
        command.assert_not_called()
        self.assertEqual(list(output.iterdir()), [])

    def test_cli_candidate_default_and_explicit_rc4(self):
        required = ["--manager-entry", "/offline/bxdl", "--cli-root", "/offline/root",
                    "--manager-sha256", "a" * 64, "--output", "/offline/out", "--instance-id", "manual-node",
                    "--http-port", "29680", "--p2p-port", "30680"]
        self.assertEqual(runner.parser().parse_args(required).candidate, "rc3")
        self.assertEqual(runner.parser().parse_args(required + ["--candidate", "rc4"]).candidate, "rc4")



class SetupRecoveryBoundaries(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()

    def detail(self, revision, instance_id, prepared, next_input=None):
        return {"inspection": {"summary": {"planRevision": revision,
                "preparation": "PREPARED_RECORDED" if prepared else "DRAFT",
                "installation": "NOT_PERFORMED"},
                "verification": "VERIFIED" if prepared else "NOT_PREPARED",
                "fields": [{"name": "instanceId", "value": instance_id}],
                "nextInput": next_input, "missingInputs": ["trustPolicy"] if next_input else []},
                "application": None}

    def test_draft_id_is_opt_in_valid_distinct_and_does_not_change_legacy(self):
        self.assertEqual(runner.draft_instance_id(SimpleNamespace(instance_id="node-1")), "node-1")
        args = SimpleNamespace(instance_id="node-1", exercise_setup_recovery=True)
        draft = runner.draft_instance_id(args)
        self.assertNotEqual(draft, args.instance_id)
        self.assertRegex(draft, r"^[a-z][a-z0-9-]{0,31}$")
        for value in ("Uppercase", "a" * 33, "../outside"):
            with self.subTest(value=value), self.assertRaisesRegex(h.Failure, "RECOVERY_INSTANCE_ID_INVALID"):
                runner.draft_instance_id(SimpleNamespace(instance_id=value, exercise_setup_recovery=True))

    def test_cancel_dto_requires_archive_saved_and_trust_next_without_review(self):
        valid = self.detail(0, "draft-node", False, "trustPolicy")
        runner.preparation_state(valid, 0, "draft-node", False, "trustPolicy")
        changes = [lambda d: d["inspection"].update(nextInput="archive"),
                   lambda d: d["inspection"]["missingInputs"].append("archive"),
                   lambda d: d["inspection"].update(verification="VERIFIED"),
                   lambda d: d["inspection"]["summary"].update(planRevision=1),
                   lambda d: d.update(application={"record": "READABLE"})]
        for change in changes:
            changed = copy.deepcopy(valid)
            change(changed)
            with self.assertRaises(h.Failure):
                runner.preparation_state(changed, 0, "draft-node", False, "trustPolicy")

    def test_edit_dto_requires_invalidated_review_without_rewinding_revision(self):
        valid = self.detail(1, "final-node", False)
        runner.preparation_state(valid, 1, "final-node", False)
        for field, value in (("planRevision", 0), ("preparation", "PREPARED_RECORDED")):
            changed = copy.deepcopy(valid)
            changed["inspection"]["summary"][field] = value
            with self.assertRaises(h.Failure):
                runner.preparation_state(changed, 1, "final-node", False)

    def test_actual_flow_cancels_then_resumes_at_trust_and_edits_one_field(self):
        args = SimpleNamespace(instance_id="final-node", exercise_setup_recovery=True)
        home, inputs, supply = self.root / "home", self.root / "inputs", self.root / "supply"
        generated = home / "sessions/local/generated"
        generated.mkdir(parents=True)
        calls = []
        initial_product = {"instanceId": runner.draft_instance_id(args), "unchanged": "public-config"}
        native = {"backend": "rocksdb", "node": {"manual": True}}

        class Terminal:
            def __init__(self, words, label):
                self.label = label
                self.answers = []
                calls.append((list(words), label, self.answers))

            def answer(self, prompt, answer):
                self.answers.append((prompt, answer))

            def finish(self, reason, code=0):
                self.answers.append(("FINISH", (reason, code)))
                if self.label in ("resume-trust", "resume-edited-review"):
                    revision = 1 if self.label == "resume-trust" else 2
                    product = dict(initial_product)
                    if revision == 2:
                        product["instanceId"] = args.instance_id
                    h.write_json(generated / f"instance-{revision:04d}.json", product)
                    h.write_json(generated / f"node-{revision:04d}.json", native)

        details = [self.detail(0, runner.draft_instance_id(args), False, "trustPolicy"),
                   self.detail(1, runner.draft_instance_id(args), True),
                   self.detail(1, args.instance_id, False), self.detail(2, args.instance_id, True)]
        with patch.object(h, "command") as command:
            result, notes = {}, []
            runner.prepare_session(args, Terminal, lambda action: ["setup", action, "local"],
                                   lambda label: details.pop(0), home, inputs, supply, result, notes.append)
        command.assert_not_called()
        self.assertEqual(calls[0][2][2], ("패키지 검증 방법", ":cancel"))
        self.assertEqual(calls[0][2][-1][1][1], 5)
        self.assertEqual(calls[1][0], ["setup", "resume", "local"])
        self.assertEqual(calls[1][2][0], ("패키지 검증 방법", "1"))
        self.assertFalse(any(prompt == "패키지 archive 파일" for prompt, _ in calls[1][2]))
        self.assertEqual(calls[2][0][-2:], ["--field", "instanceId"])
        self.assertEqual(calls[2][2][0][1], args.instance_id)
        self.assertEqual(result["setupRecovery"]["finalRevision"], 2)
        self.assertTrue(result["setupRecovery"]["onlyInstanceIdChanged"])
        self.assertEqual(result["setupRecovery"]["firstRevisionSnapshots"],
                         result["setupRecovery"]["afterResumeOldRevisionSnapshots"])

    def test_old_generated_bytes_or_inode_replacement_is_rejected(self):
        home = self.root / "home"
        generated = home / "sessions/local/generated"
        generated.mkdir(parents=True)
        for name in ("instance", "node"):
            h.write_json(generated / f"{name}-0001.json", {"public": "original"})
        before = runner.revision_files(home, 1)
        path = generated / "instance-0001.json"
        old = path.read_bytes()
        path.unlink()
        h.write(path, old)
        with self.assertRaisesRegex(h.Failure, "PREVIOUS_GENERATED_REVISION_CHANGED"):
            runner.require_old_revision_unchanged(before)

    def test_retained_binding_requires_owned_trust_and_this_runs_original_supply(self):
        control, supply = self.root / "control", self.root / "supply"
        (control / "trust").mkdir(parents=True)
        binding = {"schemaVersion": 2, "supply": {}}
        fields = {"archive": "package.tar.gz", "publicKey": "public-key.pem", "lock": "engine-lock.json"}
        expected = {}
        for field, name in fields.items():
            path = control / "trust" / name
            h.write(path, ("PUBLIC_STUB_" + name).encode())
            expected[name] = h.sha(path)
            binding[field] = str(path)
            binding["supply"][field] = {"path": str(supply / name), "sha256": expected[name]}
        h.write_json(control / "binding.json", binding)
        self.assertEqual(runner.retained_supply(control, supply, expected)["filesVerified"], 3)
        binding["supply"]["archive"]["path"] = str(self.root / "external/package.tar.gz")
        (control / "binding.json").write_text(__import__("json").dumps(binding))
        with self.assertRaisesRegex(h.Failure, "MANAGED_SUPPLY_IDENTITY"):
            runner.retained_supply(control, supply, expected)

    def test_managed_show_needs_retained_binding_verification_not_source_recheck(self):
        detail = {"inspection": {"verification": "NOT_CHECKED", "summary": {"preparation": "PREPARED_RECORDED"}},
                  "application": {"record": "READABLE", "nameBinding": "CONNECTED", "runtime": "NOT_OBSERVED",
                  "summary": {"sourceMatch": "MATCH", "verification": "PASSED", "registrationConfirmed": True,
                              "installation": "CONFIRMED", "registration": "CONFIRMED", "preparationRevision": 2}}}
        runner.require_managed_application(detail, 2)
        for field, value in (("verification", "FAILED"), ("sourceMatch", "UNAVAILABLE"),
                             ("registrationConfirmed", False), ("preparationRevision", 1)):
            bad = copy.deepcopy(detail)
            bad["application"]["summary"][field] = value
            with self.assertRaisesRegex(h.Failure, "MANAGED_APPLICATION_NOT_VERIFIED"):
                runner.require_managed_application(bad, 2)


class OwnedSupplyRecovery(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        supply = self.root / "supply"
        supply.mkdir(mode=0o700)
        self.expected = {}
        for name in runner.SUPPLY:
            h.write(supply / name, ("OFFLINE_PUBLIC_SUPPLY_" + name).encode())
            self.expected[name] = h.sha(supply / name)
        self.relocation = runner.OwnedSupplyAbsence(self.root, self.expected)

    def test_hide_restore_preserves_three_files_and_never_touches_external_source(self):
        external = self.root / "original-fixture"
        external.mkdir()
        (external / "sentinel").write_text("preserve")
        before = {name: (self.root / "supply" / name).stat().st_ino for name in runner.SUPPLY}
        self.relocation.hide()
        self.relocation.require_absent()
        self.assertEqual(list((self.root / "supply").iterdir()), [])
        result = {"outcome": "INCOMPLETE"}
        runner.restore_owned_supply(self.relocation, result)
        self.assertEqual(result["supplyRestoration"]["outcome"], "PASS")
        self.assertEqual(before, {name: (self.root / "supply" / name).stat().st_ino for name in runner.SUPPLY})
        self.assertFalse((self.root / "withheld supply").exists())
        self.assertEqual((external / "sentinel").read_text(), "preserve")

    def test_partial_move_failure_is_restored_without_hiding_primary_failure(self):
        rename = runner.os.rename
        calls = 0

        def fail_second(source, target):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise OSError("OFFLINE_INJECTED_RENAME_FAILURE")
            rename(source, target)

        with patch.object(runner.os, "rename", side_effect=fail_second), self.assertRaises(OSError):
            self.relocation.hide()
        result = {"outcome": "INCOMPLETE", "failure": "PRIMARY_TEST_FAILURE"}
        runner.restore_owned_supply(self.relocation, result)
        self.assertEqual(result["failure"], "PRIMARY_TEST_FAILURE")
        self.assertEqual(result["outcome"], "INCOMPLETE")
        self.assertEqual(result["supplyRestoration"]["outcome"], "PASS")
        self.assertEqual({name: h.sha(self.root / "supply" / name) for name in runner.SUPPLY}, self.expected)

    def test_collision_does_not_overwrite_new_file_and_cannot_leave_pass(self):
        self.relocation.hide()
        path = self.root / "supply/package.tar.gz"
        path.write_text("DO_NOT_OVERWRITE")
        result = {"outcome": "PASS", "failure": "PRIMARY_TEST_FAILURE"}
        runner.restore_owned_supply(self.relocation, result)
        self.assertEqual(path.read_text(), "DO_NOT_OVERWRITE")
        self.assertEqual(result["outcome"], "INCOMPLETE")
        self.assertEqual(result["failure"], "PRIMARY_TEST_FAILURE")
        self.assertEqual(result["supplyRestoration"]["outcome"], "INCOMPLETE")
        self.assertTrue((self.root / "withheld supply/package.tar.gz").exists())
        self.assertTrue((self.root / "supply/public-key.pem").exists())

    def test_changed_hidden_file_is_preserved_for_operator_and_not_restored_as_original(self):
        self.relocation.hide()
        path = self.root / "withheld supply/engine-lock.json"
        path.write_text("TAMPERED_STUB")
        result = {"outcome": "PASS"}
        runner.restore_owned_supply(self.relocation, result)
        self.assertEqual(result["outcome"], "INCOMPLETE")
        self.assertTrue(path.exists())
        self.assertFalse((self.root / "supply/engine-lock.json").exists())

    def test_unexpected_member_or_symlink_is_refused_before_any_move(self):
        extra = self.root / "supply/extra"
        extra.write_text("extra")
        with self.assertRaisesRegex(h.Failure, "SUPPLY_ABSENCE_MEMBERSHIP"):
            self.relocation.hide()
        self.assertFalse((self.root / "withheld supply").exists())
        extra.unlink()
        path = self.root / "supply/public-key.pem"
        path.unlink()
        path.symlink_to(self.root / "supply/engine-lock.json")
        with self.assertRaisesRegex(h.Failure, "INPUT_NOT_REGULAR"):
            self.relocation.hide()
        self.assertFalse((self.root / "withheld supply").exists())



if __name__ == "__main__":
    unittest.main()
