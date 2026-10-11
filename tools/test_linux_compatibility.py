"""Fast, offline checks for ABI gates and the shared candidate workflow."""
import io
import json
import os
import sys
import subprocess
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from tools import linux_compatibility as linux

ROOT = Path(__file__).resolve().parents[1]


class LinuxCompatibility(unittest.TestCase):
    def test_shared_smoke_uses_supplied_binary_without_package_manager_or_rebuild(self):
        parent = ROOT / 'target/linux-ci-unit'
        parent.mkdir(parents=True, exist_ok=True)
        part = dict(product_version='1.0.0-alpha.5', source_commit='a'*40,
                    source_manifest_sha256='b'*64, supported_formats={}, default_formats={})
        package_id, digest = '0'*28 + '1236', 'c'*64
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            root = Path(directory)
            binary = root/'candidate'
            def respond(args, **kwargs):
                self.assertEqual(args[0], binary)
                command = list(map(str,args[1:]))
                out = '{}'
                if command == ['--format','json','--version']: out = json.dumps({'result':part})
                elif command == ['--format','json','--help']: out = json.dumps({'result':{'usage':'help\n'}})
                elif command == ['--help']: out = 'help\n'
                elif command[:4] == ['--format','json','pack','install']:
                    out = json.dumps({'result': {'revision': {
                        'package_id': package_id, 'content_digest': 'sha256:' + digest}}})
                elif command[:2] == ['pack','install']:
                    self.fail('Revision identity must come from JSON, not human installation text')
                elif command[:2] == ['instance','create']:
                    self.assertEqual(command, ['instance','create','debian-ci','--revision',
                                               package_id + ':' + digest])
                elif command[:1] == ['invoke']: out = 'debian12-hook-ok\n'
                elif command[:4] == ['--format','json','instance','delete']: out = json.dumps({'result':{'run':{'state':{'outcome':'succeeded'}}}})
                return subprocess.CompletedProcess(args, kwargs.get('code',0), out.encode(), b'')
            with patch.object(linux,'run',side_effect=respond):
                result = linux.smoke(binary,root/'work',part)
            self.assertTrue(any(x['exit_code']==1 for x in result))
            self.assertTrue(any(x['command'][:4] == ['--format','json','pack','install'] for x in result))

    @unittest.skipIf(os.name == 'nt', 'POSIX Homebrew entrypoint semantics')
    def test_homebrew_entrypoint_is_not_dereferenced_into_repository_prefix(self):
        parent = ROOT / 'target/linux-ci-unit'
        parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            root = Path(directory)
            actual = root / 'Homebrew/bin/brew'
            actual.parent.mkdir(parents=True)
            actual.touch()
            entry = root / 'bin/brew'
            entry.parent.mkdir()
            entry.symlink_to(actual)
            with patch.object(sys, 'argv', ['linux', 'brew', '--work', str(root/'work'), '--artifacts', str(root/'artifacts'), '--manager', str(entry)]), patch.object(linux, 'brew') as invoked:
                linux.main()
            self.assertEqual(invoked.call_args.args[2], entry)
            self.assertNotEqual(invoked.call_args.args[2], actual)

    def test_glibc_requirements_include_weak_newer_symbols_and_sort_numerically(self):
        self.assertEqual(linux.needed_glibc('Name: GLIBC_2.9\nName: GLIBC_2.36\nName: GLIBC_2.2.5'),
                         ['2.2.5', '2.9', '2.36'])
        for text in ['Name: GLIBC_2.39', 'Name: GLIBC_2.36\nName: GLIBC_2.39',
                     'Name: GLIBC_2.36\nName: GLIBC_PRIVATE', '', 'Name: GLIBCXX_3.4.30']:
            with self.subTest(text=text), self.assertRaises(ValueError):
                linux.needed_glibc(text)

    def test_formula_staging_changes_transport_only_not_dependencies_or_install(self):
        template = (ROOT / 'Formula/pactrun-preview.rb').read_text()
        changed = linux.stage_formula(template, 'http://127.0.0.1:12345/candidate.tar.gz', '1.0.0-alpha.3', 'a' * 64)
        strip = lambda s: '\n'.join(line for line in s.splitlines() if not line.startswith(('  url ', '  version ', '  sha256 ')))
        self.assertEqual(strip(template), strip(changed))
        for url in ["https://example.invalid/x'", 'https://example.invalid/x\n']:
            with self.assertRaises(ValueError):
                linux.stage_formula(template, url, '1.0.0', 'a' * 64)
        with self.assertRaises(ValueError):
            linux.stage_formula(template.replace('  sha256 ', '  missing '), 'https://example.invalid/x', '1.0.0', 'a' * 64)

    def test_archive_refuses_traversal_links_and_duplicates_before_extraction(self):
        parent = ROOT / 'target/linux-ci-unit'
        parent.mkdir(parents=True, exist_ok=True)
        for scenario in ['traversal', 'absolute', 'symlink', 'hardlink', 'duplicate']:
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory(dir=parent) as directory:
                root = Path(directory)
                archive = root / 'input.tar'
                with tarfile.open(archive, 'w') as out:
                    entry = tarfile.TarInfo('../escape' if scenario == 'traversal' else '/escape' if scenario == 'absolute' else 'file')
                    if scenario in ['symlink', 'hardlink']:
                        entry.type = tarfile.SYMTYPE if scenario == 'symlink' else tarfile.LNKTYPE
                        entry.linkname = '../outside'
                        out.addfile(entry)
                    else:
                        entry.size = 1
                        out.addfile(entry, io.BytesIO(b'x'))
                        if scenario == 'duplicate':
                            out.addfile(entry, io.BytesIO(b'x'))
                with self.assertRaises(ValueError):
                    linux.unpack(archive, root / 'output')
                self.assertFalse((root / 'output').exists())

    def test_ci_and_release_use_one_linux_build_and_both_runtime_gates(self):
        ci = (ROOT / '.github/workflows/ci.yml').read_text()
        candidate = (ROOT / '.github/workflows/release-candidate.yml').read_text()
        shared = (ROOT / '.github/workflows/linux-compatibility.yml').read_text()
        self.assertIn('uses: ./.github/workflows/linux-compatibility.yml', ci)
        self.assertIn('uses: ./.github/workflows/linux-compatibility.yml', candidate)
        self.assertNotIn('platform: linux-x86_64', candidate)
        self.assertIn('needs: [scope, ubuntu, windows, debian12]', ci)
        self.assertEqual(shared.count('needs: build'), 2)
        self.assertEqual(shared.count('uses: actions/download-artifact@v4'), 2)
        self.assertEqual(shared.count('debian:12-slim@sha256:'), 2)
        self.assertIn('rust:1.98.1-slim-bookworm@sha256:', shared)
        self.assertIn('RUSTUP_TOOLCHAIN: 1.98.1', shared)
        self.assertIn('runuser -u pactrun-ci -- python3 -B -m tools.linux_compatibility direct', shared)
        self.assertIn('runuser -u pactrun-ci -- python3 -B -m tools.linux_compatibility brew', shared)
        self.assertIn('contents: read', shared)
        self.assertNotIn('contents: write', shared)
        self.assertNotIn('--ignore-dependencies', shared)
        self.assertNotIn('LD_LIBRARY_PATH:', shared)
        self.assertNotIn('${{ runner.temp }}', shared)
        self.assertIn('git config --global --add safe.directory "$GITHUB_WORKSPACE"', shared)


if __name__ == '__main__':
    unittest.main()
