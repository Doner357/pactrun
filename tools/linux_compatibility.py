"""Build once on Bookworm; qualify those archives without rebuilding on test hosts.

CI-only tooling. No published asset/catalog mutation or host libc replacement.
Run as `python3 -m tools.linux_compatibility` from the checked-out source.
"""
import argparse
import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tarfile
import threading
import tomllib
import urllib.request

from tools import release_artifacts as release

ROOT = Path(__file__).resolve().parents[1]
MAX_GLIBC = (2, 36)
RUST_VERSION = '1.98.1'
CONTROL_URL = 'https://github.com/Doner357/pactrun/releases/download/v1.0.0-alpha.3/pactrun-1.0.0-alpha.3-linux-x86_64.tar.gz'
CONTROL_SHA256 = '458e863385984e7205cba384b41bcab2c778afe0c84a1e50ca160d9b06782255'


def run(args, *, cwd=None, env=None, code=0, stdin=None, log=None):
    result = subprocess.run(list(map(str, args)), cwd=cwd, env=env, input=stdin,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=900)
    if log is not None:
        log.write_bytes(result.stdout + result.stderr)
    if result.returncode != code:
        raise RuntimeError(f'{args[0]} exit {result.returncode}, expected {code}:\n'
                           + result.stdout.decode(errors='replace')[-4000:]
                           + result.stderr.decode(errors='replace')[-4000:])
    return result


def environment():
    os_release = dict(line.split('=', 1) for line in Path('/etc/os-release').read_text().splitlines() if '=' in line)
    libc = run(['/usr/bin/getconf', 'GNU_LIBC_VERSION']).stdout.decode().strip()
    if os_release.get('ID', '').strip('"') != 'debian' or os_release.get('VERSION_ID', '').strip('"') != '12' or libc != 'glibc 2.36' or platform.machine() != 'x86_64':
        raise ValueError('This gate requires actual Debian 12 x86-64 userland and system glibc 2.36')
    return {'os_release': os_release, 'system_libc': libc, 'kernel': platform.release(),
            'kernel_scope': 'shared CI host kernel, not Debian 12 VM certification', 'uid': os.getuid()}


def needed_glibc(text):
    if 'GLIBC_PRIVATE' in text:
        raise ValueError('Private glibc ABI is not a supported runtime contract')
    versions = sorted({tuple(map(int, x.split('.'))) for x in re.findall(r'Name:\s+GLIBC_([0-9]+(?:\.[0-9]+)+)\b', text)})
    if not versions:
        raise ValueError('Missing glibc version requirements; do not silently certify an unknown ELF')
    if versions[-1] > MAX_GLIBC:
        raise ValueError(f'glibc requirement {versions[-1]} exceeds Debian 12 baseline {MAX_GLIBC}')
    return ['.'.join(map(str, v)) for v in versions]


def audit_elf(path):
    if path.read_bytes()[:6] != b'\x7fELF\x02\x01':
        raise ValueError('Expected a native ELF64 little-endian executable')
    header = run(['readelf', '-hW', path]).stdout.decode()
    if 'Advanced Micro Devices X86-64' not in header:
        raise ValueError('Expected x86-64 ELF machine')
    versions = needed_glibc(run(['readelf', '--version-info', '-W', path]).stdout.decode())
    program_headers = run(['readelf', '-lW', path]).stdout.decode()
    interpreter = re.search(r'Requesting program interpreter:\s*([^\]]+)\]', program_headers)
    if not interpreter or interpreter[1] != '/lib64/ld-linux-x86-64.so.2':
        raise ValueError('Unexpected ELF interpreter; do not mask compatibility with a private loader')
    dynamic = run(['readelf', '-dW', path]).stdout.decode()
    if '(RPATH)' in dynamic or '(RUNPATH)' in dynamic:
        raise ValueError('Unexpected embedded library search path')
    return {'sha256': release.sha(path), 'interpreter': interpreter[1], 'glibc_versions': versions,
            'needed_libraries': re.findall(r'\(NEEDED\).*?\[([^\]]+)\]', dynamic)}


