"""Pure and offline negative tests for reuse; never contacts GitHub."""
import hashlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from tools import ci_reuse as reuse


class ReuseTests(unittest.TestCase):
    def setUp(self):
        (reuse.ROOT / 'target').mkdir(exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix='reuse-test-', dir=reuse.ROOT / 'target')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.current = {'key': 'a' * 64, 'inputs': {'platform': 'linux'},
                        'commit': 'b' * 40, 'tree': 'c' * 40, 'run_id': 20, 'attempt': 1}
        self.run = {'id': 10, 'run_attempt': 2, 'status': 'completed', 'conclusion': 'success',
                    'path': '.github/workflows/ci.yml', 'event': 'pull_request',
                    'head_repository': {'full_name': 'owner/repo'}}
        self.receipt = dict(self.current, format=reuse.FORMAT, verified=True, run_id=10, attempt=2)

    def test_only_separately_validated_delivery_metadata_is_excluded(self):
        original = b'100644 blob aaa\tsrc/main.rs\0'
        self.assertEqual(reuse.input_digest(original), reuse.input_digest(original + b'100644 blob bbb\treleases/catalog.json\0'))
        for path in [b'docs/spec/contract.md', b'tools/native_package_acceptance.py',
                     b'.config/nextest.toml', b'Cargo.lock', b'unknown', b'.github/workflows/ci.yml']:
            self.assertNotEqual(reuse.input_digest(original), reuse.input_digest(original + b'100644 blob ccc\t' + path + b'\0'))
        self.assertNotEqual(reuse.input_digest(original), reuse.input_digest(original.replace(b'aaa', b'ddd')))

    def test_only_successful_same_repository_expected_workflow_runs_are_eligible(self):
        self.assertTrue(reuse.acceptable_run(self.run, 'owner/repo', 20))
        for change in [{'conclusion': 'failure'}, {'conclusion': 'cancelled'}, {'status': 'in_progress'},
                       {'path': '.github/workflows/other.yml'}, {'head_repository': None},
                       {'head_repository': {'full_name': 'fork/repo'}}, {'id': 20}, {'event': 'workflow_run'}]:
            self.assertFalse(reuse.acceptable_run(dict(self.run, **change), 'owner/repo', 20))

    def test_receipt_binds_full_inputs_and_attempt_not_only_a_hash_or_filename(self):
        self.assertTrue(reuse.matches(self.receipt, self.current, self.run))
        for change in [{'verified': False}, {'format': 'unknown'}, {'key': 'x'}, {'run_id': 9},
                       {'attempt': 1}, {'inputs': {'platform': 'windows'}}, {'commit': 'short'}]:
            self.assertFalse(reuse.matches(dict(self.receipt, **change), self.current, self.run))
        self.assertFalse(reuse.matches([], self.current, self.run))

    def test_binary_api_response_is_captured_not_rendered(self):
        with patch.object(reuse, 'command', return_value=b'zip') as command:
            self.assertEqual(reuse.api('repos/owner/repo/actions/artifacts/1/zip'), b'zip')
            self.assertIn('--allow-escape-sequences', command.call_args.args)

    def test_forks_and_non_hosted_environments_are_not_reusable(self):
        event = self.root / 'event.json'
        event.write_text(json.dumps({'pull_request': {'head': {'repo': {'full_name': 'fork/repo'}}}}))
        with patch.dict(os.environ, GITHUB_REPOSITORY='owner/repo', GITHUB_EVENT_NAME='pull_request', GITHUB_EVENT_PATH=str(event)):
            with self.assertRaises(ValueError):
                reuse.context('linux')
        with patch.dict(os.environ, GITHUB_REPOSITORY='owner/repo', GITHUB_EVENT_NAME='push', PACTRUN_RUNNER_ENVIRONMENT='self-hosted'):
            with self.assertRaises(ValueError):
                reuse.context('linux')

    def archive(self, members):
        data = io.BytesIO()
        with zipfile.ZipFile(data, 'w') as archive:
            for name, content in members.items():
                archive.writestr(name, content)
        return data.getvalue()

    def test_archive_is_read_without_extraction_and_rejects_extra_or_oversized_members(self):
        payload = json.dumps(self.receipt)
        self.assertEqual(reuse.receipt_from_zip(self.archive({'runtime.json': payload})), self.receipt)
        for members in [{'../runtime.json': payload}, {'runtime.json': payload, 'other': ''},
                        {'runtime.json': '[]'}, {'runtime.json': 'x' * (reuse.MAX_RECEIPT + 1)}]:
            with self.assertRaises(ValueError):
                reuse.receipt_from_zip(self.archive(members))
        with self.assertRaises(ValueError):
            reuse.receipt_from_zip(b'x' * (reuse.MAX_ZIP + 1))

    def test_record_requires_a_successful_clean_runtime_measurement(self):
        source = {'kind': 'git', 'commit': self.current['commit'], 'tree': self.current['tree'],
                  'tracked_diff_sha256': hashlib.sha256(b'').hexdigest(), 'untracked_paths': []}
        data = {'completed': True, 'exit_code': 0, 'source': source,
                'command': ['cargo', 'xtask', 'rust-ci'], 'elapsed_seconds': 10}
        path = self.root / 'measurement.json'
        path.write_text(json.dumps(data))
        self.assertTrue(reuse.record(self.current, path)['verified'])
        for change in [{'completed': False}, {'exit_code': 1}, {'exit_code': False},
                       {'command': ['cargo', 'test', 'one_test']},
                       {'source': dict(source, tracked_diff_sha256='dirty')},
                       {'source': dict(source, untracked_paths=['src/generated.rs'])},
                       {'source': dict(source, commit='d' * 40)}]:
            path.write_text(json.dumps(dict(data, **change)))
            with self.assertRaises(ValueError):
                reuse.record(self.current, path)

    def test_manual_comparisons_never_reuse_and_lookup_failure_requests_fresh_tests(self):
        output = self.root / 'github-output'
        with patch.dict(os.environ, GITHUB_OUTPUT=str(output), GITHUB_EVENT_NAME='workflow_dispatch'), \
             patch.object(reuse, 'context', return_value=self.current), patch.object(reuse, 'lookup') as lookup:
            self.assertEqual(reuse.main(['check', '--platform', 'linux']), 0)
            lookup.assert_not_called()
        self.assertEqual(output.read_text(), 'reused=false\n')
        with patch.dict(os.environ, GITHUB_EVENT_NAME='push', GITHUB_OUTPUT=str(output)), \
             patch.object(reuse, 'context', return_value=self.current), \
             patch.object(reuse, 'lookup', side_effect=OSError('unavailable')):
            self.assertEqual(reuse.main(['check', '--platform', 'linux']), 0)
        self.assertEqual(output.read_text(), 'reused=false\nreused=false\n')


if __name__ == '__main__':
    unittest.main()
