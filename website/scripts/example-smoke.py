"""Exercise tutorial blocks using an explicit binary and isolated persistent storage."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
parser.add_argument('--workspace', required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
binary = Path(args.binary).resolve(strict=True)
workspace = Path(args.workspace).resolve()
workspace.mkdir(parents=True, exist_ok=True)
work = Path(tempfile.mkdtemp(prefix='f-example-', dir=workspace))
for child in ['database', 'runtime-content', 'staging']:
    (work / 'store' / child).mkdir(parents=True)
env = dict(os.environ, PACTRUN_STORAGE_ROOT=str(work / 'store'))
steps = []

def run(*arguments, expected=0):
    result = subprocess.run([str(binary), *arguments], cwd=work, env=env,
                            capture_output=True, text=True, encoding='utf-8', timeout=40)
    if result.returncode != expected:
        raise AssertionError((arguments, result.returncode, result.stdout, result.stderr))
    steps.append({'command': list(arguments), 'exit': result.returncode})
    return result.stdout

def block(file, language, heading=None):
    source = (root / file).read_text(encoding='utf-8')
    if heading:
        source = source.split('\n### ' + heading + '\n', 1)[1]
    match = re.search(r'^\x60{3}' + language + r'\n(.*?)^\x60{3}', source, re.M | re.S)
    if not match:
        raise AssertionError('Missing example: ' + file)
    return match.group(1)

def install(name, manifest, script=None):
    pack = work / name
    pack.mkdir()
    package_id = run('pack', 'generate-id').strip()
    assert re.fullmatch('[0-9a-f]{32}', package_id), package_id
    manifest = re.sub(r'package_id: "[0-9a-f]+"', 'package_id: "' + package_id + '"', manifest)
    if script is not None:
        filename, content = script
        (pack / filename).write_text(content, encoding='utf-8')
    (pack / 'pactrun.yaml').write_text(manifest, encoding='utf-8')
    output = run('pack', 'install', str(pack))
    reference = re.search(r'exact:[0-9a-f]{32}/sha256:[0-9a-f]{64}', output)
    assert reference, output
    return reference.group()

version = run('--version').strip()
reference = install('minimal', block('docs/introduction.md', 'yaml'))
run('revision', 'list', '--no-trunc')
run('instance', 'create', 'demo', '--revision', reference)
run('instance', 'show', 'demo')
run('input', 'list', 'demo')
run('action', 'list', 'demo')
run('invoke', 'demo', 'missing', expected=1)
run('instance', 'delete', 'demo', '--plan')
run('instance', 'delete', 'demo')
run('instance', 'history', 'list')

author = 'docs/package-authors/fundamentals/authoring-model.md'
manifest = block(author, 'yaml', 'Windows variant' if os.name == 'nt' else None)
if os.name == 'nt':
    script = ('inspect.ps1', block(author, 'powershell'))
else:
    script = ('inspect.sh', block(author, 'sh'))
reference = install('hook', manifest, script)
run('instance', 'create', 'hook-demo', '--revision', reference)
run('action', 'show', 'hook-demo', 'inspect')
run('invoke', 'hook-demo', 'inspect', '--plan')
output = run('invoke', 'hook-demo', 'inspect', '--action-timeout-ms', '10000')
assert 'Hello from the Pack' in output, output
runs = json.loads(run('--format', 'json', 'run', 'list', 'hook-demo'))

def find_run_id(value):
    if isinstance(value, dict):
        for key, child in value.items():
            if key == 'run_id' and isinstance(child, str) and re.fullmatch('[0-9a-f]{32}', child):
                return child
        for child in value.values():
            found = find_run_id(child)
            if found: return found
    if isinstance(value, list):
        for child in value:
            found = find_run_id(child)
            if found: return found
run_id = find_run_id(runs)
assert run_id, runs
run('run', 'show', run_id)
run('revision', 'export', reference, '--output', str(work / 'tutorial-export'))
assert (work / 'tutorial-export.pack').is_file()
run('pack', 'install', str(work / 'tutorial-export.pack'))
run('instance', 'delete', 'hook-demo', '--plan')
run('instance', 'delete', 'hook-demo')
run('storage', 'gc', '--plan')
receipt = {'status': 'Passed', 'version': version,
           'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
           'tutorial_sha256': {file: hashlib.sha256((root / file).read_bytes()).hexdigest()
                               for file in ['docs/introduction.md', author]},
           'steps': steps, 'retained_workspace': str(work)}
(work / 'receipt.json').write_text(json.dumps(receipt, indent=2), encoding='utf-8')
print(json.dumps(receipt, indent=2))