def unpack(archive, destination):
    """Only regular files and relative directory entries; never follow links."""
    with tarfile.open(archive) as data:
        entries = data.getmembers()
        names = set()
        for item in entries:
            if not release.safe_name(item.name) or Path(item.name).is_absolute() or item.name in names or not (item.isfile() or item.isdir()):
                raise ValueError('Unsafe or duplicate archive entry')
            names.add(item.name)
        destination.mkdir(exist_ok=False)
        for item in entries:
            path = destination / item.name
            if item.isdir():
                path.mkdir(parents=True, exist_ok=True)
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                with data.extractfile(item) as source, path.open('xb') as out:
                    shutil.copyfileobj(source, out)
                path.chmod(0o755 if item.mode & 0o111 else 0o644)


def build(work, candidate_version):
    info = environment()
    compiler = run(['rustc', '--version']).stdout.decode().strip()
    if not compiler.startswith('rustc ' + RUST_VERSION + ' '):
        raise ValueError('Unexpected release compiler')
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    if candidate_version:
        release.qualify_candidate_source(ROOT, os.environ['REQUESTED_SHA'], candidate_version,
                                         os.environ['GITHUB_REF'], os.environ['GITHUB_SHA'])
        if version != candidate_version:
            raise ValueError('Cargo version differs from requested release candidate')
    work.mkdir(exist_ok=False)
    snapshot, source, output = [work / part for part in ['snapshot', 'source', 'output']]
    release.source(ROOT, snapshot)
    unpack(snapshot / 'source.tar.gz', source)
    fetched = run(['cargo', 'fetch', '--locked', '--manifest-path', source / 'Cargo.toml', '--target', 'x86_64-unknown-linux-gnu'])
    (work / 'cargo-fetch.log').write_bytes(fetched.stdout + fetched.stderr)
    url = f'https://github.com/Doner357/pactrun/releases/download/v{version}' if candidate_version else 'https://example.invalid/pactrun-ci'
    release.build(source, snapshot, output, 'linux-x86_64', url)
    binaries = output / 'cargo-target/x86_64-unknown-linux-gnu/release'
    info.update({'rustc': compiler, 'glibc_ceiling': '2.36', 'binaries': {
        name: audit_elf(binaries / name) for name in ['pactrun', 'pactrun-launcher']}})
    (work / 'linux-abi.json').write_text(json.dumps(info, indent=2) + '\n')


def candidate(artifacts):
    part = json.loads((artifacts / 'output/release-part.json').read_text())
    provenance = json.loads((artifacts / 'snapshot/source.json').read_text())
    if any(part[k] != v for k, v in provenance.items()) or release.sha(artifacts / 'snapshot/source.sha256') != part['source_manifest_sha256']:
        raise ValueError('Candidate source provenance mismatch')
    checks = json.loads((artifacts / 'output/artifact-checksums.json').read_text())
    for name, expected in checks.items():
        if not release.safe_name(name) or Path(name).name != name:
            raise ValueError('Expected a flat artifact name')
        file = artifacts / 'output/assets' / name
        if release.sha(file) != expected['sha256'] or file.stat().st_size != expected['bytes']:
            raise ValueError('Candidate archive differs from qualified checksum')
    return part, json.loads((artifacts / 'linux-abi.json').read_text())


