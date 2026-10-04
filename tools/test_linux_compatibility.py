"""Fast, offline checks for ABI gates and the shared candidate workflow."""
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

from tools import linux_compatibility as linux

ROOT = Path(__file__).resolve().parents[1]


class LinuxCompatibility(unittest.TestCase):
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
        self.assertIn('runuser -u pactrun-ci -- python3 -B -m tools.linux_compatibility direct', shared)
        self.assertIn('runuser -u pactrun-ci -- python3 -B -m tools.linux_compatibility brew', shared)
        self.assertIn('contents: read', shared)
        self.assertNotIn('contents: write', shared)
        self.assertNotIn('--ignore-dependencies', shared)
        self.assertNotIn('LD_LIBRARY_PATH:', shared)


if __name__ == '__main__':
    unittest.main()
