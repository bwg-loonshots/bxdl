"""Shutdown evidence uses OS listeners; socket bind failures are not listeners."""
import unittest
from unittest.mock import patch
from types import SimpleNamespace
import support


class ListenerObservationTests(unittest.TestCase):
    def test_empty_exit_one_is_the_only_clean_observation(self):
        self.assertEqual(support.parse_listeners(1, b'', b'', [12345])['outcome'], 'PASS')
        for code, out, err in [(0, b'', b''), (1, b'p123\n', b''),
                               (1, b'', b'permission denied'), (2, b'', b'')]:
            self.assertEqual(support.parse_listeners(code, out, err, [12345])['outcome'], 'UNKNOWN')

    def test_ipv4_ipv6_and_wildcard_listeners_are_reported(self):
        out = b'p123\nf7\nn127.0.0.1:12345\nf8\nn[::1]:12345\np456\nf4\nn*:23456\n'
        result = support.parse_listeners(0, out, b'', [12345, 23456])
        self.assertEqual(result['outcome'], 'FAIL')
        self.assertEqual(result['listeners'], [{'pid': 123, 'port': 12345}, {'pid': 456, 'port': 23456}])

    def test_unexpected_or_partial_records_never_mean_clean(self):
        for out in [b'n*:12345\n', b'p123\nf7\nn*:54321\n', b'p123\nf7\n',
                    b'p123\nf7\nn*:12345\nBAD\n', b'p123\nf7\nn*:12345->a:2\n', b'p123\nf7\nn*:12345\np456\n',
                    b'p123\nf7\nn10.0.0.1:4000->10.0.0.2:12345\n']:
            self.assertEqual(support.parse_listeners(0, out, b'', [12345])['outcome'], 'UNKNOWN')

    def test_uses_listen_filter_without_a_bind_probe(self):
        with patch('support.shutil.which', return_value='/usr/sbin/lsof'), patch('support.subprocess.run') as run:
            run.return_value = SimpleNamespace(returncode=1, stdout=b'', stderr=b'')
            self.assertEqual(support.observe_listeners([12345])['outcome'], 'PASS')
            self.assertEqual(run.call_args.args[0], ['/usr/sbin/lsof', '-nP', '-iTCP:12345', '-sTCP:LISTEN', '-F', 'pfn'])

    def test_missing_tool_or_timeout_remains_unknown(self):
        with patch('support.shutil.which', return_value=None):
            self.assertEqual(support.observe_listeners([12345])['outcome'], 'UNKNOWN')
        with patch('support.shutil.which', return_value='/usr/sbin/lsof'), patch('support.subprocess.run', side_effect=support.subprocess.TimeoutExpired('lsof', 5)):
            self.assertEqual(support.observe_listeners([12345])['outcome'], 'UNKNOWN')

    def test_invalid_ports_are_rejected_before_inspection(self):
        for ports in [[], [0], [65536], [True], ['12345']]:
            with self.assertRaises(support.Failure):
                support.observe_listeners(ports)