def smoke(program, work, part):
    work.mkdir(exist_ok=False)
    env = dict(os.environ, PACTRUN_STORAGE_ROOT=str(work / 'store'))
    for key in ['LD_LIBRARY_PATH', 'LD_PRELOAD', 'LD_AUDIT']:
        env.pop(key, None)
    observations = []

    def cli(*args, code=0, stdin=None):
        result = run([program, *args], cwd=work, env=env, code=code, stdin=stdin)
        observations.append({'command': list(map(str, args)), 'exit_code': result.returncode})
        return result.stdout.decode()

    info = json.loads(cli('--format', 'json', '--version'))['result']
    for key in ['product_version', 'source_commit', 'source_manifest_sha256', 'supported_formats', 'default_formats']:
        if info[key] != part[key]:
            raise ValueError('Executed binary is not the candidate: ' + key)
    human = cli('--help')
    if json.loads(cli('--format', 'json', '--help'))['result']['usage'] != human:
        raise ValueError('Human/machine help differ')
    if (work / 'store').exists():
        raise ValueError('Startup-only probes created managed data')
    for name in ['database', 'runtime-content', 'staging']:
        (work / 'store' / name).mkdir(parents=True)
    pack = work / 'pack'
    pack.mkdir()
    (pack / 'pactrun.yaml').write_text('''source_format: 1.0-alpha.1
package_id: 00000000000000000000000000001236
revision:
  inputs: [{id: secret, protection: secret}]
  service_storages: [{id: state}]
  actions:
    - id: inspect
      access: observe
      parameters: []
      outputs: []
      hook:
        protocol_version: 1.0-alpha.1
        launch: {kind: shell_loader, shell: sh, command: sh, script: script}
        args: []
        io: {terminal: output}
runtime_content:
  files: [{id: script, source: inspect.sh, path: inspect.sh, executable: false}]
''')
    (pack / 'inspect.sh').write_text('"$PACTRUN_EXECUTABLE" hook session >/dev/null || exit 1\nprintf "debian12-hook-ok\\n"\n')
    revision = cli('pack', 'install', pack).splitlines()[0]
    cli('instance', 'create', 'debian-ci', '--revision', revision)
    cli('input', 'set', 'debian-ci', 'secret', '--stdin', stdin=b'ci-only-secret-value')
    denied = work / 'denied-secret'
    cli('input', 'export', 'debian-ci', 'secret', '--output', denied, code=1)
    if denied.exists():
        raise ValueError('Unauthorized Secret export created a file')
    cli('invoke', 'debian-ci', 'inspect', '--plan')
    if 'debian12-hook-ok' not in cli('invoke', 'debian-ci', 'inspect'):
        raise ValueError('Actual packaged Hook did not complete')
    cli('instance', 'delete', 'debian-ci', '--plan')
    deleted = json.loads(cli('--format', 'json', 'instance', 'delete', 'debian-ci'))
    if deleted['result']['run']['state']['outcome'] != 'succeeded':
        raise ValueError('Ordinary-user storage finalization did not succeed')
    return observations


def direct(artifacts, work):
    host = environment()
    if os.getuid() == 0 or shutil.which('cargo') or shutil.which('brew'):
        raise ValueError('Direct gate requires a non-root runtime without Cargo or Homebrew')
    work.mkdir(exist_ok=False)
    part, audit = candidate(artifacts)
    cases = {}
    for flavor in ['normal', 'test', 'standalone']:
        stem = 'pactrun-test' if flavor == 'test' else 'pactrun'
        suffix = '-standalone' if flavor == 'standalone' else ''
        archive = artifacts / f'output/assets/{stem}-{part["product_version"]}-linux-x86_64{suffix}.tar.gz'
        install = work / flavor
        unpack(archive, install)
        payload = install / ('pactrun' if flavor == 'standalone' else 'libexec/pactrun')
        if release.sha(payload) != audit['binaries']['pactrun']['sha256']:
            raise ValueError('Archive payload is not the ABI-audited executable')
        entry = payload if flavor == 'standalone' else install / 'bin' / stem
        if flavor != 'standalone' and release.sha(entry) != audit['binaries']['pactrun-launcher']['sha256']:
            raise ValueError('Archive launcher is not the ABI-audited executable')
        cases[flavor] = smoke(entry, work / (flavor + '-data'), part)
    # A real regression control: the immutable published alpha.3 archive must
    # still fail for the original reason, not for unrelated permissions or setup.
    control = work / 'published-alpha3.tar.gz'
    with urllib.request.urlopen(CONTROL_URL, timeout=120) as response, control.open('xb') as out:
        shutil.copyfileobj(response, out)
    if release.sha(control) != CONTROL_SHA256:
        raise ValueError('Published regression control checksum mismatch')
    unpack(control, work / 'control')
    result = subprocess.run([str(work / 'control/libexec/pactrun'), '--version'], capture_output=True, timeout=30)
    if result.returncode == 0 or b'GLIBC_2.39' not in result.stderr or b'not found' not in result.stderr:
        raise ValueError('Regression control did not reproduce the reported glibc mismatch')
    host.update({'candidate': part['source_commit'], 'cases': cases, 'published_alpha3_failure_reproduced': True})
    (work / 'result.json').write_text(json.dumps(host, indent=2) + '\n')


