"""Offline regressions for installed-manager routing and continuity oracles."""
import json
import os
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import Mock, patch

import run_management_acceptance as runner
import support as h


class ManagerRouting(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.home = self.root / "management-home"
        self.node = {"label": "n1", "control": self.root / "control"}

    def test_named_calls_replace_only_exact_control_and_keep_flags(self):
        for command in runner.NAMED:
            with self.subTest(command=command):
                values = [command, "--instance", self.node["control"], "--timeout-seconds", "180"]
                expected = [command, "n1", "--home", self.home, "--timeout-seconds", "180"]
                self.assertEqual(runner.named_arguments(self.node, values, self.home), expected)
                self.assertEqual(values[1], "--instance")

    def test_named_mapping_rejects_wrong_control_missing_value_and_duplicate(self):
        for values in (["start", "--instance", self.root / "other"], ["start", "--instance"],
                       ["start", "--instance", self.node["control"], "--instance", self.node["control"]],
                       ["start", "--instance", self.node["control"], "--home", self.home], ["start"]):
            with self.subTest(values=values), self.assertRaises(h.Failure):
                runner.named_arguments(self.node, values, self.home)

    def test_registration_and_install_are_not_rewritten(self):
        values = ["instance", "register", "--instance", self.node["control"]]
        self.assertEqual(runner.named_arguments(self.node, values, self.home), values)
        values = ["install", self.root / "archive", "--destination", self.root / "package"]
        self.assertEqual(runner.named_arguments(None, values, self.home), values)

    def test_stable_entry_retains_symlink_across_selection_changes(self):
        entry = self.root / "bin/bxdl"
        entry.parent.mkdir()
        a, b = self.root / "release-a", self.root / "release-b"
        a.write_text("A")
        b.write_text("B")
        current = self.root / "current"
        current.symlink_to(a)
        entry.symlink_to(current)
        saved = runner.stable_entry(entry)
        self.assertEqual(saved, entry)
        self.assertEqual(saved.read_text(), "A")
        current.unlink()
        current.symlink_to(b)
        self.assertEqual(saved.read_text(), "B")

    def test_stable_entry_rejects_plain_binary_relative_or_wrong_name(self):
        plain = self.root / "bxdl"
        plain.write_text("binary")
        wrong = self.root / "other"
        wrong.symlink_to(plain)
        for value in (plain, Path("bin/bxdl"), wrong):
            with self.subTest(value=value), self.assertRaises(h.Failure):
                runner.stable_entry(value)

    def test_command_uses_stable_entry_and_records_manager_identity(self):
        cli_root = self.root / "cli"
        current = cli_root / "current/bin"
        current.mkdir(parents=True)
        (current / "bxdl").write_text("manager")
        entry = self.root / "bin/bxdl"
        entry.parent.mkdir()
        entry.symlink_to(current / "bxdl")
        obj = runner.ManagementAcceptance.__new__(runner.ManagementAcceptance)
        obj.a = SimpleNamespace(cli_root=cli_root, manager_home=self.home)
        obj.cli, obj.e = entry, Mock()
        obj.results, obj.observed_pids, obj.started_attempts = {"managerCommands": []}, set(), set()
        response = {"data": {"pid": 1234, "attemptId": "a1"}}
        with patch.object(h, "command", return_value=response) as command:
            self.assertEqual(obj.command(self.node, ["start", "--instance", self.node["control"]], "test"), response)
        self.assertEqual(command.call_args.args[:2], (entry, ["start", "n1", "--home", self.home]))
        self.assertEqual(obj.started_attempts, {("n1", "a1")})
        self.assertEqual(obj.observed_pids, {1234})

    def test_info_metadata_must_identify_actual_entry_payload_and_root(self):
        cli_root = self.root / "cli"
        for release in ("release-a", "release-b"):
            path = cli_root / "releases" / release / "payload/bin/bxdl"
            path.parent.mkdir(parents=True)
            path.write_text(release)
        a = cli_root / "releases/release-a/payload/bin/bxdl"
        b = cli_root / "releases/release-b/payload/bin/bxdl"
        entry = self.root / "bxdl"
        entry.symlink_to(a)
        obj = runner.ManagementAcceptance.__new__(runner.ManagementAcceptance)
        obj.a, obj.cli = SimpleNamespace(cli_root=cli_root), entry
        info = {"status": "READY", "executable": str(entry), "root": str(cli_root),
                "invokedExecutable": str(entry), "current": {"releaseId": "release-a", "version": "dev",
                "revision": "a" * 40, "manifestSha256": "f" * 64}}
        version = {"version": "dev", "revision": "a" * 40}
        obj.command = Mock(side_effect=[{"data": info}, {"data": version}])
        self.assertEqual(obj.manager_identity("valid")["binarySha256"], h.sha(a))
        for changed in ({**info, "current": {**info["current"], "releaseId": "release-b"}},
                        {**info, "invokedExecutable": str(b)}, {**info, "root": str(self.root)}):
            obj.command = Mock(side_effect=[{"data": changed}, {"data": version}])
            with self.subTest(changed=changed), self.assertRaises(h.Failure):
                obj.manager_identity("invalid")

    def test_payload_rejects_duplicate_members_before_reading_content(self):
        package = self.root / "package"
        package.mkdir()
        manifest = package / "manifest.json"
        manifest.write_text(json.dumps({"files": [{"path": "duplicate"}] * 270}))
        with patch.object(runner, "MANIFEST_SHA", h.sha(manifest)), self.assertRaisesRegex(h.Failure, "MEMBER_COUNT"):
            runner.verify_payload(package)

    def test_payload_rejects_added_files(self):
        package = self.root / "package"
        package.mkdir()
        manifest = package / "manifest.json"
        manifest.write_text(json.dumps({"files": [{"path": f"f{i}"} for i in range(270)]}))
        (package / "unexpected").write_text("added")
        with patch.object(runner, "MANIFEST_SHA", h.sha(manifest)), self.assertRaisesRegex(h.Failure, "MEMBERSHIP"):
            runner.verify_payload(package)


class LiveContinuity(unittest.TestCase):
    def object(self):
        obj = runner.ManagementAcceptance.__new__(runner.ManagementAcceptance)
        obj.a = SimpleNamespace(cli_root=Path('/test/cli'), bundle_next=Path('/test/bundle'),
                                revision="b" * 40, allow_unsigned_development=False)
        obj.initial_manager = {"revision": "a" * 40, "binarySha256": "1" * 64}
        obj.next_manager = None
        obj.bundle_next_sha = "f" * 64
        obj.results = {"managerTransitions": []}
        obj.e = Mock()
        return obj

    def identities(self, pid=1234, attempt="a1"):
        return {"n1": {"pid": pid, "attemptId": attempt, "nodeInstanceId": "instance1"}}

    def test_update_requires_changed_selection_and_same_attempt_pid(self):
        obj = self.object()
        obj.live_identity = Mock(side_effect=[self.identities(), self.identities()])
        obj.command = Mock(return_value={"data": {"status": "UPDATED", "changed": True}})
        obj.manager_identity = Mock(return_value={"revision": "b" * 40, "binarySha256": "2" * 64})
        with patch.object(h, "sha", return_value=obj.bundle_next_sha):
            obj.transition("update", [{"label": "n1"}], "update")
        self.assertEqual(len(obj.results["managerTransitions"]), 1)
        self.assertTrue(obj.results["managerTransitions"][0]["sameAttemptsAndPids"])
        self.assertNotIn("--allow-unsigned-development", obj.command.call_args.args[1])

    def test_restart_or_pid_change_cannot_pass_as_cli_only_update(self):
        for after in (self.identities(pid=5678), self.identities(attempt="a2")):
            obj = self.object()
            obj.live_identity = Mock(side_effect=[self.identities(), after])
            obj.command = Mock(return_value={"data": {"status": "UPDATED", "changed": True}})
            obj.manager_identity = Mock(return_value={"revision": "b" * 40, "binarySha256": "2" * 64})
            with patch.object(h, "sha", return_value=obj.bundle_next_sha), self.assertRaisesRegex(h.Failure, "RESTARTED_NODE"):
                obj.transition("update", [{"label": "n1"}], "update")
            self.assertEqual(obj.results["managerTransitions"], [])

    def test_unchanged_selection_is_not_an_update_pass(self):
        obj = self.object()
        obj.live_identity = Mock(return_value=self.identities())
        obj.command = Mock(return_value={"data": {"status": "UNCHANGED", "changed": False}})
        with patch.object(h, "sha", return_value=obj.bundle_next_sha), self.assertRaisesRegex(h.Failure, "NOT_SWITCHED"):
            obj.transition("update", [], "update")

    def test_wrong_next_revision_or_same_binary_is_rejected(self):
        for selected in ({"revision": "c" * 40, "binarySha256": "2" * 64},
                         {"revision": "b" * 40, "binarySha256": "1" * 64}):
            obj = self.object()
            obj.live_identity = Mock(return_value=self.identities())
            obj.command = Mock(return_value={"data": {"status": "UPDATED", "changed": True}})
            obj.manager_identity = Mock(return_value=selected)
            with patch.object(h, "sha", return_value=obj.bundle_next_sha), self.assertRaisesRegex(h.Failure, "NEXT_MANAGER_IDENTITY"):
                obj.transition("update", [], "update")

    def test_rollback_requires_original_exact_identity(self):
        obj = self.object()
        obj.live_identity = Mock(return_value=self.identities())
        obj.command = Mock(return_value={"data": {"status": "ROLLED_BACK", "changed": True}})
        obj.manager_identity = Mock(return_value={**obj.initial_manager, "binarySha256": "3" * 64})
        with self.assertRaisesRegex(h.Failure, "ROLLBACK_MANAGER_CHANGED"):
            obj.transition("rollback", [], "rollback")

    def test_no_automatic_unsigned_opt_in(self):
        obj = self.object()
        obj.a.allow_unsigned_development = True
        obj.live_identity = Mock(return_value=self.identities())
        obj.command = Mock(return_value={"data": {"status": "UPDATED", "changed": True}})
        obj.manager_identity = Mock(return_value={"revision": "b" * 40, "binarySha256": "2" * 64})
        with patch.object(h, "sha", return_value=obj.bundle_next_sha):
            obj.transition("update", [], "update")
        self.assertIn("--allow-unsigned-development", obj.command.call_args.args[1])

    def test_unknown_or_missing_pid_is_not_continuity(self):
        for data in ({"pid": None}, {"pid": 0}, {"pid": True}):
            obj = self.object()
            obj.command = Mock(return_value={"data": data})
            with patch.object(h, "running", return_value=data), self.assertRaisesRegex(h.Failure, "LIVE_PID_REQUIRED"):
                obj.live_identity([{"label": "n1", "control": Path('/test/control')}], "live")


class EvidenceBoundaries(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()

    def test_empty_missing_duplicate_or_unexpected_jobs_are_rejected(self):
        expected = {("n1", "a1"), ("n2", "a2")}
        workers = list(expected)
        runner.require_attempt_membership(workers, workers, expected)
        for jobs in ([], workers[:1], workers + [workers[0]], workers + [("n3", "a3")]):
            with self.subTest(jobs=jobs), self.assertRaisesRegex(h.Failure, "JOB_ATTEMPT_MEMBERSHIP"):
                runner.require_attempt_membership(workers, jobs, expected)
        with self.assertRaises(h.Failure):
            runner.require_attempt_membership([], [], set())

    def report(self, raw):
        path = self.root / "a1/report.jsonl"
        path.parent.mkdir(exist_ok=True)
        path.write_bytes(raw)
        return path

    def test_engine_report_collects_java_pids(self):
        records = [{"attemptId": "a1", "pid": pid, "command": "run"} for pid in (123, 456)]
        path = self.report(b"".join(json.dumps(row).encode() + b"\n" for row in records))
        self.assertEqual(runner.report_processes(path), ({123, 456}, "run"))

    def test_missing_partial_invalid_or_wrong_attempt_report_is_unknown(self):
        rows = [b"", b"{", b"{}\n", b"[]\n", b"null\n"]
        valid = {"attemptId": "a1", "pid": 123, "command": "run"}
        rows += [json.dumps(valid).encode(),
                 json.dumps({**valid, "pid": True}).encode() + b"\n",
                 json.dumps({**valid, "pid": 0}).encode() + b"\n",
                 json.dumps({**valid, "attemptId": "a2"}).encode() + b"\n"]
        for raw in rows:
            with self.subTest(raw=raw), self.assertRaisesRegex(h.Failure, "REPORT_PARSE_UNKNOWN"):
                runner.report_processes(self.report(raw))
        with self.assertRaisesRegex(h.Failure, "REPORT_PARSE_UNKNOWN"):
            runner.report_processes(self.root / "absent/report.jsonl")

    def test_ps_only_excludes_current_harness_and_parent(self):
        root = Path('/private/tmp/test-owned-run')
        stdout = b"  1 /sbin/launchd\n  20 python /private/tmp/test-owned-run/a\n  21 shell /private/tmp/test-owned-run\n  22 java /private/tmp/test-owned-run/n1\n"
        self.assertEqual(runner.root_processes(stdout, root, {20, 21}), [22])
        self.assertEqual(runner.root_processes(b"1 /sbin/launchd\n", root, {20, 21}), [])

    def test_unparseable_ps_does_not_prove_process_absence(self):
        for raw in (b"broken", b"0 bad\n", b"123\n", b"1 \xff\n"):
            with self.subTest(raw=raw), self.assertRaisesRegex(h.Failure, "PROCESS_OBSERVATION_UNKNOWN"):
                runner.root_processes(raw, self.root, set())

    def test_preserved_identity_includes_data_binding_and_fixed_worker(self):
        node = {"base": self.root, "control": self.root / "control",
                "product": self.root / "product.json", "native": self.root / "native.json"}
        data = self.root / "data"
        data.mkdir()
        worker = node["control"] / "operations/a1/bxdl-worker"
        worker.parent.mkdir(parents=True)
        (node["control"] / "journal").mkdir()
        (node["control"] / "journal/revision00000001.json").write_text("registered")
        for path in (data / "engine-instance.json", node["product"], node["native"],
                     node["control"] / "binding.json", worker):
            path.write_bytes(b"original")
        with patch.object(runner, "EXECUTOR_SHA", h.sha(worker)), patch.object(runner, "EXECUTOR_SIZE", 8):
            before = runner.preserved_node_identity(node, "a1")
            self.assertEqual(set(before), {"dataDirectory", "engineIdentitySha256", "productSha256",
                                           "nativeSha256", "bindingSha256", "workerSha256", "initializationJournal"})
            (node["control"] / "binding.json").write_bytes(b"changed!")
            self.assertNotEqual(runner.preserved_node_identity(node, "a1"), before)
            worker.write_bytes(b"changed!")
            with self.assertRaisesRegex(h.Failure, "LIVE_WORKER_EXECUTOR_PIN"):
                runner.preserved_node_identity(node, "a1")

    def test_initialization_journal_detects_added_removed_and_modified_records(self):
        journal = self.root / "journal"
        journal.mkdir()
        record = journal / "revision00000001.json"
        record.write_text("registered")
        before = runner.initialization_journal(self.root)
        record.write_text("modified")
        self.assertNotEqual(runner.initialization_journal(self.root), before)
        record.write_text("registered")
        added = journal / "revision00000002.json"
        added.write_text("new init")
        self.assertNotEqual(runner.initialization_journal(self.root), before)
        added.unlink()
        self.assertEqual(runner.initialization_journal(self.root), before)
        record.unlink()
        with self.assertRaisesRegex(h.Failure, "JOURNAL_MEMBERSHIP"):
            runner.initialization_journal(self.root)

    def test_initialization_journal_rejects_symlink_or_unexpected_member(self):
        journal = self.root / "journal"
        journal.mkdir()
        outside = self.root / "record"
        outside.write_text("record")
        link = journal / "revision00000001.json"
        link.symlink_to(outside)
        with self.assertRaises(h.Failure):
            runner.initialization_journal(self.root)
        link.unlink()
        (journal / "unexpected").write_text("record")
        with self.assertRaisesRegex(h.Failure, "JOURNAL_MEMBER"):
            runner.initialization_journal(self.root)

    def test_data_identity_change_fails_even_if_pid_and_attempt_are_same(self):
        obj = LiveContinuity().object()
        same = {"attemptId": "a1", "nodeInstanceId": "node1", "pid": 123}
        obj.live_identity = Mock(side_effect=[{"n1": {**same, "dataDirectory": {"inode": 1}}},
                                              {"n1": {**same, "dataDirectory": {"inode": 2}}}])
        obj.command = Mock(return_value={"data": {"status": "UPDATED", "changed": True}})
        obj.manager_identity = Mock(return_value={"revision": "b" * 40, "binarySha256": "2" * 64})
        with patch.object(h, "sha", return_value=obj.bundle_next_sha), self.assertRaisesRegex(h.Failure, "RESTARTED_NODE"):
            obj.transition("update", [], "update")


if __name__ == "__main__":
    unittest.main()
