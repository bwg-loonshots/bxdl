"""Offline boundary and one-shot recovery regressions; no Java, node, or RPC I/O."""
import copy
import hashlib
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
import warnings
from unittest.mock import Mock, patch
import zipfile

from candidates import CANDIDATES, JAVA_SHA256, PREPARATION_CANDIDATES, RC4_PREPARATION_RESOURCES, candidate_for
import fixture_guard as fg
import prepare_fixture as pf
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
        identity = PREPARATION_CANDIDATES[candidate]
        if candidate == "rc4":
            (self.root / "data").mkdir(mode=0o700)
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
                "storage": {"backend": "rocksdb", "dataDirectory": str(self.root / "data" / f"n{index}")},
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
        if candidate == "rc4":
            summary.update(
                artifactPreparation=copy.deepcopy(RC4_PREPARATION_RESOURCES),
                genesisStatus="FRESH_INPUTS_ONLY_NOT_INITIALIZED", runtimeCandidateSupport="NOT_IMPLEMENTED",
                nodeDataCreated=False, nodeStarted=False, networkTraffic="NOT_PERFORMED",
                runtimeAcceptance="NOT_CHECKED", certificateExport="EXPORTED_PIN_MATCHED",
            )
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

    def test_rc3_validates_all_twelve_transactions_and_candidate_pin(self):
        args = self.fixture("rc3")
        pins = fg.validate_inputs(args)
        self.assertEqual(pins["candidate"], "rc3")
        self.assertEqual(len(pins["transactionFiles"]), 12)
        fg.assert_inputs_unchanged(args, pins)

    def test_rc3_cannot_use_rc2_label_or_lock(self):
        args = self.fixture("rc3")
        args.candidate = "rc2"
        with self.assertRaisesRegex(h.Failure, "PREPARATION_IDENTITY"):
            fg.validate_inputs(args)
        args.candidate = "rc3"
        lock = h.document(args.lock)
        lock["jarSha256"] = CANDIDATES["rc2"]["jarSha256"]
        self.write("lock.json", lock)
        with self.assertRaisesRegex(h.Failure, "LOCK_ARTIFACT"):
            fg.validate_inputs(args)

    def test_rc3_without_explicit_candidate_is_not_inferred(self):
        args = self.fixture("rc3")
        del args.candidate
        with self.assertRaisesRegex(h.Failure, "PREPARATION_IDENTITY"):
            fg.validate_inputs(args)

    def test_rc3_summary_size_is_checked_before_acceptance(self):
        args = self.fixture("rc3")
        summary = h.document(args.fixture / "fixture-summary.json")
        summary["engineJarSizeBytes"] += 1
        self.write("fixture-summary.json", summary)
        with self.assertRaisesRegex(h.Failure, "JAR_SIZE"):
            fg.validate_inputs(args)

    def test_rc3_never_enters_legacy_continuation(self):
        with self.assertRaisesRegex(h.Failure, "LEGACY_RC1_ONLY"):
            fg.validate_legacy_continuation(SimpleNamespace(candidate="rc3"), {}, {})

    def test_rc4_preparation_validates_without_reading_a_product_lock(self):
        args = self.fixture("rc4")
        del args.lock
        pins = fg.validate_preparation(args)
        self.assertEqual(pins["candidate"], "rc4")
        self.assertEqual(pins["scope"], "PINNED_TEST_CANDIDATE_PREPARATION_ONLY")
        self.assertIsNone(pins["engineLockSha256"])
        self.assertEqual(len(pins["transactionFiles"]), 12)
        fg.assert_preparation_unchanged(args, pins)

    def test_rc4_is_not_exposed_as_runtime_acceptance_candidate(self):
        args = self.fixture("rc4")
        self.assertNotIn("rc4", CANDIDATES)
        with self.assertRaises(KeyError):
            candidate_for(args)
        with self.assertRaisesRegex(h.Failure, "RUNTIME_CANDIDATE_NOT_SUPPORTED"):
            fg.validate_inputs(args)
        output = self.root / "runtime-not-authorized"
        with patch.object(runner.fg, "validate_inputs") as validate:
            with self.assertRaises(KeyError):
                runner.Acceptance(SimpleNamespace(candidate="rc4", output=output))
        validate.assert_not_called()
        self.assertFalse(output.exists())

    def test_rc4_preparation_requires_exact_size_and_candidate_identity(self):
        args = self.fixture("rc4")
        summary = h.document(self.root / "fixture-summary.json")
        summary["engineJarSizeBytes"] += 1
        self.write("fixture-summary.json", summary)
        with self.assertRaisesRegex(h.Failure, "JAR_SIZE"):
            fg.validate_preparation(args)
        summary["engineJarSizeBytes"] -= 1
        self.write("fixture-summary.json", summary)
        del args.candidate
        with self.assertRaisesRegex(h.Failure, "PREPARATION_IDENTITY"):
            fg.validate_preparation(args)

    def test_rc3_fixture_cannot_be_relabelled_for_rc4_preparation(self):
        args = self.fixture("rc3")
        args.candidate = "rc4"
        with self.assertRaisesRegex(h.Failure, "PREPARATION_IDENTITY"):
            fg.validate_preparation(args)

    def test_rc4_preparation_rejects_catalog_or_scope_claim_changes(self):
        args = self.fixture("rc4")
        original = h.document(self.root / "fixture-summary.json")
        summary = copy.deepcopy(original)
        summary["artifactPreparation"]["nativeCatalog"]["lockSha256"] = "0" * 64
        self.write("fixture-summary.json", summary)
        with self.assertRaisesRegex(h.Failure, "ARTIFACT_PREPARATION"):
            fg.validate_preparation(args)
        for field, value in (("nodeDataCreated", True), ("nodeStarted", True),
                             ("genesisStatus", "INITIALIZED"), ("certificateExport", "NOT_AVAILABLE"),
                             ("runtimeAcceptance", "PASS")):
            with self.subTest(field=field):
                summary = copy.deepcopy(original)
                summary[field] = value
                self.write("fixture-summary.json", summary)
                with self.assertRaisesRegex(h.Failure, "PREPARATION_SCOPE"):
                    fg.validate_preparation(args)

    def test_rc4_preparation_rejects_external_storage_and_existing_data(self):
        args = self.fixture("rc4")
        product = h.document(self.root / "products/n1.json")
        product["storage"]["dataDirectory"] = str(self.root / "other-data")
        self.write("products/n1.json", product)
        with self.assertRaisesRegex(h.Failure, "FRESH_STORAGE_PATH"):
            fg.validate_preparation(args)
        product["storage"]["dataDirectory"] = str(self.root / "data/n1")
        self.write("products/n1.json", product)
        (self.root / "data/n1").symlink_to(self.root / "missing")
        with self.assertRaisesRegex(h.Failure, "NODE_DATA_ALREADY_EXISTS"):
            fg.validate_preparation(args)

    def test_rc4_preparation_checks_reserved_transaction_and_later_input_changes(self):
        args = self.fixture("rc4")
        pins = fg.validate_preparation(args)
        changed = h.document(self.root / "transactions/tx-11.json")
        changed["rawTransaction"] = "0x02ef12"
        self.write("transactions/tx-11.json", changed)
        with self.assertRaisesRegex(h.Failure, "INPUT_SET_CHANGED"):
            fg.assert_preparation_unchanged(args, pins)
        (self.root / "transactions/tx-11.json").unlink()
        with self.assertRaisesRegex(h.Failure, "INVALID_INPUT"):
            fg.validate_preparation(args)

    def test_rc4_preparation_rejects_redirected_data_parent(self):
        args = self.fixture("rc4")
        (self.root / "data").rmdir()
        outside = self.root / "other-data"
        outside.mkdir()
        (self.root / "data").symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(h.Failure, "FRESH_STORAGE_PATH"):
            fg.validate_preparation(args)

    def test_rc4_preparation_does_not_inherit_the_legacy_diagnostic_revision(self):
        args = self.fixture("rc4")
        del args.revision
        with self.assertRaisesRegex(h.Failure, "REVISION"):
            fg.validate_preparation(args)

    def test_rc4_never_enters_legacy_continuation(self):
        with self.assertRaisesRegex(h.Failure, "LEGACY_RC1_ONLY"):
            fg.validate_legacy_continuation(SimpleNamespace(candidate="rc4"), {}, {})