def stage_formula(template, url, version, digest):
    for field, value in [('url', url), ('version', version), ('sha256', digest)]:
        if "'" in value or '\n' in value or '\r' in value:
            raise ValueError('Unsafe candidate formula value')
        template, count = re.subn(r"(?m)^  " + field + r" '[^']*'$", f"  {field} '{value}'", template)
        if count != 1:
            raise ValueError('Expected one checked-in formula field: ' + field)
    return template


def brew(artifacts, work, manager):
    host = environment()
    if os.getuid() == 0 or shutil.which('cargo'):
        raise ValueError('Homebrew gate must not run as root or rebuild the payload')
    work.mkdir(exist_ok=False)
    part, audit = candidate(artifacts)
    normal = next(x for x in part['artifacts'] if x['flavor'] == 'normal')
    name = normal['url'].rsplit('/', 1)[1]
    env = dict(os.environ, CI='1', NONINTERACTIVE='1', HOMEBREW_NO_AUTO_UPDATE='1',
               HOMEBREW_NO_ANALYTICS='1', HOMEBREW_NO_ENV_HINTS='1',
               GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM='1')
    env.pop('PACTRUN_STORAGE_ROOT', None)
    for key in ['LD_LIBRARY_PATH', 'LD_PRELOAD', 'LD_AUDIT']:
        env.pop(key, None)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(
        http.server.SimpleHTTPRequestHandler, directory=str(artifacts / 'output/assets')))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        tap = work / 'tap'
        (tap / 'Formula').mkdir(parents=True)
        template = (ROOT / 'Formula/pactrun-preview.rb').read_text()
        (tap / 'Formula/pactrun-preview.rb').write_text(stage_formula(template,
            f'http://127.0.0.1:{server.server_port}/{name}', part['product_version'], normal['sha256']))
        for command in [['init', '--initial-branch=main'], ['add', 'Formula'],
                        ['-c', 'user.name=Pactrun CI', '-c', 'user.email=ci@example.invalid', 'commit', '-m', 'Candidate transport only']]:
            run(['git', '-C', tap, *command], env=env)
        run([manager, 'tap', 'pactrun-ci/acceptance', tap.as_uri()], env=env)
        before = run([manager, 'list', '--versions'], env=env).stdout.decode()
        (work / 'dependencies-before.txt').write_text(before)
        run([manager, 'install', 'pactrun-ci/acceptance/pactrun-preview'], env=env, log=work / 'install.log')
        after = run([manager, 'list', '--versions'], env=env).stdout.decode()
        (work / 'dependencies-after.txt').write_text(after)
        prefix = Path(run([manager, '--prefix', 'pactrun-ci/acceptance/pactrun-preview'], env=env).stdout.decode().strip())
        if release.sha(prefix / 'libexec/pactrun') != audit['binaries']['pactrun']['sha256']:
            raise ValueError('Homebrew installed a different executable')
        if release.sha(prefix / 'bin/pactrun') != audit['binaries']['pactrun-launcher']['sha256']:
            raise ValueError('Homebrew installed a different launcher')
        cases = smoke(prefix / 'bin/pactrun', work / 'managed-test', part)
        run([manager, 'test', 'pactrun-ci/acceptance/pactrun-preview'], env=env)
        run([manager, 'uninstall', 'pactrun-ci/acceptance/pactrun-preview'], env=env)
        if environment()['system_libc'] != host['system_libc']:
            raise ValueError('System libc changed during Homebrew validation')
        host.update({'candidate': part['source_commit'], 'cases': cases, 'dependencies_before': before,
                     'dependencies_after': after, 'no_dependency_bypass': True, 'same_payload_and_launcher': True})
        (work / 'result.json').write_text(json.dumps(host, indent=2) + '\n')
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['build', 'direct', 'brew'])
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--artifacts', type=Path)
    parser.add_argument('--candidate-version', default='')
    parser.add_argument('--manager', type=Path)
    args = parser.parse_args()
    if args.operation != 'build' and args.artifacts is None:
        parser.error('runtime gates require --artifacts')
    if args.operation == 'brew' and args.manager is None:
        parser.error('Homebrew gate requires --manager')
    if args.operation == 'build':
        build(args.work.resolve(), args.candidate_version)
    elif args.operation == 'direct':
        direct(args.artifacts.resolve(), args.work.resolve())
    else:
        brew(args.artifacts.resolve(), args.work.resolve(), args.manager.resolve())


if __name__ == '__main__':
    main()
