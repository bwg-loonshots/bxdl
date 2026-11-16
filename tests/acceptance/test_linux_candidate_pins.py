"""Cross-file supply invariants; these do not execute or accept a Linux runtime."""
import hashlib
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SNAPSHOT = ROOT / 'contracts/runtime/temurin-21.0.13-linux-amd64'


def read(path):
    return json.loads(Path(path).read_text())


class LinuxCandidatePins(unittest.TestCase):
    def setUp(self):
        self.runtime = read(ROOT / 'packaging/runtime-lock.temurin-21-linux-amd64.json')
        self.engine = read(ROOT / 'packaging/engine-lock.rc4-temurin-linux-development.json')
        self.candidate = read(ROOT / 'packaging/linux-rc4-candidate.json')
        self.receipt = read(SNAPSHOT / 'received-supply.json')

    def test_candidate_pins_exact_repository_inputs_without_claiming_execution(self):
        for key in ('runtimeLock', 'engineLock', 'supplyReceipt'):
            ref = self.candidate[key]
            self.assertEqual(hashlib.sha256((ROOT / ref['path']).read_bytes()).hexdigest(), ref['sha256'])
        self.assertEqual(self.candidate['status'], 'SUPPLY_PINNED_NATIVE_NOT_RUN')
        self.assertIsNone(self.candidate['bootstrapCli']['binarySha256'])
        self.assertEqual(self.candidate['bootstrapCli']['nativeBuild'], 'NOT_RUN')
        self.assertEqual(self.candidate['execution']['runtimePackage'], 'NOT_ASSEMBLED')
        self.assertEqual(self.candidate['execution']['nativeRocksdb'], 'NOT_RUN')
        self.assertEqual(self.runtime['platform'], {'os': 'linux', 'arch': 'amd64', 'javaMajor': 21, 'libc': 'glibc'})

    def test_exact_rc4_identity_is_preserved_across_the_java_selection(self):
        mac = read(ROOT / 'packaging/engine-lock.rc4-temurin-development.json')
        self.assertNotEqual(mac['javaSha256'], self.engine['javaSha256'])
        self.assertEqual({k: v for k, v in mac.items() if k != 'javaSha256'},
                         {k: v for k, v in self.engine.items() if k != 'javaSha256'})
        supplied = read(ROOT / 'contracts/nigo/engine-rc4-2026-11-15/consumer-receipt.json')
        self.assertEqual(self.engine['jarSha256'], supplied['artifact']['sha256'])
        self.assertEqual(self.engine['jarSizeBytes'], supplied['artifact']['sizeBytes'])
        self.assertEqual(self.engine['expected']['source']['commit'], supplied['source']['commit'])
        self.assertEqual(self.candidate['engine']['contractFingerprint'], supplied['contract']['fingerprint'])

    def test_official_receipt_and_sbom_name_the_selected_archives(self):
        received = {f['name']: f for f in self.receipt['receivedFiles']}
        for key in ('archive', 'sourceArchive'):
            pin = self.runtime[key]
            item = received[pin['name']]
            self.assertEqual((pin['sha256'], pin['sizeBytes']), (item['sha256'], item['sizeBytes']))
            self.assertTrue(item['digestMatched'] and item['sizeMatched'])
            self.assertIn('/jdk-21.0.13%2B9/', pin['url'])
            self.assertNotIn('/latest/', pin['url'])
        comparison = self.receipt['sbomComparison']
        self.assertTrue(comparison['matched'])
        self.assertEqual(comparison['declaredArchiveSha256'], self.runtime['archive']['sha256'])
        self.assertEqual(self.receipt['signatureVerification']['status'], 'NOT_PERFORMED')

    def test_complete_runtime_inventory_and_bom_agree_without_reusing_mac_files(self):
        files = self.runtime['files']
        self.assertEqual(len(files), 257)
        self.assertEqual(len({f['path'] for f in files}), 257)
        self.assertEqual(sum(f['size'] for f in files), 166678666)
        java = next(f for f in files if f['path'] == 'bin/java')
        self.assertEqual(java['sha256'], self.runtime['javaSha256'])
        self.assertEqual(java['sha256'], self.engine['javaSha256'])
        bom = read(ROOT / 'packaging/runtime-bom.temurin-21-linux-amd64.cdx.json')
        expected = {'runtime/' + f['path']: f['sha256'] for f in files}
        observed = {c['name']: c['hashes'][0]['content'] for c in bom['components']}
        self.assertEqual(observed, expected)
        static = read(SNAPSHOT / 'observed-supply.json')
        self.assertEqual(static['fileSymlinks'], 145)
        self.assertEqual(static['materializedFiles'], len(files))
        self.assertEqual(static['runtimeExecution'], 'NOT_RUN')
        self.assertEqual(static['runtimeLockSha256'], self.candidate['runtimeLock']['sha256'])


if __name__ == '__main__':
    unittest.main()
