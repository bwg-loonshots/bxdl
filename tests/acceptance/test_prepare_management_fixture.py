"""Offline RC4 manual-node preparation: no CLI, JVM, key generation or service."""
import copy
import json
from pathlib import Path
import stat
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import prepare_management_fixture as preparation
import run_management_service as service
import runtime_profiles as rp
import support as h
import test_candidates as fixtures


class ManualFixtureBoundary(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        source = self.root / "fresh-source"
        source.mkdir(mode=0o700)
        case = fixtures.CandidateBoundary()
        case.root = source
        fixture_args = case.fixture("rc4")
        supply = self.root / "frozen-runtime-stubs"
        supply.mkdir()
        paths = {}
        profile = copy.deepcopy(rp._PROFILES["rc4"])
        for name, field in (("archive", "archiveSha256"), ("public_key", "publicKeySha256"),
                            ("lock", "engineLockSha256")):
            path = supply / name
            path.write_text("OFFLINE_PUBLIC_STUB_" + name)
            paths[name] = path
            profile[field] = h.sha(path)
        profile["archiveSizeBytes"] = paths["archive"].stat().st_size
        self.profile_patch = patch.dict(rp._PROFILES, {"rc4": profile})
        self.profile_patch.start()
        self.addCleanup(self.profile_patch.stop)
        self.args = SimpleNamespace(candidate="rc4", fixture=source, revision=fixture_args.revision,
                                    output=self.root / "prepared", instance_id="manual-input-n1",
                                    http_port=29680, p2p_port=30680, **paths)

    def test_fresh_copy_preserves_chain_and_keys_and_creates_no_process_or_data(self):
        before = preparation.file_pins(self.args.fixture, preparation.source_files(self.args.fixture))
        with patch("subprocess.Popen") as popen, patch.object(h, "command") as command:
            record = preparation.prepare(self.args)
        popen.assert_not_called()
        command.assert_not_called()
        self.assertEqual(preparation.file_pins(self.args.fixture,
                         preparation.source_files(self.args.fixture)), before)
        root = self.args.output
        inputs = root / "operational inputs"
        self.assertEqual({p.name for p in inputs.iterdir()}, preparation.INPUT_NAMES)
        self.assertEqual({p.name for p in (root / "supply").iterdir()}, preparation.SUPPLY_NAMES)
        for path in inputs.iterdir():
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            self.assertEqual(path.stat().st_nlink, 1)
        self.assertEqual(h.sha(inputs / "chain.json"), before["chain.json"])
        members = [key for key in h.document(inputs / "chain.json") if ".validators[" in key]
        self.assertEqual(len(members), 4)
        for name in preparation.fg.SECRET_FILES.values():
            original = self.args.fixture / "generated/validator-1" / name
            self.assertEqual(h.sha(inputs / name), h.sha(original))
            self.assertNotEqual((inputs / name).stat().st_ino, original.stat().st_ino)
        product, native = h.document(inputs / "instance.json"), h.document(inputs / "node.json")
        self.assertEqual(product["instanceId"], "manual-input-n1")
        self.assertEqual(native["chainFile"], product["chainDescription"])
        self.assertEqual(native["dataDirectory"], product["storage"]["dataDirectory"])
        self.assertEqual(native["backend"], "rocksdb")
        self.assertFalse(native["node"]["nigo.protocol.consensus.auto-start"])
        self.assertFalse(native["node"]["nigo.protocol.consensus.qbft.sync.enabled"])
        self.assertFalse(native["node"]["nigo.protocol.consensus.qbft.sync.archive.enabled"])
        self.assertEqual(native["node"]["server.port"], self.args.http_port)
        self.assertEqual(native["node"][preparation.PREFIX + "listen-port"], self.args.p2p_port)
        self.assertFalse(any(".peers[" in name for name in native["node"]))
        references = [product["chainDescription"], *product["secrets"].values(), native["chainFile"]]
        references += [value for key, value in native["node"].items()
                       if key.endswith(("-path", "-file"))]
        self.assertTrue(all(Path(value).parent == inputs for value in references))
        self.assertFalse((root / "fresh data").exists())
        self.assertFalse(record["genesisMembershipChanged"])
        self.assertFalse(record["newCredentialsIssued"])
        self.assertEqual(record["jvmInvocations"], 0)
        self.assertEqual(record["cliInvocations"], 0)
        preparation.validate_service_fixture(root)

    def test_converter_output_is_admitted_and_remapped_by_rc4_service_preparation(self):
        preparation.prepare(self.args)
        output = self.root / "service-run"
        output.mkdir(mode=0o700)
        args = SimpleNamespace(candidate="rc4", fixture=self.args.output, supply=None, inputs=None,
                               instance_id="manual-service-n1", http_port=29690, p2p_port=30690)
        with patch("subprocess.Popen") as popen, patch.object(h, "command") as command:
            result = service.prepare(args, output)
        popen.assert_not_called()
        command.assert_not_called()
        self.assertEqual(result["candidate"], "rc4")
        self.assertEqual(result["freshFixtureProvenance"]["candidate"], "rc4")
        inputs = output / "operational inputs"
        product, native = h.document(inputs / "instance.json"), h.document(inputs / "node.json")
        self.assertEqual(product["instanceId"], args.instance_id)
        self.assertEqual(product["http"]["port"], args.http_port)
        self.assertEqual(native["node"][preparation.PREFIX + "listen-port"], args.p2p_port)
        self.assertEqual(native["dataDirectory"], str(output / "fresh data"))
        self.assertTrue(all(Path(value).parent == inputs for value in product["secrets"].values()))
        self.assertFalse((output / "fresh data").exists())

    def test_source_candidate_or_frozen_supply_mismatch_is_rejected_before_output(self):
        cases = ("candidate", "supply", "source")
        for case in cases:
            with self.subTest(case=case):
                args = copy.copy(self.args)
                args.output = self.root / ("rejected-" + case)
                patches = []
                if case == "candidate":
                    args.candidate = "rc3"
                elif case == "supply":
                    bad = self.root / "bad-archive"
                    bad.write_text("MISMATCHED_OFFLINE_ARCHIVE")
                    args.archive = bad
                else:
                    patches.append(patch.object(preparation.fg, "validate_preparation",
                                                side_effect=h.Failure("FIXTURE_GUARD_CANDIDATE_IDENTITY")))
                for item in patches:
                    item.start()
                try:
                    with patch.object(h, "copy_file") as copy_file, self.assertRaises(h.Failure):
                        preparation.prepare(args)
                    copy_file.assert_not_called()
                    self.assertFalse(args.output.exists())
                finally:
                    for item in reversed(patches):
                        item.stop()

    def test_existing_symlink_overlapping_or_unusable_output_is_rejected(self):
        existing = self.root / "existing"
        existing.mkdir()
        (existing / "sentinel").write_text("preserve")
        dangling = self.root / "dangling"
        dangling.symlink_to(self.root / "missing")
        for value in (existing, dangling, self.args.fixture / "nested", Path("relative")):
            with self.subTest(output=value), self.assertRaises(h.Failure):
                args = copy.copy(self.args)
                args.output = value
                preparation.prepare(args)
        self.assertEqual((existing / "sentinel").read_text(), "preserve")

    def test_source_change_copied_credential_change_or_data_creation_invalidates_provenance(self):
        preparation.prepare(self.args)
        inputs = self.args.output / "operational inputs"
        for path in (self.args.fixture / "generated/validator-1/validator.password",
                     inputs / "validator.password", inputs / "chain.json"):
            with self.subTest(file=path.name):
                original = path.read_bytes()
                path.write_bytes(original + b"tampered")
                with self.assertRaises(h.Failure):
                    preparation.validate_service_fixture(self.args.output)
                path.write_bytes(original)
        data = self.args.output / "fresh data"
        data.symlink_to(self.root / "absent-data")
        with self.assertRaisesRegex(h.Failure, "SERVICE_FIXTURE_DATA_ALREADY_EXISTS"):
            preparation.validate_service_fixture(self.args.output)

    def test_forged_updated_digest_cannot_add_peers_or_external_secret_references(self):
        preparation.prepare(self.args)
        root = self.args.output
        path = root / "operational inputs/node.json"
        native, record = h.document(path), h.document(root / preparation.RECORD)
        for field, value in ((preparation.PREFIX + "peers[0].host", "127.0.0.1"),
                             (preparation.PREFIX + "keystore-path", "/outside/private/key")):
            with self.subTest(field=field):
                changed = copy.deepcopy(native)
                changed["node"][field] = value
                path.write_text(json.dumps(changed))
                forged = copy.deepcopy(record)
                forged["inputs"]["node.json"] = h.sha(path)
                (root / preparation.RECORD).write_text(json.dumps(forged))
                with self.assertRaisesRegex(h.Failure, "SERVICE_FIXTURE_TRANSFORM_CHANGED"):
                    preparation.validate_service_fixture(root)


if __name__ == "__main__":
    unittest.main()
