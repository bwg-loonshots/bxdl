"""Synthetic Linux operator input preparation; never runs Java or a node."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
SPEC = importlib.util.spec_from_file_location("bxdl_linux_test_inputs", ROOT / "tools/prepare-linux-test-inputs.py")
preparation = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(preparation)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def save(path, raw, mode=0o600):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    path.write_bytes(raw if isinstance(raw, bytes) else raw.encode())
    path.chmod(mode)


class LinuxInputPreparation(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        self.runtime, self.jar = self.root / "supply/runtime", self.root / "supply/engine.jar"
        self.locks = self.root / "source"
        runtime_files = {"bin/java": b"TEST Java never executed", "bin/keytool": b"TEST keytool never executed",
                         "lib/libjvm.so": b"TEST library full inventory", "release": b"TEST release"}
        entries = []
        for name, raw in runtime_files.items():
            mode = 0o755 if name.startswith("bin/") else 0o644
            save(self.runtime / name, raw, mode)
            entries.append({"path": name, "sha256": digest(raw), "size": len(raw), "mode": mode})
        helpers = {name: b"TEST class " + name.encode() for name in preparation.HELPERS}
        with zipfile.ZipFile(self.jar, "w") as jar:
            for name, raw in helpers.items():
                jar.writestr(name, raw)
        self.jar.chmod(0o644)
        runtime_lock = {"files": entries}
        engine_lock = {"jarSha256": preparation.contract.digest(self.jar), "jarSizeBytes": self.jar.stat().st_size,
                       "javaSha256": digest(runtime_files["bin/java"]), "expected": {"source": {"commit": "a" * 40}}}
        save(self.locks / preparation.RUNTIME_LOCK, json.dumps(runtime_lock), 0o644)
        save(self.locks / preparation.ENGINE_LOCK, json.dumps(engine_lock), 0o644)
        self.args = SimpleNamespace(runtime=self.runtime, jar=self.jar, output=self.root / "new-inputs",
                                    instance="linux-private-n1", http_port=29680, p2p_port=30680)
        self.calls = []
        self.patches = [
            patch.object(preparation, "ROOT", self.locks),
            patch.object(preparation, "RUNTIME_LOCK_SHA", preparation.contract.digest(self.locks / preparation.RUNTIME_LOCK)),
            patch.object(preparation, "ENGINE_LOCK_SHA", preparation.contract.digest(self.locks / preparation.ENGINE_LOCK)),
            patch.object(preparation, "HELPERS", {name: digest(raw) for name, raw in helpers.items()}),
            patch.object(preparation, "CONTROL_PARENT", self.root / "system/instances"),
            patch.object(preparation.builder, "native_host", return_value={"os": "linux", "arch": "amd64"}),
            patch.object(preparation.builder, "run", side_effect=self.child_run),
        ]
        for item in self.patches:
            item.start()
            self.addCleanup(item.stop)

    def child_run(self, argv, cwd, env, label, evidence=None, timeout=None):
        self.calls.append((list(map(str, argv)), cwd, env, label, evidence, timeout))
        if label == "generate":
            generated = self.args.output / "generated"
            generated.mkdir(mode=0o700)
            nodes = []
            for index in range(1, 5):
                directory = generated / f"validator-{index}"
                directory.mkdir(mode=0o700)
                for name in preparation.SECRET_FIELDS.values():
                    save(directory / name, f"TEST ONLY {index} {name}")
                leaf = f"TEST certificate {index}".encode()
                fingerprint = "0x" + digest(b"NIGO_TRANSPORT_CREDENTIAL/v1\x00MTLS\x00" + leaf)
                nodes.append({"index": index, "role": "VALIDATOR", "directory": str(directory),
                              "config": str(directory / "application.yml"), "nodeId": "0x" + f"{index:064x}",
                              "validatorId": "0x" + f"{index:040x}", "credentialFingerprint": fingerprint,
                              "httpPort": self.args.http_port + index - 1, "p2pPort": self.args.p2p_port + index - 1})
            save(generated / "devnet.json", json.dumps({"chainId": 11578, "profileId": "DEV_QBFT",
                                                       "transportSecurityScheme": "MTLS", "nodes": nodes}))
        else:
            index = int(label[-1])
            save(Path(argv[-1]), f"TEST certificate {index}")
        return b"", b""

    def test_generation_is_local_private_and_never_claims_node_acceptance(self):
        with patch.dict(os.environ, {"JAVA_TOOL_OPTIONS": "ambient-injection", "LD_PRELOAD": "ambient-injection"}):
            result = preparation.prepare(self.args)
        self.assertEqual(len(self.calls), 5)
        self.assertEqual([item[3] for item in self.calls], ["generate", *[f"export-leaf-n{i}" for i in range(1, 5)]])
        for argv, cwd, env, _, evidence, timeout in self.calls:
            self.assertEqual(cwd, self.args.output / "tool-cwd")
            self.assertEqual(evidence, self.args.output / "evidence")
            self.assertLessEqual(timeout, 180)
            self.assertNotIn("JAVA_TOOL_OPTIONS", env)
            self.assertNotIn("LD_PRELOAD", env)
            self.assertEqual(env["HOME"], str(self.args.output / "tool-home"))
            self.assertTrue(argv[0].startswith(str(self.runtime / "bin")))
            self.assertNotIn("sudo", argv)
            self.assertNotIn("--init", argv)
        self.assertIn("-Dloader.main=org.nigo.node.devnet.qbft.QbftDevnetGenerator", self.calls[0][0])
        inputs = self.args.output / "inputs"
        self.assertEqual({p.name for p in inputs.iterdir()}, {"instance.json", "node.json", "chain.json", *preparation.SECRET_FIELDS.values()})
        for path in inputs.iterdir():
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            self.assertEqual(path.stat().st_nlink, 1)
        for name in preparation.SECRET_FIELDS.values():
            original = self.args.output / "generated/validator-1" / name
            self.assertEqual(original.read_bytes(), (inputs / name).read_bytes())
            self.assertNotEqual(original.stat().st_ino, (inputs / name).stat().st_ino)
        product = preparation.bounded_json(inputs / "instance.json")
        native = preparation.bounded_json(inputs / "node.json")
        self.assertEqual(product["storage"]["dataDirectory"], str(preparation.CONTROL_PARENT / self.args.instance / "data"))
        self.assertEqual(native["dataDirectory"], product["storage"]["dataDirectory"])
        self.assertEqual(native["chainFile"], product["chainDescription"])
        self.assertEqual(native["node"][preparation.PREFIX + "validator-id"], "0x" + f"{1:040x}")
        for name in ("nigo.protocol.consensus.auto-start", "nigo.protocol.consensus.qbft.sync.enabled",
                     "nigo.protocol.consensus.qbft.sync.archive.enabled"):
            self.assertFalse(native["node"][name])
        self.assertFalse(any(".peers[" in name for name in native["node"]))
        chain = preparation.bounded_json(inputs / "chain.json")
        self.assertEqual(len([key for key in chain if ".validators[" in key]), 4)
        self.assertEqual(product["http"]["address"], "127.0.0.1")
        self.assertEqual(product["p2p"]["address"], "127.0.0.1")
        self.assertEqual(result["localReadinessTarget"], "MANUAL_START_INITIALIZED")
        self.assertEqual(result["networkAndQuorum"], "NOT_TESTED")
        self.assertEqual(result["executorProfile"], "NOT_ADMITTED")
        for key in ("systemInstall", "initialization", "systemd"):
            self.assertEqual(result[key], "NOT_RUN")
        self.assertFalse(result["nodeDataCreated"])
        self.assertFalse(result["nodeStarted"])
        self.assertFalse(preparation.CONTROL_PARENT.exists())

    def test_linux_host_or_nonroot_gate_precedes_any_output_or_child(self):
        with patch.object(preparation.builder, "native_host", side_effect=preparation.Failure("NATIVE_LINUX_X64_REQUIRED")):
            with self.assertRaisesRegex(preparation.Failure, "NATIVE_LINUX_X64_REQUIRED"):
                preparation.prepare(self.args)
        self.assertFalse(self.calls)
        self.assertFalse(self.args.output.exists())

    def test_invalid_name_ports_or_existing_control_refuse_before_generation(self):
        for field, value in (("instance", "../evil"), ("instance", "Uppercase"), ("instance", "a" * 33),
                             ("http_port", 65533), ("http_port", 1023), ("p2p_port", 29682)):
            with self.subTest(field=field, value=value):
                args = copy.copy(self.args)
                setattr(args, field, value)
                with self.assertRaises(preparation.Failure):
                    preparation.prepare(args)
        (preparation.CONTROL_PARENT / self.args.instance).mkdir(parents=True)
        with self.assertRaisesRegex(preparation.Failure, "TEST_INPUT_INSTANCE_EXISTS"):
            preparation.prepare(self.args)
        self.assertFalse(self.calls)
        self.assertFalse(self.args.output.exists())

    def test_corrupt_jar_lock_or_runtime_file_refuses_before_any_process(self):
        for path in (self.jar, self.locks / preparation.ENGINE_LOCK, self.runtime / "lib/libjvm.so"):
            with self.subTest(path=path.name):
                original = path.read_bytes()
                path.write_bytes(original + b"MUTATED")
                with self.assertRaises(preparation.Failure):
                    preparation.prepare(self.args)
                path.write_bytes(original)
        self.assertFalse(self.calls)
        self.assertFalse(self.args.output.exists())

    def test_extra_runtime_entry_or_symlink_cannot_be_hidden_from_full_inventory(self):
        extra = self.runtime / "extra.jar"
        save(extra, b"unexpected", 0o644)
        with self.assertRaisesRegex(preparation.Failure, "TEST_INPUT_RUNTIME_MEMBERSHIP"):
            preparation.prepare(self.args)
        extra.unlink()
        extra.symlink_to(self.jar)
        with self.assertRaises(preparation.Failure):
            preparation.prepare(self.args)
        extra.unlink()
        (self.runtime / "external").symlink_to(self.jar.parent, target_is_directory=True)
        with self.assertRaises(preparation.Failure):
            preparation.prepare(self.args)
        self.assertFalse(self.calls)

    def test_existing_or_symlink_output_is_preserved(self):
        self.args.output.mkdir()
        sentinel = self.args.output / "sentinel"
        sentinel.write_text("preserve")
        with self.assertRaisesRegex(preparation.Failure, "TEST_INPUT_OUTPUT_EXISTS"):
            preparation.prepare(self.args)
        self.assertEqual(sentinel.read_text(), "preserve")
        self.args.output = self.root / "dangling"
        self.args.output.symlink_to(self.root / "missing")
        with self.assertRaisesRegex(preparation.Failure, "TEST_INPUT_OUTPUT_EXISTS"):
            preparation.prepare(self.args)
        self.assertFalse(self.calls)

    def test_generation_failure_preserves_partial_private_evidence_without_receipt(self):
        def fail(argv, cwd, env, label, evidence=None, timeout=None):
            save(evidence / "generate.stderr", b"PRIVATE synthetic diagnostic")
            raise preparation.Failure("BUILD_COMMAND_FAILED")
        with patch.object(preparation.builder, "run", side_effect=fail):
            with self.assertRaisesRegex(preparation.Failure, "BUILD_COMMAND_FAILED"):
                preparation.prepare(self.args)
        self.assertTrue((self.args.output / "evidence/generate.stderr").exists())
        self.assertFalse((self.args.output / "receipt.json").exists())
        self.assertFalse(preparation.CONTROL_PARENT.exists())

    def test_certificate_mismatch_refuses_receipt_and_install_inputs(self):
        original = self.child_run
        def wrong_cert(*args, **kwargs):
            result = original(*args, **kwargs)
            if args[3] == "export-leaf-n1":
                Path(args[0][-1]).write_bytes(b"WRONG TEST CERTIFICATE")
            return result
        with patch.object(preparation.builder, "run", side_effect=wrong_cert):
            with self.assertRaisesRegex(preparation.Failure, "TEST_INPUT_CERTIFICATE_MISMATCH"):
                preparation.prepare(self.args)
        self.assertFalse((self.args.output / "receipt.json").exists())
        self.assertEqual(list((self.args.output / "inputs").iterdir()), [])

    def test_generated_secret_permissions_or_duplicate_identity_refuse_receipt(self):
        original = self.child_run
        for case in ("permissions", "identity"):
            self.args.output = self.root / ("bad-" + case)
            def corrupted(*args, **kwargs):
                result = original(*args, **kwargs)
                if args[3] == "generate":
                    generated = self.args.output / "generated"
                    if case == "permissions":
                        (generated / "validator-1/validator.password").chmod(0o644)
                    else:
                        path = generated / "devnet.json"
                        doc = preparation.bounded_json(path)
                        doc["nodes"][1]["nodeId"] = doc["nodes"][0]["nodeId"]
                        path.write_text(json.dumps(doc))
                return result
            with self.subTest(case=case), patch.object(preparation.builder, "run", side_effect=corrupted):
                with self.assertRaises(preparation.Failure):
                    preparation.prepare(self.args)
            self.assertFalse((self.args.output / "receipt.json").exists())

    def test_runtime_mutation_during_generation_cannot_produce_success_receipt(self):
        original = self.child_run
        def changed(*args, **kwargs):
            result = original(*args, **kwargs)
            if args[3] == "export-leaf-n4":
                (self.runtime / "lib/libjvm.so").write_bytes(b"MUTATED")
            return result
        with patch.object(preparation.builder, "run", side_effect=changed):
            with self.assertRaisesRegex(preparation.Failure, "TEST_INPUT_RUNTIME_MISMATCH"):
                preparation.prepare(self.args)
        self.assertFalse((self.args.output / "receipt.json").exists())


if __name__ == "__main__":
    unittest.main()
