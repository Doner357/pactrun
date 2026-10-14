"""Offline measurement checks; all fixture state stays inside target/."""
import json
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from tools.measure_command import ROOT, measure, source_identity


class MeasurementTests(unittest.TestCase):
    def setUp(self):
        (ROOT / 'target').mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix='measure-test-', dir=ROOT / 'target')
        self.addCleanup(self.temporary.cleanup)
        self.output = Path(self.temporary.name) / 'receipt.json'

    def test_success_records_source_elapsed_and_output(self):
        result = measure([sys.executable, '-B', '-c', 'print("measured")'], self.output, 'success')
        self.assertTrue(result['completed'])
        self.assertEqual(result['exit_code'], 0)
        self.assertGreaterEqual(result['elapsed_seconds'], 0)
        self.assertIn(result['source']['kind'], ('git', 'unavailable'))
        if result['source']['kind'] == 'git':
            self.assertEqual(len(result['source']['commit']), 40)
        self.assertEqual(json.loads(self.output.read_text()), result)
        self.assertIn('measured', self.output.with_suffix('.log').read_text())

    def test_nonzero_exit_and_receipt_are_preserved(self):
        result = measure([sys.executable, '-B', '-c', 'raise SystemExit(7)'], self.output, 'failure')
        self.assertTrue(result['completed'])
        self.assertEqual(result['exit_code'], 7)
        before = self.output.read_bytes()
        with self.assertRaises(FileExistsError):
            measure([sys.executable, '-B', '-c', 'raise SystemExit(0)'], self.output, 'rerun')
        self.assertEqual(self.output.read_bytes(), before)

    def test_spawn_failure_retains_incomplete_receipt(self):
        result = measure([str(self.output.parent / 'missing-executable')], self.output, 'missing')
        self.assertFalse(result['completed'])
        self.assertEqual(result['exit_code'], 127)
        self.assertIn('error_type', result)

    def test_existing_log_is_not_overwritten_or_executed(self):
        self.output.with_suffix('.log').write_text('previous')
        result = measure([sys.executable, '-B', '-c', 'raise SystemExit(0)'], self.output, 'collision')
        self.assertFalse(result['completed'])
        self.assertEqual(self.output.with_suffix('.log').read_text(), 'previous')

    def test_source_archive_never_claims_a_parent_checkout(self):
        with patch('tools.measure_command.subprocess.check_output', return_value=str(ROOT.parent).encode()):
            self.assertEqual(source_identity(), {'kind': 'unavailable', 'reason': 'not-this-source-root'})

    def test_missing_git_is_explicit_not_a_fabricated_commit(self):
        with patch('tools.measure_command.subprocess.check_output',
                   side_effect=subprocess.CalledProcessError(128, ['git'])):
            self.assertEqual(source_identity(), {'kind': 'unavailable', 'reason': 'git-metadata-unavailable'})


if __name__ == '__main__':
    unittest.main()
