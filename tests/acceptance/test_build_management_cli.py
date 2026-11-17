"""Pure builder selection checks; no compilation or candidate execution."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch


PATH = Path(__file__).resolve().parents[2] / "tools" / "build-management-cli.py"
SPEC = importlib.util.spec_from_file_location("bxdl_management_builder", PATH)
BUILDER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILDER)


class ManagementBuilderTarget(unittest.TestCase):
    def target(self, system, machine, libc, requested=None):
        with patch.object(BUILDER.platform, "system", return_value=system), \
             patch.object(BUILDER.platform, "machine", return_value=machine), \
             patch.object(BUILDER.platform, "libc_ver", return_value=(libc, "2.39")):
            return BUILDER.select_target(requested)

    def test_native_supported_targets_have_explicit_rust_build_targets(self):
        self.assertEqual(self.target("Darwin", "arm64", ""), "darwin-arm64")
        self.assertEqual(self.target("Linux", "x86_64", "glibc"), "linux-amd64")
        self.assertEqual(self.target("Linux", "AMD64", "glibc", "linux-amd64"), "linux-amd64")
        self.assertEqual(BUILDER.TARGETS["linux-amd64"], "x86_64-unknown-linux-gnu")

    def test_foreign_target_is_rejected_before_any_build_or_probe(self):
        with patch.object(BUILDER.subprocess, "run") as run, patch.object(BUILDER.subprocess, "check_output") as output:
            for system, machine, libc, requested in [
                ("Darwin", "arm64", "", "linux-amd64"),
                ("Linux", "x86_64", "glibc", "darwin-arm64"),
            ]:
                with self.subTest(system=system), self.assertRaises(SystemExit):
                    self.target(system, machine, libc, requested)
            run.assert_not_called()
            output.assert_not_called()

    def test_windows_arm_linux_musl_and_unidentified_libc_are_not_claimed(self):
        for system, machine, libc in [
            ("Windows", "AMD64", ""), ("Linux", "aarch64", "glibc"),
            ("Linux", "x86_64", "musl"), ("Linux", "x86_64", ""), ("Darwin", "x86_64", ""),
        ]:
            with self.subTest(system=system, machine=machine, libc=libc), self.assertRaises(SystemExit):
                self.target(system, machine, libc)


if __name__ == "__main__":
    unittest.main()
