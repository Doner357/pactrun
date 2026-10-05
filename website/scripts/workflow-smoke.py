"""Exercise documented Snapshot, Migration and Invoke workflows in isolated storage."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import zipfile

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
parser.add_argument('--workspace', required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
binary = Path(args.binary).resolve(strict=True)
workspace = Path(args.workspace).resolve()
workspace.mkdir(parents=True, exist_ok=True)
work = Path(tempfile.mkdtemp(prefix='workflow-', dir=workspace))
for child in ('database', 'runtime-content', 'staging'):
    (work / 'store' / child).mkdir(parents=True)
env = dict(os.environ, PACTRUN_STORAGE_ROOT=str(work / 'store'))
steps = []
operator_steps = []
files = [
    'docs/package-authors/managed-capabilities/snapshots-and-managed-data.md',
    'docs/package-authors/managed-capabilities/migrations.md',
    'docs/package-authors/fundamentals/authoring-model.md',
    'docs/pactrun-users/operations/invoke-reference.md',
]

def blocks(file, language):
    return re.findall(r'^\x60{3}' + language + r'\n(.*?)^\x60{3}', (root / file).read_text(encoding='utf-8'), re.M | re.S)

def run(*arguments, expected=0, stdin=None):
    result = subprocess.run([str(binary), *arguments], cwd=work, env=env,
                            input=stdin, capture_output=True, text=True, encoding='utf-8', timeout=45)
    if expected is None:
        assert result.returncode != 0, arguments
    elif result.returncode != expected:
        raise AssertionError((arguments, result.returncode, result.stdout, result.stderr))
    steps.append({'args': list(arguments), 'exit': result.returncode, 'storage_root': env['PACTRUN_STORAGE_ROOT']})
    return result

def package_id():
    value = run('pack', 'generate-id').stdout.strip()
    assert re.fullmatch('[0-9a-f]{32}', value)
    return value

def prepare(name, manifest, package, stem=None, script=None, install=True):
    directory = work / name
    directory.mkdir()
    manifest = re.sub(r'package_id: "[0-9a-f]+"', 'package_id: "' + package + '"', manifest)
    (directory / 'pactrun.yaml').write_text(manifest, encoding='utf-8')
    if stem:
        (directory / (stem + ('.ps1' if os.name == 'nt' else '.sh'))).write_text(script, encoding='utf-8')
    if not install:
        return directory
    installed = run('pack', 'install', str(directory)).stdout
    match = re.search(r'exact:[0-9a-f]{32}/sha256:[0-9a-f]{64}', installed)
    assert match, installed
    return match.group()


def section(file, heading):
    source = (root / file).read_text(encoding='utf-8')
    marker = '\n### ' + heading + '\n'
    assert source.count(marker) == 1, heading
    return re.split(r'^### ', source.split(marker, 1)[1], maxsplit=1, flags=re.M)[0]

def section_block(file, heading, language):
    return re.search(r'^\x60{3}' + language + r'\n(.*?)^\x60{3}', section(file, heading), re.M | re.S).group(1)

def reference(output):
    value = re.search(r'^exact:[0-9a-f]{32}/sha256:[0-9a-f]{64}$', output, re.M)
    assert value, output
    return value.group()

def documented(file, heading, values, reference_key=None, expect_refusal=False):
    results = []
    for block in re.findall(r'^\x60{3}text\n(.*?)^\x60{3}', section(file, heading), re.M | re.S):
        for line in block.splitlines():
            if not line.startswith('pactrun '):
                continue
            for key, value in values.items():
                line = line.replace('<' + key + '>', value)
            assert not re.search(r'<[^>]+>', line), line
            arguments = shlex.split(line)[1:]
            refused = expect_refusal and arguments[:2] in (['snapshot', 'restore'], ['instance', 'migrate'])
            result = run(*arguments, expected=1 if refused else 0)
            steps[-1]['tutorial_section'] = heading
            if arguments[:2] == ['pack', 'install'] and reference_key:
                values[reference_key] = reference(result.stdout)
            if arguments[:2] == ['snapshot', 'capture'] and '--plan' not in arguments:
                values['snapshot-id'] = re.search(r'^snapshot: ([0-9a-f]+)', result.stdout, re.M).group(1)
            results.append((arguments, result))
    assert results, heading
    return results

def operator_example(file, heading, expected_root=None, refusal=False):
    language = 'powershell' if os.name == 'nt' else 'sh'
    body = re.search(r'^\x60{3}' + language + r'\n(.*?)^\x60{3}', section(file, heading), re.M | re.S).group(1)
    # A new subprocess stands in for the dedicated tutorial terminal. Assert its
    # actual root selection before mirroring that environment into subsequent CLI calls.
    if expected_root:
        if os.name == 'nt':
            quoted = str(expected_root).replace("'", "''")
            body += "\nif ($env:PACTRUN_STORAGE_ROOT -ne '" + quoted + "') { throw 'Tutorial selected the wrong root.' }\n"
        else:
            body += '\n[ "$PACTRUN_STORAGE_ROOT" = ' + shlex.quote(str(expected_root)) + ' ] || exit 1\n'
    script = work / ('operator-' + str(len(operator_steps)) + ('.ps1' if os.name == 'nt' else '.sh'))
    script.write_text(body, encoding='utf-8')
    command = ['pwsh.exe', '-NoLogo', '-NoProfile', '-File', str(script)] if os.name == 'nt' else ['sh', str(script)]
    result = subprocess.run(command, cwd=work, env=env, capture_output=True, text=True, encoding='utf-8', timeout=30)
    assert (result.returncode != 0 if refusal else result.returncode == 0), (heading, result.returncode, result.stdout, result.stderr)
    operator_steps.append({'section': heading, 'exit': result.returncode, 'expected_refusal': refusal})
    if expected_root and not refusal:
        env['PACTRUN_STORAGE_ROOT'] = str(expected_root)
    return result

lang = 'powershell' if os.name == 'nt' else 'sh'
# Execute the published Snapshot command blocks, including a genuinely new store.
source_root = Path(env['PACTRUN_STORAGE_ROOT'])
values = {}
snapshot_manifest = section_block(files[0], 'Windows Snapshot variant', 'yaml') if os.name == 'nt' else blocks(files[0], 'yaml')[0]
prepare('snapshot-demo', snapshot_manifest, package_id(), 'snapshot', blocks(files[0], lang)[0], install=False)
original = documented(files[0], 'Exercise Capture and Restore', values, reference_key='snapshot-reference')
assert 'Verified snapshot bytes' in original[-1][1].stdout
snapshot_ref, snapshot_id = values['snapshot-reference'], values['snapshot-id']
run('snapshot', 'verify', snapshot_id[:8])
documented(files[0], 'Export the recovery files', values)
assert (work / 'snapshot-pack.pack').is_file() and (work / 'backup.snapshot').is_file()
recovery_root = work / 'recovery-store'
assert not recovery_root.exists()
operator_example(files[0], 'Prepare a fresh destination store', expected_root=recovery_root)
assert source_root != recovery_root
before = documented(files[0], 'Prepare a fresh destination store', values)
assert reference(before[0][1].stdout) == snapshot_ref
assert snapshot_id not in before[-1][1].stdout
# The published setup guard must not silently reuse a pre-existing destination.
guard = operator_example(files[0], 'Prepare a fresh destination store', refusal=True)
assert 'already exists' in guard.stdout + guard.stderr
fresh = documented(files[0], 'Import and restore in the destination', values)
assert 'already_present' not in fresh[0][1].stdout
assert snapshot_id in fresh[1][1].stdout
assert 'Verified snapshot bytes' in fresh[-1][1].stdout
fresh_import_output = fresh[0][1].stdout
operator_example(files[0], 'Prepare an incompatible Revision')
documented(files[0], 'Prepare an incompatible Revision', values, reference_key='other-reference')
assert values['other-reference'] != snapshot_ref
negative = documented(files[0], 'Check the incompatible Restore refusal', values, expect_refusal=True)
assert 'exact producer Revision' in negative[-1][1].stderr
assert 'Verified other revision bytes' not in negative[-1][1].stdout
documented(files[0], 'Retire destination test objects', values)
assert snapshot_id not in run('snapshot', 'list', '--no-trunc').stdout
operator_example(files[0], 'Return to the source store', expected_root=source_root)
source_cleanup = documented(files[0], 'Return to the source store', values)
assert snapshot_id in source_cleanup[0][1].stdout
fresh_recovery = {'source_root': str(source_root), 'destination_root': str(recovery_root),
                  'absent_before_import': True, 'producer_reference_matched': True,
                  'import_output': fresh_import_output, 'restore_message_observed': True,
                  'source_survived_destination_cleanup': True}

# Migration example: one same-Package carry transition with a plain Hook.
lineage = package_id()
source_ref = prepare('migration-source', blocks(files[1], 'yaml')[0], lineage)
config = work / 'config.txt'
config.write_bytes(b'documented-config\n')
run('instance', 'create', 'migration-demo', '--revision', source_ref, '--input-file', 'config=' + str(config))
target_heading = 'Windows target variant' if os.name == 'nt' else 'Declare the target'
target_yaml = section_block(files[1], target_heading, 'yaml').replace('SOURCE_DIGEST', source_ref.split('/')[1])
target_script = section_block(files[1], target_heading, lang)
target_ref = prepare('migration-target', target_yaml, lineage, 'migrate', target_script)
paths = run('--format', 'json', 'instance', 'migration-paths', 'migration-demo', '--to', target_ref)

def find_key(value, key):
    if isinstance(value, dict):
        if key in value: return value[key]
        for child in value.values():
            found = find_key(child, key)
            if found is not None: return found
    if isinstance(value, list):
        for child in value:
            found = find_key(child, key)
            if found is not None: return found

path_id = find_key(json.loads(paths.stdout), 'path_id')
assert path_id
assert path_id.startswith('mp1-')
run('instance', 'migrate', 'migration-demo', '--to', target_ref, '--path', path_id[:12], '--plan')
run('instance', 'migrate', 'migration-demo', '--to', target_ref, '--path', path_id, '--plan')
migrated = run('instance', 'migrate', 'migration-demo', '--to', target_ref, '--path', path_id)
assert 'Migration hook completed' in migrated.stdout
shown = run('instance', 'show', 'migration-demo')
assert target_ref in shown.stdout
run('input', 'export', 'migration-demo', 'settings', '--output', str(work / 'settings-copy.txt'))
assert (work / 'settings-copy.txt').read_bytes() == config.read_bytes()
# Run the exact comparison snippet, then prove it detects a mismatch and a read error.
comparison = operator_example(files[1], 'Compare the exported Input')
assert 'Input bytes match.' in comparison.stdout
copy = work / 'settings-copy.txt'
saved = copy.read_bytes()
copy.write_bytes(b'wrong-data\n')
mismatch = operator_example(files[1], 'Compare the exported Input', refusal=True)
assert 'Input bytes match.' not in mismatch.stdout
copy.unlink()
missing = operator_example(files[1], 'Compare the exported Input', refusal=True)
assert 'Input bytes match.' not in missing.stdout
copy.write_bytes(saved)
# Build the unrelated Package from the third published manifest.
operator_example(files[1], 'Prepare an unrelated Package')
unrelated = work / 'migration-unrelated'
new_id = package_id()
assert new_id != lineage
unrelated_yaml = re.sub(r'package_id: "[0-9a-f]+"', 'package_id: "' + new_id + '"', section_block(files[1], 'Prepare an unrelated Package', 'yaml'))
(unrelated / 'pactrun.yaml').write_text(unrelated_yaml, encoding='utf-8')
unrelated_ref = reference(run('pack', 'install', './migration-unrelated').stdout)
refusal = documented(files[1], 'Check the cross-Package refusal', {'unrelated-reference': unrelated_ref}, expect_refusal=True)[0][1]
assert 'one Package lineage' in refusal.stderr
documented(files[1], 'Retire the test Instance', {})

# Invoke reference: a declared parameter exercises ordinary/protected sources.
author_manifest = section_block(files[2], 'Windows variant', 'yaml') if os.name == 'nt' else blocks(files[2], 'yaml')[0]
manifest = author_manifest.replace('parameters: []', 'parameters: [{id: message, type: string, sensitive: false, default: hello}]')
invoke_ref = prepare('invoke-demo', manifest, package_id(), 'inspect', blocks(files[2], lang)[0])
run('instance', 'create', 'demo', '--revision', invoke_ref)
message = work / 'message.txt'
message.write_text('protected-example-marker', encoding='utf-8')
plan = run('invoke', 'demo', 'inspect', '--param', 'message=hello', '--plan')
assert 'startup_timeout_ms: unlimited' in plan.stdout
assert 'action_timeout_ms: unlimited' in plan.stdout
assert 'termination_grace_ms: 5000' in plan.stdout
protected = run('invoke', 'demo', 'inspect', '--param-file', 'message=' + str(message), '--plan')
assert 'protected-example-marker' not in protected.stdout + protected.stderr
run('invoke', 'demo', 'inspect', '--param-file', 'message=' + str(message), '--action-timeout-ms', '10000')
run('--format', 'json', 'invoke', 'demo', 'inspect', '--param', 'message=hello')
run('invoke', 'demo', 'inspect', '--param-stdin', 'message', stdin='hello')
run('invoke', 'demo', 'inspect', '--param', 'message=a', '--param-file', 'message=' + str(message), expected=2)
run('invoke', 'demo', 'inspect', '--action-timeout-ms', '-1', expected=2)
run('invoke', 'demo', 'inspect', '--help', expected=2)
run('invoke', 'demo', 'inspect', '--param-file', 'message=' + str(work / 'missing.txt'), expected=None)
# Disputed current-contract claims: version strings and diagnostic retention.
event = work / 'diagnostic.json'
event.write_text(json.dumps({'type': 'diagnostic', 'severity': 'info', 'code': 'review_marker', 'message': 'F_CORRECTION_DIAGNOSTIC'}), encoding='utf-8')
if os.name == 'nt':
    diagnostic_script = "& $env:PACTRUN_EXECUTABLE hook diagnostic --file '" + str(event).replace("'", "''") + "'\nexit $LASTEXITCODE\n"
else:
    diagnostic_script = '"$PACTRUN_EXECUTABLE" hook diagnostic --file "' + str(event) + '"\n'
diagnostic_manifest = author_manifest
diagnostic_ref = prepare('diagnostic-pack', diagnostic_manifest, package_id(), 'inspect', diagnostic_script)
run('instance', 'create', 'diagnostic-demo', '--revision', diagnostic_ref)
for omit in (False, True):
    options = ['--no-retain-hook-text'] if omit else []
    result = run('invoke', 'diagnostic-demo', 'inspect', '--action-timeout-ms', '10000', *options)
    assert 'F_CORRECTION_DIAGNOSTIC' in result.stderr
    run_id = re.search(r'^run: ([0-9a-f]+)', result.stderr, re.M).group(1)
    history = run('run', 'show', run_id)
    assert ('F_CORRECTION_DIAGNOSTIC' in history.stdout) == (not omit)
run('revision', 'export', diagnostic_ref, '--output', str(work / 'canonical-check'))
with zipfile.ZipFile(work / 'canonical-check.pack') as archive:
    canonical = json.loads(archive.read('revision-core.json'))
    expected = re.search(r'protocol_version: "([^"]+)"', diagnostic_manifest).group(1)
    assert canonical['format_version'] == expected
    assert canonical['actions'][0]['hook']['protocol_version'] == expected
bad = work / 'numeric-version'
bad.mkdir()
bad_manifest = diagnostic_manifest.replace('protocol_version: "' + expected + '"', 'protocol_version: 1')
(bad / 'pactrun.yaml').write_text(bad_manifest, encoding='utf-8')
(bad / 'inspect.sh').write_text('exit 0\n', encoding='utf-8')
run('pack', 'install', str(bad), expected=None)
run('storage', 'upgrade', expected=2)
for instance in ('demo', 'diagnostic-demo'):
    run('instance', 'delete', instance, '--plan')
    run('instance', 'delete', instance)
machine = json.loads(run('--format', 'json', 'instance', 'deletion', 'list').stdout)
schema = json.loads((root / 'docs/spec/interfaces/cli-machine.schema.json').read_text(encoding='utf-8'))
assert machine['format'] == schema['properties']['format']['const']
assert machine['format_version'] == schema['properties']['format_version']['const']
assert machine['status'] == 'success'
receipt = {'status': 'Passed', 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
           'document_sha256': {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in files},
           'commands': steps, 'operator_examples': operator_steps, 'fresh_snapshot_recovery': fresh_recovery, 'retained_workspace': str(work)}
(work / 'receipt.json').write_text(json.dumps(receipt, indent=2), encoding='utf-8')
print(json.dumps(receipt, indent=2))