class PreparationResources(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()

    def jar(self, *, omit=None, duplicate=None, corrupt_catalog=False, bad_checksum=False):
        expected = copy.deepcopy(RC4_PREPARATION_RESOURCES)
        catalog = expected["nativeCatalog"]
        lock = {key: catalog[key] for key in ("schema", "chainId", "acceptance", "defaultActivated", "resources")}
        raw = json.dumps(lock).encode()
        catalog.update(lockSha256=hashlib.sha256(raw).hexdigest(), lockSizeBytes=len(raw))
        nested = io.BytesIO()
        with zipfile.ZipFile(nested, "w") as vm:
            vm.writestr(catalog["lockPath"], raw + (b" " if corrupt_catalog else b""))
            vm.writestr(catalog["lockPath"].replace(".json", ".sha256"),
                        (("0" * 64 if bad_checksum else catalog["lockSha256"]) + "\n").encode())
        entries = []
        for kind in ("generator", "launcher"):
            raw_class = (kind + "-offline-class-fixture").encode()
            expected[kind]["sha256"] = hashlib.sha256(raw_class).hexdigest()
            entries.append((expected[kind]["class"], raw_class))
        entries.append((catalog["container"], nested.getvalue()))
        path = self.root / "candidate.jar"
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            with zipfile.ZipFile(path, "w") as archive:
                for name, raw_entry in entries:
                    if name != omit:
                        archive.writestr(name, raw_entry)
                        if name == duplicate:
                            archive.writestr(name, raw_entry)
        return path, expected

    def test_rc4_helper_and_catalog_lock_are_checked_without_loading_a_jvm(self):
        path, expected = self.jar()
        with patch.object(pf, "RC4_PREPARATION_RESOURCES", expected), patch.object(pf, "run_private") as child:
            observed = pf.checked_jar_resources(path, "rc4")
        self.assertEqual(observed, expected)
        self.assertEqual(observed["nativeCatalog"]["verification"], "LOCK_BYTES_AND_SUMMARY_ONLY")
        child.assert_not_called()

    def test_missing_or_duplicate_helper_is_rejected(self):
        helper = RC4_PREPARATION_RESOURCES["generator"]["class"]
        for option in ("omit", "duplicate"):
            with self.subTest(option=option):
                path, expected = self.jar(**{option: helper})
                with patch.object(pf, "RC4_PREPARATION_RESOURCES", expected):
                    with self.assertRaisesRegex(ValueError, "missing, duplicated or oversized"):
                        pf.checked_jar_resources(path, "rc4")

    def test_catalog_bytes_and_checksum_sidecar_are_both_checked(self):
        for option in ("corrupt_catalog", "bad_checksum"):
            with self.subTest(option=option):
                path, expected = self.jar(**{option: True})
                with patch.object(pf, "RC4_PREPARATION_RESOURCES", expected):
                    with self.assertRaisesRegex(ValueError, "catalog .*mismatch"):
                        pf.checked_jar_resources(path, "rc4")

    def test_resource_failure_prevents_output_creation_or_any_child(self):
        jar = self.root / "invalid.jar"
        jar.write_bytes(b"not-a-jar")
        args = SimpleNamespace(candidate="rc4", java="java", jar=str(jar), output=self.root / "output")
        with patch.object(pf, "preparation_candidate_for", return_value={"jarSha256": "unused", "jarSizeBytes": jar.stat().st_size}), \
             patch.object(pf, "pinned_file", side_effect=[self.root / "java", jar]), \
             patch.object(pf, "run_private") as child:
            with self.assertRaises(zipfile.BadZipFile):
                pf.prepare(args)
        child.assert_not_called()
        self.assertFalse(args.output.exists())

    def test_legacy_candidate_does_not_require_the_rc4_catalog(self):
        self.assertIsNone(pf.checked_jar_resources(self.root / "not-opened", "rc3"))


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


class SameDbRecoveryRC3Boundary(SameDbRecoveryBoundary):
    """Run the same skew, DB identity and no-automatic-retry checks for RC3."""
    def setUp(self):
        super().setUp()
        self.run.candidate_name = "rc3"


class ScenarioRouting(unittest.TestCase):
    def test_full_candidates_keep_their_required_recovery_and_wrong_pin_nonces(self):
        for candidate in ("rc1", "rc2", "rc3"):
            with self.subTest(candidate=candidate):
                run = runner.Acceptance.__new__(runner.Acceptance)
                run.candidate_name, run.scenario = candidate, "full"
                nodes = [{"label": f"n{i}", "genesisHash": "same", "port": i,
                          "lastAttempt": "first", "nodeInstanceId": "first"} for i in range(4)]
                run.check_free_ports, run.package = Mock(), Mock()
                run.prepare_node = Mock(side_effect=nodes)
                run.start_all, run.stop = Mock(), Mock()
                run.submit = Mock(side_effect=lambda nonce, port: str(nonce))
                run.finality = Mock(return_value={"blockHash": "fixed"})
                run.canonical_at = Mock(return_value={"blockHash": "fixed"})
                run.start = Mock(side_effect=lambda node, label: node.update(lastAttempt="second", nodeInstanceId="second"))
                run.two_of_four, run.same_db_recovery, run.wrong_pin = Mock(), Mock(), Mock()
                run.run()
                run.two_of_four.assert_called_once_with(nodes)
                if candidate in ("rc2", "rc3"):
                    run.same_db_recovery.assert_called_once_with(nodes, 5, 6)
                    run.wrong_pin.assert_called_once_with(nodes, nonce=7)
                else:
                    run.same_db_recovery.assert_not_called()
                    run.wrong_pin.assert_called_once_with(nodes)

    def test_independent_rc3_recovery_ends_without_other_fault_cases(self):
        run = runner.Acceptance.__new__(runner.Acceptance)
        run.candidate_name, run.scenario = "rc3", "same-db-recovery"
        nodes = [{"genesisHash": "same", "port": i} for i in range(4)]
        run.check_free_ports, run.package = Mock(), Mock()
        run.prepare_node = Mock(side_effect=nodes)
        run.start_all, run.submit, run.finality = Mock(), Mock(return_value="tx0"), Mock()
        run.same_db_recovery, run.two_of_four, run.wrong_pin = Mock(), Mock(), Mock()
        run.run()
        run.same_db_recovery.assert_called_once_with(nodes, 1, 2)
        run.two_of_four.assert_not_called()
        run.wrong_pin.assert_not_called()


if __name__ == "__main__":
    unittest.main()
