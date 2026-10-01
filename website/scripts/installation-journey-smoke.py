"""Run the published standalone setup and fresh-store path using supplied artifacts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--archive', required=True)
parser.add_argument('--checksum', required=True)
parser.add_argument('--fixture-binary', required=True)
parser.add_argument('--workspace', required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
archive = Path(args.archive).resolve(strict=True)
fixture_binary = Path(args.fixture_binary).resolve(strict=True)
assert hashlib.sha256(archive.read_bytes()).hexdigest() == args.checksum.lower(), 'Artifact does not match supplied trusted checksum'
base = Path(args.workspace).resolve()
base.mkdir(parents=True, exist_ok=True)
work = Path(tempfile.mkdtemp(prefix='install-journey-', dir=base))
windows = os.name == 'nt'
language = 'powershell' if windows else 'sh'
shell = 'powershell.exe' if windows else 'sh'
names = ['docs/guides/standalone-installation.md', 'docs/guides/data-location.md', 'docs/guides/use-pack.md']
documents = {name: (root / name).read_text(encoding='utf-8') for name in names}

def block(body, lang):
    return re.search(r'^```' + lang + r'\n(.*?)^```', body, re.M | re.S).group(1)

installation = block(documents[names[0]], language)
storage = block(documents[names[1]], language)
archive_name = ('pactrun-1.0.0-alpha.2-windows-x86_64-standalone.zip' if windows
                else 'pactrun-1.0.0-alpha.2-linux-x86_64-standalone.tar.gz')
environment = dict(os.environ)
environment.pop('PACTRUN_STORAGE_ROOT', None)
if windows:
    # Windows PowerShell must discover its own modules, not inherit the host's PS7 paths.
    environment.pop('PSModulePath', None)
    environment.pop('PSMODULEPATH', None)
cases = []
commands = []

def psquote(value):
    return "'" + str(value).replace("'", "''") + "'"

def execute_snippet(code, cwd, env, success):
    state_file = cwd / 'observed-state.json'
    if windows:
        wrapper = "$ok = $false\ntry {\n" + code + "\n$ok = $true\n} catch { [Console]::Error.WriteLine($_.Exception.Message) }\n"
        wrapper += "$chosen = Get-Command pactrun -ErrorAction SilentlyContinue\n"
        wrapper += "$state = [ordered]@{ok=$ok; selected=$chosen.Source; cwd=(Get-Location).Path; store=$env:PACTRUN_STORAGE_ROOT; path=$env:Path} | ConvertTo-Json -Compress\n"
        wrapper += '[IO.File]::WriteAllText(' + psquote(state_file) + ', $state)\nif (-not $ok) { exit 1 }\n'
        invocation = [shell, '-NoProfile', '-NonInteractive', '-Command', wrapper]
    else:
        state_file = cwd / 'observed-state.txt'
        wrapper = code + '\nstatus=$?\nprintf "%s\\n" "$status" "$(command -v pactrun || true)" "$PWD" "${PACTRUN_STORAGE_ROOT-}" "$PATH" > observed-state.txt\nexit "$status"\n'
        invocation = [shell, '-c', wrapper]
    result = subprocess.run(invocation, cwd=cwd, env=env, capture_output=True, text=True, encoding='utf-8', timeout=60)
    (cwd / 'stdout.log').write_text(result.stdout, encoding='utf-8')
    (cwd / 'stderr.log').write_text(result.stderr, encoding='utf-8')
    assert (result.returncode == 0) == success, (cwd, result.stdout, result.stderr)
    if windows:
        state = json.loads(state_file.read_text(encoding='utf-8-sig'))
    else:
        status, selected, current, store, path = state_file.read_text().splitlines()
        state = {'ok': status == '0', 'selected': selected, 'cwd': current, 'store': store, 'path': path}
    assert state['ok'] == success
    return state

program = None
for case in ('fresh', 'wrong-checksum', 'missing-checksum', 'existing-directory'):
    cwd = work / ('installation ' + case)
    cwd.mkdir()
    shutil.copyfile(archive, cwd / archive_name)
    if case == 'existing-directory':
        (cwd / 'pactrun-bin').mkdir()
        (cwd / 'pactrun-bin/sentinel').write_text('preserve')
    checksum = args.checksum if case not in ('wrong-checksum', 'missing-checksum') else ('0' * 64 if case == 'wrong-checksum' else 'PASTE_TRUSTED_SHA256')
    state = execute_snippet(installation.replace('PASTE_TRUSTED_SHA256', checksum), cwd, environment, case == 'fresh')
    assert not state['store'], 'Installation selected or initialized management data'
    assert not (cwd / 'pactrun-data').exists()
    if case == 'fresh':
        program = cwd / 'pactrun-bin' / ('pactrun.exe' if windows else 'pactrun')
        assert Path(state['selected']).resolve() == program.resolve()
    else:
        assert state['path'] == environment['PATH']
        if case == 'existing-directory': assert (cwd / 'pactrun-bin/sentinel').read_text() == 'preserve'
        else: assert not (cwd / 'pactrun-bin').exists()
    cases.append({'case': case, 'accepted': state['ok'], 'management_root_unchanged': True})

assert program is not None
test_env = dict(environment, PATH=str(program.parent) + os.pathsep + environment['PATH'])
store = None
for case in ('fresh', 'existing-directory', 'existing-file'):
    cwd = work / ('storage ' + case)
    cwd.mkdir()
    if case == 'existing-directory':
        (cwd / 'pactrun-data').mkdir()
        (cwd / 'pactrun-data/sentinel').write_text('preserve')
    elif case == 'existing-file': (cwd / 'pactrun-data').write_text('preserve')
    state = execute_snippet(storage, cwd, dict(test_env, PACTRUN_STORAGE_ROOT='unchanged-test-store'), case == 'fresh')
    if case == 'fresh':
        store = cwd / 'pactrun-data'
        assert Path(state['store']).resolve() == store.resolve()
        for child in ('database', 'runtime-content', 'staging'): assert (store / child).is_dir()
    else:
        assert state['store'] == 'unchanged-test-store'
        sentinel = cwd / ('pactrun-data/sentinel' if case == 'existing-directory' else 'pactrun-data')
        assert sentinel.read_text() == 'preserve'
    cases.append({'case': 'store-' + case, 'accepted': state['ok']})

def run(binary, selected_store, *arguments):
    result = subprocess.run([str(binary), *arguments], cwd=work, env=dict(test_env, PACTRUN_STORAGE_ROOT=str(selected_store)), capture_output=True, text=True, encoding='utf-8', timeout=45)
    assert result.returncode == 0, (arguments, result.stdout, result.stderr)
    commands.append({'args': list(arguments), 'exit': result.returncode, 'store': str(selected_store)})
    return result.stdout

# The supplied Pack is an explicit fixture produced outside the reader's store.
producer = work / 'fixture-store'
for child in ('database', 'runtime-content', 'staging'): (producer / child).mkdir(parents=True)
author = (root / 'docs/package-authors/fundamentals/authoring-model.md').read_text(encoding='utf-8')
if windows: author = author.split('### Windows variant\n')[1]
manifest = block(author, 'yaml')
package_id = run(fixture_binary, producer, 'pack', 'generate-id').strip()
manifest = re.sub(r'package_id: "[a-f0-9]{32}"', 'package_id: "' + package_id + '"', manifest)
source = work / 'fixture-pack'; source.mkdir()
(source / 'pactrun.yaml').write_text(manifest, encoding='utf-8')
(source / ('inspect.ps1' if windows else 'inspect.sh')).write_text(block(author, language), encoding='utf-8')
reference = run(fixture_binary, producer, 'pack', 'install', str(source)).splitlines()[0]
run(fixture_binary, producer, 'revision', 'export', reference, '--output', str(work / 'supplied'))
reference = run(program, store, 'pack', 'install', str(work / 'supplied.pack')).splitlines()[0]
for arguments in [('revision', 'show', reference), ('instance', 'list'),
                  ('instance', 'create', 'demo', '--revision', reference), ('instance', 'show', 'demo'),
                  ('input', 'list', 'demo'), ('action', 'list', 'demo'), ('invoke', 'demo', 'inspect', '--plan')]:
    run(program, store, *arguments)
assert 'Hello from the Pack' in run(program, store, 'invoke', 'demo', 'inspect')
run(program, store, 'instance', 'delete', 'demo', '--plan')
run(program, store, 'instance', 'delete', 'demo')
receipt = {'status': 'Passed', 'archive_sha256': args.checksum.lower(),
           'installed_binary_sha256': hashlib.sha256(program.read_bytes()).hexdigest(),
           'documents': {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in names},
           'cases': cases, 'commands': commands, 'retained_workspace': str(work)}
(work / 'receipt.json').write_text(json.dumps(receipt, indent=2), encoding='utf-8')
print(json.dumps(receipt, indent=2))
