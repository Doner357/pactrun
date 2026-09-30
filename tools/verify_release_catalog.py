"""Verify checked-in package files against validated immutable release records."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def compare_files(root, generated):
    expected = {p.relative_to(generated).as_posix(): p for p in generated.rglob('*') if p.is_file()}
    actual = {p.relative_to(root).as_posix(): p for directory in ('bucket', 'Formula', 'releases')
              for p in (root / directory).rglob('*') if p.is_file()}
    if actual.keys() != expected.keys():
        raise ValueError('Missing or extra generated package/catalog file')
    for name in expected:
        if actual[name].is_symlink() or actual[name].read_text(encoding='utf-8') != expected[name].read_text(encoding='utf-8'):
            raise ValueError('Generated package file differs: ' + name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--xtask', type=Path, default=Path('target/debug/xtask'))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    target = root / 'target'
    target.mkdir(exist_ok=True)
    catalog = root / 'releases/catalog.json'
    records = json.loads(catalog.read_text(encoding='utf-8'))
    for release in records:
        prefix = 'https://github.com/Doner357/pactrun/releases/download/v' + release['product_version'] + '/'
        if any(not item['url'].startswith(prefix) for item in release['artifacts']):
            raise ValueError('Published catalog must use its versioned public Release assets')
    with tempfile.TemporaryDirectory(prefix='catalog-check-', dir=target) as directory:
        work = Path(directory)
        command = [str(args.xtask.resolve()), 'release-catalog', str(catalog), str(work / 'generated')]
        event_path = os.environ.get('GITHUB_EVENT_PATH')
        if event_path:
            event = json.loads(Path(event_path).read_text(encoding='utf-8'))
            base = event.get('pull_request', {}).get('base', {}).get('sha') or event.get('before')
            if base and base != '0' * 40:
                if not re.fullmatch('[0-9a-f]{40}', base):
                    raise ValueError('Invalid previous Git revision')
                subprocess.run(['git', 'cat-file', '-e', base], cwd=root, check=True)
                exists = subprocess.check_output(['git', 'ls-tree', '--name-only', base, '--', 'releases/catalog.json'], cwd=root)
                if exists.strip():
                    prior = work / 'previous.json'
                    prior.write_bytes(subprocess.check_output(['git', 'show', base + ':releases/catalog.json'], cwd=root))
                    command.append(str(prior))
        subprocess.run(command, cwd=root, check=True)
        compare_files(root, work / 'generated')
    print('Public catalog, package definitions and previous release identities verified.')


if __name__ == '__main__':
    main()
